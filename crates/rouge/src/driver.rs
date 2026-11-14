use crate::AppError;
use crate::platform::warn;
use crate::profile::{Profile, ProfileSettings};
use rouge_core::{Cursor, Ui, WindowCommand};
use rouge_render::{Affine, Color, Gpu, Point, Rect, Renderer, Size, Surfaces, Viewport};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Drawn {
    Presented,
    Retry,
    Skipped,
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
    let changed = surfaces.latch(Instant::now());
    surfaces.maintain();
    changed || surfaces.needs_frame()
}

pub(crate) struct Driver {
    surface: Surface<'static>,
    config: SurfaceConfiguration,
    gpu: Gpu,
    renderer: Renderer,
    ui: Ui,
    background: Color,
    frames: u64,
    profile: Option<Profile>,
    cursor: Option<Cursor>,
    text_input: Option<TextInput>,
}

impl Driver {
    pub(crate) fn new(
        gpu: Gpu,
        surface: Surface<'static>,
        (width, height): (u32, u32),
        mut ui: Ui,
        background: Color,
        present_mode: Option<PresentMode>,
        profile: &ProfileSettings,
    ) -> Result<Driver, AppError> {
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
        if let Some(mode) = present_mode {
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
        let environment = format!(
            "{:?}\n{:?} {}x{} at scale {}",
            gpu.adapter.get_info(),
            config.present_mode,
            config.width,
            config.height,
            ui.scale(),
        );
        let profile = Profile::new(&mut ui, profile, environment).unwrap_or_else(|error| {
            warn(&error);
            None
        });
        Ok(Driver {
            surface,
            config,
            gpu,
            renderer,
            ui,
            background,
            frames: 0,
            profile,
            cursor: None,
            text_input: None,
        })
    }

    pub(crate) fn ui(&mut self) -> &mut Ui {
        &mut self.ui
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn frames(&self) -> u64 {
        self.frames
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn max_dimension(&self) -> u32 {
        self.gpu.device.limits().max_texture_dimension_2d
    }

    pub(crate) fn resize(&mut self, (width, height): (u32, u32), scale: f32) {
        if width == 0 || height == 0 {
            return;
        }
        self.ui.set_device_scale(scale);
        self.ui
            .set_size(Size::new(width as f32 / scale, height as f32 / scale));
        if (width, height) != (self.config.width, self.config.height) {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.gpu.device, &self.config);
        }
    }

    pub(crate) fn wants_frame(&mut self, surfaced: bool) -> bool {
        self.ui.flush();
        self.ui.needs_frame()
            || (surfaced && self.ui.has_surfaces())
            || self.profile.as_ref().is_some_and(Profile::wants_frames)
    }

    pub(crate) fn profile_finished(&self) -> bool {
        self.profile.as_ref().is_some_and(Profile::finished)
    }

    pub(crate) fn profile_summary(&mut self) -> Option<String> {
        self.profile.take().as_ref().map(Profile::summary)
    }

    pub(crate) fn commands(&mut self) -> Vec<WindowCommand> {
        self.ui.flush();
        let mut pending = Vec::new();
        loop {
            let commands = self.ui.take_window_commands();
            if commands.is_empty() {
                return pending;
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
    ) -> Drawn {
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
            Err(drawn) => return drawn,
        };
        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let viewport = Viewport::new(self.config.width, self.config.height, self.ui.scale());
        self.ui.sync_surfaces(surfaces);
        surfaces.latch(Instant::now() + interval / 2);
        let ui_frame = self.ui.frame();
        {
            let _encode = trace_span!("encode").entered();
            self.renderer.render(
                &view,
                viewport,
                self.background,
                &ui_frame.scene,
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
        Drawn::Presented
    }

    fn acquire(&mut self) -> Result<SurfaceTexture, Drawn> {
        let _acquire = trace_span!("acquire").entered();
        match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => {
                Ok(frame)
            }
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.gpu.device, &self.config);
                Err(Drawn::Retry)
            }
            CurrentSurfaceTexture::Timeout => Err(Drawn::Retry),
            _ => Err(Drawn::Skipped),
        }
    }
}
