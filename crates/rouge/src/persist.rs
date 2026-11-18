use rouge_reactive::{
    KeyedList, Scope, Signal, effect, on_cleanup, provide_context, sleep, spawn, use_context,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Error as JsonError;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::{Future, ready};
use std::hash::Hash;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;
use tokio::sync::Notify;
#[cfg(not(target_arch = "wasm32"))]
use {
    crate::platform::{Current, WinitPlatform},
    rouge_reactive::spawn_blocking,
    std::fs::{create_dir_all, read_to_string, rename, write as write_file},
    std::io::Result as IoResult,
    std::path::{Path, PathBuf},
    std::sync::mpsc::{Sender, channel},
    std::thread::{JoinHandle, spawn as spawn_thread},
    tokio::sync::oneshot::{Sender as Done, channel as done},
};

const DEBOUNCE: Duration = Duration::from_millis(400);

pub type StorageFuture<T> = Pin<Box<dyn Future<Output = T>>>;

pub trait Storage {
    fn read(&self, key: &str) -> StorageFuture<Option<String>>;
    fn write(&self, key: &str, value: String) -> StorageFuture<()>;
}

pub fn provide_storage(storage: Rc<dyn Storage>) {
    provide_context(storage);
}

pub trait Persist: Copy + 'static {
    fn load(&self, saved: &str) -> Result<(), JsonError>;
    fn save(&self) -> Result<String, JsonError>;
}

impl<T: Serialize + DeserializeOwned + 'static> Persist for Signal<T> {
    fn load(&self, saved: &str) -> Result<(), JsonError> {
        self.set(serde_json::from_str(saved)?);
        Ok(())
    }

    fn save(&self) -> Result<String, JsonError> {
        self.with(serde_json::to_string_pretty)
    }
}

impl<K, V> Persist for KeyedList<K, V>
where
    K: Eq + Hash + Clone + 'static,
    V: Serialize + DeserializeOwned + 'static,
{
    fn load(&self, saved: &str) -> Result<(), JsonError> {
        self.extend(serde_json::from_str::<Vec<V>>(saved)?);
        Ok(())
    }

    fn save(&self) -> Result<String, JsonError> {
        self.with_values(|values| serde_json::to_string_pretty(values))
    }
}

pub fn persisted<T: Persist>(key: impl Into<String>, value: T) -> T {
    let Some(storage) = use_context::<Rc<dyn Storage>>() else {
        return value;
    };
    let key = key.into();
    spawn(async move {
        if let Some(saved) = storage.read(&key).await
            && let Err(error) = value.load(&saved)
        {
            tracing::warn!(%key, %error, "ignoring saved state that no longer reads");
        }
        let unsaved = Rc::new(Cell::new(None));
        let changed = Rc::new(Notify::new());
        let (latest, notify, first) = (unsaved.clone(), changed.clone(), Cell::new(true));
        effect(move || match value.save() {
            Ok(_) if first.replace(false) => {}
            Ok(json) => {
                latest.set(Some(json));
                notify.notify_one();
            }
            Err(error) => tracing::warn!(%error, "failed to serialize state"),
        });
        let (last, closing, name) = (unsaved.clone(), storage.clone(), key.clone());
        on_cleanup(move || {
            if let Some(json) = last.take() {
                let scope = Scope::root();
                let write = closing.write(&name, json);
                scope.run(|| {
                    spawn(async move {
                        write.await;
                        scope.dispose();
                    })
                });
            }
        });
        loop {
            changed.notified().await;
            sleep(DEBOUNCE).await;
            if let Some(json) = unsaved.take() {
                storage.write(&key, json).await;
            }
        }
    });
    value
}

pub fn app_storage(name: &str) -> Rc<dyn Storage> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        match config_dir(name) {
            Some(dir) => Rc::new(FileStorage::new(dir)),
            None => Rc::new(MemoryStorage::default()),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        Rc::new(LocalStorage::new(format!("{name}/")))
    }
}

#[derive(Clone, Debug, Default)]
pub struct MemoryStorage(Rc<RefCell<HashMap<String, String>>>);

impl MemoryStorage {
    pub fn get(&self, key: &str) -> Option<String> {
        self.0.borrow().get(key).cloned()
    }

    pub fn insert(&self, key: &str, value: impl Into<String>) {
        self.0.borrow_mut().insert(key.to_owned(), value.into());
    }
}

impl Storage for MemoryStorage {
    fn read(&self, key: &str) -> StorageFuture<Option<String>> {
        Box::pin(ready(self.get(key)))
    }

    fn write(&self, key: &str, value: String) -> StorageFuture<()> {
        self.insert(key, value);
        Box::pin(ready(()))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct FileStorage {
    dir: PathBuf,
    writes: Option<Sender<(PathBuf, String, Done<()>)>>,
    writer: Option<JoinHandle<()>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl FileStorage {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let (queue, pending) = channel::<(PathBuf, String, Done<()>)>();
        let writer = spawn_thread(move || {
            for (file, json, written) in pending {
                if let Err(error) = write(&file, &json) {
                    tracing::warn!(file = %file.display(), %error, "failed to save state");
                }
                let _ = written.send(());
            }
        });
        FileStorage {
            dir: dir.into(),
            writes: Some(queue),
            writer: Some(writer),
        }
    }

    fn file(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Storage for FileStorage {
    fn read(&self, key: &str) -> StorageFuture<Option<String>> {
        let file = self.file(key);
        let read = spawn_blocking(move || read_to_string(file).ok());
        Box::pin(async move { read.await.ok().flatten() })
    }

    fn write(&self, key: &str, value: String) -> StorageFuture<()> {
        let (written, wait) = done();
        if let Some(writes) = &self.writes {
            let _ = writes.send((self.file(key), value, written));
        }
        Box::pin(async move {
            let _ = wait.await;
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for FileStorage {
    fn drop(&mut self) {
        self.writes.take();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write(file: &Path, json: &str) -> IoResult<()> {
    if let Some(dir) = file.parent() {
        create_dir_all(dir)?;
    }
    let temporary = file.with_extension("tmp");
    write_file(&temporary, json)?;
    rename(temporary, file)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn config_dir(name: &str) -> Option<PathBuf> {
    Current::config_dir(name)
}

#[cfg(target_arch = "wasm32")]
pub struct LocalStorage {
    prefix: String,
}

#[cfg(target_arch = "wasm32")]
impl LocalStorage {
    pub fn new(prefix: impl Into<String>) -> Self {
        LocalStorage {
            prefix: prefix.into(),
        }
    }

    fn item(&self, key: &str) -> Option<(web_sys::Storage, String)> {
        let storage = web_sys::window()?.local_storage().ok()??;
        Some((storage, format!("{}{key}", self.prefix)))
    }
}

#[cfg(target_arch = "wasm32")]
impl Storage for LocalStorage {
    fn read(&self, key: &str) -> StorageFuture<Option<String>> {
        let saved = self
            .item(key)
            .and_then(|(storage, name)| storage.get_item(&name).ok()?);
        Box::pin(ready(saved))
    }

    fn write(&self, key: &str, value: String) -> StorageFuture<()> {
        if let Some((storage, name)) = self.item(key) {
            let _ = storage.set_item(&name, &value);
        }
        Box::pin(ready(()))
    }
}
