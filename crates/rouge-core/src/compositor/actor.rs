use super::{Commit, Compositor, CompositorEvent, SurfaceOp};
#[cfg(not(target_arch = "wasm32"))]
use rouge_render::GpuError;
#[cfg(not(target_arch = "wasm32"))]
use rouge_render::headless::{Image, read_texture};
use rouge_render::{
    Acquire, Color, Command, Gpu, RenderStats, Renderer, SurfaceId, SurfaceStatus, Surfaces,
    Target, Viewport,
};
use std::mem::take;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use tracing::trace_span;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowConfig {
    pub width: u32,
    pub height: u32,
    pub interval: Duration,
    pub visible: bool,
}

pub enum Message {
    Open(WindowId, Target, WindowConfig),
    Configure(WindowId, WindowConfig),
    Commit(WindowId, Commit),
    Close(WindowId),
    Wake,
    Shutdown,
}

struct Window {
    id: WindowId,
    compositor: Compositor,
    renderer: Renderer,
    target: Target,
    config: WindowConfig,
    clear: Color,
    scale: f32,
    presented: Option<f64>,
    owed: bool,
    surfaced: bool,
}

impl Window {
    fn wants(&self, now: f64) -> bool {
        let settles = self.compositor.model().settles_at();
        self.config.visible && (self.owed || self.surfaced || settles.is_some_and(|end| end > now))
    }

    fn due_at(&self) -> f64 {
        self.presented.map_or(f64::NEG_INFINITY, |at| {
            at + self.config.interval.as_secs_f64() * 0.75
        })
    }

    fn acknowledge(&mut self) -> Option<(WindowId, CompositorEvent)> {
        take(&mut self.owed).then_some((self.id, CompositorEvent::Presented))
    }
}

type Status = (WindowId, SurfaceId, SurfaceStatus);

pub struct CompositorActor {
    gpu: Gpu,
    surfaces: Surfaces,
    windows: Vec<Window>,
    events: Vec<(WindowId, CompositorEvent)>,
    status: Sender<Status>,
    statuses: Receiver<Status>,
    stats: RenderStats,
}

impl CompositorActor {
    pub fn new(gpu: Gpu, surfaces: Surfaces) -> Self {
        let (status, statuses) = channel();
        CompositorActor {
            gpu,
            surfaces,
            windows: Vec::new(),
            events: Vec::new(),
            status,
            statuses,
            stats: RenderStats::default(),
        }
    }

    pub fn into_targets(self) -> Vec<Target> {
        self.windows
            .into_iter()
            .map(|window| window.target)
            .collect()
    }

    pub fn gpu(&self) -> &Gpu {
        &self.gpu
    }

    pub fn surfaces(&mut self) -> &mut Surfaces {
        &mut self.surfaces
    }

    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn image(&self, window: WindowId) -> Option<Result<Image, GpuError>> {
        let open = self.windows.iter().find(|open| open.id == window)?;
        Some(read_texture(&self.gpu, open.target.texture()?))
    }

    pub fn commands(&mut self, window: WindowId, now: f64) -> Option<Vec<Command>> {
        let open = find(&mut self.windows, window)?;
        let scene = (open.compositor).scene(open.scale, &self.surfaces, now);
        Some(scene.commands.to_vec())
    }

    pub fn handle(&mut self, message: Message) {
        match message {
            Message::Open(id, target, config) => {
                let renderer = Renderer::new(&self.gpu.device, &self.gpu.queue, target.format());
                self.windows.retain(|open| open.id != id);
                self.windows.push(Window {
                    id,
                    compositor: Compositor::default(),
                    renderer,
                    target,
                    config,
                    clear: Color::TRANSPARENT,
                    scale: 1.0,
                    presented: None,
                    owed: false,
                    surfaced: false,
                });
            }
            Message::Configure(id, config) => {
                let Some(window) = find(&mut self.windows, id) else {
                    return;
                };
                if (config.width, config.height) != window.target.size() {
                    (window.target).resize(&self.gpu, config.width, config.height);
                    window.presented = None;
                }
                let hidden = window.config.visible && !config.visible;
                window.config = config;
                if hidden {
                    self.events.extend(window.acknowledge());
                }
            }
            Message::Commit(id, commit) => self.commit(id, commit),
            Message::Close(id) => {
                self.windows.retain(|open| open.id != id);
                self.events.push((id, CompositorEvent::Closed));
            }
            Message::Wake | Message::Shutdown => {}
        }
    }

    fn commit(&mut self, id: WindowId, commit: Commit) {
        let Some(window) = find(&mut self.windows, id) else {
            return;
        };
        let held = commit.is_empty();
        window.renderer.upload_atlas(&commit.atlas);
        for op in commit.surfaces {
            match op {
                SurfaceOp::Attach(source) => {
                    let (surface, status) = (source.id(), self.status.clone());
                    self.surfaces.attach(source, move |update| {
                        let _ = status.send((id, surface, update));
                    });
                }
                SurfaceOp::Place(surface, placement) => self.surfaces.place(surface, placement),
                SurfaceOp::Detach(surface) => self.surfaces.detach(surface),
            }
        }
        (window.compositor).apply(commit.layers, commit.content, &mut self.surfaces);
        window.clear = commit.clear.unwrap_or(window.clear);
        window.scale = commit.scale.unwrap_or(window.scale);
        window.owed = true;
        if !window.config.visible && !held {
            self.events.extend(window.acknowledge());
        }
    }

    pub fn next_wake(&mut self, now: f64) -> Option<f64> {
        self.latch();
        let maintain = self.surfaces.next_deadline().map(|deadline| {
            now + deadline
                .saturating_duration_since(Instant::now())
                .as_secs_f64()
        });
        let wanting = self.windows.iter().filter(|window| window.wants(now));
        wanting.map(Window::due_at).chain(maintain).reduce(f64::min)
    }

    pub fn frame(&mut self, now: f64) {
        self.latch();
        for index in 0..self.windows.len() {
            let window = &self.windows[index];
            if window.wants(now) && window.due_at() <= now {
                self.present(index, now);
            }
        }
        self.finish();
    }

    pub fn draw(&mut self, window: WindowId, now: f64) {
        self.latch();
        if let Some(index) = self.windows.iter().position(|open| open.id == window) {
            self.present(index, now);
        }
        self.finish();
    }

    fn latch(&mut self) {
        let changed = self.surfaces.latch();
        let surfaced = changed || self.surfaces.needs_frame();
        for window in &mut self.windows {
            window.surfaced |= surfaced && window.compositor.model().has_surfaces();
        }
    }

    fn finish(&mut self) {
        self.surfaces.maintain();
        let statuses = self.statuses.try_iter();
        self.events.extend(
            statuses.map(|(window, id, status)| (window, CompositorEvent::Surface(id, status))),
        );
    }

    fn present(&mut self, index: usize, now: f64) {
        let _frame = trace_span!("composite").entered();
        let window = &mut self.windows[index];
        if !window.config.visible {
            return;
        }
        let acquired = {
            let _acquire = trace_span!("acquire").entered();
            window.target.acquire(&self.gpu)
        };
        window.presented = Some(now);
        let frame = match acquired {
            Acquire::Frame(frame) => frame,
            Acquire::Retry => return,
            Acquire::Skip => {
                self.events.extend(window.acknowledge());
                return;
            }
        };
        let (width, height) = window.target.size();
        let scale = window.scale;
        let scene = window.compositor.scene(scale, &self.surfaces, now);
        {
            let _encode = trace_span!("encode").entered();
            window.renderer.render(
                &frame.view,
                Viewport::new(width, height, scale),
                window.clear,
                scene,
                &self.surfaces,
            );
        }
        self.stats = window.renderer.stats();
        {
            let _present = trace_span!("present").entered();
            window.target.present(&self.gpu, frame);
        }
        window.surfaced = false;
        (self.surfaces).presented(Instant::now(), window.config.interval);
        self.events.extend(window.acknowledge());
    }

    pub fn events(&mut self) -> impl Iterator<Item = (WindowId, CompositorEvent)> + '_ {
        self.events.drain(..)
    }
}

fn find(windows: &mut [Window], id: WindowId) -> Option<&mut Window> {
    windows.iter_mut().find(|window| window.id == id)
}
