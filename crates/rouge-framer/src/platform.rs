mod android;
mod apple;
mod desktop;
mod ios;
mod linux;
mod macos;
mod web;

use crate::error::{Result, invalid};
use crate::packager::Build;
use crate::{RougePublish, Target};
pub use android::{Android, AndroidToolchain, PlayInternal};
pub use apple::AppleToolchain;
pub use desktop::GitHubReleases;
use framer::github::Job;
use framer::yaml::Yaml;
pub use ios::{Device, Ios, Orientation, TestFlight};
pub use linux::Linux;
pub use macos::Macos;
pub use web::{GitHubPages, Web};

pub(crate) trait Platform {
    fn target(&self) -> Target;

    fn job(&self, config: &RougePublish) -> Job;

    fn preview_job(&self, _config: &RougePublish) -> Option<Job> {
        None
    }

    fn environment(&self) -> Yaml {
        format!(
            "${{{{ inputs.channel == 'check' && 'rouge-package-check' || format('rouge-{}-{{0}}', inputs.channel) }}}}",
            self.target()
        )
        .into()
    }

    fn files(&self, _config: &RougePublish) -> Vec<(String, String)> {
        Vec::new()
    }

    fn ignored(&self) -> &'static str {
        ""
    }

    fn validate(&self, _config: &RougePublish) -> Result<(), String> {
        Ok(())
    }

    fn setup(&self, _build: &Build) -> Result<()> {
        invalid(format!("{} has no setup step", self.target()))
    }

    fn doctor(&self, _build: &Build) -> Result<()> {
        Ok(())
    }

    fn build(&self, build: &Build) -> Result<()>;

    fn run_local(&self, _build: &Build, _device: Option<&str>) -> Result<()> {
        invalid("run is only supported for iOS and Android")
    }

    fn stage(&self, _build: &Build) -> Result<()> {
        invalid("only Pages deployments need a staging step")
    }

    fn publish(&self, build: &Build) -> Result<()>;

    fn link(&self, _build: &Build) -> Result<()> {
        invalid(format!("publish {} prints its own link", self.target()))
    }
}

pub(crate) fn os_version(text: &str) -> Option<(u32, u32)> {
    let mut parts = text.split('.').map(str::parse);
    let version = (parts.next()?.ok()?, parts.next().unwrap_or(Ok(0)).ok()?);
    parts
        .all(|part: Result<u32, _>| part.is_ok())
        .then_some(version)
}

pub(crate) fn pinned(version: &str) -> bool {
    !version.is_empty()
        && version
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
}
