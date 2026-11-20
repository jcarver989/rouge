use crate::error::{Result, ensure, invalid};
use crate::{RougePublish, Target, secrets};
use cargo_metadata::semver::Version;
use cargo_metadata::{MetadataCommand, TargetKind};
use std::path::{Path, PathBuf};

pub(crate) struct App {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) icon: Option<PathBuf>,
    pub(crate) package: String,
    pub(crate) target: String,
    pub(crate) version: Version,
    pub(crate) lock: PathBuf,
}

impl App {
    pub(crate) fn resolve(config: &RougePublish, root: &Path, target: Target) -> Result<Self> {
        let mut command = MetadataCommand::new();
        command
            .current_dir(root)
            .no_deps()
            .other_options(["--locked".to_owned(), "--offline".to_owned()]);
        for secret in secrets::all() {
            command.env_remove(secret);
        }
        let metadata = command.exec()?;
        let package = only(
            match &config.package {
                Some(name) => metadata
                    .packages
                    .iter()
                    .filter(|package| package.name.as_str() == name)
                    .collect(),
                None => metadata.workspace_default_packages(),
            },
            "Select one workspace package with RougePublish::package()",
        )?;
        let (kind, selected) = match target {
            Target::Android => (TargetKind::CDyLib, &config.android_library),
            _ => (TargetKind::Bin, &config.binary),
        };
        let cargo_target = only(
            package
                .targets
                .iter()
                .filter(|candidate| candidate.is_kind(kind.clone()))
                .filter(|candidate| selected.as_ref().is_none_or(|name| *name == candidate.name))
                .collect(),
            "Package needs one Cargo binary (web/desktop/iOS) or cdylib (Android); use binary() or android_library() to select it",
        )?;
        let version = package.version.clone();
        ensure(
            target != Target::Ios || (version.pre.is_empty() && version.build.is_empty()),
            "iOS requires a Cargo package version in MAJOR.MINOR.PATCH format, without a suffix",
        )?;
        let icon = config.icon.as_ref().map(|icon| root.join(icon));
        if let Some(icon) = &icon {
            ensure(
                icon.is_file(),
                format!("Icon does not exist: {}", icon.display()),
            )?;
        }
        Ok(Self {
            id: config.id.clone(),
            name: config
                .name
                .clone()
                .unwrap_or_else(|| package.name.to_string()),
            icon,
            package: package.name.to_string(),
            target: cargo_target.name.clone(),
            version,
            lock: metadata.workspace_root.join("Cargo.lock").into(),
        })
    }
}

fn only<T>(mut candidates: Vec<T>, message: &str) -> Result<T> {
    match (candidates.pop(), candidates.is_empty()) {
        (Some(only), true) => Ok(only),
        _ => invalid(message),
    }
}
