use serde::Serialize;
use serde::de::DeserializeOwned;
use std::env::{home_dir, var_os};
use std::fs::{create_dir_all, read_to_string, rename, write as write_file};
use std::io::{Error, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::select;
use tokio::sync::watch::{Sender, channel};
use tokio::task::{JoinHandle, spawn_blocking};
use tokio::time::sleep;

const DEBOUNCE: Duration = Duration::from_millis(400);

#[derive(Clone)]
pub struct Store<T> {
    values: Sender<T>,
}

impl<T: Serialize + DeserializeOwned + Clone + Default + Send + Sync + 'static> Store<T> {
    pub fn load(file: Option<PathBuf>, runtime: &Handle) -> (Self, JoinHandle<Result<()>>) {
        let state: T = file.as_deref().and_then(read).unwrap_or_default();
        let (values, mut changes) = channel(state);
        let flush = runtime.spawn(async move {
            let Some(file) = file else { return Ok(()) };
            while changes.changed().await.is_ok() {
                loop {
                    select! {
                        result = changes.changed() => if result.is_err() { break; },
                        () = sleep(DEBOUNCE) => break,
                    }
                }
                let state = changes.borrow_and_update().clone();
                let path = file.clone();
                spawn_blocking(move || write(&path, &state)).await??;
            }
            Ok(())
        });
        (Self { values }, flush)
    }

    pub fn state(&self) -> T {
        self.values.borrow().clone()
    }

    pub fn update(&self, change: impl FnOnce(&mut T) -> bool) {
        self.values.send_if_modified(change);
    }
}

pub fn config_dir(name: &str) -> Option<PathBuf> {
    if let Some(dir) = var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
    {
        return Some(dir.join(name.to_lowercase()));
    }
    if cfg!(target_os = "macos") {
        home_dir().map(|home| home.join("Library/Application Support").join(name))
    } else if cfg!(target_os = "windows") {
        var_os("APPDATA").map(|root| PathBuf::from(root).join(name))
    } else {
        home_dir().map(|home| home.join(".config").join(name.to_lowercase()))
    }
}

pub fn read<T: DeserializeOwned>(file: &Path) -> Option<T> {
    serde_json::from_str(&read_to_string(file).ok()?).ok()
}

pub fn write<T: Serialize>(file: &Path, value: &T) -> Result<()> {
    if let Some(dir) = file.parent() {
        create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(value).map_err(Error::other)?;
    let temporary = file.with_extension("tmp");
    write_file(&temporary, json)?;
    rename(temporary, file)
}
