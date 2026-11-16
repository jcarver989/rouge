#![allow(dead_code)]

use rouge_core::testing::LINEAR;
use rouge_core::{
    Animation, Color, Commit, CompositorActor, CompositorEvent, Content, EffectSlot, LayerId,
    LayerProp, Layers, Message, MotionId, Nested, Point, Pose, Quad, Rect, ShaderEffect, Size,
    SurfaceOp, SurfaceSource, SurfaceStatus, Timing, WindowConfig, WindowId,
};
use rouge_render::headless::{Image, headless_gpu};
use rouge_render::{
    DrawList, EffectInstance, GpuError, RenderStats, SurfaceConfig, Surfaces, Target,
};
use std::error::Error;
use std::future::Future;
use std::mem::take;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

pub(crate) const WINDOW: WindowId = WindowId(0);

const CONFIG: WindowConfig = WindowConfig {
    width: 100,
    height: 100,
    scale: 1.0,
    interval: Duration::from_millis(10),
    visible: true,
};

pub(crate) fn quads(quads: impl IntoIterator<Item = Quad>) -> DrawList {
    let mut list = DrawList::new();
    quads.into_iter().for_each(|quad| list.push_quad(quad));
    list
}

pub(crate) fn background() -> DrawList {
    quads([Quad::new(Rect::new(0.0, 0.0, 100.0, 100.0), Color::WHITE)])
}

pub(crate) fn square() -> DrawList {
    quads([Quad::new(Rect::new(0.0, 0.0, 20.0, 20.0), Color::RED)])
}

pub(crate) struct StageBuilder {
    background: Option<DrawList>,
    child: Option<DrawList>,
    origin: Point,
    size: Size,
    clip: Option<Rect>,
    surface: Option<SurfaceSource>,
}

impl Default for StageBuilder {
    fn default() -> Self {
        StageBuilder {
            background: None,
            child: None,
            origin: Point::ZERO,
            size: Size::new(20.0, 20.0),
            clip: None,
            surface: None,
        }
    }
}

impl StageBuilder {
    pub(crate) fn background(mut self, list: DrawList) -> Self {
        self.background = Some(list);
        self
    }

    pub(crate) fn at(mut self, x: f32, y: f32) -> Self {
        self.origin = Point::new(x, y);
        self
    }

    pub(crate) fn child(mut self, list: DrawList) -> Self {
        self.child = Some(list);
        self
    }

    pub(crate) fn clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    pub(crate) fn sized(mut self, width: f32, height: f32) -> Self {
        self.size = Size::new(width, height);
        self
    }

    pub(crate) fn surface(mut self, source: SurfaceSource) -> Self {
        self.surface = Some(source);
        self
    }

    pub(crate) fn build(self) -> Result<Stage, GpuError> {
        let gpu = headless_gpu()?;
        let mut actor = CompositorActor::new(gpu.clone(), Surfaces::new(&gpu));
        actor.handle(Message::Open(
            WINDOW,
            Target::offscreen(&gpu, CONFIG.width, CONFIG.height),
            CONFIG,
        ));
        let mut layers = Layers::new();
        let (root, child) = (layers.create_layer(), layers.create_layer());
        let mut stage = Stage {
            root,
            child,
            layers,
            actor,
            surfaces: Vec::new(),
            status: SurfaceStatus::Waiting,
            presented: 0,
            time: 0.0,
        };
        stage.layers.set_root(stage.root);
        stage
            .layers
            .set_frame(stage.root, Rect::new(0.0, 0.0, 100.0, 100.0));
        let mut nested = Vec::new();
        let surface = self.surface.map(|source| {
            let id = source.id();
            let placement = SurfaceConfig {
                width: self.size.width as u32,
                height: self.size.height as u32,
                scale: 1.0,
                visible: true,
            };
            stage.surfaces.push(SurfaceOp::Attach(source));
            stage.surfaces.push(SurfaceOp::Place(id, placement));
            id
        });
        let child = self.child.or_else(|| surface.map(|_| DrawList::new()));
        if let Some(list) = child {
            let frame = Rect {
                origin: self.origin,
                size: self.size,
            };
            stage.layers.set_frame(stage.child, frame);
            stage.layers.set_surface(stage.child, surface);
            stage.content(stage.child, vec![list], vec![]);
            nested.push(Nested {
                layer: stage.child,
                clip: self.clip,
            });
        }
        let behind = self.background.unwrap_or_else(background);
        stage.content(stage.root, vec![behind, DrawList::new()], nested);
        stage.sync();
        Ok(stage)
    }
}

pub(crate) struct Stage {
    pub(crate) root: LayerId,
    pub(crate) child: LayerId,
    pub(crate) layers: Layers,
    pub(crate) actor: CompositorActor,
    surfaces: Vec<SurfaceOp>,
    status: SurfaceStatus,
    presented: usize,
    time: f64,
}

impl Stage {
    pub(crate) fn set(&mut self, id: LayerId, prop: LayerProp, value: f32) {
        self.layers.set_prop(id, prop, value, false);
    }

    pub(crate) fn pose(&mut self, id: LayerId, pose: Pose, animate: bool) {
        for (prop, value) in pose.iter() {
            self.layers.set_prop(id, prop, value, animate);
        }
    }

    pub(crate) fn effect<T: ShaderEffect>(
        &mut self,
        id: LayerId,
        slot: EffectSlot,
        effect: &T,
        animate: bool,
    ) {
        let effect = Some(EffectInstance::new(effect));
        self.layers.set_effect(id, slot, effect, animate);
    }

    pub(crate) fn effect_off(&mut self, id: LayerId, slot: EffectSlot) {
        self.layers.set_effect(id, slot, None, false);
    }

    pub(crate) fn exit(&mut self, to: Pose) {
        self.layers.exit(self.child, to, LINEAR.into());
    }

    pub(crate) fn scroll_extent(&mut self, height: f32) {
        let (viewport, content) = (Size::new(100.0, 100.0), Size::new(100.0, height));
        self.layers.set_scroll_extent(self.child, viewport, content);
    }

    pub(crate) fn offset(&self) -> Point {
        let scroll = self.layers.scroll(self.child);
        scroll.map_or(Point::new(f32::NAN, f32::NAN), |scroll| scroll.offset)
    }

    pub(crate) fn bind(&mut self, motion: MotionId, initial: f32) {
        self.layers.create_motion(motion, initial);
        self.layers
            .bind_prop(self.child, LayerProp::TranslateX, motion);
    }

    pub(crate) fn animate(&mut self, motion: MotionId, target: f32, timing: Timing) -> Animation {
        let (done, animation) = Animation::pending();
        self.layers.animate_motion(motion, target, timing, done);
        animation
    }

    pub(crate) fn content(&mut self, id: LayerId, lists: Vec<DrawList>, nested: Vec<Nested>) {
        self.layers.set_content(id, Content { lists, nested });
    }

    pub(crate) fn run(&mut self, seconds: f32) {
        let frames = (seconds * 100.0).round() as usize;
        for _ in 0..frames {
            self.time += 0.01;
            self.layers.advance(self.time);
        }
    }

    pub(crate) fn x(&self, id: LayerId) -> f32 {
        let x = self
            .layers
            .pose(id)
            .map_or(f32::NAN, |pose| pose.translate.x);
        (x * 100.0).round() / 100.0
    }

    pub(crate) fn sync(&mut self) {
        let commit = Commit {
            surfaces: take(&mut self.surfaces),
            ..self.layers.commit()
        };
        self.actor.handle(Message::Commit(WINDOW, commit));
        self.collect();
    }

    pub(crate) fn show(&mut self, visible: bool) {
        let config = WindowConfig { visible, ..CONFIG };
        self.actor.handle(Message::Configure(WINDOW, config));
        self.collect();
    }

    pub(crate) fn frame(&mut self, now: f64) {
        self.actor.frame(now);
        self.collect();
    }

    pub(crate) fn draw(&mut self) -> RenderStats {
        self.sync();
        self.actor.draw(WINDOW, self.time);
        self.collect();
        self.actor.stats()
    }

    fn collect(&mut self) {
        for (_, event) in self.actor.events() {
            match event {
                CompositorEvent::Presented => self.presented += 1,
                CompositorEvent::Surface(_, status) => self.status = status,
                CompositorEvent::Closed => {}
            }
        }
    }

    pub(crate) fn presented(&mut self) -> usize {
        take(&mut self.presented)
    }

    pub(crate) fn surfaces(&mut self) -> &mut Surfaces {
        self.actor.surfaces()
    }

    pub(crate) fn status(&self) -> SurfaceStatus {
        self.status.clone()
    }

    pub(crate) fn wait(&self) -> Result<(), GpuError> {
        self.actor.gpu().wait()
    }

    pub(crate) fn image(&self) -> Result<Image, Box<dyn Error>> {
        Ok(self
            .actor
            .image(WINDOW)
            .ok_or("the stage window is closed")??)
    }
}

pub(crate) fn settled(animation: &mut Animation) -> Option<bool> {
    match Pin::new(animation).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(completed) => Some(completed),
        Poll::Pending => None,
    }
}
