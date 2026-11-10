#![allow(dead_code)]

use rouge_render::headless::{Headless, Image};
use rouge_render::{
    Atlas, Color, Command, Commit, Compositor, Content, DrawList, Ease, Effect, EffectDraw,
    EffectInstance, EffectSlot, Glass, GpuError, Item, LayerCommand, LayerDraw, LayerId, LayerProp,
    MotionCommand, MotionId, Nested, Point, Pose, Quad, Rect, RenderStats, Size, Timing, Tween,
    Viewport,
};
use std::error::Error;
use tokio::sync::oneshot::{Receiver, channel};

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) const ROOT: LayerId = LayerId(0);
pub(crate) const CHILD: LayerId = LayerId(1);
pub(crate) const WHITE: [u8; 4] = [255, 255, 255, 255];
pub(crate) const RED: [u8; 4] = [255, 0, 0, 255];
pub(crate) const BLUE: [u8; 4] = [0, 0, 255, 255];
pub(crate) const CLEAR: [u8; 4] = [0, 0, 0, 0];
pub(crate) const LINEAR: Tween = Tween::new(1.0).ease(Ease::Linear);

pub(crate) struct Canvas {
    headless: Headless,
    atlas: Atlas,
    clear: Color,
}

impl Canvas {
    pub(crate) fn new(size: u32, scale: f32, clear: Color) -> Result<Self, GpuError> {
        Ok(Canvas {
            headless: Headless::new(Viewport::new(size, size, scale))?,
            atlas: Atlas::new(),
            clear,
        })
    }

    pub(crate) fn atlas(&mut self) -> &mut Atlas {
        &mut self.atlas
    }

    pub(crate) fn frame(&mut self, layers: &[LayerDraw<'_>]) -> RenderStats {
        let items: Vec<_> = layers.iter().copied().map(Item::Layer).collect();
        self.headless.render(self.clear, &items, &mut self.atlas)
    }

    pub(crate) fn image(&self) -> Result<Image, GpuError> {
        self.headless.image()
    }
}

pub(crate) fn render_layers(size: u32, layers: &[LayerDraw<'_>]) -> Result<Image, GpuError> {
    let mut canvas = Canvas::new(size, 1.0, Color::TRANSPARENT)?;
    canvas.frame(layers);
    canvas.image()
}

pub(crate) fn over(scene: &DrawList, glass: EffectDraw) -> Result<Image, GpuError> {
    let empty = DrawList::new();
    let glass = LayerDraw {
        backdrop: Some(glass),
        ..LayerDraw::new(&empty)
    };
    render_layers(100, &[LayerDraw::new(scene), glass])
}

pub(crate) fn quads(quads: impl IntoIterator<Item = Quad>) -> DrawList {
    let mut list = DrawList::new();
    quads.into_iter().for_each(|quad| list.push_quad(quad));
    list
}

pub(crate) fn full(color: Color) -> Quad {
    Quad::new(Rect::new(0.0, 0.0, 100.0, 100.0), color)
}

pub(crate) fn split_scene() -> DrawList {
    quads([
        Quad::new(Rect::new(0.0, 0.0, 50.0, 100.0), Color::BLACK),
        Quad::new(Rect::new(50.0, 0.0, 50.0, 100.0), Color::WHITE),
    ])
}

pub(crate) fn backdrop(bounds: Rect, blur: f32) -> EffectDraw {
    EffectDraw::new(&Glass::frosted(blur).edge_width(12.0), bounds)
}

pub(crate) fn background() -> DrawList {
    quads([full(Color::WHITE)])
}

pub(crate) fn square() -> DrawList {
    quads([Quad::new(Rect::new(0.0, 0.0, 20.0, 20.0), Color::RED)])
}

#[derive(Default)]
pub(crate) struct StageBuilder {
    background: Option<DrawList>,
    child: Option<DrawList>,
    origin: Point,
    clip: Option<Rect>,
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

    pub(crate) fn build(self) -> Result<Stage, GpuError> {
        let mut stage = Stage {
            compositor: Compositor::new(),
            headless: Headless::new(Viewport::new(100, 100, 1.0))?,
            atlas: Atlas::new(),
        };
        stage.apply(Command::Create(ROOT));
        stage.apply(Command::Root(ROOT));
        stage.layer(ROOT, LayerCommand::Frame(Rect::new(0.0, 0.0, 100.0, 100.0)));
        let mut nested = Vec::new();
        if let Some(list) = self.child {
            stage.apply(Command::Create(CHILD));
            let frame = Rect {
                origin: self.origin,
                size: Size::new(20.0, 20.0),
            };
            stage.layer(CHILD, LayerCommand::Frame(frame));
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
}

impl Stage {
    pub(crate) fn apply(&mut self, command: Command) {
        let mut commit = Commit::new();
        commit.push(command);
        self.compositor.apply(commit);
    }

    pub(crate) fn layer(&mut self, id: LayerId, command: LayerCommand) {
        self.apply(Command::Layer(id, command));
    }

    pub(crate) fn set(&mut self, id: LayerId, prop: LayerProp, value: f32) {
        self.layer(
            id,
            LayerCommand::Set {
                prop,
                value,
                animate: false,
            },
        );
    }

    pub(crate) fn pose(&mut self, id: LayerId, pose: Pose, animate: bool) {
        for (prop, value) in pose.iter() {
            self.layer(
                id,
                LayerCommand::Set {
                    prop,
                    value,
                    animate,
                },
            );
        }
    }

    pub(crate) fn effect<T: Effect>(
        &mut self,
        id: LayerId,
        slot: EffectSlot,
        effect: &T,
        animate: bool,
    ) {
        let effect = Some(EffectInstance::new(effect));
        self.layer(
            id,
            LayerCommand::Effect {
                slot,
                effect,
                animate,
            },
        );
    }

    pub(crate) fn effect_off(&mut self, id: LayerId, slot: EffectSlot) {
        self.layer(
            id,
            LayerCommand::Effect {
                slot,
                effect: None,
                animate: false,
            },
        );
    }

    pub(crate) fn exit(&mut self, to: Pose) {
        let timing = LINEAR.into();
        self.layer(CHILD, LayerCommand::Exit { to, timing });
    }

    pub(crate) fn scroll_extent(&mut self, height: f32) {
        let (viewport, content) = (Size::new(100.0, 100.0), Size::new(100.0, height));
        self.layer(CHILD, LayerCommand::ScrollExtent { viewport, content });
    }

    pub(crate) fn offset(&self) -> Point {
        let scroll = self.compositor.scroll(CHILD);
        scroll.map_or(Point::new(f32::NAN, f32::NAN), |scroll| scroll.offset)
    }

    pub(crate) fn bind(&mut self, motion: MotionId, initial: f32) {
        self.apply(Command::Motion(motion, MotionCommand::Create(initial)));
        let prop = LayerProp::TranslateX;
        self.layer(CHILD, LayerCommand::Bind { prop, motion });
    }

    pub(crate) fn animate(
        &mut self,
        motion: MotionId,
        target: f32,
        timing: Timing,
    ) -> Receiver<bool> {
        let (done, settled) = channel();
        let done = Some(done);
        let animate = MotionCommand::Animate {
            target,
            timing,
            done,
        };
        self.apply(Command::Motion(motion, animate));
        settled
    }

    pub(crate) fn content(&mut self, id: LayerId, lists: Vec<DrawList>, nested: Vec<Nested>) {
        self.layer(id, LayerCommand::Content(Content { lists, nested }));
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
        self.headless.render(
            Color::TRANSPARENT,
            &self.compositor.scene(1.0),
            &mut self.atlas,
        )
    }

    pub(crate) fn image(&self) -> Result<Image, GpuError> {
        self.headless.image()
    }
}
