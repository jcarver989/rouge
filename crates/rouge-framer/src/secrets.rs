use crate::error::{PublishError, Result, invalid};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
#[cfg(unix)]
use std::{
    fs::Permissions,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
};
use tempfile::{Builder, TempDir};

pub(crate) const APPLE_API: &[&str] = &["APPLE_API_KEY", "APPLE_KEY_ID", "APPLE_ISSUER_ID"];
pub(crate) const APPLE: &[&str] = &[
    "APPLE_CERTIFICATE_BASE64",
    "APPLE_CERTIFICATE_PASSWORD",
    "APPLE_API_KEY",
    "APPLE_KEY_ID",
    "APPLE_ISSUER_ID",
];
pub(crate) const IOS_PROFILE: &str = "IOS_PROFILE_BASE64";
pub(crate) const ANDROID: &[&str] = &[
    "ANDROID_UPLOAD_KEYSTORE_BASE64",
    "ANDROID_UPLOAD_STORE_PASSWORD",
    "ANDROID_UPLOAD_KEY_ALIAS",
    "ANDROID_UPLOAD_KEY_PASSWORD",
];
pub(crate) const PLAY_ACCOUNT: &str = "PLAY_SERVICE_ACCOUNT_JSON";

pub(crate) fn all() -> impl Iterator<Item = &'static str> {
    APPLE
        .iter()
        .chain(ANDROID)
        .chain([&IOS_PROFILE, &PLAY_ACCOUNT])
        .copied()
        .chain(["GH_TOKEN", "GITHUB_TOKEN"])
}

pub(crate) fn secret(name: &str) -> Result<String> {
    match env::var(name) {
        Ok(value) if !value.is_empty() => Ok(value),
        _ => invalid(format!(
            "Missing {name}; see PUBLISHING.md for where to set it"
        )),
    }
}

pub(crate) fn require(names: &[&str]) -> Result<()> {
    names.iter().try_for_each(|name| secret(name).map(drop))
}

pub(crate) fn decoded(name: &str) -> Result<Vec<u8>> {
    let compact: String = secret(name)?.split_ascii_whitespace().collect();
    STANDARD
        .decode(compact)
        .map_err(|source| PublishError::Base64 {
            name: name.to_owned(),
            source,
        })
}

pub(crate) fn scratch() -> Result<TempDir> {
    let mut builder = Builder::new();
    builder.prefix("rouge-sign-");
    #[cfg(unix)]
    builder.permissions(Permissions::from_mode(0o700));
    Ok(builder.tempdir()?)
}

pub(crate) fn write(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    Ok(options.open(path)?.write_all(contents.as_ref())?)
}
