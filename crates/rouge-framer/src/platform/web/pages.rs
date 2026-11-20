use crate::Channel;
use crate::error::Result;
use crate::packager::Build;
use std::path::{Path, PathBuf};
use xshell::{Shell, cmd};

const STATE: &str = "refs/heads/rouge-pages-state";
const AUTHOR: &str = "user.name=github-actions[bot]";
const EMAIL: &str = "user.email=41898282+github-actions[bot]@users.noreply.github.com";
const PREVIEW_INDEX: &str =
    "<!doctype html><meta charset=\"utf-8\"><a href=\"./preview/\">Open preview</a>\n";

fn candidate(build: &Build) -> String {
    format!("refs/rouge/pages-staged-{}", build.channel)
}

pub(crate) fn publish(build: &Build) -> Result<()> {
    let (sh, staged) = (&build.sh, candidate(build));
    cmd!(sh, "git push --quiet origin {staged}:{STATE}").run()?;
    Ok(cmd!(sh, "git update-ref -d {staged}").run()?)
}

pub(crate) fn stage(build: &Build) -> Result<()> {
    let sh = &build.sh;
    let staged = candidate(build);
    cmd!(sh, "git update-ref -d {staged}").run()?;
    let scratch = sh.create_temp_dir()?;
    let site = scratch.path().join("site");
    let tracked = !cmd!(sh, "git ls-remote --refs origin {STATE}")
        .read()?
        .is_empty();
    if tracked {
        cmd!(sh, "git fetch --quiet --no-tags --depth=1 origin {STATE}").run()?;
        cmd!(sh, "git worktree add --quiet --detach {site} FETCH_HEAD").run()?;
    } else {
        cmd!(sh, "git init --quiet {site}").run()?;
    }
    let result = update(build, &site, tracked, &staged);
    if tracked {
        cmd!(sh, "git worktree remove --force {site}").run()?;
    }
    result
}

fn update(build: &Build, site: &Path, tracked: bool, candidate: &str) -> Result<()> {
    let (sh, web) = (&build.sh, &build.out);
    let staged = build.root.join(".rouge/out/pages");
    if build.channel == Channel::Preview {
        let preview = site.join("preview");
        sh.remove_path(&preview)?;
        sh.create_dir(&preview)?;
        cmd!(sh, "cp -R {web}/. {preview}").run()?;
        if !site.join("index.html").exists() {
            sh.write_file(site.join("index.html"), PREVIEW_INDEX)?;
        }
    } else {
        for entry in published(sh, site, &[".git", "preview"])? {
            sh.remove_path(entry)?;
        }
        cmd!(sh, "cp -R {web}/. {site}").run()?;
    }
    sh.remove_path(&staged)?;
    sh.create_dir(&staged)?;
    let entries = published(sh, site, &[".git"])?;
    cmd!(sh, "cp -R {entries...} {staged}").run()?;
    let head = cmd!(sh, "git rev-parse HEAD").read()?;
    let message = format!("Rouge Pages {} {head}", build.channel);
    cmd!(sh, "git -C {site} add --all").run()?;
    cmd!(
        sh,
        "git -C {site} -c {AUTHOR} -c {EMAIL} commit --quiet --allow-empty -m {message}"
    )
    .run()?;
    let commit = if tracked {
        cmd!(sh, "git -C {site} rev-parse HEAD").read()?
    } else {
        cmd!(sh, "git fetch --quiet --no-tags {site} HEAD").run()?;
        "FETCH_HEAD".into()
    };
    Ok(cmd!(sh, "git update-ref {candidate} {commit}").run()?)
}

fn published(sh: &Shell, site: &Path, skipped: &[&str]) -> Result<Vec<PathBuf>> {
    let mut entries = sh.read_dir(site)?;
    entries.retain(|entry| {
        entry
            .file_name()
            .is_none_or(|name| !skipped.iter().any(|skip| name == *skip))
    });
    Ok(entries)
}
