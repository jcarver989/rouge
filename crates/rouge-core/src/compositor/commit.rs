use super::model::{LayerDelta, LayerId, Nested};
use rouge_render::{
    AtlasUploads, Color, DrawList, Point, SurfaceConfig, SurfaceId, SurfaceSource, SurfaceStatus,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ListId {
    pub block: u64,
    pub index: u32,
}

pub struct PlacedList {
    pub id: ListId,
    pub segment: usize,
    pub offset: Point,
    pub list: Option<DrawList>,
}

#[derive(Default)]
pub struct Content {
    pub lists: Vec<PlacedList>,
    pub nested: Vec<Nested>,
}

impl Content {
    pub fn new(lists: Vec<DrawList>, nested: Vec<Nested>) -> Self {
        let place = |(segment, list)| PlacedList {
            id: ListId {
                block: 0,
                index: segment as u32,
            },
            segment: nested.len().min(segment),
            offset: Point::ZERO,
            list: Some(list),
        };
        Content {
            lists: lists.into_iter().enumerate().map(place).collect(),
            nested,
        }
    }
}

pub enum SurfaceOp {
    Attach(SurfaceSource),
    Place(SurfaceId, SurfaceConfig),
    Detach(SurfaceId),
}

pub enum ListDelta {
    Replace(Vec<PlacedList>),
    Patch(Vec<(usize, PlacedList)>),
}

#[derive(Default)]
pub struct Commit {
    pub layers: LayerDelta,
    pub content: Vec<(LayerId, ListDelta)>,
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
