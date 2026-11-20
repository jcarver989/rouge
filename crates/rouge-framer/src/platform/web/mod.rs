mod pages;

use crate::error::{Result, ensure, invalid};
use crate::packager::Build;
use crate::platform::Platform;
use crate::workflow::{self, CHECKOUT, Ci, PUBLISHING, ci_options};
use crate::{Channel, RougePublish, Target};
use framer::github::{Job, Step};
use framer::yaml::Yaml;
use std::env;
use std::path::{Path, PathBuf};
use xshell::cmd;

const TRIPLE: &str = "wasm32-unknown-unknown";
const PAGE_URL: &str = "${{ steps.deployment.outputs.page_url }}";
const PAGES_ARTIFACT: &str =
    "actions/upload-pages-artifact@7b1f4a764d45c48632c6b24a0339c27f5614fb0b";
const PAGES_DEPLOY: &str = "actions/deploy-pages@d6db90164ac5ed86f2b6aed7e0febac5b3c0c03e";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GitHubPages;

#[derive(Clone, Debug)]
pub struct Web {
    ci: Ci,
}

impl Web {
    pub fn new(_destination: GitHubPages) -> Self {
        Self { ci: Ci::default() }
    }
}

impl From<GitHubPages> for Web {
    fn from(destination: GitHubPages) -> Self {
        Self::new(destination)
    }
}

ci_options!(Web);

impl Platform for Web {
    fn target(&self) -> Target {
        Target::Web
    }

    fn job(&self, _config: &RougePublish) -> Job {
        let target = self.target();
        let checkout = Step::uses(CHECKOUT).with("persist-credentials", PUBLISHING);
        self.ci
            .job(target, "ubuntu-24.04", 45, checkout)
            .permission("contents", "write")
            .permission("pages", "write")
            .permission("id-token", "write")
            .step(
                workflow::rouge("Check Pages setup", "doctor", target)
                    .condition(PUBLISHING)
                    .env("GH_TOKEN", "${{ github.token }}"),
            )
            .step(workflow::rouge("Build web package", "build", target))
            .step(workflow::inspect(target, "*"))
            .step(workflow::rouge("Stage Pages channel", "stage", target).condition(PUBLISHING))
            .step(
                Step::uses(PAGES_ARTIFACT)
                    .name("Stage Pages deployment")
                    .condition(PUBLISHING)
                    .with("path", ".rouge/out/pages"),
            )
            .step(
                Step::uses(PAGES_DEPLOY)
                    .name("Deploy Pages")
                    .id("deployment")
                    .condition(PUBLISHING),
            )
            .step(
                workflow::rouge("Record deployed Pages channel", "publish", target)
                    .condition(PUBLISHING),
            )
            .step(
                workflow::rouge("Print website link", "link", target)
                    .condition(PUBLISHING)
                    .env("SITE_URL", PAGE_URL),
            )
    }

    fn environment(&self) -> Yaml {
        Yaml::map()
            .with(
                "name",
                "${{ inputs.channel == 'check' && 'rouge-package-check' || 'github-pages' }}",
            )
            .with("url", PAGE_URL)
    }

    fn guide(&self) -> &'static str {
        include_str!("guide.md")
    }

    fn doctor(&self, build: &Build) -> Result<()> {
        if !build.publishing() {
            return Ok(());
        }
        let sh = &build.sh;
        let repository = env::var("GITHUB_REPOSITORY").unwrap_or_else(|_| "{owner}/{repo}".into());
        let source = cmd!(sh, "gh api repos/{repository}/pages --jq .build_type")
            .ignore_stderr()
            .read();
        ensure(
            source.is_ok_and(|source| source == "workflow"),
            "Set Settings → Pages → Build and deployment → Source to GitHub Actions.",
        )
    }

    fn build(&self, build: &Build) -> Result<()> {
        let bindgen = resolve_bindgen(build)?;
        let wasm = compile_wasm(build)?;
        generate_bindings(build, &bindgen, &wasm)?;
        write_site(build)
    }

    fn stage(&self, build: &Build) -> Result<()> {
        pages::stage(build)
    }

    fn publish(&self, build: &Build) -> Result<()> {
        pages::publish(build)
    }

    fn link(&self, build: &Build) -> Result<()> {
        let Ok(site) = env::var("SITE_URL") else {
            return invalid("Set SITE_URL to the deployed Pages URL");
        };
        let site = site.trim_end_matches('/');
        let path = if build.channel == Channel::Preview {
            "preview/"
        } else {
            ""
        };
        build.summarize(&format!("[Open the web app]({site}/{path})"))
    }
}

fn resolve_bindgen(build: &Build) -> Result<PathBuf> {
    let (sh, app) = (&build.sh, &build.app);
    let lock = sh.read_file(&app.lock)?;
    let lines: Vec<&str> = lock.lines().collect();
    let mut versions = lines
        .windows(2)
        .filter(|pair| pair[0] == "name = \"wasm-bindgen\"")
        .filter_map(|pair| pair[1].strip_prefix("version = \"")?.strip_suffix('"'));
    let (Some(version), None) = (versions.next(), versions.next()) else {
        return invalid("Expected exactly one wasm-bindgen version in Cargo.lock");
    };
    build.tool("wasm-bindgen", "wasm-bindgen-cli", version)
}

fn compile_wasm(build: &Build) -> Result<PathBuf> {
    let sh = &build.sh;
    build
        .isolated(cmd!(sh, "rustup target add {TRIPLE}"))
        .run()?;
    Ok(build
        .compile(TRIPLE)?
        .join(format!("{}.wasm", build.app.target)))
}

fn generate_bindings(build: &Build, bindgen: &Path, wasm: &Path) -> Result<()> {
    let (sh, out) = (&build.sh, &build.out);
    cmd!(
        sh,
        "{bindgen} --target web --no-typescript --out-name app --out-dir {out} {wasm}"
    )
    .run()?;
    Ok(())
}

fn write_site(build: &Build) -> Result<()> {
    let (sh, app, out) = (&build.sh, &build.app, &build.out);
    let name = app
        .name
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;");
    let mut link = "";
    if let Some(icon) = &app.icon {
        build.copy(icon, out.join("icon.png"))?;
        link = "\n  <link rel=\"icon\" href=\"./icon.png\">";
    }
    let page = include_str!("index.html")
        .replace("@@NAME@@", &name)
        .replace("@@ICON@@", link);
    Ok(sh.write_file(out.join("index.html"), page)?)
}
