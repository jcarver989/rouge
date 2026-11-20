use crate::error::{Result, ensure, invalid};
use crate::packager::Build;
use crate::workflow::{self, Ci, PUBLISHING};
use crate::{Channel, Target};
use framer::github::{Job, Step};
use serde_json::json;
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

const RECEIPT: &str = ".release.json";

fn receipt(build: &Build, head: &str, checksum: &str) -> String {
    json!({
        "channel": build.channel.to_string(),
        "commit": head,
        "package": build.app.package,
        "version": build.app.version.to_string(),
        "build_number": build.number,
        "sha256sum": checksum,
    })
    .to_string()
}

pub(crate) fn record(build: &Build, package: &str) -> Result<()> {
    let sh = &build.sh;
    let head = cmd!(sh, "git rev-parse HEAD").read()?;
    let _out = sh.push_dir(&build.out);
    let checksum = cmd!(sh, "shasum -a 256 -- {package}").read()?;
    Ok(sh.write_file(RECEIPT, receipt(build, &head, &checksum))?)
}

pub(crate) fn publish(build: &Build, package: &str) -> Result<()> {
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
    let rebuild = || {
        format!(
            "Run cargo rouge build {platform} --channel {} for this release before publishing",
            build.channel
        )
    };
    let Ok(recorded) = sh.read_file(RECEIPT) else {
        return invalid(rebuild());
    };
    ensure(sh.path_exists(package), rebuild())?;
    let checksum = cmd!(sh, "shasum -a 256 -- {package}").read()?;
    ensure(recorded == receipt(build, &head, &checksum), rebuild())?;
    sh.write_file("SHA256SUMS", format!("{checksum}\n"))?;
    let link = cmd!(
        sh,
        "gh release create {tag} --title {tag} --generate-notes --target {head} {flags...} -- {package} SHA256SUMS"
    )
    .read()?;
    build.summarize(&format!("[Download the {platform} app]({link})"))
}
