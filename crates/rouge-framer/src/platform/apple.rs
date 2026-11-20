use crate::error::{Result, ensure};
use crate::packager::{Build, need};
use crate::platform::pinned;
use crate::secrets::{self, APPLE, APPLE_API, secret};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use xshell::cmd;

#[derive(Clone, Debug)]
pub struct AppleToolchain {
    pub(crate) rcodesign: String,
    pub(crate) fastlane: String,
    pub(crate) ruby: String,
}

impl Default for AppleToolchain {
    fn default() -> Self {
        Self::new()
    }
}

impl AppleToolchain {
    pub fn new() -> Self {
        Self {
            rcodesign: "0.29.0".into(),
            fastlane: "2.232.2".into(),
            ruby: "3.4".into(),
        }
    }

    pub fn rcodesign(mut self, version: impl Into<String>) -> Self {
        self.rcodesign = version.into();
        self
    }

    pub fn fastlane(mut self, version: impl Into<String>) -> Self {
        self.fastlane = version.into();
        self
    }

    pub fn ruby(mut self, version: impl Into<String>) -> Self {
        self.ruby = version.into();
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        match [&self.rcodesign, &self.fastlane, &self.ruby]
            .into_iter()
            .find(|version| !pinned(version))
        {
            Some(version) => Err(format!("`{version}` is not a tool version")),
            None => Ok(()),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Bundle {
    pub(crate) info: Vec<(String, Value)>,
    pub(crate) entitlements: Vec<(String, Value)>,
    pub(crate) toolchain: AppleToolchain,
}

macro_rules! bundle_options {
    ($platform:ty) => {
        impl $platform {
            pub fn info(
                mut self,
                key: impl Into<String>,
                value: impl Into<serde_json::Value>,
            ) -> Self {
                self.bundle.info.push((key.into(), value.into()));
                self
            }

            pub fn entitlement(
                mut self,
                key: impl Into<String>,
                value: impl Into<serde_json::Value>,
            ) -> Self {
                self.bundle.entitlements.push((key.into(), value.into()));
                self
            }

            pub fn toolchain(mut self, toolchain: crate::AppleToolchain) -> Self {
                self.bundle.toolchain = toolchain;
                self
            }
        }
    };
}
pub(crate) use bundle_options;

pub(crate) fn extend(values: &mut Value, entries: &[(String, Value)]) {
    for (key, value) in entries {
        values[key.as_str()] = value.clone();
    }
}

pub(crate) struct ApiKey {
    pub(crate) directory: PathBuf,
    pub(crate) file: PathBuf,
    pub(crate) id: String,
    pub(crate) issuer: String,
    pub(crate) contents: String,
}

pub(crate) fn doctor(build: &Build) -> Result<()> {
    ensure(cfg!(target_os = "macos"), "Use macOS with Xcode selected")?;
    need("xcrun")?;
    if build.publishing() {
        secrets::require(APPLE)?;
    }
    Ok(())
}

pub(crate) fn info(build: &Build) -> Value {
    let (app, version) = (&build.app, &build.app.version);
    json!({
        "CFBundleIdentifier": app.id,
        "CFBundleDisplayName": app.name,
        "CFBundleName": app.name,
        "CFBundleExecutable": app.target,
        "CFBundlePackageType": "APPL",
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleShortVersionString": format!("{}.{}.{}", version.major, version.minor, version.patch),
        "CFBundleVersion": build.number.to_string(),
    })
}

pub(crate) fn sign(
    build: &Build,
    scratch: &Path,
    version: &str,
    flags: &[&Path],
    path: &Path,
) -> Result<PathBuf> {
    let sh = &build.sh;
    let rcodesign = build.tool("rcodesign", "apple-codesign", version)?;
    let certificate = scratch.join("certificate.p12");
    let password = scratch.join("certificate.password");
    sh.write_file(&certificate, secrets::decoded(APPLE[0])?)?;
    sh.write_file(&password, secret(APPLE[1])?)?;
    cmd!(
        sh,
        "{rcodesign} sign --p12-file {certificate} --p12-password-file {password} {flags...} {path}"
    )
    .run()?;
    cmd!(sh, "codesign --verify --strict {path}").run()?;
    Ok(rcodesign)
}

pub(crate) fn api_key(build: &Build, scratch: &Path) -> Result<ApiKey> {
    secrets::require(APPLE_API)?;
    let id = secret("APPLE_KEY_ID")?;
    ensure(
        id.bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
        "Invalid APPLE_KEY_ID",
    )?;
    let directory = scratch.join("private_keys");
    let file = directory.join(format!("AuthKey_{id}.p8"));
    let contents = secret("APPLE_API_KEY")?;
    build.sh.write_file(&file, format!("{contents}\n"))?;
    Ok(ApiKey {
        directory,
        file,
        id,
        issuer: secret("APPLE_ISSUER_ID")?,
        contents,
    })
}
