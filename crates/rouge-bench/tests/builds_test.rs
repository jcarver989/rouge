#![cfg(not(target_arch = "wasm32"))]

mod common;

use common::TestResult;
use std::fs::{create_dir_all, write};
use std::io::Error;
use std::process::{Command, Stdio};
use tempfile::{TempDir, tempdir};

#[test]
fn comparing_without_a_merge_base_requests_an_explicit_base() -> TestResult {
    let repo = Repo::new()?;
    let output = repo.compare().output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("specify --base"));
    Ok(())
}

#[test]
fn concurrent_comparisons_use_their_requested_revisions() -> TestResult {
    let repo = Repo::new()?;
    repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"])?;
    repo.commit("1")?;
    let forward = repo.compare().args(["--with", "HEAD"]).spawn()?;
    let reverse = repo
        .compare()
        .args(["--base", "HEAD", "--with", "origin/main"])
        .spawn()?;
    let forward = forward.wait_with_output()?;
    let reverse = reverse.wait_with_output()?;
    assert!(
        forward.status.success(),
        "{}",
        String::from_utf8_lossy(&forward.stderr)
    );
    assert!(String::from_utf8(forward.stdout)?.contains("1 better"));
    assert_eq!(
        reverse.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&reverse.stderr)
    );
    assert!(String::from_utf8(reverse.stdout)?.contains("1 with something worse"));
    Ok(())
}

struct Repo(TempDir);

impl Repo {
    fn new() -> TestResult<Self> {
        let repo = Repo(tempdir()?);
        let root = repo.0.path();
        repo.git(&["init", "-b", "feature"])?;
        let bench = root.join("crates/rouge-bench");
        create_dir_all(bench.join("src"))?;
        write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = ['crates/*']\nresolver = '3'\n",
        )?;
        write(
            bench.join("Cargo.toml"),
            "[package]\nname = 'rouge-bench'\nversion = '0.1.0'\nedition = '2024'\n",
        )?;
        write(
            bench.join("src/main.rs"),
            r#"fn main() { print!("{}", include_str!("../../../sample.json")); }"#,
        )?;
        repo.git(&["add", "Cargo.toml", "crates"])?;
        repo.commit("2")?;
        Ok(repo)
    }

    fn commit(&self, layouts: &str) -> TestResult {
        write(
            self.0.path().join("sample.json"),
            format!(r#"{{"frame.layouts":{layouts}}}"#),
        )?;
        self.git(&["add", "sample.json"])?;
        self.git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            layouts,
        ])
    }

    fn git(&self, args: &[&str]) -> TestResult {
        let output = Command::new("git")
            .current_dir(self.0.path())
            .args(args)
            .output()?;
        if !output.status.success() {
            return Err(Error::other(String::from_utf8(output.stderr)?).into());
        }
        Ok(())
    }

    fn compare(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rouge-bench"));
        command
            .current_dir(self.0.path())
            .args(["compare", "gallery", "--passes", "1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
}
