#![cfg(unix)]

use rouge_framer::{
    Action, Channel, GitHubReleases, Packager, PublishError, RougePublish, Target, TestFlight,
};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use rouge_framer::{Device, Ios, Orientation};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use serde_json::{Value, json};
use std::env;
use std::error::Error;
use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::tempdir;
use xshell::{Shell, cmd};

type Result = std::result::Result<(), Box<dyn Error>>;

#[test]
fn desktop_publishes_only_the_recorded_release() -> Result {
    in_app("desktop_publishes_only_the_recorded_release", |sh| {
        let packager = Packager::new(RougePublish::new(APP_ID).desktop(GitHubReleases), ".");
        let release = packager.clone().channel(Channel::Production);
        cmd!(sh, "git init --quiet").run()?;
        commit(sh)?;
        tool(
            sh,
            "gh",
            "printf '%s\\n' \"$@\" > \"$ROUGE_TEST_APP/upload\"",
        )?;
        sh.write_file(
            ".rouge/out/macos/app-1.0.0-macos-arm64.dmg",
            "unsigned check artifact",
        )?;
        assert!(matches!(
            release.run(Action::Publish, Target::Macos),
            Err(PublishError::Invalid(_))
        ));

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            let target = Target::Linux;
            let archive = ".rouge/out/linux/app-1.0.0-linux-x86_64.tar.gz";
            packager.run(Action::Build, target)?;
            assert!(matches!(
                release.run(Action::Publish, target),
                Err(PublishError::Invalid(_))
            ));
            let preview = packager.channel(Channel::Preview);
            preview.run(Action::Build, target)?;
            assert!(matches!(
                release.run(Action::Publish, target),
                Err(PublishError::Invalid(_))
            ));
            release.run(Action::Build, target)?;
            sh.write_file(".rouge/out/linux/unrelated.txt", "not a release asset")?;
            release.run(Action::Publish, target)?;
            let uploaded = sh.read_file("upload")?;
            assert!(uploaded.ends_with("--\napp-1.0.0-linux-x86_64.tar.gz\nSHA256SUMS\n"));
            sh.remove_path("upload")?;
            fs::OpenOptions::new()
                .append(true)
                .open(archive)
                .and_then(|mut file| std::io::Write::write_all(&mut file, b"modified"))?;
            assert!(matches!(
                release.run(Action::Publish, target),
                Err(PublishError::Invalid(_))
            ));
            release.run(Action::Build, target)?;
            commit(sh)?;
            assert!(matches!(
                release.run(Action::Publish, target),
                Err(PublishError::Invalid(_))
            ));
        }
        assert!(!sh.path_exists("upload"));
        Ok(())
    })
}

#[test]
fn signing_tools_install_without_credentials_and_use_private_key_files() -> Result {
    in_app(
        "signing_tools_install_without_credentials_and_use_private_key_files",
        |sh| {
            let packager = Packager::new(
                RougePublish::new(APP_ID)
                    .icon("icon.png")
                    .ios(TestFlight::new("1234")),
                ".",
            )
            .channel(Channel::Preview);
            sh.write_file("icon.png", "unused during upload")?;
            tool(sh, "rcodesign", "echo 'rcodesign 0.0.0'")?;
            tool(
                sh,
                "cargo",
                r#"if [ "$1" != install ]; then exec "$REAL_CARGO" "$@"; fi
[ -z "${APPLE_API_KEY+x}${APPLE_CERTIFICATE_PASSWORD+x}${ANDROID_UPLOAD_STORE_PASSWORD+x}${GH_TOKEN+x}" ]
touch "$ROUGE_TEST_APP/installed""#,
            )?;
            packager.run(Action::Setup, Target::Ios)?;
            assert!(sh.path_exists("installed"));
            tool(
                sh,
                "xcrun",
                r"python3 - <<'PY'
import os, pathlib, stat
keys = pathlib.Path(os.environ['API_PRIVATE_KEYS_DIR'])
assert stat.S_IMODE(keys.parent.stat().st_mode) == 0o700
key = keys / 'AuthKey_TEST.p8'
assert stat.S_IMODE(key.stat().st_mode) == 0o600
assert key.read_text().strip() == 'test-only'
(pathlib.Path(os.environ['ROUGE_TEST_APP']) / 'scratch-path').write_text(str(keys.parent))
PY",
            )?;
            packager.run(Action::Publish, Target::Ios)?;
            assert!(!Path::new(&sh.read_file("scratch-path")?).exists());
            Ok(())
        },
    )
}

#[test]
fn rejects_local_runs_for_other_targets() {
    let packager = Packager::new(RougePublish::new(APP_ID).desktop(GitHubReleases), ".");
    assert!(matches!(
        packager.run(Action::Run, Target::Macos),
        Err(PublishError::Invalid(message)) if message.contains("only supported for iOS")
    ));
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn builds_and_relaunches_on_the_best_matching_simulator() -> Result {
    in_app(
        "builds_and_relaunches_on_the_best_matching_simulator",
        |sh| {
            let ios = Ios::new(TestFlight::new("0"))
                .minimum_os("17.2.1")
                .devices([Device::Phone])
                .orientations([Orientation::Portrait])
                .info("NSCameraUsageDescription", "Scan receipts")
                .info("CFBundleSupportedPlatforms", json!(["iPhoneOS"]))
                .info("DTSDKName", "iphoneos")
                .entitlement("aps-environment", "production");
            let app = RougePublish::new(APP_ID)
                .name("Fixture")
                .icon("icon.png")
                .ios(ios);
            let packager = Packager::new(app, ".")
                .channel(Channel::Production)
                .build_number(123);
            let archive = ".rouge/out/ios/unsigned-app.zip";
            let bundle = sh.current_dir().join(".rouge/out/ios-sim/app.app");
            sh.write_file(archive, "preserve device artifact")?;
            sh.write_file(bundle.join("embedded.mobileprovision"), "stale profile")?;
            sh.write_file("icon.png", "test icon")?;
            simulator_tools(sh)?;

            let older = simulator(1, "Shared name", "Booted", "iPhone", true);
            let newer = simulator(0xABC, "Shared name", "Shutdown", "iPhone", true);
            let pad = simulator(3, "Pad", "Booted", "iPad", true);
            sh.write_file("devices.json", json!({"devices": {
            "com.apple.CoreSimulator.SimRuntime.iOS-17-2": [simulator(4, "Old", "Booted", "iPhone", true)],
            "com.apple.CoreSimulator.SimRuntime.iOS-17-2-1": [older],
            "com.apple.CoreSimulator.SimRuntime.iOS-17-9": [simulator(5, "Middle", "Shutdown", "iPhone", true)],
            "com.apple.CoreSimulator.SimRuntime.iOS-17-10": [newer, pad, simulator(6, "Unavailable", "Booted", "iPhone", false)],
            "com.apple.CoreSimulator.SimRuntime.watchOS-99-0": [simulator(7, "Watch", "Booted", "iPhone", true)]
        }}).to_string())?;

            for (selector, message) in [
                ("Shared name", "use a UDID"),
                ("Pad", "No compatible iOS simulator"),
                ("Old", "17.2.1 or newer"),
            ] {
                assert!(matches!(
                    packager.clone().simulator_device(selector).run(Action::Run, Target::Ios),
                    Err(PublishError::Invalid(details)) if details.contains(message)
                ));
            }
            assert!(!sh.path_exists("commands"));
            packager.run(Action::Run, Target::Ios)?;
            assert_launch(sh, &bundle, 1)?;

            packager
                .clone()
                .simulator_device(udid(0xABC).to_ascii_lowercase())
                .run(Action::Run, Target::Ios)?;
            assert_launch(sh, &bundle, 0xABC)?;
            let commands = sh.read_file("commands")?;
            let target = "aarch64-apple-ios-sim";
            assert!(commands.contains(&format!("rustup target add {target}\n")));
            assert!(commands.contains(&format!(
                "cargo build --locked --package app --bin app --target {target}\n"
            )));
            assert!(commands.contains("--platform iphonesimulator --app-icon AppIcon --target-device iphone --minimum-deployment-target 17.2.1"));
            assert!(!bundle.join("embedded.mobileprovision").exists());
            assert_eq!(sh.read_file(archive)?, "preserve device artifact");
            let info: Value = plist::from_file(bundle.join("Info.plist"))?;
            for (key, value) in [
                ("CFBundleIdentifier", json!(APP_ID)),
                ("CFBundleDisplayName", json!("Fixture")),
                ("CFBundleExecutable", json!("app")),
                ("CFBundleVersion", json!("1")),
                ("CFBundleSupportedPlatforms", json!(["iPhoneSimulator"])),
                ("DTPlatformName", json!("iphonesimulator")),
                ("DTSDKName", json!("iphonesimulator26.2")),
                ("MinimumOSVersion", json!("17.2.1")),
                ("UIDeviceFamily", json!([1])),
                (
                    "UISupportedInterfaceOrientations",
                    json!(["UIInterfaceOrientationPortrait"]),
                ),
                ("CFBundleIconName", json!("AppIcon")),
                ("NSCameraUsageDescription", json!("Scan receipts")),
            ] {
                assert_eq!(info[key], value, "{key}");
            }

            sh.write_file("devices.json", json!({"devices": {"com.apple.CoreSimulator.SimRuntime.iOS-17-2": [simulator(4, "Old", "Booted", "iPhone", true)]}}).to_string())?;
            assert!(matches!(
                packager.run(Action::Run, Target::Ios),
                Err(PublishError::Invalid(message)) if message.contains("install an iOS 17.2.1 or newer runtime")
            ));
            assert_eq!(sh.read_file("commands")?, commands);

            let packager = Packager::new(
                RougePublish::new(APP_ID)
                    .icon("icon.png")
                    .ios(TestFlight::new("0")),
                ".",
            );
            sh.write_file("devices.json", json!({"devices": {
            "com.apple.CoreSimulator.SimRuntime.iOS-17-2": [simulator(1, "Old", "Shutdown", "iPhone", true)],
            "com.apple.CoreSimulator.SimRuntime.iOS-17-10": [
                simulator(2, "A Pad", "Shutdown", "iPad", true),
                simulator(3, "Phone", "Shutdown", "iPhone", true),
                simulator(4, "Other Phone", "Shutdown", "iPhone", true),
                simulator(5, "Other Phone", "Shutdown", "iPhone", true)
            ]
        }}).to_string())?;
            packager.run(Action::Run, Target::Ios)?;
            assert_launch(sh, &bundle, 4)?;
            packager
                .simulator_device("A Pad")
                .run(Action::Run, Target::Ios)?;
            assert_launch(sh, &bundle, 2)?;
            Ok(())
        },
    )
}

const APP_ID: &str = "com.example.app";

fn in_app(test: &str, run: impl FnOnce(&Shell) -> Result) -> Result {
    let sh = Shell::new()?;
    if env::var_os("ROUGE_TEST_APP").is_some() {
        return run(&sh);
    }
    let app = tempdir()?;
    let root = app.path();
    sh.change_dir(root);
    sh.write_file(
        "Cargo.toml",
        "[workspace]\n[package]\nname='app'\nversion='1.0.0'\nedition='2024'\n",
    )?;
    sh.write_file("src/main.rs", "fn main() {}")?;
    cmd!(sh, "cargo generate-lockfile --offline").run()?;
    let bin = sh.create_dir("tools")?;
    let path = env::join_paths([bin].into_iter().chain(env::split_paths(
        &env::var_os("PATH").ok_or("missing PATH")?,
    )))?;
    let exe = env::current_exe()?;
    cmd!(sh, "{exe} --exact {test} --nocapture")
        .env("ROUGE_TEST_APP", root)
        .env("PATH", path)
        .env("REAL_CARGO", env!("CARGO"))
        .env("CARGO_TARGET_DIR", root.join("target"))
        .env_remove("GITHUB_STEP_SUMMARY")
        .env("IOS_PROFILE_BASE64", "test")
        .env("APPLE_API_KEY", "test-only")
        .env("APPLE_KEY_ID", "TEST")
        .env("APPLE_ISSUER_ID", "test")
        .env("APPLE_CERTIFICATE_PASSWORD", "test")
        .env("ANDROID_UPLOAD_STORE_PASSWORD", "test")
        .env("GH_TOKEN", "test")
        .run()?;
    Ok(())
}

fn commit(sh: &Shell) -> Result {
    cmd!(sh, "git -c user.name=Test -c user.email=test@example.com commit --quiet --allow-empty -m fixture").run()?;
    Ok(())
}

fn tool(sh: &Shell, name: &str, script: &str) -> Result {
    let path = sh.current_dir().join("tools").join(name);
    sh.write_file(&path, format!("#!/bin/sh\nset -eu\n{script}\n"))?;
    fs::set_permissions(path, Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn udid(index: u32) -> String {
    format!("{index:08X}-0000-4000-8000-000000000000")
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn simulator(index: u32, name: &str, state: &str, family: &str, available: bool) -> Value {
    json!({
        "udid": udid(index), "name": name, "state": state, "isAvailable": available,
        "deviceTypeIdentifier": format!("com.apple.CoreSimulator.SimDeviceType.{family}-Test")
    })
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn assert_launch(sh: &Shell, bundle: &Path, index: u32) -> Result {
    let bundle = bundle.display();
    let udid = udid(index);
    let commands = sh.read_file("commands")?;
    assert!(commands.ends_with(&format!(
        "codesign --sign - {bundle}\nxcrun simctl bootstatus {udid} -b\nopen -a Simulator\nxcrun simctl install {udid} {bundle}\nxcrun simctl launch --terminate-running-process {udid} {APP_ID}\n"
    )), "{commands}");
    Ok(())
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn simulator_tools(sh: &Shell) -> Result {
    plist::to_file_xml(
        sh.current_dir().join("assets.plist"),
        &json!({"CFBundleIconName": "AppIcon"}),
    )?;
    let isolated = r#"[ -z "${APPLE_API_KEY+x}${APPLE_CERTIFICATE_PASSWORD+x}${IOS_PROFILE_BASE64+x}${GH_TOKEN+x}" ]"#;
    let log = r#"printf '%s %s\n' "${0##*/}" "$*" >> "$ROUGE_TEST_APP/commands""#;
    for (name, script) in [
        (
            "cargo",
            r#"if [ "$1" = metadata ]; then exec "$REAL_CARGO" "$@"; fi
[ "$SDKROOT" = /fixture/iphonesimulator ]
[ -n "$IPHONEOS_DEPLOYMENT_TARGET" ]
mkdir -p "$CARGO_TARGET_DIR/aarch64-apple-ios-sim/debug"
touch "$CARGO_TARGET_DIR/aarch64-apple-ios-sim/debug/app""#,
        ),
        (
            "xcrun",
            r#"case "$*" in
  '--find simctl') echo /fixture/simctl; exit ;;
  'simctl list devices available --json') cat "$ROUGE_TEST_APP/devices.json"; exit ;;
esac
case "$*" in
  '--sdk iphonesimulator --find actool') echo /fixture/actool ;;
  '--sdk iphonesimulator --show-sdk-path') echo /fixture/iphonesimulator ;;
  '--sdk iphonesimulator --show-sdk-version') echo 26.2 ;;
  '--sdk iphonesimulator --show-sdk-build-version') echo 23C53 ;;
  '--sdk iphonesimulator actool '*) for last do :; done; cp "$ROUGE_TEST_APP/assets.plist" "$last" ;;
  'simctl bootstatus '*|'simctl install '*|'simctl launch '*) ;;
  *) exit 1 ;;
esac"#,
        ),
        ("rustup", ""),
        (
            "sips",
            "printf '%s\n' 'pixelWidth: 1024' 'pixelHeight: 1024' 'hasAlpha: no'",
        ),
        (
            "xcodebuild",
            "printf '%s\n' 'Xcode 26.2' 'Build version 17C52'",
        ),
        (
            "codesign",
            r#"[ "$#" = 3 ] && [ "$1" = --sign ] && [ "$2" = - ] && [ -f "$3/app" ]"#,
        ),
        ("open", ""),
    ] {
        let skip_log = match name {
            "cargo" => r#"[ "$1" = metadata ]"#,
            "xcrun" => r#"[ "$1" = --find ] || [ "${2-}" = list ]"#,
            _ => "false",
        };
        tool(
            sh,
            name,
            &format!("{isolated}\nif ! ({skip_log}); then {log}; fi\n{script}"),
        )?;
    }
    Ok(())
}
