use crate::error::Result;
use crate::packager::Build;
use crate::platform::apple::{Bundle, bundle_options};
use crate::platform::{GitHubReleases, Platform, apple, desktop, os_version};
use crate::secrets::{self, APPLE};
use crate::workflow::{self, Ci, PUBLISHING, ci_options};
use crate::{RougePublish, Target};
use framer::github::Job;
use serde_json::json;
use std::path::Path;
use xshell::cmd;

const TRIPLE: &str = "aarch64-apple-darwin";

#[derive(Clone, Debug)]
pub struct Macos {
    minimum_os: Option<String>,
    category: Option<String>,
    bundle: Bundle,
    ci: Ci,
}

impl Macos {
    pub fn new(_destination: GitHubReleases) -> Self {
        Self {
            minimum_os: None,
            category: None,
            bundle: Bundle::default(),
            ci: Ci::default(),
        }
    }

    pub fn minimum_os(mut self, version: impl Into<String>) -> Self {
        self.minimum_os = Some(version.into());
        self
    }

    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }
}

impl From<GitHubReleases> for Macos {
    fn from(destination: GitHubReleases) -> Self {
        Self::new(destination)
    }
}

bundle_options!(Macos);
ci_options!(Macos);

impl Platform for Macos {
    fn target(&self) -> Target {
        Target::Macos
    }

    fn job(&self, _config: &RougePublish) -> Job {
        desktop::job(
            self.target(),
            &self.ci,
            "macos-26",
            Some(
                workflow::rouge("Install signing tool", "setup", self.target())
                    .condition(PUBLISHING),
            ),
            APPLE,
        )
    }

    fn guide(&self) -> &'static str {
        include_str!("guide.md")
    }

    fn validate(&self, _config: &RougePublish) -> Result<(), String> {
        self.bundle.toolchain.validate()?;
        match &self.minimum_os {
            Some(version) if os_version(version).is_none() => Err(format!(
                "`{version}` is not a macOS version; use one such as 13.0"
            )),
            _ => Ok(()),
        }
    }

    fn setup(&self, build: &Build) -> Result<()> {
        self.bundle.toolchain.signer(build).map(drop)
    }

    fn doctor(&self, build: &Build) -> Result<()> {
        apple::doctor(build)
    }

    fn build(&self, build: &Build) -> Result<()> {
        self.doctor(build)?;
        let scratch = secrets::scratch()?;
        let bundle = scratch.path().join(format!("{}.app", build.app.target));
        let contents = bundle.join("Contents");

        self.compile_bundle_binary(build, &contents)?;
        self.write_bundle_info(build, scratch.path(), &contents)?;
        let signer = build
            .publishing()
            .then(|| self.bundle.toolchain.signer(build))
            .transpose()?;
        if let Some(tool) = &signer {
            self.sign_bundle(build, scratch.path(), tool, &bundle)?;
        }
        let package = package_name(build);
        let image = build.out.join(&package);
        create_disk_image(build, &bundle, &image)?;
        if let Some(tool) = &signer {
            sign_and_notarize_image(build, scratch.path(), tool, &image)?;
            desktop::record(build, &package)?;
        }
        Ok(())
    }

    fn publish(&self, build: &Build) -> Result<()> {
        desktop::publish(build, &package_name(build))
    }
}

impl Macos {
    fn compile_bundle_binary(&self, build: &Build, contents: &Path) -> Result<()> {
        let sh = &build.sh;
        let binary = &build.app.target;
        build
            .isolated(cmd!(sh, "rustup target add {TRIPLE}"))
            .run()?;
        let compiled = {
            let _minimum = self
                .minimum_os
                .as_ref()
                .map(|version| sh.push_env("MACOSX_DEPLOYMENT_TARGET", version));
            build.compile(TRIPLE, true)?
        };
        build.copy(compiled.join(binary), contents.join("MacOS").join(binary))
    }

    fn write_bundle_info(&self, build: &Build, scratch: &Path, contents: &Path) -> Result<()> {
        let mut info = apple::info(build);
        if let Some(version) = &self.minimum_os {
            info["LSMinimumSystemVersion"] = version.as_str().into();
        }
        if let Some(category) = &self.category {
            info["LSApplicationCategoryType"] = category.as_str().into();
        }
        apple::extend(&mut info, &self.bundle.info);
        if let Some(source) = &build.app.icon {
            compile_icon(build, scratch, contents, source)?;
            info["CFBundleIconFile"] = "AppIcon.icns".into();
        }
        plist::to_file_xml(contents.join("Info.plist"), &info)?;
        Ok(())
    }

    fn sign_bundle(
        &self,
        build: &Build,
        scratch: &Path,
        rcodesign: &Path,
        bundle: &Path,
    ) -> Result<()> {
        let entitlements = scratch.join("entitlements.plist");
        let mut flags = vec![Path::new("--for-notarization")];
        if !self.bundle.entitlements.is_empty() {
            let mut granted = json!({});
            apple::extend(&mut granted, &self.bundle.entitlements);
            plist::to_file_xml(&entitlements, &granted)?;
            flags.extend([Path::new("--entitlements-xml-file"), &entitlements]);
        }
        apple::sign(build, scratch, rcodesign, &flags, bundle)
    }
}

fn sign_and_notarize_image(
    build: &Build,
    scratch: &Path,
    rcodesign: &Path,
    image: &Path,
) -> Result<()> {
    let sh = &build.sh;
    apple::sign(build, scratch, rcodesign, &[], image)?;
    let key = apple::api_key(build, scratch)?;
    let (issuer, id, file) = (&key.issuer, &key.id, &key.file);
    let encoded = scratch.join("notary.json");
    let json = cmd!(
        sh,
        "{rcodesign} encode-app-store-connect-api-key {issuer} {id} {file}"
    )
    .read()?;
    secrets::write(&encoded, json)?;
    cmd!(
        sh,
        "{rcodesign} notary-submit --api-key-file {encoded} --staple {image}"
    )
    .run()?;
    cmd!(sh, "xcrun stapler validate {image}").run()?;
    Ok(())
}

fn compile_icon(build: &Build, scratch: &Path, contents: &Path, source: &Path) -> Result<()> {
    let sh = &build.sh;
    let iconset = sh.create_dir(scratch.join("AppIcon.iconset"))?;
    let resources = sh.create_dir(contents.join("Resources"))?;
    for size in [16_u32, 32, 128, 256, 512] {
        for (pixels, suffix) in [(size, ""), (size * 2, "@2x")] {
            let pixels = pixels.to_string();
            let name = format!("icon_{size}x{size}{suffix}.png");
            cmd!(
                sh,
                "sips -z {pixels} {pixels} {source} --out {iconset}/{name}"
            )
            .ignore_stdout()
            .run()?;
        }
    }
    cmd!(sh, "iconutil -c icns -o {resources}/AppIcon.icns {iconset}").run()?;
    Ok(())
}

fn package_name(build: &Build) -> String {
    format!("{}-{}-macos-arm64.dmg", build.app.target, build.app.version)
}

fn create_disk_image(build: &Build, bundle: &Path, image: &Path) -> Result<()> {
    let (sh, app) = (&build.sh, &build.app);
    let binary = &app.target;
    cmd!(
        sh,
        "hdiutil create -srcfolder {bundle} -volname {binary} -format UDZO {image}"
    )
    .run()?;
    Ok(())
}
