use crate::error::{Result, ensure};
use crate::packager::Build;
use crate::platform::{GitHubReleases, Platform, desktop};
use crate::workflow::{Ci, ci_options};
use crate::{RougePublish, Target};
use framer::github::{Job, Step};
use std::path::Path;
use xshell::cmd;

const TRIPLE: &str = "x86_64-unknown-linux-gnu";
#[derive(Clone, Debug)]
pub struct Linux {
    apt_packages: Vec<String>,
    ci: Ci,
}

impl Linux {
    pub fn new(_destination: GitHubReleases) -> Self {
        Self {
            apt_packages: vec!["libfontconfig1-dev".into()],
            ci: Ci::default(),
        }
    }

    pub fn apt_packages<T: Into<String>>(mut self, packages: impl IntoIterator<Item = T>) -> Self {
        self.apt_packages
            .extend(packages.into_iter().map(Into::into));
        self
    }
}

impl From<GitHubReleases> for Linux {
    fn from(destination: GitHubReleases) -> Self {
        Self::new(destination)
    }
}

ci_options!(Linux);

impl Platform for Linux {
    fn target(&self) -> Target {
        Target::Linux
    }

    fn group(&self) -> String {
        "desktop".into()
    }

    fn job(&self, _config: &RougePublish) -> Job {
        let install = format!(
            "sudo apt-get update && sudo apt-get install -y {}",
            self.apt_packages.join(" ")
        );
        desktop::job(
            self.target(),
            &self.ci,
            "ubuntu-24.04",
            Some(Step::run(install)),
            &[],
        )
    }

    fn guide(&self) -> &'static str {
        include_str!("guide.md")
    }

    fn requirements(&self) -> String {
        format!(
            " Needs the development packages `{}`.",
            self.apt_packages.join("`, `")
        )
    }

    fn validate(&self, _config: &RougePublish) -> Result<(), String> {
        let named = |package: &&String| {
            !package.is_empty()
                && package
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b".+-:".contains(&c))
        };
        match self.apt_packages.iter().find(|package| !named(package)) {
            Some(package) => Err(format!("`{package}` is not an apt package name")),
            None => Ok(()),
        }
    }

    fn doctor(&self, _build: &Build) -> Result<()> {
        ensure(
            cfg!(all(target_os = "linux", target_arch = "x86_64")),
            "Use an x86-64 Linux host",
        )
    }

    fn build(&self, build: &Build) -> Result<()> {
        self.doctor(build)?;
        let app = &build.app;
        let stem = format!("{}-{}-linux-x86_64", app.target, app.version);
        let staging = build.out.join(&stem);

        stage_release(build, &staging)?;
        archive_release(build, &stem, &staging)
    }

    fn publish(&self, build: &Build) -> Result<()> {
        desktop::publish(build)
    }
}

fn stage_release(build: &Build, staging: &Path) -> Result<()> {
    let sh = &build.sh;
    let binary = &build.app.target;
    build.copy(build.compile(TRIPLE)?.join(binary), staging.join(binary))?;
    sh.write_file(
        staging.join("README.txt"),
        format!(
            "Launch ./{binary}. Requires a graphical Wayland or X11 session, a working GPU driver, and Fontconfig.\n"
        ),
    )?;
    Ok(())
}

fn archive_release(build: &Build, stem: &str, staging: &Path) -> Result<()> {
    let (sh, out) = (&build.sh, &build.out);
    cmd!(sh, "tar -C {out} -czf {staging}.tar.gz {stem}").run()?;
    Ok(sh.remove_path(staging)?)
}
