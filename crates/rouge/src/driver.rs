use crate::AppError;
use crate::platform::warn;
use crate::profile::{Profile, ProfileSettings};
use rouge_core::{AnyView, Appearance, Clipboard, Cursor, Menu, Ui, Wake, WindowCommand};
use rouge_render::{Affine, Gpu, Point, Rect, Renderer, Size, Surfaces, Viewport};
use std::borrow::Cow;
use std::time::Duration;
use tracing::trace_span;
use web_time::Instant;
use wgpu::{
    CurrentSurfaceTexture, Instance, PresentMode, Surface, SurfaceConfiguration, SurfaceTarget,
    SurfaceTexture, TextureViewDescriptor,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TextInput {
    Off,
    On { area: Option<Rect> },
}

#[derive(Debug, Default)]
pub(crate) struct Changes {
    pub(crate) cursor: Option<Cursor>,
    pub(crate) text_input: Option<TextInput>,
}

pub(crate) async fn connect(
    shared: &mut Option<(Instance, Gpu)>,
    instance: impl FnOnce() -> Instance,
    target: impl Into<SurfaceTarget<'static>>,
) -> Result<(Gpu, Surface<'static>), AppError> {
    let instance = shared
        .as_ref()
        .map_or_else(instance, |(instance, _)| instance.clone());
    let surface = instance.create_surface(target)?;
    let gpu = if let Some((_, gpu)) = shared {
        gpu.clone()
    } else {
        let gpu = Gpu::new(instance.clone(), Some(&surface)).await?;
        shared.insert((instance, gpu)).1.clone()
    };
    Ok((gpu, surface))
}

pub(crate) fn latch<'a>(
    surfaces: &mut Surfaces,
    drivers: impl IntoIterator<Item = &'a mut Driver>,
) -> bool {
    for driver in drivers {
        driver.ui.sync_surfaces(surfaces);
    }
    let changed = surfaces.latch();
    surfaces.maintain();
    changed || surfaces.needs_frame()
}

pub(crate) struct Setup<'a> {
    pub(crate) pixels: (u32, u32),
    pub(crate) scale: f32,
    pub(crate) appearance: Appearance,
    pub(crate) menus: &'a [Menu],
    pub(crate) fonts: &'a [Cow<'static, [u8]>],
    pub(crate) profile: &'a ProfileSettings,
}

pub(crate) struct Driver {
    surface: Surface<'static>,
    config: SurfaceConfiguration,
    gpu: Gpu,
    renderer: Renderer,
    ui: Ui,
    frames: u64,
    exit_after_frames: Option<u64>,
    finished: bool,
    profile: Option<Profile>,
    cursor: Option<Cursor>,
    text_input: Option<TextInput>,
}

impl Driver {
    pub(crate) fn open(
        gpu: Gpu,
        surface: Surface<'static>,
        setup: Setup<'_>,
        clipboard: impl Clipboard + 'static,
        root: impl FnOnce() -> AnyView + 'static,
    ) -> Result<Driver, AppError> {
        let Setup {
            pixels: (width, height),
            scale,
            appearance,
            menus,
            fonts,
            profile,
        } = setup;
        let logical = Size::new(width as f32 / scale, height as f32 / scale);
        let mut ui = Ui::with_fonts(logical, fonts.iter().cloned(), root);
        ui.set_device_scale(scale);
        ui.set_appearance(appearance);
        ui.set_clipboard(clipboard);
        ui.bind_menus(menus);
        let capabilities = surface.get_capabilities(&gpu.adapter);
        let mut config = surface
            .get_default_config(&gpu.adapter, width.max(1), height.max(1))
            .ok_or(AppError::UnsupportedSurface)?;
        if let Some(linear) = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
        {
            config.format = linear;
        }
        config.present_mode = PresentMode::AutoVsync;
        if let Some(mode) = profile.present_mode {
            if capabilities.present_modes.contains(&mode)
                || matches!(mode, PresentMode::AutoVsync | PresentMode::AutoNoVsync)
            {
                config.present_mode = mode;
            } else {
                warn(&format!(
                    "present mode {mode:?} is not supported here ({:?})",
                    capabilities.present_modes
                ));
            }
        }
        surface.configure(&gpu.device, &config);
        let renderer = Renderer::new(&gpu.device, &gpu.queue, config.format);
        let environment = || {
            format!(
                "{:?}\n{:?} {}x{} at scale {}",
                gpu.adapter.get_info(),
                config.present_mode,
                config.width,
                config.height,
                ui.scale(),
            )
        };
        let exit_after_frames = profile.exit_after_frames;
        let stats = ui.stats();
        let profile = Profile::new(stats, profile, environment).unwrap_or_else(|error| {
            warn(&error);
            None
        });
        Ok(Driver {
            surface,
            config,
            gpu,
            renderer,
            ui,
            frames: 0,
            exit_after_frames,
            finished: false,
            profile,
            cursor: None,
            text_input: None,
        })
    }

    pub(crate) fn ui(&mut self) -> &mut Ui {
        &mut self.ui
    }

    pub(crate) fn resize(&mut self, (width, height): (u32, u32), scale: f32) -> Option<(u32, u32)> {
        let max = self.gpu.device.limits().max_texture_dimension_2d;
        let (width, height) = (width.min(max), height.min(max));
        if width == 0 || height == 0 {
            return None;
        }
        self.ui.set_device_scale(scale);
        self.ui
            .set_size(Size::new(width as f32 / scale, height as f32 / scale));
        if (width, height) != (self.config.width, self.config.height) {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.gpu.device, &self.config);
        }
        Some((width, height))
    }

    pub(crate) fn frame_due(&mut self, surfaced: bool) -> Option<Instant> {
        self.ui.flush();
        let now = Instant::now();
        if (surfaced && self.ui.has_surfaces())
            || self.profile.as_ref().is_some_and(Profile::wants_frames)
        {
            return Some(now);
        }
        match self.ui.next_frame() {
            Wake::Idle => None,
            Wake::Now => Some(now),
            Wake::At(at) => Some(now + Duration::from_secs_f64((at - self.ui.time()).max(0.0))),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn finished(&self) -> bool {
        self.finished
    }

    pub(crate) fn commands(&mut self, pending: &mut Vec<WindowCommand>) {
        self.ui.flush();
        loop {
            let commands = self.ui.take_window_commands();
            if commands.is_empty() {
                return;
            }
            for command in commands {
                match command {
                    WindowCommand::Menu(item) => {
                        item.dispatch(&mut self.ui);
                    }
                    command => pending.push(command),
                }
            }
        }
    }

    pub(crate) fn changes(&mut self) -> Changes {
        let mut changes = Changes::default();
        let cursor = self.ui.cursor();
        if self.cursor != Some(cursor) {
            self.cursor = Some(cursor);
            changes.cursor = Some(cursor);
        }
        let text_input = if self.ui.accepts_text() {
            let scale = self.ui.scale();
            TextInput::On {
                area: self
                    .ui
                    .text_cursor_area()
                    .map(|area| Affine::scale(scale, scale).apply_rect(&area)),
            }
        } else {
            TextInput::Off
        };
        if self.text_input != Some(text_input) {
            self.text_input = Some(text_input);
            changes.text_input = Some(text_input);
        }
        changes
    }

    pub(crate) fn draw(
        &mut self,
        surfaces: &mut Surfaces,
        interval: Duration,
        before_present: impl FnOnce(),
    ) -> bool {
        if let Some(profile) = &mut self.profile {
            let scale = self.ui.scale();
            let center = Point::new(
                self.config.width as f32 / scale / 2.0,
                self.config.height as f32 / scale / 2.0,
            );
            profile.before_update(&mut self.ui, center);
        }
        self.ui.update();
        let frame = match self.acquire() {
            Ok(frame) => frame,
            Err(retry) => return retry,
        };
        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let viewport = Viewport::new(self.config.width, self.config.height, self.ui.scale());
        self.ui.sync_surfaces(surfaces);
        surfaces.latch();
        let clear = self.ui.clear_color();
        let ui_frame = self.ui.frame(surfaces);
        {
            let _encode = trace_span!("encode").entered();
            self.renderer.render(
                &view,
                viewport,
                clear,
                ui_frame.scene,
                ui_frame.atlas,
                surfaces,
            );
        }
        {
            let _present = trace_span!("present").entered();
            before_present();
            self.gpu.queue.present(frame);
        }
        surfaces.presented(Instant::now(), interval);
        surfaces.maintain();
        if let Some(profile) = &mut self.profile {
            profile.after_frame(&mut self.ui);
        }
        self.frames += 1;
        if self.profile.as_ref().is_some_and(Profile::finished)
            || self.exit_after_frames.is_some_and(|n| self.frames >= n)
        {
            self.finished = true;
            self.profile = None;
        }
        false
    }

    fn acquire(&mut self) -> Result<SurfaceTexture, bool> {
        let _acquire = trace_span!("acquire").entered();
        match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => {
                Ok(frame)
            }
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.gpu.device, &self.config);
                Err(true)
            }
            CurrentSurfaceTexture::Timeout => Err(true),
            _ => Err(false),
        }
    }
}
