use crate::platform;
use rouge_core::{AnyView, Decorations, Menu, View};
use rouge_render::{GpuError, Size};
use std::borrow::Cow;
use thiserror::Error;
#[cfg(feature = "tokio-rt")]
use tokio::runtime::Handle;
use wgpu::CreateSurfaceError;
#[cfg(not(target_arch = "wasm32"))]
use winit::error::{EventLoopError, OsError};

#[derive(Clone, Debug)]
pub struct WindowOptions {
    pub(crate) title: String,
    pub(crate) size: Size,
    pub(crate) resizable: bool,
    pub(crate) min_size: Option<Size>,
    pub(crate) maximized: bool,
    pub(crate) decorations: Decorations,
    pub(crate) canvas: Option<String>,
}

impl Default for WindowOptions {
    fn default() -> Self {
        WindowOptions {
            title: "Rouge".into(),
            size: Size::new(800.0, 600.0),
            resizable: true,
            min_size: None,
            maximized: false,
            decorations: Decorations::Server,
            canvas: None,
        }
    }
}

impl WindowOptions {
    pub fn min_size(mut self, width: f32, height: f32) -> Self {
        self.min_size = Some(Size::new(width, height));
        self
    }

    pub fn maximized(mut self, maximized: bool) -> Self {
        self.maximized = maximized;
        self
    }

    pub fn decorations(mut self, decorations: Decorations) -> Self {
        self.decorations = decorations;
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = Size::new(width, height);
        self
    }

    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    pub fn canvas(mut self, id: impl Into<String>) -> Self {
        self.canvas = Some(id.into());
        self
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    #[cfg(not(target_arch = "wasm32"))]
    #[error("event loop error: {0}")]
    EventLoop(#[from] EventLoopError),
    #[cfg(not(target_arch = "wasm32"))]
    #[error("failed to create window: {0}")]
    Window(#[from] OsError),
    #[cfg(target_arch = "wasm32")]
    #[error("browser operation failed: {0}")]
    Browser(String),
    #[error("failed to create rendering surface: {0}")]
    Surface(#[from] CreateSurfaceError),
    #[error("the GPU adapter cannot present to the window surface")]
    UnsupportedSurface,
    #[cfg(target_os = "macos")]
    #[error("failed to create application menu: {0}")]
    Menu(#[from] muda::Error),
    #[error(transparent)]
    Gpu(#[from] GpuError),
}

pub(crate) type RootFn = Box<dyn FnOnce() -> AnyView>;

#[derive(Default)]
pub struct App {
    pub(crate) windows: Vec<(WindowOptions, RootFn)>,
    pub(crate) menus: Vec<Menu>,
    pub(crate) fonts: Vec<Cow<'static, [u8]>>,
    #[cfg(feature = "tokio-rt")]
    pub(crate) tokio: Option<Handle>,
}

impl App {
    pub fn menus(mut self, menus: Vec<Menu>) -> Self {
        self.menus = menus;
        self
    }

    pub fn new() -> App {
        App::default()
    }

    pub fn fonts<T: Into<Cow<'static, [u8]>>>(
        mut self,
        fonts: impl IntoIterator<Item = T>,
    ) -> Self {
        self.fonts.extend(fonts.into_iter().map(Into::into));
        self
    }

    #[cfg(feature = "tokio-rt")]
    pub fn tokio(mut self, handle: Handle) -> Self {
        self.tokio = Some(handle);
        self
    }

    pub fn window<T: View>(
        mut self,
        options: WindowOptions,
        root: impl FnOnce() -> T + 'static,
    ) -> Self {
        self.windows
            .push((options, Box::new(move || root().into_any())));
        self
    }

    pub fn run(self) -> Result<(), AppError> {
        platform::run(self)
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn mount(self) -> Result<platform::web::Mounted, AppError> {
        platform::web::mount(self).await
    }
}
