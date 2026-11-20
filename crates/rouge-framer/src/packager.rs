use crate::app::App;
use crate::error::{Result, ensure, invalid};
use crate::{RougePublish, secrets};
use clap::ValueEnum;
use std::env;
use std::fmt::{self, Display, Formatter};
use std::path::{self, Path, PathBuf};
use xshell::{Cmd, Shell, cmd};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Action {
    Setup,
    Doctor,
    Build,
    Run,
    Stage,
    Publish,
    Link,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Target {
    Web,
    Linux,
    Macos,
    Ios,
    Android,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum Channel {
    #[default]
    Check,
    Preview,
    Production,
}

impl Display for Target {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&format!("{self:?}").to_lowercase())
    }
}

impl Display for Channel {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&format!("{self:?}").to_lowercase())
    }
}

#[derive(Clone, Debug)]
pub struct Packager {
    config: RougePublish,
    root: PathBuf,
    channel: Channel,
    build_number: u32,
    simulator_device: Option<String>,
}

pub(crate) struct Build {
    pub(crate) sh: Shell,
    pub(crate) app: App,
    pub(crate) target: Target,
    pub(crate) channel: Channel,
    pub(crate) number: u32,
    pub(crate) root: PathBuf,
    pub(crate) out: PathBuf,
    cargo_target: PathBuf,
}

impl Packager {
    pub fn new(config: RougePublish, root: impl Into<PathBuf>) -> Self {
        Self {
            config,
            root: root.into(),
            channel: Channel::default(),
            build_number: 1,
            simulator_device: None,
        }
    }

    pub fn channel(mut self, channel: Channel) -> Self {
        self.channel = channel;
        self
    }

    pub fn build_number(mut self, build_number: u32) -> Self {
        self.build_number = build_number;
        self
    }

    pub fn simulator_device(mut self, device: impl Into<String>) -> Self {
        self.simulator_device = Some(device.into());
        self
    }

    pub fn run(&self, action: Action, target: Target) -> Result<()> {
        self.config.validate()?;
        let simulator = action == Action::Run;
        ensure(
            !simulator || target == Target::Ios,
            "run is only supported for iOS",
        )?;
        let Some(platform) = self.config.platform(target) else {
            return invalid(format!(
                "{target} is not enabled; add it to RougePublish in .framer/src/main.rs"
            ));
        };
        ensure(
            !matches!(action, Action::Stage | Action::Publish) || self.channel != Channel::Check,
            "Set ROUGE_CHANNEL to preview or production before publishing",
        )?;
        let build = self.prepare(target, simulator)?;
        match action {
            Action::Setup => platform.setup(&build),
            Action::Doctor => platform.doctor(&build),
            Action::Stage => platform.stage(&build),
            Action::Publish => platform.publish(&build),
            Action::Link => platform.link(&build),
            Action::Run => platform.run_local(&build, self.simulator_device.as_deref()),
            Action::Build => {
                build.sh.remove_path(&build.out)?;
                build.sh.create_dir(&build.out)?;
                platform.build(&build)?;
                println!("Packages: {}", build.out.display());
                Ok(())
            }
        }
    }

    fn prepare(&self, target: Target, simulator: bool) -> Result<Build> {
        let sh = Shell::new()?;
        let root = path::absolute(sh.current_dir().join(&self.root)).unwrap_or(self.root.clone());
        sh.change_dir(&root);
        let out = root.join(".rouge/out");
        Ok(Build {
            sh,
            app: App::resolve(&self.config, &root, target)?,
            target,
            channel: if simulator {
                Channel::Check
            } else {
                self.channel
            },
            number: if simulator { 1 } else { self.build_number },
            cargo_target: env::var_os("CARGO_TARGET_DIR")
                .map_or_else(|| out.join("cargo"), PathBuf::from),
            out: if simulator {
                out.join("ios-sim")
            } else {
                out.join(target.to_string())
            },
            root,
        })
    }
}

impl Build {
    pub(crate) fn publishing(&self) -> bool {
        self.channel != Channel::Check
    }

    pub(crate) fn isolated<'a>(&self, command: Cmd<'a>) -> Cmd<'a> {
        secrets::all()
            .fold(command, Cmd::env_remove)
            .env("CARGO_TARGET_DIR", &self.cargo_target)
    }

    pub(crate) fn compile(&self, triple: &str, release: bool) -> Result<PathBuf> {
        let (sh, package, binary) = (&self.sh, &self.app.package, &self.app.target);
        let (profile, dir) = if release {
            (Some("--release"), "release")
        } else {
            (None, "debug")
        };
        self.isolated(cmd!(
            sh,
            "cargo build --locked {profile...} --package {package} --bin {binary} --target {triple}"
        ))
        .run()?;
        Ok(self.cargo_target.join(triple).join(dir))
    }

    pub(crate) fn tool(&self, binary: &str, package: &str, version: &str) -> Result<PathBuf> {
        let sh = &self.sh;
        let found = self
            .isolated(cmd!(sh, "{binary} --version"))
            .quiet()
            .ignore_stderr()
            .read();
        if found.is_ok_and(|found| found.ends_with(&format!(" {version}"))) {
            return Ok(binary.into());
        }
        let tools = self
            .root
            .join(format!(".rouge/out/tools/{package}-{version}"));
        let installed = tools.join("bin").join(binary);
        if !installed.is_file() {
            self.isolated(cmd!(
                sh,
                "cargo install {package} --locked --version ={version} --root {tools}"
            ))
            .run()?;
        }
        Ok(installed)
    }

    pub(crate) fn copy(&self, from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
        if let Some(parent) = to.as_ref().parent() {
            self.sh.create_dir(parent)?;
        }
        Ok(self.sh.copy_file(from, to)?)
    }

    pub(crate) fn summarize(&self, line: &str) -> Result<()> {
        println!("{line}");
        let Some(summary) = env::var_os("GITHUB_STEP_SUMMARY") else {
            return Ok(());
        };
        let existing = self.sh.read_file(&summary).unwrap_or_default();
        Ok(self.sh.write_file(summary, format!("{existing}{line}\n"))?)
    }
}

pub(crate) fn need(program: &str) -> Result<()> {
    let found = env::var_os("PATH")
        .is_some_and(|paths| env::split_paths(&paths).any(|dir| dir.join(program).is_file()));
    ensure(found, format!("Install {program} and put it on PATH"))
}
