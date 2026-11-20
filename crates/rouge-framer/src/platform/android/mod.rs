use crate::error::{Result, ensure, invalid};
use crate::packager::{Build, need};
use crate::platform::Platform;
use crate::platform::pinned;
use crate::secrets::{self, ANDROID, PLAY_ACCOUNT, secret};
use crate::workflow::{self, Ci, PUBLISHING, ci_options};
use crate::{Channel, RougePublish, Target};
use framer::github::{Job, Step};
use std::env;
use std::path::{Path, PathBuf};
use xshell::cmd;

const SUPPORTED_SDK: u32 = 28;
const CHECK_PASSWORD: &str = "rouge-check-only";
const CHECK_ALIAS: &str = "rouge-check";
const JAVA: &str = "actions/setup-java@b6effb05e454b25005698d916606bdc6ffcbf961";
const SETUP_ANDROID: &str =
    "android-actions/setup-android@9fc6c4e9069bf8d3d10b2204b1fb8f6ef7065407";
const GRADLE: &str = "gradle/actions/setup-gradle@ed408507eac070d1f99cc633dbcf757c94c7933a";
const PLAY: &str = "r0adkll/upload-google-play@e738b9dd8f2476ea806d921b64aacd24f34515a5";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayInternal;

#[derive(Clone, Debug)]
pub struct AndroidToolchain {
    ndk: String,
    build_tools: String,
    cargo_ndk: String,
    gradle: String,
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
            gradle: "8.13".into(),
            gradle_plugin: "8.11.1".into(),
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

    pub fn gradle(mut self, version: impl Into<String>) -> Self {
        self.gradle = version.into();
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
    target_sdk: u32,
    permissions: Vec<String>,
    toolchain: AndroidToolchain,
    ci: Ci,
}

impl Android {
    pub fn new(_destination: PlayInternal) -> Self {
        Self {
            min_sdk: SUPPORTED_SDK,
            target_sdk: 36,
            permissions: Vec::new(),
            toolchain: AndroidToolchain::new(),
            ci: Ci::default(),
        }
    }

    pub fn min_sdk(mut self, level: u32) -> Self {
        self.min_sdk = level;
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
        cmd!(sh, "cargo ndk --version")
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

    fn job(&self, config: &RougePublish) -> Job {
        let target = self.target();
        let toolchain = &self.toolchain;
        self.ci
            .job(target, "ubuntu-24.04", 60, workflow::checkout())
            .step(workflow::secrets(
                workflow::rouge("Check Play credentials", "doctor", target).condition(PUBLISHING),
                &[ANDROID, &[PLAY_ACCOUNT]].concat(),
            ))
            .step(
                Step::uses(JAVA)
                    .with("distribution", "temurin")
                    .with("java-version", &toolchain.java),
            )
            .step(Step::uses(SETUP_ANDROID))
            .step(Step::uses(GRADLE).with("gradle-version", &toolchain.gradle))
            .step(
                Step::run("sudo apt-get update && sudo apt-get install -y imagemagick")
                    .name("Install ImageMagick"),
            )
            .step(workflow::rouge(
                "Install Android build tools",
                "setup",
                target,
            ))
            .step(workflow::secrets(
                workflow::rouge("Build signed Android App Bundle", "build", target),
                ANDROID,
            ))
            .step(workflow::inspect(target, "app.aab"))
            .step(
                Step::uses(PLAY)
                    .name("Upload Android App Bundle")
                    .condition(PUBLISHING)
                    .with(
                        "serviceAccountJsonPlainText",
                        format!("${{{{ secrets.{PLAY_ACCOUNT} }}}}"),
                    )
                    .with("packageName", &config.id)
                    .with("releaseFiles", ".rouge/out/android/app.aab")
                    .with(
                        "tracks",
                        "${{ inputs.channel == 'production' && 'production' || 'internal' }}",
                    )
                    .with("status", "completed"),
            )
            .step(workflow::rouge("Print Google Play link", "link", target).condition(PUBLISHING))
    }

    fn guide(&self) -> &'static str {
        include_str!("guide.md")
    }

    fn requirements(&self) -> String {
        let AndroidToolchain {
            ndk,
            build_tools,
            cargo_ndk,
            gradle,
            java,
            ..
        } = &self.toolchain;
        format!(
            " Needs JDK {java}, Gradle {gradle}, SDK platform {}, build-tools {build_tools}, NDK `{ndk}`, `cargo-ndk` {cargo_ndk}, and `ANDROID_HOME` set to the SDK; `cargo rouge setup android` installs the SDK packages and `cargo-ndk`.",
            self.target_sdk
        )
    }

    fn validate(&self, _config: &RougePublish) -> Result<(), String> {
        let AndroidToolchain {
            ndk,
            build_tools,
            cargo_ndk,
            gradle,
            gradle_plugin,
            java,
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
        let (sdk, toolchain) = (self.target_sdk.to_string(), &self.toolchain);
        let (build_tools, ndk, cargo_ndk) =
            (&toolchain.build_tools, &toolchain.ndk, &toolchain.cargo_ndk);
        cmd!(
            sh,
            "sdkmanager platforms;android-{sdk} build-tools;{build_tools} ndk;{ndk}"
        )
        .run()?;
        cmd!(sh, "rustup target add aarch64-linux-android").run()?;
        if !self.cargo_ndk(build) {
            cmd!(
                sh,
                "cargo install cargo-ndk --locked --version ={cargo_ndk}"
            )
            .run()?;
        }
        Ok(())
    }

    fn doctor(&self, build: &Build) -> Result<()> {
        sdk()?;
        if build.publishing() {
            secrets::require(ANDROID)?;
            secret(PLAY_ACCOUNT)?;
        }
        Ok(())
    }

    fn build(&self, build: &Build) -> Result<()> {
        let (ndk, image_tool) = self.check_build_tools(build)?;
        let scratch = build.sh.create_temp_dir()?;
        let project = scratch.path();
        let main = project.join("src/main");

        let keystore = prepare_keystore(build, project)?;
        self.compile_library(build, &ndk, &main.join("jniLibs"))?;
        self.write_gradle_project(build, project, &main, image_tool)?;
        self.bundle_release(build, project, &keystore)?;
        copy_and_verify_bundle(build, project)
    }

    fn publish(&self, _build: &Build) -> Result<()> {
        invalid("Android is uploaded by the workflow's Google Play action")
    }

    fn link(&self, build: &Build) -> Result<()> {
        let id = &build.app.id;
        build.summarize(&if build.channel == Channel::Preview {
            format!("[Join Google Play internal testing](https://play.google.com/apps/testing/{id})")
        } else {
            format!(
                "[Open Google Play (once available)](https://play.google.com/store/apps/details?id={id})"
            )
        })
    }
}

impl Android {
    fn check_build_tools(&self, build: &Build) -> Result<(PathBuf, Option<&'static str>)> {
        let toolchain = &self.toolchain;
        let sdk = sdk()?;
        let ndk = sdk.join("ndk").join(&toolchain.ndk);
        let image_tool = ["magick", "convert"]
            .into_iter()
            .find(|tool| need(tool).is_ok());
        ["gradle", "keytool", "jarsigner"]
            .into_iter()
            .try_for_each(need)?;
        ensure(
            ndk.is_dir(),
            format!("Install NDK {} with sdkmanager", toolchain.ndk),
        )?;
        let target_sdk = self.target_sdk;
        ensure(
            sdk.join(format!("platforms/android-{target_sdk}")).is_dir(),
            format!("Install platform android-{target_sdk} with sdkmanager"),
        )?;
        ensure(
            self.cargo_ndk(build),
            format!("Install cargo-ndk {}", toolchain.cargo_ndk),
        )?;
        Ok((ndk, image_tool))
    }

    fn compile_library(&self, build: &Build, ndk: &Path, libraries: &Path) -> Result<()> {
        let (sh, app) = (&build.sh, &build.app);
        let package = &app.package;
        let min_sdk = self.min_sdk.to_string();
        build
            .isolated(cmd!(sh, "cargo ndk -t arm64-v8a -p {min_sdk} -o {libraries} build --locked --release --lib --package {package}"))
            .env("ANDROID_NDK_HOME", ndk)
            .run()?;
        let library = format!("arm64-v8a/lib{}.so", app.target.replace('-', "_"));
        ensure(
            libraries.join(library).is_file(),
            "Cargo did not build the configured Android cdylib",
        )
    }

    fn write_gradle_project(
        &self,
        build: &Build,
        project: &Path,
        main: &Path,
        image_tool: Option<&str>,
    ) -> Result<()> {
        let sh = &build.sh;
        let icon = package_icon(build, main, image_tool)?;
        sh.write_file(
            project.join("settings.gradle"),
            include_str!("settings.gradle"),
        )?;
        sh.write_file(
            project.join("build.gradle"),
            format!(
                "plugins {{ id 'com.android.application' version '{}' }}\n{}",
                self.toolchain.gradle_plugin,
                include_str!("build.gradle")
            ),
        )?;
        let permissions = self
            .permissions
            .iter()
            .map(|name| format!("\n    <uses-permission android:name=\"{name}\" />"))
            .collect::<Vec<_>>()
            .concat();
        sh.write_file(
            main.join("AndroidManifest.xml"),
            include_str!("AndroidManifest.xml")
                .replace("@@ICON@@", icon)
                .replace("@@PERMISSIONS@@", &permissions),
        )?;
        Ok(())
    }

    fn bundle_release(&self, build: &Build, project: &Path, keystore: &Keystore) -> Result<()> {
        let (sh, app) = (&build.sh, &build.app);
        cmd!(
            sh,
            "gradle --no-daemon --project-dir {project} bundleRelease"
        )
        .env("APP_ID", &app.id)
        .env("APP_NAME", &app.name)
        .env("APP_TARGET", &app.target)
        .env("APP_VERSION", app.version.to_string())
        .env("ROUGE_BUILD_NUMBER", build.number.to_string())
        .env("ROUGE_MIN_SDK", self.min_sdk.to_string())
        .env("ROUGE_TARGET_SDK", self.target_sdk.to_string())
        .env("ROUGE_KEYSTORE", &keystore.path)
        .env(ANDROID[1], &keystore.store_password)
        .env(ANDROID[2], &keystore.alias)
        .env(ANDROID[3], &keystore.key_password)
        .run()?;
        Ok(())
    }
}

struct Keystore {
    path: PathBuf,
    store_password: String,
    alias: String,
    key_password: String,
}

fn prepare_keystore(build: &Build, project: &Path) -> Result<Keystore> {
    let sh = &build.sh;
    let path = project.join("upload.p12");
    let [store_password, alias, key_password] = if build.publishing() {
        secrets::require(ANDROID)?;
        sh.write_file(&path, secrets::decoded(ANDROID[0])?)?;
        [
            secret(ANDROID[1])?,
            secret(ANDROID[2])?,
            secret(ANDROID[3])?,
        ]
    } else {
        cmd!(sh, "keytool -genkeypair -keystore {path} -storetype PKCS12 -storepass {CHECK_PASSWORD} -alias {CHECK_ALIAS} -keyalg RSA -keysize 2048 -validity 1 -dname 'CN=Rouge package check'")
            .ignore_stdout()
            .run()?;
        [CHECK_PASSWORD, CHECK_ALIAS, CHECK_PASSWORD].map(String::from)
    };
    Ok(Keystore {
        path,
        store_password,
        alias,
        key_password,
    })
}

fn package_icon(build: &Build, main: &Path, image_tool: Option<&str>) -> Result<&'static str> {
    let Some(source) = &build.app.icon else {
        return Ok("");
    };
    let Some(image_tool) = image_tool else {
        return invalid("Install ImageMagick (magick or convert) to package the icon");
    };
    let sh = &build.sh;
    let drawable = sh.create_dir(main.join("res/drawable"))?;
    cmd!(
        sh,
        "{image_tool} {source} -resize 192x192 {drawable}/rouge_icon.png"
    )
    .run()?;
    Ok(" android:icon=\"@drawable/rouge_icon\"")
}

fn copy_and_verify_bundle(build: &Build, project: &Path) -> Result<()> {
    let sh = &build.sh;
    let bundle = build.out.join("app.aab");
    build.copy(
        project.join("build/outputs/bundle/release/RougeApp-release.aab"),
        &bundle,
    )?;
    cmd!(sh, "jarsigner -verify {bundle}").run()?;
    Ok(())
}

fn sdk() -> Result<PathBuf> {
    match env::var_os("ANDROID_HOME") {
        Some(home) if !home.is_empty() => Ok(home.into()),
        _ => invalid("Set ANDROID_HOME to the Android SDK, then see PUBLISHING.md"),
    }
}
