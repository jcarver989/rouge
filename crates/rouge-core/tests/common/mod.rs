#![allow(dead_code)]

use rouge_core::testing::LINEAR;
use rouge_core::{
    Animation, Color, Compositor, Content, EffectSlot, LayerId, LayerProp, MotionId, Nested, Point,
    Pose, Quad, Rect, ShaderEffect, Size, SurfaceSource, SurfaceStatus, Timing,
};
use rouge_render::headless::{Headless, Image};
use rouge_render::{
    Atlas, DrawList, EffectInstance, GpuError, Placement, RenderStats, Surfaces, Viewport,
};
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::time::Instant;

pub(crate) const ROOT: LayerId = LayerId(0);
pub(crate) const CHILD: LayerId = LayerId(1);

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
        let mut stage = Stage {
            compositor: Compositor::new(),
            headless: Headless::new(Viewport::new(100, 100, 1.0))?,
            atlas: Atlas::new(),
            status: Rc::new(RefCell::new(SurfaceStatus::Waiting)),
        };
        stage.compositor.create_layer(ROOT);
        stage.compositor.set_root(ROOT);
        stage
            .compositor
            .set_frame(ROOT, Rect::new(0.0, 0.0, 100.0, 100.0));
        let mut nested = Vec::new();
        let surface = self.surface.map(|source| {
            let id = source.id();
            let status = stage.status.clone();
            let surfaces = stage.surfaces();
            surfaces.attach(source, move |update| *status.borrow_mut() = update);
            surfaces.place(
                id,
                Placement {
                    width: self.size.width as u32,
                    height: self.size.height as u32,
                    scale: 1.0,
                    visible: true,
                },
            );
            id
        });
        let child = self.child.or_else(|| surface.map(|_| DrawList::new()));
        if let Some(list) = child {
            stage.compositor.create_layer(CHILD);
            let frame = Rect {
                origin: self.origin,
                size: self.size,
            };
            stage.compositor.set_frame(CHILD, frame);
            stage.compositor.set_surface(CHILD, surface);
            stage.content(CHILD, vec![list], vec![]);
            nested.push(Nested {
                layer: CHILD,
                clip: self.clip,
            });
        }
        let behind = self.background.unwrap_or_else(background);
        stage.content(ROOT, vec![behind, DrawList::new()], nested);
        Ok(stage)
    }
}

pub(crate) struct Stage {
    pub(crate) compositor: Compositor,
    pub(crate) headless: Headless,
    pub(crate) atlas: Atlas,
    status: Rc<RefCell<SurfaceStatus>>,
}

impl Stage {
    pub(crate) fn set(&mut self, id: LayerId, prop: LayerProp, value: f32) {
        self.compositor.set_prop(id, prop, value, false);
    }

    pub(crate) fn pose(&mut self, id: LayerId, pose: Pose, animate: bool) {
        for (prop, value) in pose.iter() {
            self.compositor.set_prop(id, prop, value, animate);
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
        self.compositor.set_effect(id, slot, effect, animate);
    }

    pub(crate) fn effect_off(&mut self, id: LayerId, slot: EffectSlot) {
        self.compositor.set_effect(id, slot, None, false);
    }

    pub(crate) fn exit(&mut self, to: Pose) {
        self.compositor.exit(CHILD, to, LINEAR.into());
    }

    pub(crate) fn scroll_extent(&mut self, height: f32) {
        let (viewport, content) = (Size::new(100.0, 100.0), Size::new(100.0, height));
        self.compositor.set_scroll_extent(CHILD, viewport, content);
    }

    pub(crate) fn offset(&self) -> Point {
        let scroll = self.compositor.scroll(CHILD);
        scroll.map_or(Point::new(f32::NAN, f32::NAN), |scroll| scroll.offset)
    }

    pub(crate) fn bind(&mut self, motion: MotionId, initial: f32) {
        self.compositor.create_motion(motion, initial);
        self.compositor
            .bind_prop(CHILD, LayerProp::TranslateX, motion);
    }

    pub(crate) fn animate(&mut self, motion: MotionId, target: f32, timing: Timing) -> Animation {
        let (done, animation) = Animation::pending();
        self.compositor.animate_motion(motion, target, timing, done);
        animation
    }

    pub(crate) fn content(&mut self, id: LayerId, lists: Vec<DrawList>, nested: Vec<Nested>) {
        self.compositor.set_content(id, Content { lists, nested });
    }

    pub(crate) fn run(&mut self, seconds: f32) {
        let frames = (seconds * 100.0).round() as usize;
        (0..frames).for_each(|_| self.compositor.tick(0.01));
    }

    pub(crate) fn x(&self, id: LayerId) -> f32 {
        let x = self
            .compositor
            .pose(id)
            .map_or(f32::NAN, |pose| pose.translate.x);
        (x * 100.0).round() / 100.0
    }

    pub(crate) fn draw(&mut self) -> RenderStats {
        self.compositor.sync_surfaces(self.headless.surfaces());
        self.headless.surfaces().latch(Instant::now());
        self.headless.render(
            Color::TRANSPARENT,
            &self.compositor.scene(1.0),
            &mut self.atlas,
        )
    }

    pub(crate) fn surfaces(&mut self) -> &mut Surfaces {
        self.headless.surfaces()
    }

    pub(crate) fn status(&self) -> SurfaceStatus {
        self.status.borrow().clone()
    }

    pub(crate) fn image(&self) -> Result<Image, GpuError> {
        self.headless.image()
    }
}

pub(crate) fn settled(animation: &mut Animation) -> Option<bool> {
    match Pin::new(animation).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(completed) => Some(completed),
        Poll::Pending => None,
    }
}
