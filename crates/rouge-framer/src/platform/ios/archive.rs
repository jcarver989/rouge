use super::{DEVICE, xcodebuild};
use crate::error::{Result, ensure};
use crate::packager::{Build, need};
use crate::platform::{apple, os_version};
use crate::secrets::{self, secret};
use serde_json::json;
use std::path::{Path, PathBuf};
use xshell::cmd;

pub(super) const TEAM_ID: &str = "APPLE_TEAM_ID";
const APP: &str = "RougeApp.app";
const ARCHIVE: &str = "app.xcarchive";

pub(super) fn check_xcode(build: &Build) -> Result<()> {
    ensure(
        cfg!(target_os = "macos"),
        "Use macOS with full Xcode selected",
    )?;
    need("xcrun")?;
    let sh = &build.sh;
    let version = build.isolated(cmd!(sh, "xcodebuild -version")).read()?;
    let sdk = build
        .isolated(cmd!(sh, "xcrun --sdk iphoneos --show-sdk-version"))
        .read()?;
    let xcode = version.split_whitespace().nth(1).and_then(os_version);
    let sdk = os_version(&sdk).map_or(0, |(major, _)| major);
    ensure(
        xcode.is_some_and(|(major, _)| major >= 26) && sdk >= 26,
        "iOS device archives require full Xcode 26 or later and an iOS 26 or later SDK; select it with DEVELOPER_DIR or xcode-select",
    )
}

pub(super) fn build(build: &Build) -> Result<()> {
    let sh = &build.sh;
    let scratch = secrets::scratch()?;
    let root = scratch.path();
    let archive = build.out.join(ARCHIVE);
    build
        .isolated(cmd!(sh, "rustup target add {DEVICE}"))
        .run()?;
    let xcodebuild = xcodebuild(build, &root.join("DerivedData"))?
        .args(["-configuration", "Release"])
        .args(["-destination", "generic/platform=iOS", "-archivePath"])
        .arg(&archive);
    if build.publishing() {
        let team = secret(TEAM_ID)?;
        let key = apple::api_key(build, root)?;
        xcodebuild
            .arg("CODE_SIGN_STYLE=Automatic")
            .arg(format!("DEVELOPMENT_TEAM={team}"))
            .args(key.xcode_auth())
            .arg("archive")
            .run()?;
    } else {
        xcodebuild
            .args(["CODE_SIGNING_ALLOWED=NO", "archive"])
            .run()?;
    }
    let bundle = attach_symbols(build, &archive)?;
    let symbols = archive.join("dSYMs");
    let zip = build.out.join("dSYMs.zip");
    build
        .isolated(cmd!(sh, "ditto -c -k --keepParent {symbols} {zip}"))
        .run()?;
    if build.publishing() {
        build
            .isolated(cmd!(sh, "codesign --verify --strict {bundle}"))
            .run()?;
    } else {
        let zip = build.out.join("unsigned-app.zip");
        build
            .isolated(cmd!(sh, "ditto -c -k --keepParent {bundle} {zip}"))
            .run()?;
    }
    Ok(())
}

pub(super) fn upload(build: &Build) -> Result<()> {
    let sh = &build.sh;
    let archive = build.out.join(ARCHIVE);
    ensure(
        archive.is_dir(),
        "Build a signed archive with `cargo rouge build ios --channel preview` before uploading",
    )?;
    let scratch = secrets::scratch()?;
    let root = scratch.path();
    let key = apple::api_key(build, root)?;
    let options = root.join("ExportOptions.plist");
    let destination = root.join("Exported");
    plist::to_file_xml(
        &options,
        &json!({
            "method": "app-store-connect",
            "destination": "upload",
            "manageAppVersionAndBuildNumber": false,
            "signingStyle": "automatic",
            "teamID": secret(TEAM_ID)?,
        }),
    )?;
    let auth = key.xcode_auth();
    build.isolated(cmd!(sh, "xcodebuild -exportArchive -archivePath {archive} -exportPath {destination} -exportOptionsPlist {options} {auth...}")).run()?;
    Ok(())
}

fn attach_symbols(build: &Build, archive: &Path) -> Result<PathBuf> {
    let sh = &build.sh;
    let bundle = archive.join("Products/Applications").join(APP);
    let executable = bundle.join(&build.app.target);
    let symbols = build
        .cargo_target
        .join(DEVICE)
        .join("release")
        .join(format!("{}.dSYM", build.app.target));
    let archived_symbols = archive.join("dSYMs").join(format!("{APP}.dSYM"));
    ensure(symbols.is_dir(), "Cargo did not generate a release dSYM")?;
    build
        .isolated(cmd!(sh, "ditto {symbols} {archived_symbols}"))
        .run()?;
    let binary_uuid = build
        .isolated(cmd!(sh, "xcrun dwarfdump --uuid {executable}"))
        .read()?;
    let symbols_uuid = build
        .isolated(cmd!(sh, "xcrun dwarfdump --uuid {archived_symbols}"))
        .read()?;
    let binary_uuid = uuid(&binary_uuid);
    ensure(
        binary_uuid.is_some() && binary_uuid == uuid(&symbols_uuid),
        "The archive dSYM does not match the Rust executable",
    )?;
    Ok(bundle)
}

fn uuid(output: &str) -> Option<&str> {
    output.strip_prefix("UUID: ")?.split_whitespace().next()
}
