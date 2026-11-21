mod archive;
mod simulator;

use crate::error::{Result, ensure, invalid};
use crate::packager::Build;
use crate::platform::apple::{Bundle, bundle_options};
use crate::platform::{Platform, apple, os_version};
use crate::secrets::{self, APPLE_API, secret};
use crate::workflow::{self, Ci, PREVIEWING, ci_options};
use crate::{Channel, RougePublish, Target};
use framer::github::Job;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use xshell::{Cmd, cmd};

const SUPPORTED: (u32, u32) = (15, 0);
const XCODE_INFO: [&str; 3] = [
    "UIDeviceFamily",
    "MinimumOSVersion",
    "CFBundleSupportedPlatforms",
];
const ICON_REQUIREMENT: &str = "iOS requires an opaque 1024 × 1024 PNG icon";

const DEVICE: &str = "aarch64-apple-ios";
const SIMULATOR: &str = "aarch64-apple-ios-sim";
const PROJECT: &str = "ios/RougeApp.xcodeproj";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestFlight {
    app_store_id: String,
    invite_url: Option<String>,
}

impl TestFlight {
    pub fn new(app_store_id: impl Into<String>) -> Self {
        Self {
            app_store_id: app_store_id.into(),
            invite_url: None,
        }
    }

    pub fn invite_url(mut self, url: impl Into<String>) -> Self {
        self.invite_url = Some(url.into());
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Device {
    Phone,
    Pad,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Orientation {
    Portrait,
    PortraitUpsideDown,
    LandscapeLeft,
    LandscapeRight,
}

#[derive(Clone, Debug)]
pub struct Ios {
    destination: TestFlight,
    minimum_os: String,
    devices: Vec<Device>,
    orientations: Option<Vec<Orientation>>,
    bundle: Bundle,
    privacy_manifest: Option<PathBuf>,
    ci: Ci,
}

impl Ios {
    pub fn new(destination: TestFlight) -> Self {
        Self {
            destination,
            minimum_os: "16.0".into(),
            devices: vec![Device::Phone, Device::Pad],
            orientations: None,
            bundle: Bundle::default(),
            privacy_manifest: None,
            ci: Ci::default(),
        }
    }

    pub fn minimum_os(mut self, version: impl Into<String>) -> Self {
        self.minimum_os = version.into();
        self
    }

    pub fn devices(mut self, devices: impl IntoIterator<Item = Device>) -> Self {
        self.devices = devices.into_iter().collect();
        self
    }

    pub fn orientations(mut self, orientations: impl IntoIterator<Item = Orientation>) -> Self {
        self.orientations = Some(orientations.into_iter().collect());
        self
    }

    pub fn privacy_manifest(mut self, path: impl Into<PathBuf>) -> Self {
        self.privacy_manifest = Some(path.into());
        self
    }

    fn families(&self) -> Vec<u32> {
        self.devices
            .iter()
            .map(|device| if *device == Device::Phone { 1 } else { 2 })
            .collect()
    }

    fn info_plist(&self) -> Value {
        let named = |orientations: &[Orientation]| -> Vec<String> {
            orientations
                .iter()
                .map(|orientation| format!("UIInterfaceOrientation{orientation:?}"))
                .collect()
        };
        let phone = [
            Orientation::Portrait,
            Orientation::LandscapeLeft,
            Orientation::LandscapeRight,
        ];
        let pad = [
            Orientation::Portrait,
            Orientation::PortraitUpsideDown,
            Orientation::LandscapeLeft,
            Orientation::LandscapeRight,
        ];
        let chosen = self.orientations.as_deref();
        let mut info = json!({
            "UILaunchScreen": {},
            "UISupportedInterfaceOrientations": named(chosen.unwrap_or(&phone)),
            "UISupportedInterfaceOrientations~ipad": named(chosen.unwrap_or(&pad)),
        });
        apple::extend(&mut info, &self.bundle.info);
        if let Some(fields) = info.as_object_mut() {
            fields.retain(|key, _| !key.starts_with("DT") && !XCODE_INFO.contains(&key.as_str()));
        }
        for (key, value) in [
            ("CFBundleIdentifier", json!("$(PRODUCT_BUNDLE_IDENTIFIER)")),
            ("CFBundleDisplayName", json!("$(ROUGE_DISPLAY_NAME)")),
            ("CFBundleName", json!("$(ROUGE_DISPLAY_NAME)")),
            ("CFBundleExecutable", json!("$(EXECUTABLE_NAME)")),
            ("CFBundlePackageType", json!("APPL")),
            ("CFBundleInfoDictionaryVersion", json!("6.0")),
            ("CFBundleShortVersionString", json!("$(MARKETING_VERSION)")),
            ("CFBundleVersion", json!("$(CURRENT_PROJECT_VERSION)")),
            ("LSRequiresIPhoneOS", json!(true)),
        ] {
            info[key] = value;
        }
        info
    }
}

impl From<TestFlight> for Ios {
    fn from(destination: TestFlight) -> Self {
        Self::new(destination)
    }
}

bundle_options!(Ios);
ci_options!(Ios);

impl Platform for Ios {
    fn target(&self) -> Target {
        Target::Ios
    }

    fn job(&self, _config: &RougePublish) -> Job {
        let target = self.target();
        let mut job = self.ci.job(target, "macos-26", 90, workflow::checkout());
        if self.ci.runner.is_none() {
            job = job.env(
                "DEVELOPER_DIR",
                "/Applications/Xcode_26.6.app/Contents/Developer",
            );
        }
        job.step(workflow::preview_secrets(
            workflow::rouge("Build iOS device archive", "build", target)
                .env(archive::TEAM_ID, "${{ vars.APPLE_TEAM_ID }}"),
            APPLE_API,
        ))
        .step(workflow::inspect(target, "unsigned-app.zip"))
        .step(workflow::artifact(
            target,
            "Save iOS crash symbols",
            "dsyms",
            "dSYMs.zip",
            PREVIEWING,
        ))
        .step(workflow::preview_secrets(
            workflow::rouge("Upload to App Store Connect", "publish", target).condition(PREVIEWING),
            APPLE_API,
        ))
    }

    fn guide(&self) -> &'static str {
        include_str!("guide.md")
    }

    fn files(&self, config: &RougePublish) -> Vec<(String, String)> {
        let quoted = |value: &str| format!("{value:?}");
        let families = self
            .families()
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let executable = config.binary.as_ref().or(config.package.as_ref());
        let mut settings = vec![
            ("ALWAYS_SEARCH_USER_PATHS", "NO".to_owned()),
            ("ASSETCATALOG_COMPILER_APPICON_NAME", "AppIcon".to_owned()),
            ("CURRENT_PROJECT_VERSION", "1".to_owned()),
            ("DEBUG_INFORMATION_FORMAT", "dwarf".to_owned()),
            ("ENABLE_USER_SCRIPT_SANDBOXING", "NO".to_owned()),
            ("INFOPLIST_FILE", "Info.plist".to_owned()),
            ("IPHONEOS_DEPLOYMENT_TARGET", quoted(&self.minimum_os)),
            ("MARKETING_VERSION", "1.0.0".to_owned()),
            ("PRODUCT_BUNDLE_IDENTIFIER", quoted(&config.id)),
            ("PRODUCT_NAME", "RougeApp".to_owned()),
            (
                "ROUGE_DISPLAY_NAME",
                quoted(config.name.as_deref().unwrap_or("RougeApp")),
            ),
            ("SKIP_INSTALL", "NO".to_owned()),
            ("STRIP_INSTALLED_PRODUCT", "NO".to_owned()),
            ("TARGETED_DEVICE_FAMILY", quoted(&families)),
        ];
        if let Some(executable) = executable {
            settings.push(("EXECUTABLE_NAME", quoted(executable)));
        }
        if !self.bundle.entitlements.is_empty() {
            settings.push(("CODE_SIGN_ENTITLEMENTS", "App.entitlements".to_owned()));
        }
        let settings = settings
            .iter()
            .map(|(key, value)| format!("                {key} = {value};"))
            .collect::<Vec<_>>()
            .join("\n");
        let privacy = self
            .privacy_manifest
            .as_ref()
            .map_or("PrivacyInfo.xcprivacy".to_owned(), |path| {
                format!("../{}", path.display())
            });
        let package = config
            .package
            .as_ref()
            .map_or(String::new(), |package| format!(" --package {package}"));
        let author = json!({"author": "xcode", "version": 1});
        let icon = json!({
            "images": [{"filename": "icon.png", "idiom": "universal", "platform": "ios", "size": "1024x1024"}],
            "info": author,
        });
        let plist = |value: &Value| {
            let mut xml = Vec::new();
            plist::to_writer_xml(&mut xml, value)
                .ok()
                .and_then(|()| String::from_utf8(xml).ok())
                .unwrap_or_default()
        };
        let mut files = vec![
            (
                format!("{PROJECT}/project.pbxproj"),
                include_str!("xcode/project.pbxproj")
                    .replace("@@SETTINGS@@", &settings)
                    .replace("@@PRIVACY@@", &quoted(&privacy)),
            ),
            (
                format!("{PROJECT}/xcshareddata/xcschemes/RougeApp.xcscheme"),
                include_str!("xcode/RougeApp.xcscheme").to_owned(),
            ),
            (
                "ios/cargo.sh".to_owned(),
                include_str!("xcode/cargo.sh")
                    .replace("@@PACKAGE@@", &package)
                    .replace("@@ICON@@", config.icon.as_deref().unwrap_or_default()),
            ),
            ("ios/Info.plist".to_owned(), plist(&self.info_plist())),
            (
                "ios/Assets.xcassets/Contents.json".to_owned(),
                json!({"info": author}).to_string(),
            ),
            (
                "ios/Assets.xcassets/AppIcon.appiconset/Contents.json".to_owned(),
                icon.to_string(),
            ),
        ];
        if self.privacy_manifest.is_none() {
            files.push(("ios/PrivacyInfo.xcprivacy".to_owned(), plist(&json!({}))));
        }
        if !self.bundle.entitlements.is_empty() {
            let mut entitlements = json!({});
            apple::extend(&mut entitlements, &self.bundle.entitlements);
            files.push(("ios/App.entitlements".to_owned(), plist(&entitlements)));
        }
        files
    }

    fn ignored(&self) -> &'static str {
        "/ios/Assets.xcassets/AppIcon.appiconset/icon.png\n/ios/RougeApp.xcodeproj/project.xcworkspace/\n/ios/RougeApp.xcodeproj/xcuserdata/\n"
    }

    fn validate(&self, config: &RougePublish) -> Result<(), String> {
        let TestFlight {
            app_store_id,
            invite_url,
        } = &self.destination;
        if self.privacy_manifest.as_ref().is_some_and(|path| {
            path.file_name()
                .is_none_or(|name| name != "PrivacyInfo.xcprivacy")
                || !path
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_)))
        }) {
            return Err(
                "Ios::privacy_manifest expects a repository-relative PrivacyInfo.xcprivacy path"
                    .into(),
            );
        }
        if config.icon.is_none() {
            return Err(ICON_REQUIREMENT.into());
        }
        if app_store_id.is_empty() || !app_store_id.bytes().all(|c| c.is_ascii_digit()) {
            return Err("TestFlight::new expects the numeric App Store Connect Apple ID".into());
        }
        if os_version(&self.minimum_os).is_none_or(|version| version < SUPPORTED) {
            return Err(format!(
                "Ios::minimum_os(\"{}\") is too low; Rouge supports iOS {}.{} and later",
                self.minimum_os, SUPPORTED.0, SUPPORTED.1
            ));
        }
        if self.devices.is_empty() {
            return Err("Ios::devices needs at least one of Device::Phone or Device::Pad".into());
        }
        if self.orientations.as_ref().is_some_and(Vec::is_empty) {
            return Err("Ios::orientations needs at least one orientation".into());
        }
        let invitation = |url: &String| {
            url.strip_prefix("https://testflight.apple.com/join/")
                .is_some_and(|code| {
                    !code.is_empty() && code.bytes().all(|c| c.is_ascii_alphanumeric())
                })
        };
        if invite_url.as_ref().is_some_and(|url| !invitation(url)) {
            return Err("use a TestFlight public invitation URL".into());
        }
        Ok(())
    }

    fn setup(&self, build: &Build) -> Result<()> {
        archive::check_xcode(build)?;
        let sh = &build.sh;
        build
            .isolated(cmd!(sh, "rustup target add {DEVICE}"))
            .run()?;
        Ok(())
    }

    fn doctor(&self, build: &Build) -> Result<()> {
        archive::check_xcode(build)?;
        check_icon(build)?;
        if build.publishing() {
            secrets::require(APPLE_API)?;
            secret(archive::TEAM_ID)?;
        }
        Ok(())
    }

    fn build(&self, build: &Build) -> Result<()> {
        self.doctor(build)?;
        archive::build(build)
    }

    fn run_local(&self, build: &Build, device: Option<&str>) -> Result<()> {
        simulator::run(self, build, device)
    }

    fn publish(&self, build: &Build) -> Result<()> {
        let connect = format!(
            "https://appstoreconnect.apple.com/apps/{}",
            self.destination.app_store_id
        );
        ensure(
            build.channel == Channel::Preview,
            "Promote the tested TestFlight build to production in App Store Connect; see PUBLISHING.md",
        )?;
        archive::upload(build)?;
        let invite = self
            .destination
            .invite_url
            .clone()
            .unwrap_or_else(|| format!("{connect}/testflight"));
        build.summarize_mobile(&format!("Uploaded to App Store Connect; [TestFlight]({invite}) availability depends on Apple processing and tester setup."))
    }
}

fn xcodebuild<'a>(build: &'a Build, derived: &Path) -> Result<Cmd<'a>> {
    let (sh, app) = (&build.sh, &build.app);
    let project = build.root.join(PROJECT);
    ensure(
        project.is_dir(),
        "Run `cargo framer` to generate the ios/ Xcode project",
    )?;
    let settings = [
        format!("EXECUTABLE_NAME={}", app.target),
        format!("ROUGE_DISPLAY_NAME={}", app.name),
        format!(
            "MARKETING_VERSION={}.{}.{}",
            app.version.major, app.version.minor, app.version.patch
        ),
        format!("CURRENT_PROJECT_VERSION={}", build.number),
        format!("CARGO_TARGET_DIR={}", build.cargo_target.display()),
    ];
    Ok(build.isolated(cmd!(sh, "xcodebuild -quiet -project {project} -scheme RougeApp -derivedDataPath {derived} {settings...}")))
}

fn check_icon(build: &Build) -> Result<()> {
    let sh = &build.sh;
    let icon = icon(build)?;
    let properties = build
        .isolated(cmd!(
            sh,
            "sips -g pixelWidth -g pixelHeight -g hasAlpha {icon}"
        ))
        .read()?;
    ensure(
        ["pixelWidth: 1024", "pixelHeight: 1024", "hasAlpha: no"]
            .iter()
            .all(|property| properties.contains(property)),
        ICON_REQUIREMENT,
    )
}

fn icon(build: &Build) -> Result<&Path> {
    match &build.app.icon {
        Some(icon) => Ok(icon),
        None => invalid(ICON_REQUIREMENT),
    }
}
