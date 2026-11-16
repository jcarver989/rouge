use crate::AppError;
use crate::platform::warn;
use crate::profile::{Profile, ProfileSettings};
use rouge_core::{
    AnyView, Appearance, Clipboard, CompositorActor, CompositorEvent, Cursor, Mailbox, Menu,
    Message, Ui, Wake, WindowCommand, WindowConfig, WindowId,
};
use rouge_render::{Affine, Gpu, Point, Rect, Size, Surfaces, Target};
use std::borrow::Cow;
use std::sync::mpsc::Sender;
use std::time::Duration;
use web_time::Instant;
use wgpu::{Instance, PresentMode, Surface, SurfaceTarget};

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

pub(crate) struct Setup<'a> {
    pub(crate) pixels: (u32, u32),
    pub(crate) scale: f32,
    pub(crate) interval: Duration,
    pub(crate) epoch: Instant,
    pub(crate) appearance: Appearance,
    pub(crate) menus: &'a [Menu],
    pub(crate) fonts: &'a [Cow<'static, [u8]>],
    pub(crate) profile: &'a ProfileSettings,
}

pub(crate) struct Inline {
    actor: CompositorActor,
    mailbox: Mailbox,
    epoch: Instant,
}

impl Inline {
    pub(crate) fn new(
        gpu: &Gpu,
        mailbox: Mailbox,
        epoch: Instant,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let surfaces = Surfaces::new(gpu).with_waker(wake);
        Inline {
            actor: CompositorActor::new(gpu.clone(), surfaces),
            mailbox,
            epoch,
        }
    }

    pub(crate) fn turn(&mut self) -> Option<Instant> {
        let now = self.epoch.elapsed().as_secs_f64();
        let _ = self.mailbox.turn(&mut self.actor, now);
        self.next_wake()
    }

    pub(crate) fn next_wake(&mut self) -> Option<Instant> {
        let now = self.epoch.elapsed().as_secs_f64();
        let due = self.actor.next_wake(now)?;
        Some(Instant::now() + Duration::from_secs_f64((due - now).max(0.0)))
    }
}

pub(crate) struct Driver {
    window: WindowId,
    messages: Sender<Message>,
    config: WindowConfig,
    ui: Ui,
    token: bool,
    max_texture: u32,
    frames: u64,
    exit_after_frames: Option<u64>,
    finished: bool,
    profile: Option<Profile>,
    cursor: Option<Cursor>,
    text_input: Option<TextInput>,
}

impl Driver {
    pub(crate) fn open(
        window: WindowId,
        gpu: &Gpu,
        surface: Surface<'static>,
        setup: Setup<'_>,
        messages: Sender<Message>,
        clipboard: impl Clipboard + 'static,
        root: impl FnOnce() -> AnyView + 'static,
    ) -> Result<(Driver, Target), AppError> {
        let Setup {
            pixels: (width, height),
            scale,
            interval,
            epoch,
            appearance,
            menus,
            fonts,
            profile,
        } = setup;
        let logical = Size::new(width as f32 / scale, height as f32 / scale);
        let mut ui = Ui::with_fonts(logical, fonts.iter().cloned(), root);
        ui.set_epoch(epoch);
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
        let driver = Driver {
            window,
            messages,
            config: WindowConfig {
                width: config.width,
                height: config.height,
                interval,
                visible: true,
            },
            ui,
            token: true,
            max_texture: gpu.device.limits().max_texture_dimension_2d,
            frames: 0,
            exit_after_frames,
            finished: false,
            profile,
            cursor: None,
            text_input: None,
        };
        Ok((driver, Target::swapchain(gpu, surface, config)))
    }

    pub(crate) fn attach(&self, target: Target) {
        self.send(Message::Open(self.window, target, self.config));
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn close(&self) {
        self.send(Message::Close(self.window));
    }

    fn send(&self, message: Message) {
        let _ = self.messages.send(message);
    }

    pub(crate) fn configure(&mut self, change: impl FnOnce(&mut WindowConfig)) {
        let before = self.config;
        change(&mut self.config);
        if self.config != before {
            self.send(Message::Configure(self.window, self.config));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn interval(&self) -> Duration {
        self.config.interval
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn window(&self) -> WindowId {
        self.window
    }

    pub(crate) fn ui(&mut self) -> &mut Ui {
        &mut self.ui
    }

    pub(crate) fn resize(&mut self, (width, height): (u32, u32), scale: f32) -> Option<(u32, u32)> {
        let (width, height) = (width.min(self.max_texture), height.min(self.max_texture));
        if width == 0 || height == 0 {
            return None;
        }
        self.ui.set_device_scale(scale);
        self.ui
            .set_size(Size::new(width as f32 / scale, height as f32 / scale));
        self.configure(|config| {
            (config.width, config.height) = (width, height);
        });
        Some((width, height))
    }

    pub(crate) fn frame_due(&mut self, now: Instant) -> Option<Instant> {
        self.ui.flush();
        if !self.token {
            return None;
        }
        if self.counts_frames() {
            return Some(now);
        }
        match self.ui.next_frame() {
            Wake::Idle => None,
            Wake::Now => Some(now),
            Wake::At(at) => Some(now + Duration::from_secs_f64((at - self.ui.time()).max(0.0))),
        }
    }

    pub(crate) fn step(&mut self) {
        if !self.token {
            return;
        }
        if let Some(profile) = &mut self.profile {
            let (WindowConfig { width, height, .. }, scale) = (self.config, self.ui.scale());
            let center = Point::new(width as f32 / scale / 2.0, height as f32 / scale / 2.0);
            profile.before_update(&mut self.ui, center);
        }
        self.ui.update();
        let commit = self.ui.commit();
        if !commit.is_empty() || self.counts_frames() || self.ui.next_frame() == Wake::Now {
            self.send(Message::Commit(self.window, commit));
            self.token = false;
        }
    }

    fn counts_frames(&self) -> bool {
        self.exit_after_frames.is_some() || self.profile.as_ref().is_some_and(Profile::wants_frames)
    }

    pub(crate) fn handle(&mut self, event: CompositorEvent) {
        if event != CompositorEvent::Presented {
            return self.ui.handle(event);
        }
        self.token = true;
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
}
