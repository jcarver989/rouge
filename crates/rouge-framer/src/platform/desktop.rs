use crate::error::{Result, ensure, invalid};
use crate::packager::Build;
use crate::workflow::{self, Ci, PUBLISHING};
use crate::{Channel, Target};
use framer::github::{Job, Step};
use std::env;
use xshell::cmd;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GitHubReleases;

pub(crate) fn job(
    target: Target,
    ci: &Ci,
    runner: &str,
    setup: Option<Step>,
    secrets: &[&str],
) -> Job {
    let build = workflow::rouge(&format!("Package {target}"), "build", target);
    let mut job = ci
        .job(target, runner, 60, workflow::checkout())
        .permission("contents", "write");
    if let Some(setup) = setup {
        job = job.step(setup);
    }
    job.step(workflow::secrets(build, secrets))
        .step(workflow::inspect(target, "*"))
        .step(
            workflow::rouge("Publish download", "publish", target)
                .condition(PUBLISHING)
                .env("GH_TOKEN", "${{ github.token }}"),
        )
}

pub(crate) fn publish(build: &Build) -> Result<()> {
    let (sh, platform) = (&build.sh, build.target);
    let mut flags = Vec::new();
    let tag = if build.channel == Channel::Preview {
        let (Ok(run), Ok(attempt)) = (env::var("GITHUB_RUN_ID"), env::var("GITHUB_RUN_ATTEMPT"))
        else {
            return invalid("Preview releases are created by the GitHub workflow");
        };
        flags = vec!["--prerelease", "--latest=false"];
        format!("rouge-{platform}-preview-{run}-{attempt}")
    } else {
        format!("rouge-{platform}-v{}", build.app.version)
    };
    let head = cmd!(sh, "git rev-parse HEAD").read()?;
    let _out = sh.push_dir(&build.out);
    sh.remove_path("SHA256SUMS")?;
    let packages: Vec<_> = sh
        .read_dir(".")?
        .into_iter()
        .filter_map(|package| package.file_name().map(ToOwned::to_owned))
        .collect();
    ensure(
        !packages.is_empty(),
        format!("No packages to publish; run build {platform} first"),
    )?;
    let packages = &packages;
    let checksums = cmd!(sh, "shasum -a 256 -- {packages...}").read()?;
    sh.write_file("SHA256SUMS", format!("{checksums}\n"))?;
    let link = cmd!(
        sh,
        "gh release create {tag} --title {tag} --generate-notes --target {head} {flags...} -- {packages...} SHA256SUMS"
    )
    .read()?;
    build.summarize(&format!("[Download the {platform} app]({link})"))
}
