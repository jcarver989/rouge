use framer::github::Step;
use framer::testing::render_preset;
use framer::{DiskFs, Mode, Project};
use rouge_framer::{
    Action, Android, AndroidToolchain, AppleToolchain, Channel, Device, GitHubPages,
    GitHubReleases, Ios, Linux, Macos, Orientation, Packager, PlayInternal, PublishError,
    RougePublish, Target, TestFlight, Web,
};
use std::fs;
use std::io::Error;
use tempfile::tempdir;

#[test]
fn only_generates_enabled_targets() -> Result<(), Box<dyn std::error::Error>> {
    let fs = render_preset(RougePublish::new(APP_ID).web(GitHubPages))?;
    let workflow = fs.get(WORKFLOW).ok_or_else(|| missing(WORKFLOW))?;
    let checklist = fs
        .get("PUBLISHING.md")
        .ok_or_else(|| missing("PUBLISHING.md"))?;
    assert!(workflow.contains("options: [all, web]"));
    assert!(workflow.contains("actions/deploy-pages@"));
    assert!(!workflow.contains("secrets."));
    assert!(checklist.contains("/preview/"));
    assert!(!checklist.contains("TestFlight"));
    assert!(workflow.contains("run: cargo rouge publish web"));
    assert!(!workflow.contains("cargo rouge build ios"));
    assert!(fs.paths().all(|path| !path.starts_with(".rouge")));
    Ok(())
}

#[test]
fn renders_a_full_default_branch_workflow() -> Result<(), Box<dyn std::error::Error>> {
    let fs = render_preset(all_targets())?;
    let workflow = fs.get(WORKFLOW).ok_or_else(|| missing(WORKFLOW))?;
    assert!(workflow.contains("options: [all, web, desktop, ios, android]"));
    assert!(!workflow.contains("@@"));
    assert!(!workflow.contains("pull_request"));
    assert_eq!(
        workflow
            .matches("github.event.repository.default_branch")
            .count(),
        5
    );
    assert!(workflow.contains("${{ secrets.IOS_PROFILE_BASE64 }}"));
    assert!(workflow.contains("${{ secrets.ANDROID_UPLOAD_KEYSTORE_BASE64 }}"));
    assert!(workflow.contains("packageName: com.example.app"));
    for target in ["web", "linux", "macos", "ios", "android"] {
        assert!(workflow.contains(&format!("run: cargo rouge build {target}")));
    }
    assert!(!workflow.contains("run: |"));
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
fn platform_options_change_the_generated_workflow_and_checklist()
-> Result<(), Box<dyn std::error::Error>> {
    let fs = render_preset(
        RougePublish::new(APP_ID)
            .icon("assets/icon.png")
            .web(Web::new(GitHubPages).timeout_minutes(20))
            .linux(Linux::new(GitHubReleases).apt_packages(["libasound2-dev"]))
            .macos(Macos::new(GitHubReleases).runner("macos-15"))
            .ios(
                Ios::new(TestFlight::new("12345"))
                    .toolchain(AppleToolchain::new().fastlane("2.300.0").ruby("3.5"))
                    .setup(Step::run("brew install just").name("Install just")),
            )
            .android(
                Android::new(PlayInternal).target_sdk(37).toolchain(
                    AndroidToolchain::new()
                        .gradle("9.0")
                        .java("21")
                        .ndk("29.0.1"),
                ),
            ),
    )?;
    let workflow = fs.get(WORKFLOW).ok_or_else(|| missing(WORKFLOW))?;
    let checklist = fs
        .get("PUBLISHING.md")
        .ok_or_else(|| missing("PUBLISHING.md"))?;
    for expected in [
        "timeout-minutes: 20",
        "apt-get install -y libfontconfig1-dev libasound2-dev",
        "runs-on: macos-15",
        "gem install fastlane --no-document --version 2.300.0",
        "ruby-version: \"3.5\"",
        "name: Install just",
        "gradle-version: \"9.0\"",
        "java-version: \"21\"",
    ] {
        assert!(workflow.contains(expected), "missing {expected}");
    }
    assert!(checklist.contains("JDK 21, Gradle 9.0, SDK platform 37"));
    assert!(checklist.contains("NDK `29.0.1`"));
    assert!(checklist.contains("`libfontconfig1-dev`, `libasound2-dev`"));
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
            RougePublish::new(APP_ID)
                .android(Android::new(PlayInternal).permission("INTERNET\" />")),
            "permission name",
        ),
        (
            RougePublish::new(APP_ID)
                .icon("icon.png")
                .ios(ios().minimum_os("14.0")),
            "iOS 15.0",
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
fn accepts_typed_ios_options() -> Result<(), Box<dyn std::error::Error>> {
    render_preset(
        RougePublish::new(APP_ID).icon("icon.png").ios(
            Ios::new(TestFlight::new("12345"))
                .minimum_os("17.0")
                .devices([Device::Phone])
                .orientations([Orientation::Portrait])
                .info("NSCameraUsageDescription", "Scan receipts")
                .entitlement("aps-environment", "production"),
        ),
    )?;
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
