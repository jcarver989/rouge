use crate::prop::{IntoProp, Prop, PropFrom};
use crate::styled;
use crate::tree::ElementId;
use crate::view::{BuildCx, Div, Patches, View, div, send};
use image::{ImageError as DecodeError, load_from_memory};
use rouge_macros::component;
#[cfg(not(target_arch = "wasm32"))]
use rouge_reactive::{TaskError, spawn_blocking};
use rouge_reactive::{effect, spawn};
use rouge_render::{ImageData, ImageError};
#[cfg(not(target_arch = "wasm32"))]
use std::fs::read;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Error as IoError;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq)]
pub enum ImageSource {
    Data(ImageData),
    #[cfg(not(target_arch = "wasm32"))]
    File(PathBuf),
    Bytes(Arc<[u8]>),
}

impl PropFrom<ImageData> for ImageSource {
    fn prop_from(value: ImageData) -> Self {
        ImageSource::Data(value)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PropFrom<PathBuf> for ImageSource {
    fn prop_from(value: PathBuf) -> Self {
        ImageSource::File(value)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PropFrom<&Path> for ImageSource {
    fn prop_from(value: &Path) -> Self {
        ImageSource::File(value.to_owned())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PropFrom<&str> for ImageSource {
    fn prop_from(value: &str) -> Self {
        ImageSource::File(value.into())
    }
}

impl PropFrom<Vec<u8>> for ImageSource {
    fn prop_from(value: Vec<u8>) -> Self {
        ImageSource::Bytes(value.into())
    }
}

#[derive(Debug, Error)]
pub enum ImageLoadError {
    #[cfg(not(target_arch = "wasm32"))]
    #[error("failed to read image `{path}`")]
    Read {
        path: PathBuf,
        #[source]
        source: IoError,
    },
    #[error("failed to decode image")]
    Decode(#[from] DecodeError),
    #[error(transparent)]
    Data(#[from] ImageError),
    #[cfg(not(target_arch = "wasm32"))]
    #[error("image loading did not complete")]
    Task(#[from] TaskError),
}

pub fn decode_image(bytes: &[u8]) -> Result<ImageData, ImageLoadError> {
    let rgba = load_from_memory(bytes)?.into_rgba8();
    Ok(ImageData::rgba8(
        rgba.width(),
        rgba.height(),
        rgba.into_raw(),
    )?)
}

fn load(source: ImageSource) -> Result<ImageData, ImageLoadError> {
    match source {
        ImageSource::Data(data) => Ok(data),
        ImageSource::Bytes(bytes) => decode_image(&bytes),
        #[cfg(not(target_arch = "wasm32"))]
        ImageSource::File(path) => match read(&path) {
            Ok(bytes) => decode_image(&bytes),
            Err(source) => Err(ImageLoadError::Read { path, source }),
        },
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Fit {
    Fill,
    #[default]
    Contain,
    Cover,
    None,
}

#[derive(Default)]
pub(crate) struct ImageSpec {
    pub data: Option<ImageData>,
    pub fit: Fit,
}

type ErrorHandler = Rc<dyn Fn(&ImageLoadError)>;

pub struct Image {
    div: Div,
    source: Prop<ImageSource>,
    fit: Fit,
    on_error: Option<ErrorHandler>,
}

#[component]
pub fn image<U>(source: impl IntoProp<ImageSource, U>) -> Image {
    Image {
        div: div(),
        source: source.into_prop(),
        fit: Fit::default(),
        on_error: None,
    }
}

impl Image {
    pub fn fit(mut self, fit: Fit) -> Self {
        self.fit = fit;
        self
    }

    pub fn on_error(mut self, f: impl Fn(&ImageLoadError) + 'static) -> Self {
        self.on_error = Some(Rc::new(f));
        self
    }
}

styled!(Image => div);

fn show(patches: &Patches, id: ElementId, source: ImageSource, on_error: Option<ErrorHandler>) {
    if let ImageSource::Data(data) = source {
        return send(patches, move |cx| cx.tree.set_image(id, Some(data)));
    }
    let patches = patches.clone();
    spawn(async move {
        #[cfg(not(target_arch = "wasm32"))]
        let loaded = spawn_blocking(move || load(source))
            .await
            .unwrap_or_else(|error| Err(error.into()));
        #[cfg(target_arch = "wasm32")]
        let loaded = load(source);
        match loaded {
            Ok(data) => send(&patches, move |cx| cx.tree.set_image(id, Some(data))),
            Err(error) => {
                send(&patches, move |cx| cx.tree.set_image(id, None));
                if let Some(on_error) = on_error {
                    on_error(&error);
                }
            }
        }
    });
}

impl View for Image {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let Image {
            mut div,
            source,
            fit,
            on_error,
        } = self;
        div.spec.image = Some(ImageSpec { data: None, fit });
        let id = div.build(cx);
        let patches = cx.patches.clone();
        match source {
            Prop::Static(source) => show(&patches, id, source, on_error),
            Prop::Dynamic(compute) => {
                effect(move || show(&patches, id, compute(), on_error.clone()));
            }
        }
        id
    }
}
