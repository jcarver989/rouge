mod verify;

use crate::error::{Result, ensure, invalid};
use crate::packager::{Build, need};
use crate::platform::Platform;
use crate::platform::pinned;
use crate::secrets::{self, ANDROID, secret};
use crate::workflow::{self, Ci, PREVIEWING, ci_options};
use crate::{RougePublish, Target};
use framer::github::{Job, Step};
use std::env;
use std::path::{Path, PathBuf};
use xshell::cmd;

const SUPPORTED_SDK: u32 = 30;
const GAMES_ACTIVITY: &str = "4.4.0";
const APPCOMPAT: &str = "1.7.0";
const PLAY_TARGET_SDK: u32 = 36;
const COMPILE_SDK: u32 = 37;
const SDK_COMMAND_LINE_TOOLS: &str = "16111833";
const CHECK_PASSWORD: &str = "rouge-check-only";
const CHECK_ALIAS: &str = "rouge-check";
const JAVA: &str = "actions/setup-java@b6effb05e454b25005698d916606bdc6ffcbf961";
const SETUP_ANDROID: &str =
    "android-actions/setup-android@9fc6c4e9069bf8d3d10b2204b1fb8f6ef7065407";
const GRADLE: &str = "gradle/actions/setup-gradle@ed408507eac070d1f99cc633dbcf757c94c7933a";
const GOOGLE_AUTH: &str = "google-github-actions/auth@7c6bc770dae815cd3e89ee6cdf493a5fab2cc093";
const DOWNLOAD: &str = "actions/download-artifact@9000827ccba6bdab643e8b6fd33ac0654aef8333";
const PLAY: &str = "r0adkll/upload-google-play@e738b9dd8f2476ea806d921b64aacd24f34515a5";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayInternal;

#[derive(Clone, Debug)]
pub struct AndroidToolchain {
    ndk: String,
    build_tools: String,
    cargo_ndk: String,
    gradle: String,
    gradle_sha256: String,
    gradle_plugin: String,
    java: String,
}

impl Default for AndroidToolchain {
    fn default() -> Self {
        Self::new()
    }
}

impl AndroidToolchain {
    pub fn new() -> Self {
        Self {
            ndk: "28.2.13676358".into(),
            build_tools: "36.0.0".into(),
            cargo_ndk: "4.1.2".into(),
            gradle: "9.6.0".into(),
            gradle_sha256: "bbaeb2fef8710818cf0e261201dab964c572f92b942812df0c3620d62a529a01"
                .into(),
            gradle_plugin: "9.5.1".into(),
            java: "17".into(),
        }
    }

    pub fn ndk(mut self, version: impl Into<String>) -> Self {
        self.ndk = version.into();
        self
    }

    pub fn build_tools(mut self, version: impl Into<String>) -> Self {
        self.build_tools = version.into();
        self
    }

    pub fn cargo_ndk(mut self, version: impl Into<String>) -> Self {
        self.cargo_ndk = version.into();
        self
    }

    pub fn gradle(mut self, version: impl Into<String>, sha256: impl Into<String>) -> Self {
        self.gradle = version.into();
        self.gradle_sha256 = sha256.into();
        self
    }

    pub fn gradle_plugin(mut self, version: impl Into<String>) -> Self {
        self.gradle_plugin = version.into();
        self
    }

    pub fn java(mut self, version: impl Into<String>) -> Self {
        self.java = version.into();
        self
    }
}

#[derive(Clone, Debug)]
pub struct Android {
    min_sdk: u32,
    compile_sdk: u32,
    target_sdk: u32,
    permissions: Vec<String>,
    toolchain: AndroidToolchain,
    ci: Ci,
}

impl Android {
    pub fn new(_destination: PlayInternal) -> Self {
        Self {
            min_sdk: SUPPORTED_SDK,
            compile_sdk: COMPILE_SDK,
            target_sdk: PLAY_TARGET_SDK,
            permissions: Vec::new(),
            toolchain: AndroidToolchain::new(),
            ci: Ci::default(),
        }
    }

    pub fn min_sdk(mut self, level: u32) -> Self {
        self.min_sdk = level;
        self
    }

    pub fn compile_sdk(mut self, level: u32) -> Self {
        self.compile_sdk = level;
        self
    }

    pub fn target_sdk(mut self, level: u32) -> Self {
        self.target_sdk = level;
        self
    }

    pub fn permission(mut self, name: impl Into<String>) -> Self {
        self.permissions.push(name.into());
        self
    }

    pub fn toolchain(mut self, toolchain: AndroidToolchain) -> Self {
        self.toolchain = toolchain;
        self
    }

    fn cargo_ndk(&self, build: &Build) -> bool {
        let sh = &build.sh;
        build
            .isolated(cmd!(sh, "cargo ndk --version"))
            .quiet()
            .ignore_stderr()
            .read()
            .is_ok_and(|version| version == format!("cargo-ndk {}", self.toolchain.cargo_ndk))
    }
}

impl From<PlayInternal> for Android {
    fn from(destination: PlayInternal) -> Self {
        Self::new(destination)
    }
}

ci_options!(Android);

impl Platform for Android {
    fn target(&self) -> Target {
        Target::Android
    }

    fn job(&self, _config: &RougePublish) -> Job {
        let target = self.target();
        self.ci
            .job(target, "ubuntu-24.04", 60, workflow::checkout())
            .step(
                Step::uses(JAVA)
                    .with("distribution", "temurin")
                    .with("java-version", &self.toolchain.java),
            )
            .step(Step::uses(SETUP_ANDROID).with("cmdline-tools-version", SDK_COMMAND_LINE_TOOLS))
            .step(Step::uses(GRADLE))
            .step(workflow::rouge(
                "Install Android build tools",
                "setup",
                target,
            ))
            .step(workflow::preview_secrets(
                workflow::rouge("Build and verify Android App Bundle", "build", target),
                ANDROID,
            ))
            .step(workflow::inspect(target, "app.aab"))
            .step(workflow::artifact(
                target,
                "Save verified bundle for Play upload",
                "preview",
                "app.aab",
                PREVIEWING,
            ))
    }

    fn preview_job(&self, config: &RougePublish) -> Option<Job> {
        Some(
            Job::new("android-preview")
                .runs_on("ubuntu-24.04")
                .timeout_minutes(25)
                .needs("android")
                .permission("contents", "read")
                .permission("id-token", "write")
                .step(workflow::checkout())
                .step(
                    Step::uses(DOWNLOAD)
                        .with("name", "rouge-android-preview")
                        .with("path", ".rouge/out/android"),
                )
                .step(
                    Step::uses(GOOGLE_AUTH)
                        .id("auth")
                        .with(
                            "workload_identity_provider",
                            "${{ vars.GOOGLE_PLAY_WORKLOAD_IDENTITY_PROVIDER }}",
                        )
                        .with("service_account", "${{ vars.GOOGLE_PLAY_SERVICE_ACCOUNT }}")
                        .with("export_environment_variables", "false"),
                )
                .step(
                    Step::uses(PLAY)
                        .name("Upload verified bundle to Play internal testing")
                        .with(
                            "serviceAccountJson",
                            "${{ steps.auth.outputs.credentials_file_path }}",
                        )
                        .with("packageName", &config.id)
                        .with("releaseFiles", ".rouge/out/android/app.aab")
                        .with("tracks", "internal")
                        .with("status", "completed"),
                )
                .step(workflow::rouge(
                    "Print Google Play link",
                    "link",
                    self.target(),
                )),
        )
    }

    fn files(&self, config: &RougePublish) -> Vec<(String, String)> {
        let quoted = |value: &str| value.replace('\\', "\\\\").replace('\'', "\\'");
        let toolchain = &self.toolchain;
        let library = config.android_library.as_ref().or(config.package.as_ref());
        let package = config.package.as_ref().map_or(String::new(), |package| {
            format!(", '--package', '{}'", quoted(package))
        });
        let (icon_task, icon) = config.icon.as_ref().map_or(("", ""), |_| {
            (
                include_str!("icon.gradle"),
                " android:icon=\"@drawable/rouge_icon\"",
            )
        });
        let permissions = self
            .permissions
            .iter()
            .map(|name| format!("\n    <uses-permission android:name=\"{name}\" />"))
            .collect::<Vec<_>>()
            .concat();
        let gradle = include_str!("build.gradle")
            .replace("@@ICON@@", icon_task)
            .replace(
                "@@ICON_PATH@@",
                &quoted(config.icon.as_deref().unwrap_or_default()),
            )
            .replace("@@PLUGIN@@", &toolchain.gradle_plugin)
            .replace("@@ID@@", &config.id)
            .replace("@@COMPILE_SDK@@", &self.compile_sdk.to_string())
            .replace("@@TARGET_SDK@@", &self.target_sdk.to_string())
            .replace("@@MIN_SDK@@", &self.min_sdk.to_string())
            .replace("@@BUILD_TOOLS@@", &toolchain.build_tools)
            .replace("@@NDK@@", &toolchain.ndk)
            .replace(
                "@@NAME@@",
                &quoted(config.name.as_deref().unwrap_or("RougeApp")),
            )
            .replace("@@GAMES_ACTIVITY@@", GAMES_ACTIVITY)
            .replace("@@APPCOMPAT@@", APPCOMPAT)
            .replace("@@LIBRARY@@", &quoted(library.map_or("", String::as_str)))
            .replace("@@PACKAGE@@", &package);
        vec![
            ("android/build.gradle".to_owned(), gradle),
            (
                "android/settings.gradle".to_owned(),
                include_str!("settings.gradle").to_owned(),
            ),
            (
                "android/gradle/wrapper/gradle-wrapper.properties".to_owned(),
                format!(
                    "distributionUrl=https://services.gradle.org/distributions/gradle-{}-bin.zip\ndistributionSha256Sum={}\n",
                    toolchain.gradle, toolchain.gradle_sha256
                ),
            ),
            (
                "android/src/main/AndroidManifest.xml".to_owned(),
                include_str!("AndroidManifest.xml")
                    .replace("@@ICON@@", icon)
                    .replace("@@PERMISSIONS@@", &permissions),
            ),
            (
                format!(
                    "android/src/main/java/{}/RougeActivity.java",
                    config.id.replace('.', "/")
                ),
                include_str!("RougeActivity.java").replace("@@ID@@", &config.id),
            ),
            (
                "android/proguard-rules.pro".to_owned(),
                include_str!("proguard-rules.pro").replace("@@ID@@", &config.id),
            ),
        ]
    }

    fn ignored(&self) -> &'static str {
        "/android/.gradle/\n/android/build/\n/android/gradle/wrapper/gradle-wrapper.jar\n/android/src/*/jniLibs/\n/android/src/main/res/drawable-nodpi/rouge_icon.png\n/gha-creds-*.json\n"
    }

    fn validate(&self, _config: &RougePublish) -> Result<(), String> {
        let AndroidToolchain {
            ndk,
            build_tools,
            cargo_ndk,
            gradle,
            gradle_plugin,
            java,
            ..
        } = &self.toolchain;
        if self.min_sdk < SUPPORTED_SDK {
            return Err(format!(
                "Android::min_sdk({}) is too low; Rouge supports API level {SUPPORTED_SDK} and later",
                self.min_sdk
            ));
        }
        if self.target_sdk < self.min_sdk {
            return Err("Android::target_sdk must not be lower than min_sdk".into());
        }
        if self.compile_sdk < self.target_sdk {
            return Err("Android::compile_sdk must not be lower than target_sdk".into());
        }
        let checksum = &self.toolchain.gradle_sha256;
        if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(
                "AndroidToolchain::gradle expects the SHA-256 checksum of the Gradle distribution"
                    .into(),
            );
        }
        let named = |permission: &&String| {
            !permission.is_empty()
                && permission
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'_')
        };
        if let Some(permission) = self.permissions.iter().find(|name| !named(name)) {
            return Err(format!(
                "`{permission}` is not a permission name; use one such as android.permission.INTERNET"
            ));
        }
        match [ndk, build_tools, cargo_ndk, gradle, gradle_plugin, java]
            .into_iter()
            .find(|version| !pinned(version))
        {
            Some(version) => Err(format!("`{version}` is not a tool version")),
            None => Ok(()),
        }
    }

    fn setup(&self, build: &Build) -> Result<()> {
        let sh = &build.sh;
        need("sdkmanager")?;
        let (platform, toolchain) = (self.platform_package(), &self.toolchain);
        let (build_tools, ndk, cargo_ndk) =
            (&toolchain.build_tools, &toolchain.ndk, &toolchain.cargo_ndk);
        build
            .isolated(cmd!(
                sh,
                "sdkmanager platforms;{platform} build-tools;{build_tools} ndk;{ndk} platform-tools"
            ))
            .run()?;
        build
            .isolated(cmd!(sh, "rustup target add aarch64-linux-android"))
            .run()?;
        if !self.cargo_ndk(build) {
            build
                .isolated(cmd!(
                    sh,
                    "cargo install cargo-ndk --locked --version ={cargo_ndk}"
                ))
                .run()?;
        }
        Ok(())
    }

    fn doctor(&self, build: &Build) -> Result<()> {
        self.check_build_tools(build)?;
        if build.publishing() {
            ensure(
                self.target_sdk >= PLAY_TARGET_SDK,
                format!("Google Play requires target SDK {PLAY_TARGET_SDK} or newer"),
            )?;
            secrets::require(ANDROID)?;
        }
        Ok(())
    }

    fn build(&self, build: &Build) -> Result<()> {
        self.doctor(build)?;
        let scratch = secrets::scratch()?;
        let keystore = if build.publishing() {
            Keystore::upload(scratch.path())?
        } else {
            Keystore::check(build, scratch.path())?
        };
        let project = gradle(build, "bundleRelease", Some(&keystore))?;
        let libraries = project.join("src/release/jniLibs/arm64-v8a");
        let library = format!("lib{}.so", build.app.target.replace('-', "_"));
        ensure(
            libraries.join(library).is_file(),
            "Cargo did not build the configured Android cdylib",
        )?;
        verify::alignment(build, &self.ndk_path(&sdk()?), &libraries)?;
        let bundle = project.join("build/outputs/bundle/release/RougeApp-release.aab");
        verify::signer(build, &bundle, &keystore)?;
        let symbols =
            project.join("build/outputs/native-debug-symbols/release/native-debug-symbols.zip");
        if symbols.is_file() {
            build.copy(symbols, build.out.join("native-debug-symbols.zip"))?;
        }
        build.copy(bundle, build.out.join("app.aab"))
    }

    fn run_local(&self, build: &Build, device: Option<&str>) -> Result<()> {
        let sh = &build.sh;
        let adb = sdk()?.join("platform-tools/adb");
        let device = device.map(|serial| ["-s", serial]);
        let device = device.as_ref().map_or(&[][..], |selected| &selected[..]);
        self.check_build_tools(build)?;
        let project = gradle(build, "assembleDebug", None)?;
        let apk = project.join("build/outputs/apk/debug/RougeApp-debug.apk");
        let component = format!("{}/.RougeActivity", build.app.id);
        build
            .isolated(cmd!(sh, "{adb} {device...} install -r {apk}"))
            .run()?;
        let launch = build
            .isolated(cmd!(
                sh,
                "{adb} {device...} shell am start -W -n {component}"
            ))
            .read()?;
        println!("{launch}");
        ensure(
            launch.lines().any(|line| line.trim() == "Status: ok"),
            "Android failed to launch the app; see adb output above",
        )?;
        if env::var("ROUGE_ANDROID_LOGS").is_ok_and(|logs| logs == "1") {
            let filter = ["RustStdoutStderr:V", "AndroidRuntime:E", "*:S"];
            build
                .isolated(cmd!(sh, "{adb} {device...} logcat -v color {filter...}"))
                .run()?;
        }
        Ok(())
    }

    fn publish(&self, _build: &Build) -> Result<()> {
        invalid(
            "Android previews are uploaded by the generated workflow's Google Play action; promote a tested build to production in Play Console",
        )
    }

    fn link(&self, build: &Build) -> Result<()> {
        let id = &build.app.id;
        build.summarize_mobile(&format!("Uploaded to Google Play internal testing. [Join testing](https://play.google.com/apps/testing/{id}) when the build is available."))
    }
}

impl Android {
    fn platform_package(&self) -> String {
        if self.compile_sdk >= 37 {
            format!("android-{}.0", self.compile_sdk)
        } else {
            format!("android-{}", self.compile_sdk)
        }
    }

    fn ndk_path(&self, sdk: &Path) -> PathBuf {
        sdk.join("ndk").join(&self.toolchain.ndk)
    }

    fn check_build_tools(&self, build: &Build) -> Result<()> {
        let (sdk, toolchain) = (sdk()?, &self.toolchain);
        need("java")?;
        let platform = self.platform_package();
        let build_tools = sdk.join("build-tools").join(&toolchain.build_tools);
        [
            (
                self.ndk_path(&sdk).is_dir(),
                format!("NDK {}", toolchain.ndk),
            ),
            (
                sdk.join("platforms").join(&platform).is_dir(),
                format!("platform {platform}"),
            ),
            (
                build_tools.is_dir(),
                format!("Build Tools {}", toolchain.build_tools),
            ),
            (
                self.cargo_ndk(build),
                format!("cargo-ndk {}", toolchain.cargo_ndk),
            ),
        ]
        .into_iter()
        .try_for_each(|(installed, name)| {
            ensure(
                installed,
                format!("Install {name} with `cargo rouge setup android`"),
            )
        })
    }
}

fn gradle(build: &Build, task: &str, keystore: Option<&Keystore>) -> Result<PathBuf> {
    let (sh, app) = (&build.sh, &build.app);
    let project = build.root.join("android");
    ensure(
        project.join("build.gradle").is_file(),
        "Run `cargo framer` to generate the android/ Gradle project",
    )?;
    let wrapper = project.join("gradle/wrapper/gradle-wrapper.jar");
    if !wrapper.is_file() {
        sh.write_file(&wrapper, include_bytes!("gradle-wrapper.jar"))?;
    }
    let mut gradle = build
        .isolated(cmd!(
            sh,
            "java -Xmx64m -Xms64m -jar {wrapper} --no-daemon --project-dir {project} {task}"
        ))
        .env("APP_NAME", &app.name)
        .env("APP_TARGET", &app.target)
        .env("APP_VERSION", app.version.to_string())
        .env("ROUGE_BUILD_NUMBER", build.number.to_string())
        .env_remove("ROUGE_KEYSTORE");
    if let Some(key) = keystore {
        gradle = gradle
            .env("ROUGE_KEYSTORE", &key.path)
            .env(ANDROID[1], &key.store_password)
            .env(ANDROID[2], &key.alias)
            .env(ANDROID[3], &key.key_password);
    }
    gradle.run()?;
    Ok(project)
}

struct Keystore {
    path: PathBuf,
    store_password: String,
    alias: String,
    key_password: String,
}

impl Keystore {
    fn upload(project: &Path) -> Result<Self> {
        secrets::require(ANDROID)?;
        let path = project.join("upload.keystore");
        secrets::write(&path, secrets::decoded(ANDROID[0])?)?;
        Ok(Self {
            path,
            store_password: secret(ANDROID[1])?,
            alias: secret(ANDROID[2])?,
            key_password: secret(ANDROID[3])?,
        })
    }

    fn check(build: &Build, project: &Path) -> Result<Self> {
        let sh = &build.sh;
        let path = project.join("check-signing.p12");
        build.isolated(cmd!(sh, "keytool -genkeypair -keystore {path} -storetype PKCS12 -storepass {CHECK_PASSWORD} -alias {CHECK_ALIAS} -keyalg RSA -keysize 2048 -validity 1 -dname 'CN=Rouge package check'")).ignore_stdout().run()?;
        Ok(Self {
            path,
            store_password: CHECK_PASSWORD.into(),
            alias: CHECK_ALIAS.into(),
            key_password: CHECK_PASSWORD.into(),
        })
    }
}

fn sdk() -> Result<PathBuf> {
    match env::var_os("ANDROID_HOME") {
        Some(home) if !home.is_empty() => Ok(home.into()),
        _ => invalid("Set ANDROID_HOME to the Android SDK directory"),
    }
}
