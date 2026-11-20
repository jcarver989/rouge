mod simulator;

use crate::error::{Result, ensure, invalid};
use crate::packager::{Build, need};
use crate::platform::apple::{Bundle, bundle_options};
use crate::platform::{Platform, apple, os_version};
use crate::secrets::{self, APPLE, APPLE_API, IOS_PROFILE, secret};
use crate::workflow::{self, Ci, PUBLISHING, ci_options};
use crate::{Channel, RougePublish, Target};
use framer::github::{Job, Step};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use xshell::cmd;

const SUPPORTED: (u32, u32) = (15, 0);
const RELEASING: &str = "${{ inputs.channel == 'production' }}";
const RUBY: &str = "ruby/setup-ruby@f9c4492e86def3e644ab32a0ef185eda5b0c20d0";
const ICON_REQUIREMENT: &str = "iOS requires an opaque 1024 × 1024 PNG icon";
const PROFILE_REQUIREMENT: &str =
    "IOS_PROFILE_BASE64 must be an App Store distribution profile for the configured app ID";

#[derive(Clone, Copy)]
struct Sdk {
    name: &'static str,
    platform: &'static str,
    triple: &'static str,
    release: bool,
}

const DEVICE: Sdk = Sdk {
    name: "iphoneos",
    platform: "iPhoneOS",
    triple: "aarch64-apple-ios",
    release: true,
};
const SIMULATOR: Sdk = Sdk {
    name: "iphonesimulator",
    platform: "iPhoneSimulator",
    triple: "aarch64-apple-ios-sim",
    release: false,
};

impl Sdk {
    fn query(self, build: &Build, query: &str) -> Result<String> {
        let (sh, sdk) = (&build.sh, self.name);
        Ok(build
            .isolated(cmd!(sh, "xcrun --sdk {sdk} {query}"))
            .read()?)
    }
}

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

    fn device_info(&self, sdk: Sdk) -> Value {
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
        let families: Vec<u32> = self
            .devices
            .iter()
            .map(|device| if *device == Device::Phone { 1 } else { 2 })
            .collect();
        json!({
            "MinimumOSVersion": self.minimum_os,
            "LSRequiresIPhoneOS": true,
            "UIDeviceFamily": families,
            "CFBundleSupportedPlatforms": [sdk.platform],
            "UILaunchScreen": {},
            "UISupportedInterfaceOrientations": named(chosen.unwrap_or(&phone)),
            "UISupportedInterfaceOrientations~ipad": named(chosen.unwrap_or(&pad)),
        })
    }
}

impl From<TestFlight> for Ios {
    fn from(destination: TestFlight) -> Self {
        Self::new(destination)
    }
}

bundle_options!(Ios);
ci_options!(Ios);

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Profile {
    entitlements: Entitlements,
    team_identifier: Vec<String>,
    application_identifier_prefix: Vec<String>,
    #[serde(default)]
    provisioned_devices: Vec<String>,
    #[serde(default)]
    provisions_all_devices: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Entitlements {
    application_identifier: String,
    #[serde(default)]
    get_task_allow: bool,
    #[serde(default)]
    beta_reports_active: bool,
}

impl Platform for Ios {
    fn target(&self) -> Target {
        Target::Ios
    }

    fn job(&self, _config: &RougePublish) -> Job {
        let target = self.target();
        let signing = |step| workflow::secrets(workflow::secrets(step, APPLE), &[IOS_PROFILE]);
        let toolchain = &self.bundle.toolchain;
        self.ci
            .job(target, "macos-26", 90, workflow::checkout())
            .step(workflow::rouge("Install signing tool", "setup", target).condition(PUBLISHING))
            .step(signing(
                workflow::rouge("Check signing setup", "doctor", target).condition(PUBLISHING),
            ))
            .step(signing(workflow::rouge(
                "Build iOS device package",
                "build",
                target,
            )))
            .step(workflow::inspect(target, "*"))
            .step(
                Step::uses(RUBY)
                    .condition(RELEASING)
                    .with("ruby-version", &toolchain.ruby),
            )
            .step(
                Step::run(format!(
                    "gem install fastlane --no-document --version {}",
                    toolchain.fastlane
                ))
                .name("Install Fastlane")
                .condition(RELEASING),
            )
            .step(workflow::secrets(
                workflow::rouge("Upload to App Store Connect", "publish", target)
                    .condition(PUBLISHING),
                APPLE_API,
            ))
    }

    fn guide(&self) -> &'static str {
        include_str!("guide.md")
    }

    fn validate(&self, config: &RougePublish) -> Result<(), String> {
        let TestFlight {
            app_store_id,
            invite_url,
        } = &self.destination;
        self.bundle.toolchain.validate()?;
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
        self.bundle.toolchain.signer(build).map(drop)
    }

    fn doctor(&self, build: &Build) -> Result<()> {
        apple::doctor(build)?;
        if build.publishing() {
            secret(IOS_PROFILE)?;
        }
        check_icon(build, DEVICE)
    }

    fn build(&self, build: &Build) -> Result<()> {
        self.doctor(build)?;
        let scratch = secrets::scratch()?;
        let bundle = scratch
            .path()
            .join(format!("Payload/{}.app", build.app.target));

        self.assemble_bundle(build, scratch.path(), &bundle, DEVICE)?;
        if build.publishing() {
            self.sign_bundle(build, scratch.path(), &bundle)?;
            package_ipa(build, scratch.path())
        } else {
            package_unsigned_app(build, &bundle)
        }
    }

    fn run_local(&self, build: &Build, device: Option<&str>) -> Result<()> {
        simulator::run(self, build, device)
    }

    fn publish(&self, build: &Build) -> Result<()> {
        let (sh, app) = (&build.sh, &build.app);
        let scratch = secrets::scratch()?;
        let key = apple::api_key(build, scratch.path())?;
        let (id, issuer, version) = (&key.id, &key.issuer, app.version.to_string());
        let package = build.out.join("app.ipa");
        let connect = format!(
            "https://appstoreconnect.apple.com/apps/{}",
            self.destination.app_store_id
        );
        if build.channel == Channel::Preview {
            cmd!(
                sh,
                "xcrun altool --upload-app --type ios --file {package} --apiKey {id} --apiIssuer {issuer}"
            )
            .env("API_PRIVATE_KEYS_DIR", &key.directory)
            .run()?;
            let invite = self
                .destination
                .invite_url
                .clone()
                .unwrap_or_else(|| format!("{connect}/testflight"));
            return build.summarize(&format!(
                "[TestFlight (once Apple finishes processing)]({invite})"
            ));
        }
        need("fastlane")?;
        let credentials = scratch.path().join("key.json");
        let bundle_id = &app.id;
        secrets::write(
            &credentials,
            json!({"key_id": id, "issuer_id": issuer, "key": key.contents}).to_string(),
        )?;
        cmd!(sh, "fastlane deliver --api_key_path {credentials} --ipa {package} --app_identifier {bundle_id} --app_version {version} --skip_metadata true --skip_screenshots true --precheck_include_in_app_purchases false --submit_for_review true --automatic_release false --force true")
            .run()?;
        build.summarize(&format!("[Check App Review status]({connect})"))
    }
}

impl Ios {
    fn assemble_bundle(
        &self,
        build: &Build,
        scratch: &Path,
        bundle: &Path,
        sdk: Sdk,
    ) -> Result<()> {
        self.compile_bundle_binary(build, bundle, sdk)?;
        let assets = self.compile_assets(build, scratch, bundle, sdk)?;
        self.write_bundle_info(build, bundle, &assets, sdk)
    }

    fn compile_bundle_binary(&self, build: &Build, bundle: &Path, sdk: Sdk) -> Result<()> {
        let sh = &build.sh;
        let binary = &build.app.target;
        let triple = sdk.triple;
        build
            .isolated(cmd!(sh, "rustup target add {triple}"))
            .run()?;
        let compiled = {
            let _deployment = sh.push_env("IPHONEOS_DEPLOYMENT_TARGET", &self.minimum_os);
            let _sdk = sh.push_env("SDKROOT", sdk.query(build, "--show-sdk-path")?);
            build.compile(triple, sdk.release)?
        };
        build.copy(compiled.join(binary), bundle.join(binary))
    }

    fn compile_assets(
        &self,
        build: &Build,
        scratch: &Path,
        bundle: &Path,
        sdk: Sdk,
    ) -> Result<PathBuf> {
        let (sh, sdk) = (&build.sh, sdk.name);
        let catalog = scratch.join("Assets.xcassets");
        let icons = catalog.join("AppIcon.appiconset");
        let assets = scratch.join("assets.plist");
        let minimum = &self.minimum_os;
        let devices = self.devices.iter().flat_map(|device| {
            let name = if *device == Device::Phone {
                "iphone"
            } else {
                "ipad"
            };
            ["--target-device", name]
        });
        build.copy(icon(build)?, icons.join("icon.png"))?;
        let author = json!({"author": "xcode", "version": 1});
        sh.write_file(
            catalog.join("Contents.json"),
            json!({"info": author}).to_string(),
        )?;
        sh.write_file(
            icons.join("Contents.json"),
            json!({
                "images": [{"filename": "icon.png", "idiom": "universal", "platform": "ios", "size": "1024x1024"}],
                "info": author,
            })
            .to_string(),
        )?;
        build.isolated(cmd!(sh, "xcrun --sdk {sdk} actool {catalog} --compile {bundle} --platform {sdk} --app-icon AppIcon {devices...} --minimum-deployment-target {minimum} --output-partial-info-plist {assets}"))
            .ignore_stdout()
            .run()?;
        Ok(assets)
    }

    fn write_bundle_info(
        &self,
        build: &Build,
        bundle: &Path,
        assets: &Path,
        sdk: Sdk,
    ) -> Result<()> {
        let sh = &build.sh;
        let sdk_version = sdk.query(build, "--show-sdk-version")?;
        let sdk_build = sdk.query(build, "--show-sdk-build-version")?;
        let xcode = build.isolated(cmd!(sh, "xcodebuild -version")).read()?;
        let mut info = json!({});
        if !sdk.release {
            apple::extend(&mut info, &self.bundle.info);
        }
        merge(&mut info, apple::info(build));
        merge(&mut info, self.device_info(sdk));
        merge(&mut info, plist::from_file(assets)?);
        merge(
            &mut info,
            json!({
                "DTPlatformName": sdk.name,
                "DTPlatformVersion": sdk_version,
                "DTSDKName": format!("{}{sdk_version}", sdk.name),
                "DTPlatformBuild": sdk_build,
                "DTSDKBuild": sdk_build,
                "DTXcodeBuild": xcode.rsplit(' ').next(),
            }),
        );
        if sdk.release {
            apple::extend(&mut info, &self.bundle.info);
        }
        plist::to_file_xml(bundle.join("Info.plist"), &info)?;
        Ok(())
    }

    fn sign_bundle(&self, build: &Build, scratch: &Path, bundle: &Path) -> Result<()> {
        let sh = &build.sh;
        let embedded = bundle.join("embedded.mobileprovision");
        let decoded = scratch.join("profile.plist");
        let entitlements = scratch.join("entitlements.plist");
        let signer = self.bundle.toolchain.signer(build)?;
        secrets::write(&embedded, secrets::decoded(IOS_PROFILE)?)?;
        cmd!(sh, "security cms -D -i {embedded} -o {decoded}").run()?;
        let mut granted = self::entitlements(&plist::from_file(&decoded)?, &build.app.id)?;
        apple::extend(&mut granted, &self.bundle.entitlements);
        plist::to_file_xml(&entitlements, &granted)?;
        let entitled = [Path::new("--entitlements-xml-file"), &entitlements];
        apple::sign(build, scratch, &signer, &entitled, bundle)
    }
}

fn package_ipa(build: &Build, scratch: &Path) -> Result<()> {
    let (sh, out) = (&build.sh, &build.out);
    let _payload = sh.push_dir(scratch);
    cmd!(sh, "/usr/bin/zip -q -r {out}/app.ipa Payload").run()?;
    Ok(())
}

fn package_unsigned_app(build: &Build, bundle: &Path) -> Result<()> {
    let (sh, out) = (&build.sh, &build.out);
    cmd!(
        sh,
        "ditto -c -k --keepParent {bundle} {out}/unsigned-app.zip"
    )
    .run()?;
    Ok(())
}

fn check_icon(build: &Build, sdk: Sdk) -> Result<()> {
    let (sh, sdk) = (&build.sh, sdk.name);
    let icon = icon(build)?;
    build
        .isolated(cmd!(sh, "xcrun --sdk {sdk} --find actool"))
        .ignore_stdout()
        .run()?;
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

fn merge(info: &mut Value, extra: Value) {
    if let (Some(info), Value::Object(extra)) = (info.as_object_mut(), extra) {
        info.extend(extra);
    }
}

fn entitlements(profile: &Profile, id: &str) -> Result<Value> {
    let granted = &profile.entitlements;
    let (Some(team), Some(prefix)) = (
        profile.team_identifier.first(),
        profile.application_identifier_prefix.first(),
    ) else {
        return invalid(PROFILE_REQUIREMENT);
    };
    let application = format!("{prefix}.{id}");
    ensure(
        granted.application_identifier == application
            && !granted.get_task_allow
            && profile.provisioned_devices.is_empty()
            && !profile.provisions_all_devices,
        PROFILE_REQUIREMENT,
    )?;
    let mut entitlements = json!({
        "application-identifier": application,
        "com.apple.developer.team-identifier": team,
        "get-task-allow": false,
    });
    if granted.beta_reports_active {
        entitlements["beta-reports-active"] = true.into();
    }
    Ok(entitlements)
}
