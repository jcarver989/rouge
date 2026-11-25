use crate::error::BenchError;
use rustc_hash::FxHasher;
use std::env::current_exe;
use std::ffi::OsStr;
use std::fs::{copy, create_dir_all, read, read_dir, remove_dir_all};
use std::hash::{Hash, Hasher};
use std::io::Error as IoError;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::{NamedTempFile, tempdir_in};

const BINARY: &str = "rouge-bench";
const CRATE: &str = "crates/rouge-bench";
const MAIN: &str = "main";
const SHORT: usize = 10;

pub struct Build {
    pub label: String,
    pub binary: PathBuf,
}

pub struct Builds {
    root: PathBuf,
}

impl Builds {
    pub fn open() -> Result<Self, BenchError> {
        let root = git(Path::new("."), &["rev-parse", "--show-toplevel"])?;
        Ok(Builds {
            root: PathBuf::from(root),
        })
    }

    pub fn out(&self) -> Result<PathBuf, BenchError> {
        let out = self.root.join("target/bench");
        create_dir_all(&out)?;
        Ok(out)
    }

    pub fn current(&self) -> Result<Build, BenchError> {
        Ok(Build {
            label: "working tree".to_owned(),
            binary: current_exe()?,
        })
    }

    pub fn save(&self, name: &str) -> Result<Build, BenchError> {
        let binary = self.saved(name)?;
        store(&self.current()?.binary, &binary)?;
        Ok(Build {
            label: name.to_owned(),
            binary,
        })
    }

    pub fn base(&self) -> Result<Build, BenchError> {
        for base in [MAIN, "origin/main"] {
            if let Ok(commit) = git(&self.root, &["merge-base", "HEAD", base]) {
                return self.revision(&commit, base);
            }
        }
        Err(BenchError::NoMergeBase)
    }

    pub fn resolve(&self, name: &str) -> Result<Build, BenchError> {
        if let Ok(binary) = self.saved(name)
            && binary.exists()
        {
            return Ok(Build {
                label: name.to_owned(),
                binary,
            });
        }
        let commit = format!("{name}^{{commit}}");
        let commit = git(
            &self.root,
            &["rev-parse", "--verify", "--end-of-options", &commit],
        )
        .map_err(|_| BenchError::UnknownBuild(name.to_owned()))?;
        self.revision(&commit, name)
    }

    fn revision(&self, commit: &str, name: &str) -> Result<Build, BenchError> {
        let short = commit.get(..SHORT).unwrap_or(commit);
        let label = if commit.starts_with(name) {
            short.to_owned()
        } else {
            format!("{name} ({short})")
        };
        let binary = self.out()?.join("builds").join(self.key(short)?);
        let binary = binary.join(BINARY);
        if !binary.exists() {
            self.build(commit, &label, &binary)?;
        }
        Ok(Build { label, binary })
    }

    fn build(&self, commit: &str, label: &str, binary: &Path) -> Result<(), BenchError> {
        eprintln!("building {label}; the result is kept for the next comparison");
        let scratch = tempdir_in(self.out()?)?;
        let tree = scratch.path().join("tree");
        let archive = scratch.path().join("tree.tar");
        create_dir_all(&tree)?;
        let tar = archive.to_string_lossy();
        git(&self.root, &["archive", "--output", &tar, commit])?;
        let mut unpack = Command::new("tar");
        unpack.arg("-xmf").arg(&archive).arg("-C").arg(&tree);
        output(&mut unpack)?;
        let overlay = tree.join(CRATE);
        if overlay.exists() {
            remove_dir_all(&overlay)?;
        }
        copy_tree(&self.root.join(CRATE), &overlay)?;
        let target = scratch.path().join("target");
        let built = Command::new("cargo")
            .args(["build", "--release", "-p", BINARY])
            .env("CARGO_TARGET_DIR", &target)
            .current_dir(&tree)
            .stdout(Stdio::null())
            .status()?;
        if !built.success() {
            return Err(BenchError::BuildFailed(label.to_owned()));
        }
        store(&target.join("release").join(BINARY), binary)?;
        Ok(())
    }

    fn saved(&self, name: &str) -> Result<PathBuf, BenchError> {
        let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.');
        if name.is_empty() || name.starts_with('.') || !name.chars().all(plain) {
            return Err(BenchError::BadName(name.to_owned()));
        }
        Ok(self.out()?.join("saved").join(name).join(BINARY))
    }

    fn key(&self, short: &str) -> Result<String, BenchError> {
        let mut hasher = FxHasher::default();
        for file in files(&self.root.join(CRATE))? {
            file.strip_prefix(&self.root)
                .unwrap_or(&file)
                .hash(&mut hasher);
            read(&file)?.hash(&mut hasher);
        }
        Ok(format!("{short}-{:016x}", hasher.finish()))
    }
}

fn store(from: &Path, to: &Path) -> Result<(), IoError> {
    let parent = to.parent().unwrap_or(Path::new("."));
    create_dir_all(parent)?;
    let partial = NamedTempFile::new_in(parent)?.into_temp_path();
    copy(from, &partial)?;
    partial.persist(to).map_err(|error| error.error)
}

fn files(dir: &Path) -> Result<Vec<PathBuf>, IoError> {
    let mut found = Vec::new();
    for entry in read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            found.extend(files(&path)?);
        } else {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), IoError> {
    for file in files(from)? {
        let copied = to.join(file.strip_prefix(from).unwrap_or(&file));
        if let Some(parent) = copied.parent() {
            create_dir_all(parent)?;
        }
        copy(&file, copied)?;
    }
    Ok(())
}

fn git<T: AsRef<OsStr>>(dir: &Path, arguments: &[T]) -> Result<String, BenchError> {
    output(Command::new("git").current_dir(dir).args(arguments))
}

fn output(command: &mut Command) -> Result<String, BenchError> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(BenchError::Command {
            command: format!("{command:?}"),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
