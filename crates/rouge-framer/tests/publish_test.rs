use framer::github::Step;
use framer::testing::render_preset;
use framer::{DiskFs, Mode, Project};
use rouge_framer::{
    Action, Android, AndroidToolchain, Channel, Device, GitHubPages, GitHubReleases, Ios, Linux,
    Macos, Orientation, Packager, PlayInternal, PublishError, RougePublish, Target, TestFlight,
    Web,
};
use serde_json::{Value, json};
use std::fs;
use std::io::Error;
use tempfile::tempdir;

#[test]
fn only_generates_enabled_targets() -> Result<(), Box<dyn std::error::Error>> {
    let fs = render_preset(RougePublish::new(APP_ID).web(GitHubPages))?;
    let workflow = fs.get(WORKFLOW).ok_or_else(|| missing(WORKFLOW))?;
    assert!(fs.get("PUBLISHING.md").is_none());
    assert!(workflow.contains("options: [all, web]"));
    assert!(workflow.contains("actions/deploy-pages@"));
    assert!(!workflow.contains("secrets."));
    let steps = [
        "run: cargo rouge stage web",
        "actions/upload-pages-artifact@",
        "actions/deploy-pages@",
        "run: cargo rouge publish web",
    ];
    let mut remaining = workflow;
    for step in steps {
        (_, remaining) = remaining.split_once(step).ok_or_else(|| missing(step))?;
    }
    assert!(!workflow.contains("cargo rouge build ios"));
    assert!(fs.get("mise.toml").is_none());
    assert!(fs.paths().all(|path| !path.starts_with(".rouge")));
    Ok(())
}

#[test]
fn renders_a_full_default_branch_workflow() -> Result<(), Box<dyn std::error::Error>> {
    let fs = render_preset(all_targets())?;
    let workflow = fs.get(WORKFLOW).ok_or_else(|| missing(WORKFLOW))?;
    assert!(workflow.contains("options: [all, web, linux, macos, ios, android]"));
    assert!(!workflow.contains("@@"));
    assert!(!workflow.contains("pull_request"));
    assert_eq!(
        workflow
            .matches("github.event.repository.default_branch")
            .count(),
        6
    );
    assert!(workflow.contains("ROUGE_CHANNEL: ${{ inputs.channel }}"));
    assert!(
        workflow.contains("ROUGE_BUILD_NUMBER: ${{ inputs.build_number || github.run_number }}")
    );
    assert!(workflow.contains("permissions:\n  contents: read"));
    assert_eq!(workflow.matches("id-token: write").count(), 2);
    for target in ["web", "linux", "macos"] {
        let job = workflow_job(workflow, target)?;
        assert!(job.contains(&format!("run: cargo rouge build {target}")));
        assert!(job.contains(&format!("run: cargo rouge publish {target}")));
        assert_eq!(job.contains("id-token: write"), target == "web");
        assert!(!job.contains("include-hidden-files"));
    }
    for (target, secret) in [
        ("ios", "APPLE_API_KEY"),
        ("android", "ANDROID_UPLOAD_KEYSTORE_BASE64"),
    ] {
        let job = workflow_job(workflow, target)?;
        for expected in [
            "inputs.channel != 'production'",
            &format!("inputs.target == '{target}'"),
            &format!("inputs.channel == 'preview' && secrets.{secret} || ''"),
            "include-hidden-files",
        ] {
            assert!(job.contains(expected), "missing {expected} in {job}");
        }
        assert_eq!(
            job.matches(&format!("run: cargo rouge build {target}"))
                .count(),
            1
        );
        assert!(!job.contains("id-token: write"));
        for line in job.lines().filter(|line| line.contains("secrets.")) {
            assert!(line.contains("inputs.channel == 'preview'"), "{line}");
        }
    }
    let preview = workflow_job(workflow, "android-preview")?;
    for expected in [
        "inputs.channel == 'preview'",
        "rouge-android-preview",
        "id-token: write",
        "google-github-actions/auth@",
        "vars.GOOGLE_PLAY_WORKLOAD_IDENTITY_PROVIDER",
        "vars.GOOGLE_PLAY_SERVICE_ACCOUNT",
        "steps.auth.outputs.credentials_file_path",
        "r0adkll/upload-google-play@",
        "packageName: com.example.app",
    ] {
        assert!(
            preview.contains(expected),
            "missing {expected} in {preview}"
        );
    }
    assert!(!preview.contains("secrets."));
    assert!(!preview.contains("github.run_attempt"));
    assert!(!workflow.contains("secrets.IOS_PROFILE_BASE64"));
    assert!(!workflow.contains("PLAY_SERVICE_ACCOUNT_JSON"));
    if let Some(path) = std::env::var_os("ROUGE_TEST_WORKFLOW") {
        std::fs::write(path, workflow).map_err(|source| framer::FramerError::Io {
            path: WORKFLOW.into(),
            source,
        })?;
    }
    Ok(())
}

#[test]
fn validates_identity_and_platform_requirements() {
    for bad in [
        RougePublish::new("bad/path").web(GitHubPages),
        RougePublish::new(APP_ID),
        RougePublish::new(APP_ID).ios(TestFlight::new("1234")),
        RougePublish::new(APP_ID)
            .icon("../secrets.png")
            .web(GitHubPages),
        RougePublish::new(APP_ID)
            .icon("icon.png")
            .ios(TestFlight::new("x")),
        RougePublish::new(APP_ID)
            .icon("icon.png")
            .ios(TestFlight::new("1234").invite_url("https://elsewhere.example")),
    ] {
        assert!(matches!(
            render_preset(bad),
            Err(framer::FramerError::Invalid { .. })
        ));
    }
}

#[test]
fn adds_the_packaging_command_as_a_cargo_alias() -> Result<(), Box<dyn std::error::Error>> {
    let fs = render_preset(RougePublish::new(APP_ID).web(GitHubPages))?;
    let config = fs
        .get(".cargo/config.toml")
        .ok_or_else(|| missing(".cargo/config.toml"))?;
    assert!(
        config.contains("rouge = \"run --quiet --locked --manifest-path .framer/Cargo.toml --\"")
    );
    Ok(())
}

#[test]
fn rejects_targets_that_are_not_enabled() {
    let packager = Packager::new(RougePublish::new(APP_ID).web(GitHubPages), ".");
    for target in [Target::Linux, Target::Macos, Target::Ios, Target::Android] {
        assert!(matches!(
            packager.run(Action::Doctor, target),
            Err(PublishError::Invalid(message)) if message.contains("not enabled")
        ));
    }
}

#[test]
fn generated_files_pass_framer_check() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let mut fs = DiskFs::new(root.path());
    let mut project = Project::new();
    project.apply(
        RougePublish::new(APP_ID)
            .web(GitHubPages)
            .linux(GitHubReleases),
    );
    project.synth_with(&mut fs, Mode::Write)?;
    let report = project.synth_with(&mut fs, Mode::Check)?;
    assert!(report.is_clean(), "{}", report.diff());
    Ok(())
}

#[test]
fn adds_and_removes_only_the_simulator_task() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let mut fs = DiskFs::new(root.path());
    let mise_config = root.path().join("mise.toml");
    let existing = "[tasks.hello]\nrun = \"echo hello\"\n";
    fs::write(&mise_config, existing)?;
    let config = RougePublish::new(APP_ID).web(GitHubPages).icon("icon.png");
    let mut project = Project::new();
    project.apply(config.clone().ios(TestFlight::new("0")));
    project.synth_with(&mut fs, Mode::Write)?;

    let generated = fs::read_to_string(&mise_config)?;
    assert!(generated.starts_with(existing));
    assert!(generated.contains("[tasks.ios-sim]"));
    assert!(generated.contains("run = \"cargo rouge run ios\""));
    assert!(!root.path().join("justfile").exists());
    let workflow = fs::read_to_string(root.path().join(WORKFLOW))?;
    assert!(workflow.contains("run: cargo rouge build ios"));
    assert!(!workflow.contains("ios-sim"));
    assert!(!workflow.contains("cargo rouge run ios"));
    let report = project.synth_with(&mut fs, Mode::Check)?;
    assert!(report.is_clean(), "{}", report.diff());

    let mut project = Project::new();
    project.apply(config);
    let report = project.synth_with(&mut fs, Mode::Check)?;
    assert!(!report.is_clean());
    project.synth_with(&mut fs, Mode::Write)?;
    assert_eq!(fs::read_to_string(&mise_config)?, existing);
    let report = project.synth_with(&mut fs, Mode::Check)?;
    assert!(report.is_clean(), "{}", report.diff());
    Ok(())
}

#[test]
fn only_publishes_preview_and_production_channels() -> Result<(), Box<dyn std::error::Error>> {
    let app = tempdir()?;
    fs::create_dir(app.path().join("src"))?;
    fs::write(
        app.path().join("Cargo.toml"),
        "[package]\nname = \"publish-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n",
    )?;
    fs::write(app.path().join("src/lib.rs"), "")?;
    fs::write(
        app.path().join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"publish-fixture\"\nversion = \"0.1.0\"\n",
    )?;
    let packager = Packager::new(
        RougePublish::new(APP_ID)
            .web(GitHubPages)
            .android(PlayInternal),
        app.path(),
    );
    assert!(matches!(
        packager.run(Action::Publish, Target::Web),
        Err(PublishError::Invalid(message)) if message.contains("ROUGE_CHANNEL")
    ));
    assert!(matches!(
        packager.channel(Channel::Preview).run(Action::Publish, Target::Android),
        Err(PublishError::Invalid(message)) if message.contains("Google Play")
    ));
    Ok(())
}

#[test]
fn platform_options_change_the_generated_workflow() -> Result<(), Box<dyn std::error::Error>> {
    let fs = render_preset(
        RougePublish::new(APP_ID)
            .icon("assets/icon.png")
            .web(Web::new(GitHubPages).timeout_minutes(20))
            .linux(Linux::new(GitHubReleases).apt_packages(["libasound2-dev"]))
            .macos(Macos::new(GitHubReleases).runner("macos-15"))
            .ios(
                Ios::new(TestFlight::new("12345"))
                    .runner("macos-26-xlarge")
                    .setup(Step::run("brew install xcbeautify").name("Install xcbeautify")),
            )
            .android(
                Android::new(PlayInternal).target_sdk(37).toolchain(
                    AndroidToolchain::new()
                        .gradle("9.5", "b".repeat(64))
                        .java("21")
                        .ndk("29.0.1"),
                ),
            ),
    )?;
    let workflow = fs.get(WORKFLOW).ok_or_else(|| missing(WORKFLOW))?;
    for expected in [
        "timeout-minutes: 20",
        "apt-get install -y libfontconfig1-dev libasound2-dev",
        "runs-on: macos-15",
        "runs-on: macos-26-xlarge",
        "name: Install xcbeautify",
        "java-version: \"21\"",
    ] {
        assert!(workflow.contains(expected), "missing {expected}");
    }
    Ok(())
}

#[test]
fn rejects_platform_options_rouge_cannot_build() {
    let ios = || Ios::new(TestFlight::new("12345"));
    for (bad, reason) in [
        (
            RougePublish::new(APP_ID).android(Android::new(PlayInternal).min_sdk(24)),
            "API level 28",
        ),
        (
            RougePublish::new(APP_ID).android(Android::new(PlayInternal).target_sdk(27)),
            "target_sdk",
        ),
        (
            RougePublish::new(APP_ID).android(Android::new(PlayInternal).compile_sdk(35)),
            "compile_sdk",
        ),
        (
            RougePublish::new(APP_ID).android(
                Android::new(PlayInternal)
                    .toolchain(AndroidToolchain::new().gradle("9.5", "not-a-digest")),
            ),
            "AndroidToolchain::gradle",
        ),
        (
            RougePublish::new(APP_ID)
                .android(Android::new(PlayInternal).permission("INTERNET\" />")),
            "permission name",
        ),
        (
            RougePublish::new(APP_ID)
                .icon("icon.png")
                .ios(ios().minimum_os("16.4")),
            "iOS 17.0",
        ),
        (
            RougePublish::new(APP_ID)
                .icon("icon.png")
                .ios(ios().devices([])),
            "Device::Phone",
        ),
        (
            RougePublish::new(APP_ID)
                .icon("icon.png")
                .ios(ios().orientations([Orientation::Portrait; 0])),
            "orientation",
        ),
        (
            RougePublish::new(APP_ID).macos(Macos::new(GitHubReleases).minimum_os("latest")),
            "macOS version",
        ),
        (
            RougePublish::new(APP_ID)
                .linux(Linux::new(GitHubReleases).apt_packages(["curl; rm -rf /"])),
            "apt package",
        ),
    ] {
        match render_preset(bad) {
            Err(framer::FramerError::Invalid { message, .. }) => {
                assert!(message.contains(reason), "{message}");
            }
            other => panic!("expected `{reason}`, got {:?}", other.map(|_| ())),
        }
    }
}

#[test]
fn generates_an_xcode_project_from_typed_ios_options() -> Result<(), Box<dyn std::error::Error>> {
    let fs = render_preset(
        RougePublish::new(APP_ID)
            .name("Fixture")
            .icon("icon.png")
            .ios(
                Ios::new(TestFlight::new("12345"))
                    .minimum_os("17.2.1")
                    .devices([Device::Phone])
                    .orientations([Orientation::Portrait])
                    .info("NSCameraUsageDescription", "Scan receipts")
                    .info("CFBundleSupportedPlatforms", "iPhoneOS")
                    .info("DTSDKName", "iphoneos")
                    .entitlement("aps-environment", "production"),
            ),
    )?;
    let file = |path: &'static str| fs.get(path).ok_or_else(|| missing(path));
    let project = file("ios/RougeApp.xcodeproj/project.pbxproj")?;
    assert!(!project.contains("@@"));
    for setting in [
        "IPHONEOS_DEPLOYMENT_TARGET = \"17.2.1\";",
        "TARGETED_DEVICE_FAMILY = \"1\";",
        "PRODUCT_BUNDLE_IDENTIFIER = \"com.example.app\";",
        "ROUGE_DISPLAY_NAME = \"Fixture\";",
        "CODE_SIGN_ENTITLEMENTS = App.entitlements;",
        "path = \"PrivacyInfo.xcprivacy\";",
    ] {
        assert_eq!(project.matches(setting).count().min(1), 1, "{setting}");
    }
    let info: Value = plist::from_bytes(file("ios/Info.plist")?.as_bytes())?;
    assert!(
        info.as_object()
            .is_some_and(|keys| keys.keys().all(|key| !key.starts_with("DT")))
    );
    for (key, value) in [
        ("CFBundleIdentifier", json!("$(PRODUCT_BUNDLE_IDENTIFIER)")),
        ("CFBundleDisplayName", json!("$(ROUGE_DISPLAY_NAME)")),
        ("CFBundleExecutable", json!("$(EXECUTABLE_NAME)")),
        ("CFBundleVersion", json!("$(CURRENT_PROJECT_VERSION)")),
        ("CFBundleSupportedPlatforms", Value::Null),
        (
            "UISupportedInterfaceOrientations",
            json!(["UIInterfaceOrientationPortrait"]),
        ),
        ("NSCameraUsageDescription", json!("Scan receipts")),
    ] {
        assert_eq!(info[key], value, "{key}");
    }
    let entitlements: Value = plist::from_bytes(file("ios/App.entitlements")?.as_bytes())?;
    assert_eq!(entitlements["aps-environment"], "production");
    let _: Value = plist::from_bytes(file("ios/PrivacyInfo.xcprivacy")?.as_bytes())?;
    assert!(file("ios/cargo.sh")?.contains("cp \"icon.png\""));
    assert!(file(".gitignore")?.contains("/ios/Assets.xcassets/AppIcon.appiconset/icon.png"));
    Ok(())
}

#[test]
fn generates_a_gradle_project_that_builds_the_rust_library()
-> Result<(), Box<dyn std::error::Error>> {
    let fs = render_preset(
        RougePublish::new(APP_ID)
            .name("It's an app")
            .icon("icon.png")
            .package("fixture-app")
            .android(Android::new(PlayInternal).permission("android.permission.INTERNET")),
    )?;
    let file = |path: &'static str| fs.get(path).ok_or_else(|| missing(path));
    let gradle = file("android/build.gradle")?;
    assert!(!gradle.contains("@@"));
    for expected in [
        "id 'com.android.application' version '9.5.1'",
        "namespace 'com.example.app'",
        "compileSdk { version = release(37) }",
        "ndkVersion '28.2.13676358'",
        "minSdk 28",
        "targetSdk 36",
        "env('APP_NAME', 'It\\'s an app')",
        "env('APP_TARGET', 'fixture-app')",
        "'cargo', 'ndk', '-t', 'arm64-v8a', '-P', '28'",
        "'--lib', '--package', 'fixture-app']",
        "from '../icon.png'",
    ] {
        assert!(gradle.contains(expected), "missing {expected} in {gradle}");
    }
    let manifest = file("android/src/main/AndroidManifest.xml")?;
    assert!(manifest.contains("<uses-permission android:name=\"android.permission.INTERNET\" />"));
    assert!(manifest.contains("android:icon=\"@drawable/rouge_icon\""));
    assert!(
        file("android/gradle/wrapper/gradle-wrapper.properties")?.contains("gradle-9.6.0-bin.zip")
    );
    assert!(file("android/settings.gradle")?.contains("rootProject.name = 'RougeApp'"));
    assert!(file(".gitignore")?.contains("/android/src/*/jniLibs/"));
    let web = render_preset(RougePublish::new(APP_ID).web(GitHubPages))?;
    assert!(
        web.paths()
            .all(|path| !path.starts_with("ios") && !path.starts_with("android"))
    );
    Ok(())
}

const APP_ID: &str = "com.example.app";
const WORKFLOW: &str = ".github/workflows/rouge-publish.yml";

fn all_targets() -> RougePublish {
    RougePublish::new(APP_ID)
        .name("App Name")
        .icon("assets/icon.png")
        .web(GitHubPages)
        .desktop(GitHubReleases)
        .ios(TestFlight::new("12345").invite_url("https://testflight.apple.com/join/testCode"))
        .android(PlayInternal)
}

fn missing(path: &str) -> Error {
    Error::other(format!("Missing generated {path}"))
}

fn workflow_job<'a>(workflow: &'a str, id: &str) -> Result<&'a str, Error> {
    let (_, job) = workflow
        .split_once(&format!("\n  {id}:\n"))
        .ok_or_else(|| missing(id))?;
    let len = job
        .split_inclusive('\n')
        .take_while(|line| line.starts_with("    "))
        .map(str::len)
        .sum();
    Ok(&job[..len])
}
