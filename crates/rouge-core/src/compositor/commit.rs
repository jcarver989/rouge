use super::model::{LayerDelta, LayerId};
use rouge_render::{
    AtlasUploads, Color, DrawList, SurfaceConfig, SurfaceId, SurfaceSource, SurfaceStatus,
};

pub enum SurfaceOp {
    Attach(SurfaceSource),
    Place(SurfaceId, SurfaceConfig),
    Detach(SurfaceId),
}

#[derive(Default)]
pub struct Commit {
    pub layers: LayerDelta,
    pub content: Vec<(LayerId, Vec<DrawList>)>,
    pub atlas: AtlasUploads,
    pub surfaces: Vec<SurfaceOp>,
    pub clear: Option<Color>,
    pub scale: Option<f32>,
}

impl Commit {
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
            && self.content.is_empty()
            && self.atlas.is_empty()
            && self.surfaces.is_empty()
            && self.clear.is_none()
            && self.scale.is_none()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CompositorEvent {
    Presented,
    Surface(SurfaceId, SurfaceStatus),
    Closed,
}
