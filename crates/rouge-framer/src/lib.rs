#![doc = include_str!("../README.md")]

mod app;
mod error;
mod packager;
mod platform;
mod secrets;
mod workflow;

use clap::{Parser, value_parser};
pub use error::PublishError;
use framer::just::{Justfile, Recipe};
use framer::toml_edit::value;
use framer::{Component, FramerError, Output, Preset, Project, Result};
pub use packager::{Action, Channel, Packager, Target};
use platform::Platform;
pub use platform::{
    Android, AndroidToolchain, AppleToolchain, Device, GitHubPages, GitHubReleases, Ios, Linux,
    Macos, Orientation, PlayInternal, TestFlight, Web,
};
use std::env;
use std::path::{Component as PathComponent, Path, PathBuf};
use std::process::ExitCode;

const ALIAS: &str = "run --quiet --locked --manifest-path .framer/Cargo.toml --";

#[derive(Parser)]
#[command(
    bin_name = "cargo rouge",
    about = "Run, package and publish this Rouge app"
)]
struct Cli {
    action: Action,
    target: Target,
    #[arg(long, env = "ROUGE_CHANNEL", value_enum, default_value_t)]
    channel: Channel,
    #[arg(long, env = "ROUGE_BUILD_NUMBER", default_value_t = 1, value_parser = value_parser!(u32).range(1..=999_999_999))]
    build_number: u32,
    #[arg(
        long,
        env = "ROUGE_IOS_DEVICE",
        help = "iOS simulator name or UDID for run"
    )]
    device: Option<String>,
}

pub fn run(project: &Project) -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args == ["--check"] {
        return project.synth();
    }
    let cli = Cli::parse();
    let root = env::var_os("FRAMER_ROOT").map_or_else(|| PathBuf::from("."), PathBuf::from);
    let packaged = project
        .get::<RougePublish>()
        .ok_or_else(|| {
            PublishError::Invalid("Apply RougePublish in .framer/src/main.rs first".into())
        })
        .and_then(|config| {
            let packager = Packager::new(config.clone(), root)
                .channel(cli.channel)
                .build_number(cli.build_number);
            cli.device
                .into_iter()
                .fold(packager, |packager, device| {
                    packager.simulator_device(device)
                })
                .run(cli.action, cli.target)
        });
    match packaged {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Clone, Debug)]
pub struct RougePublish {
    id: String,
    name: Option<String>,
    icon: Option<String>,
    package: Option<String>,
    binary: Option<String>,
    android_library: Option<String>,
    web: Option<Web>,
    linux: Option<Linux>,
    macos: Option<Macos>,
    ios: Option<Ios>,
    android: Option<Android>,
}

impl RougePublish {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
            icon: None,
            package: None,
            binary: None,
            android_library: None,
            web: None,
            linux: None,
            macos: None,
            ios: None,
            android: None,
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn icon(mut self, path: impl Into<String>) -> Self {
        self.icon = Some(path.into());
        self
    }

    pub fn package(mut self, package: impl Into<String>) -> Self {
        self.package = Some(package.into());
        self
    }

    pub fn binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = Some(binary.into());
        self
    }

    pub fn android_library(mut self, library: impl Into<String>) -> Self {
        self.android_library = Some(library.into());
        self
    }

    pub fn web(mut self, web: impl Into<Web>) -> Self {
        self.web = Some(web.into());
        self
    }

    pub fn desktop(self, destination: GitHubReleases) -> Self {
        self.linux(destination).macos(destination)
    }

    pub fn linux(mut self, linux: impl Into<Linux>) -> Self {
        self.linux = Some(linux.into());
        self
    }

    pub fn macos(mut self, macos: impl Into<Macos>) -> Self {
        self.macos = Some(macos.into());
        self
    }

    pub fn ios(mut self, ios: impl Into<Ios>) -> Self {
        self.ios = Some(ios.into());
        self
    }

    pub fn android(mut self, android: impl Into<Android>) -> Self {
        self.android = Some(android.into());
        self
    }

    fn platforms(&self) -> impl Iterator<Item = &dyn Platform> {
        let platforms: [Option<&dyn Platform>; 5] = [
            self.web.as_ref().map(|platform| platform as _),
            self.linux.as_ref().map(|platform| platform as _),
            self.macos.as_ref().map(|platform| platform as _),
            self.ios.as_ref().map(|platform| platform as _),
            self.android.as_ref().map(|platform| platform as _),
        ];
        platforms.into_iter().flatten()
    }

    fn platform(&self, target: Target) -> Option<&dyn Platform> {
        self.platforms()
            .find(|platform| platform.target() == target)
    }

    fn validate(&self) -> Result<()> {
        let invalid = |message: &str| FramerError::Invalid {
            path: ".framer/src/main.rs".into(),
            message: format!("RougePublish: {message}"),
        };
        if self.id.split('.').count() < 2
            || !self.id.split('.').all(|part| {
                part.starts_with(|c: char| c.is_ascii_lowercase())
                    && part
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            })
        {
            return Err(invalid(
                "use a lowercase reverse-DNS app ID such as com.example.myapp",
            ));
        }
        if self.platforms().next().is_none() {
            return Err(invalid("enable at least one publishing target"));
        }
        if let Some(name) = &self.name
            && (name.trim().is_empty() || name.chars().any(char::is_control))
        {
            return Err(invalid("the display name must be nonempty and on one line"));
        }
        if let Some(path) = &self.icon
            && (Path::new(path).extension().and_then(|value| value.to_str()) != Some("png")
                || !Path::new(path)
                    .components()
                    .all(|part| matches!(part, PathComponent::Normal(_))))
        {
            return Err(invalid("the icon must be a repository-relative PNG path"));
        }
        self.platforms()
            .try_for_each(|platform| platform.validate(self))
            .map_err(|message| invalid(&message))
    }
}

impl Preset for RougePublish {
    fn apply(self, project: &mut Project) {
        project.add(self);
    }
}

impl Component for RougePublish {
    fn synthesize(&self, output: &mut Output) -> Result<()> {
        self.validate()?;
        output.toml(".cargo/config.toml", "alias.rouge", value(ALIAS))?;
        output.block(
            ".gitignore",
            "rouge/publishing",
            "/.rouge/out/\n/.framer/target/\n",
        )?;
        output.managed(
            ".github/workflows/rouge-publish.yml",
            workflow::render(self),
        )?;
        output.managed("PUBLISHING.md", workflow::checklist(self))?;
        if self.ios.is_some() {
            Justfile::new("rouge/ios-sim")
                .recipe(
                    Recipe::new("ios-sim")
                        .doc("Build and launch the app in the iOS Simulator")
                        .line("cargo rouge run ios"),
                )
                .synthesize(output)?;
        }
        Ok(())
    }
}
