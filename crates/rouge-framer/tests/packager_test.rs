#![cfg(unix)]

use framer::{DiskFs, Mode, Project};
use rouge_framer::{
    Action, Channel, GitHubPages, GitHubReleases, Packager, PlayInternal, PublishError,
    RougePublish, Target, TestFlight, Web,
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
            #[cfg(target_os = "macos")]
            {
                tool(sh, "xcodebuild", "echo Xcode 26.2")?;
                tool(sh, "xcrun", "echo 26.2")?;
                tool(
                    sh,
                    "rustup",
                    r#"[ "$*" = 'target add aarch64-apple-ios' ]
[ -z "${APPLE_API_KEY+x}${IOS_PROFILE_BASE64+x}${ANDROID_UPLOAD_STORE_PASSWORD+x}${GH_TOKEN+x}" ]
touch "$ROUGE_TEST_APP/installed""#,
                )?;
                packager.run(Action::Setup, Target::Ios)?;
                assert!(sh.path_exists("installed"));
            }
            tool(
                sh,
                "xcodebuild",
                r#"python3 - "$@" <<'PY'
import pathlib, plistlib, stat, sys
args = sys.argv[1:]
assert args[0] == '-exportArchive'
assert pathlib.Path(args[args.index('-archivePath') + 1]).name == 'app.xcarchive'
assert args[args.index('-authenticationKeyID') + 1] == 'TEST'
options = plistlib.loads(pathlib.Path(args[args.index('-exportOptionsPlist') + 1]).read_bytes())
assert options['destination'] == 'upload' and options['teamID'] == 'TEAM123456'
key = pathlib.Path(args[args.index('-authenticationKeyPath') + 1])
scratch = key.parent.parent
assert stat.S_IMODE(scratch.stat().st_mode) == 0o700
assert stat.S_IMODE(key.stat().st_mode) == 0o600
assert key.read_text().strip() == 'test-only'
pathlib.Path('scratch-path').write_text(str(scratch))
PY"#,
            )?;
            assert!(matches!(
                packager.run(Action::Publish, Target::Ios),
                Err(PublishError::Invalid(message)) if message.contains("signed archive")
            ));
            assert!(!sh.path_exists("scratch-path"));
            sh.create_dir(".rouge/out/ios/app.xcarchive")?;
            assert!(matches!(
                packager.clone().channel(Channel::Production).run(Action::Publish, Target::Ios),
                Err(PublishError::Invalid(message)) if message.contains("App Store Connect")
            ));
            assert!(!sh.path_exists("scratch-path"));
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
                .info("NSCameraUsageDescription", "Scan receipts");
            let app = RougePublish::new(APP_ID)
                .name("Fixture")
                .icon("icon.png")
                .ios(ios);
            let packager = Packager::new(app.clone(), ".")
                .channel(Channel::Production)
                .build_number(123);
            let archive = ".rouge/out/ios/unsigned-app.zip";
            let root = sh.current_dir();
            let derived = root.join(".rouge/out/ios-sim/DerivedData");
            let bundle = derived.join("Build/Products/Debug-iphonesimulator/RougeApp.app");
            sh.write_file(archive, "preserve device artifact")?;
            sh.write_file("icon.png", "test icon")?;
            generate(sh, app)?;
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
                    packager.clone().device(selector).run(Action::Run, Target::Ios),
                    Err(PublishError::Invalid(details)) if details.contains(message)
                ));
            }
            assert!(!sh.path_exists("commands"));
            packager.run(Action::Run, Target::Ios)?;
            assert_launch(sh, &bundle, 1)?;

            packager
                .clone()
                .device(udid(0xABC).to_ascii_lowercase())
                .run(Action::Run, Target::Ios)?;
            assert_launch(sh, &bundle, 0xABC)?;
            let commands = sh.read_file("commands")?;
            assert!(commands.contains("rustup target add aarch64-apple-ios-sim\n"));
            assert!(commands.contains(&format!(
                "xcodebuild -quiet -project {} -scheme RougeApp -derivedDataPath {} EXECUTABLE_NAME=app ROUGE_DISPLAY_NAME=Fixture MARKETING_VERSION=1.0.0 CURRENT_PROJECT_VERSION=1 CARGO_TARGET_DIR={} -configuration Debug -destination id={} build\n",
                root.join("ios/RougeApp.xcodeproj").display(),
                derived.display(),
                env::var("CARGO_TARGET_DIR")?,
                udid(0xABC),
            )), "{commands}");
            assert_eq!(sh.read_file(archive)?, "preserve device artifact");

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
            packager.device("A Pad").run(Action::Run, Target::Ios)?;
            assert_launch(sh, &bundle, 2)?;
            Ok(())
        },
    )
}

#[test]
fn android_doctor_requires_the_configured_compile_sdk() -> Result {
    in_app("android_doctor_requires_the_configured_compile_sdk", |sh| {
        sh.create_dir("sdk/ndk/28.2.13676358")?;
        sh.create_dir("sdk/platforms/android-37")?;
        tool(sh, "java", "exit 1")?;
        let packager = Packager::new(RougePublish::new(APP_ID).android(PlayInternal), ".");
        assert!(
            matches!(packager.run(Action::Doctor, Target::Android), Err(PublishError::Invalid(message)) if message.contains("platform android-37.0"))
        );
        Ok(())
    })
}

#[test]
fn android_accepts_only_aligned_libraries_in_bundles_signed_with_the_build_key() -> Result {
    in_app(
        "android_accepts_only_aligned_libraries_in_bundles_signed_with_the_build_key",
        |sh| {
            if cmd!(sh, "java -version")
                .quiet()
                .ignore_stderr()
                .run()
                .is_err()
                || cmd!(sh, "javac -version")
                    .quiet()
                    .ignore_stdout()
                    .ignore_stderr()
                    .run()
                    .is_err()
            {
                return Ok(());
            }
            for path in ["platforms/android-37.0", "build-tools/36.0.0"] {
                sh.create_dir(format!("sdk/{path}"))?;
            }
            let readelf = sh
                .current_dir()
                .join("sdk/ndk/28.2.13676358/toolchains/llvm/prebuilt/host/bin/llvm-readelf");
            tool(
                sh,
                readelf.to_str().ok_or("non-UTF-8 NDK path")?,
                r#"align=0x4000
[ "$(cat "$ROUGE_TEST_APP/mode")" != misaligned ] || align=0x1000
echo "  LOAD 0x000000 0x0000000000000000 0x0000000000000000 0x001000 0x001000 R E $align""#,
            )?;
            tool(
                sh,
                "cargo",
                r#"if [ "$1" != ndk ]; then exec "$REAL_CARGO" "$@"; fi
echo 'cargo-ndk 4.1.2'"#,
            )?;
            tool(
                sh,
                "java",
                r#"[ "$3" = -jar ] && [ "$8" = bundleRelease ]
[ "$APP_TARGET" = app ] && [ "$APP_VERSION" = 1.0.0 ] && [ "$ROUGE_BUILD_NUMBER" = 1 ]
cd "$7"
[ -f build.gradle ] && [ -f gradle/wrapper/gradle-wrapper.jar ]
mkdir -p src/release/jniLibs/arm64-v8a
touch src/release/jniLibs/arm64-v8a/libapp.so
bundle=build/outputs/bundle/release/RougeApp-release.aab
mkdir -p "${bundle%/*}"
printf payload > payload
jar cf "$bundle" payload
mode=$(cat "$ROUGE_TEST_APP/mode")
[ "$mode" != unsigned ] || exit 0
key=$ROUGE_KEYSTORE
if [ "$mode" = wrong ]; then
    key=wrong.p12
    keytool -genkeypair -keystore "$key" -storepass "$ANDROID_UPLOAD_STORE_PASSWORD" -alias "$ANDROID_UPLOAD_KEY_ALIAS" -keyalg RSA -dname CN=Wrong
fi
jarsigner -keystore "$key" -storepass "$ANDROID_UPLOAD_STORE_PASSWORD" "$bundle" "$ANDROID_UPLOAD_KEY_ALIAS""#,
            )?;
            let app = RougePublish::new(APP_ID).android(PlayInternal);
            let packager = Packager::new(app.clone(), ".");
            assert!(matches!(
                packager.run(Action::Build, Target::Android),
                Err(PublishError::Invalid(message)) if message.contains("cargo framer")
            ));
            generate(sh, app)?;
            for (mode, expected) in [
                ("misaligned", "16 KB alignment"),
                ("unsigned", "not signed with the configured upload key"),
                ("wrong", "not signed with the configured upload key"),
            ] {
                sh.write_file("mode", mode)?;
                assert!(
                    matches!(packager.run(Action::Build, Target::Android), Err(PublishError::Invalid(message)) if message.contains(expected)),
                    "{mode}"
                );
                assert!(!sh.path_exists(".rouge/out/android/app.aab"));
            }
            sh.write_file("mode", "matching")?;
            packager.run(Action::Build, Target::Android)?;
            assert!(sh.path_exists(".rouge/out/android/app.aab"));
            Ok(())
        },
    )
}

const APP_ID: &str = "com.example.app";

#[test]
fn web_build_optimizes_precompresses_and_enforces_size_budgets() -> Result {
    in_app(
        "web_build_optimizes_precompresses_and_enforces_size_budgets",
        |sh| {
            sh.write_file(
                "Cargo.lock",
                "version = 4\n\n[[package]]\nname = \"wasm-bindgen\"\nversion = \"0.2.100\"\n",
            )?;
            tool(sh, "rustup", "")?;
            tool(
                sh,
                "cargo",
                r#"mkdir -p "$CARGO_TARGET_DIR/wasm32-unknown-unknown/release"
printf '0123456789abcdef0123456789abcdef' > "$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/app.wasm""#,
            )?;
            tool(
                sh,
                "wasm-bindgen",
                r#"if [ "$1" = --version ]; then echo 'wasm-bindgen 0.2.100'; exit; fi
cp "$8" "$7/app_bg.wasm"
printf 'export default function init() {}\n' > "$7/app.js""#,
            )?;
            tool(
                sh,
                "wasm-opt",
                r#"[ "$1" = -O3 ]; printf '0123456789abcdef' > app_bg.wasm"#,
            )?;
            tool(
                sh,
                "brotli",
                r#"shift 3; for asset do printf 'br:%s' "$asset" > "$asset.br"; done"#,
            )?;
            let passing = Packager::new(
                RougePublish::new(APP_ID).web(
                    Web::new(GitHubPages)
                        .max_wasm_bytes(32)
                        .max_brotli_bytes(32),
                ),
                ".",
            );
            passing.run(Action::Build, Target::Web)?;

            assert_eq!(fs::metadata(".rouge/out/web/app_bg.wasm")?.len(), 16);
            for asset in ["app_bg.wasm.br", "app.js.br", "index.html.br"] {
                assert!(sh.path_exists(format!(".rouge/out/web/{asset}")), "{asset}");
            }

            for (web, asset) in [
                (Web::new(GitHubPages).max_wasm_bytes(8), "app_bg.wasm"),
                (Web::new(GitHubPages).max_brotli_bytes(4), "app_bg.wasm.br"),
            ] {
                let packager = Packager::new(RougePublish::new(APP_ID).web(web), ".");
                assert!(
                    matches!(packager.run(Action::Build, Target::Web), Err(PublishError::Invalid(message)) if message.starts_with(&format!("{asset} is ")))
                );
            }
            Ok(())
        },
    )
}

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
        "[workspace]\n[package]\nname='app'\nversion='1.0.0'\nedition='2024'\n[lib]\ncrate-type=['cdylib']\n",
    )?;
    sh.write_file("src/main.rs", "fn main() {}")?;
    sh.write_file("src/lib.rs", "")?;
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
        .env("ANDROID_HOME", root.join("sdk"))
        .env("IOS_PROFILE_BASE64", "test")
        .env("APPLE_API_KEY", "test-only")
        .env("APPLE_KEY_ID", "TEST")
        .env("APPLE_ISSUER_ID", "test")
        .env("APPLE_TEAM_ID", "TEAM123456")
        .env("APPLE_CERTIFICATE_PASSWORD", "test")
        .env("ANDROID_UPLOAD_STORE_PASSWORD", "test")
        .env("GH_TOKEN", "test")
        .run()?;
    Ok(())
}

fn generate(sh: &Shell, app: RougePublish) -> Result {
    let mut project = Project::new();
    project.apply(app);
    project.synth_with(&mut DiskFs::new(sh.current_dir()), Mode::Write)?;
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
        " build\nxcrun simctl bootstatus {udid} -b\nopen -a Simulator\nxcrun simctl install {udid} {bundle}\nxcrun simctl launch --terminate-running-process {udid} {APP_ID}\n"
    )), "{commands}");
    Ok(())
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn simulator_tools(sh: &Shell) -> Result {
    let isolated = r#"[ -z "${APPLE_API_KEY+x}${APPLE_CERTIFICATE_PASSWORD+x}${IOS_PROFILE_BASE64+x}${GH_TOKEN+x}" ]"#;
    let log = r#"printf '%s %s\n' "${0##*/}" "$*" >> "$ROUGE_TEST_APP/commands""#;
    for (name, script) in [
        (
            "xcrun",
            r#"case "$*" in
  '--find simctl') echo /fixture/simctl; exit ;;
  'simctl list devices available --json') cat "$ROUGE_TEST_APP/devices.json"; exit ;;
esac
case "$*" in
  'simctl bootstatus '*|'simctl install '*|'simctl launch '*) ;;
  *) exit 1 ;;
esac"#,
        ),
        ("rustup", ""),
        (
            "sips",
            "printf '%s\n' 'pixelWidth: 1024' 'pixelHeight: 1024' 'hasAlpha: no'",
        ),
        ("xcodebuild", ""),
        ("open", ""),
    ] {
        let skip_log = match name {
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
