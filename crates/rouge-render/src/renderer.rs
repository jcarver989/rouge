use crate::atlas::INITIAL_HEIGHT;
use crate::effect::{EffectDraw, PASSTHROUGH_WGSL, Passthrough, SurfaceSample};
use crate::gpu::{PipelineDesc, begin_pass, create_pipeline, create_texture};
use crate::scene::{DrawList, DrawLists, Instance, ListKey};
use crate::surface::{AlphaMode, BufferRef, Origin, SurfaceId, Surfaces};
use crate::{
    Affine, Atlas, AtlasUploads, Color, Corners, ImageData, ImageFormat, Inputs, Point, Rect,
};
use bytemuck::{Pod, Zeroable, bytes_of, cast_slice};
use rustc_hash::{FxHashMap, FxHasher};
use std::collections::hash_map::Entry;
use std::hash::{Hash, Hasher};
use std::mem::take;
use std::ops::Range;
use std::ptr;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBinding,
    BufferBindingType, BufferDescriptor, BufferSize, BufferUsages, Color as ClearColor,
    CommandEncoder, CommandEncoderDescriptor, Device, Extent3d, FilterMode, LoadOp, Origin3d,
    PipelineLayout, PipelineLayoutDescriptor, Queue, RenderPass, RenderPipeline, Sampler,
    SamplerBindingType, SamplerDescriptor, ShaderModule, ShaderModuleDescriptor, ShaderSource,
    ShaderStages, TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureFormat,
    TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor, TextureViewDimension,
    VertexBufferLayout, VertexStepMode,
};

const COMMON_WGSL: &str = include_str!("shaders/common.wgsl");
const PRIMITIVE_WGSL: &str = include_str!("shaders/primitive.wgsl");
const EFFECT_WGSL: &str = include_str!("shaders/effect.wgsl");
const FULLSCREEN_WGSL: &str = include_str!("shaders/fullscreen.wgsl");
const GROUP_TEXTURE_STEP: u32 = 64;
const BLUR_TEXTURE_STEP: u32 = 32;
const MAX_BLUR_LEVELS: u32 = 6;
const SCRATCH_PADDING: u32 = 512;
const UNBOUNDED_MARGIN: f32 = 1.0e9;
const PAGE_BYTES: usize = 16 << 10;
const ROW_BYTES: usize = 16;
const IDLE_FRAMES: u64 = 600;
const MAINTENANCE_FRAMES: u64 = 60;
const IMAGE_BUDGET_BYTES: u64 = 256 << 20;
const INSTANCE_MIN_BYTES: u64 = 64 << 10;
const MAX_DAMAGE_RECTS: usize = 6;
const DAMAGE_SLACK: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

impl Viewport {
    pub const fn new(width: u32, height: u32, scale: f32) -> Self {
        Viewport {
            width,
            height,
            scale,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerDraw {
    pub list: Option<ListKey>,
    pub transform: Affine,
    pub opacity: f32,
    pub fill: [f32; 4],
    pub radii: Corners,
    pub backdrop: Option<EffectDraw>,
    pub scissor: Option<Rect>,
}

impl Default for LayerDraw {
    fn default() -> Self {
        LayerDraw {
            list: None,
            transform: Affine::IDENTITY,
            opacity: 1.0,
            fill: [0.0; 4],
            radii: Corners::ZERO,
            backdrop: None,
            scissor: None,
        }
    }
}

impl LayerDraw {
    pub fn new(list: ListKey) -> Self {
        LayerDraw {
            list: Some(list),
            ..LayerDraw::default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Group {
    pub key: u64,
    pub signature: u64,
    pub bounds: Rect,
    pub offset: (i32, i32),
    pub opacity: f32,
    pub scissor: Option<Rect>,
    pub transform: Affine,
    pub effect: Option<EffectDraw>,
    pub len: u32,
}

impl Group {
    fn shift(&self) -> Point {
        Point::new(self.offset.0 as f32, self.offset.1 as f32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceDraw {
    pub id: SurfaceId,
    pub frame: Option<u64>,
    pub bounds: Rect,
    pub transform: Affine,
    pub opacity: f32,
    pub radii: Corners,
    pub scissor: Option<Rect>,
}

impl SurfaceDraw {
    pub fn new(id: SurfaceId, bounds: Rect) -> Self {
        SurfaceDraw {
            id,
            frame: None,
            bounds,
            transform: Affine::IDENTITY,
            opacity: 1.0,
            radii: Corners::ZERO,
            scissor: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Draw(LayerDraw),
    Group(Group),
    Surface(SurfaceDraw),
}

#[derive(Clone, Copy)]
pub struct Scene<'a> {
    pub commands: &'a [Command],
    pub lists: &'a DrawLists,
}

impl<'a> Scene<'a> {
    pub fn new(commands: &'a [Command], lists: &'a DrawLists) -> Self {
        Scene { commands, lists }
    }

    pub fn summarize<T: Hasher>(
        &self,
        range: Range<usize>,
        scale: f32,
        hasher: &mut T,
    ) -> Option<Rect> {
        let mut bounds: Option<Rect> = None;
        let mut index = range.start;
        while index < range.end {
            self.fingerprint(index, hasher);
            if let Some(extent) = self.extent(index, scale) {
                bounds = Some(bounds.map_or(extent, |bounds| bounds.union(&extent)));
            }
            index = self.next(index);
        }
        bounds
    }

    fn next(&self, index: usize) -> usize {
        match &self.commands[index] {
            Command::Group(group) => index + 1 + group.len as usize,
            Command::Draw(_) | Command::Surface(_) => index + 1,
        }
    }

    fn extent(&self, index: usize, scale: f32) -> Option<Rect> {
        let (reach, scissor) = match &self.commands[index] {
            Command::Draw(draw) => {
                let list = draw
                    .list
                    .and_then(|key| self.lists.get(key)?.extent())
                    .map(|extent| device_rect(&draw.transform, &extent, scale).inflate(1.0));
                let backdrop = draw.backdrop.map(|backdrop| {
                    device_rect(
                        &draw.transform,
                        &backdrop.bounds.inflate(backdrop.effect.margin()),
                        scale,
                    )
                });
                let reach = match (list, backdrop) {
                    (Some(list), Some(backdrop)) => Some(list.union(&backdrop)),
                    (list, backdrop) => list.or(backdrop),
                };
                (reach?, draw.scissor)
            }
            Command::Surface(draw) => (
                device_rect(&draw.transform, &draw.bounds, scale).inflate(1.0),
                draw.scissor,
            ),
            Command::Group(group) => {
                let spread = group.effect.map_or(0.0, |effect| {
                    let blur = Kernel::new(effect.effect.content_blur() * scale);
                    effect.effect.margin() * scale + blur.reach()
                });
                (
                    group.bounds.translate(group.shift()).inflate(spread),
                    group.scissor,
                )
            }
        };
        let reach = scissor.map_or(reach, |scissor| scissor.intersect(&reach));
        (!reach.is_empty()).then_some(reach)
    }

    fn fingerprint<T: Hasher>(&self, index: usize, hasher: &mut T) {
        let rect = |rect: Option<Rect>, hasher: &mut T| {
            rect.map(|rect| rect.to_array().map(f32::to_bits))
                .hash(hasher);
        };
        let affine = |t: &Affine, hasher: &mut T| {
            [t.a, t.b, t.c, t.d, t.tx, t.ty]
                .map(f32::to_bits)
                .hash(hasher);
        };
        let effect = |effect: &Option<EffectDraw>, hasher: &mut T| {
            effect.is_some().hash(hasher);
            if let Some(effect) = effect {
                let wgsl = effect.effect.wgsl();
                (wgsl.as_ptr().addr(), wgsl.len(), effect.effect.inputs()).hash(hasher);
                effect.effect.values().map(f32::to_bits).hash(hasher);
                effect
                    .corner_radii
                    .to_array()
                    .map(f32::to_bits)
                    .hash(hasher);
                rect(Some(effect.bounds), hasher);
            }
        };
        match &self.commands[index] {
            Command::Draw(draw) => {
                let list = draw.list.and_then(|key| self.lists.get(key));
                (0u8, list.map(|list| (list.id(), list.revision()))).hash(hasher);
                affine(&draw.transform, hasher);
                draw.opacity.to_bits().hash(hasher);
                draw.fill.map(f32::to_bits).hash(hasher);
                draw.radii.to_array().map(f32::to_bits).hash(hasher);
                rect(draw.scissor, hasher);
                effect(&draw.backdrop, hasher);
            }
            Command::Surface(draw) => {
                (2u8, draw.id, draw.frame, draw.opacity.to_bits()).hash(hasher);
                rect(Some(draw.bounds), hasher);
                affine(&draw.transform, hasher);
                draw.radii.to_array().map(f32::to_bits).hash(hasher);
                rect(draw.scissor, hasher);
            }
            Command::Group(group) => {
                (1u8, group.key, group.signature, group.offset).hash(hasher);
                group.opacity.to_bits().hash(hasher);
                rect(Some(group.bounds), hasher);
                rect(group.scissor, hasher);
                affine(&group.transform, hasher);
                effect(&group.effect, hasher);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub instance_bytes_uploaded: u64,
    pub damaged: u64,
    pub draw_calls: u32,
    pub layers: u32,
    pub backdrop_passes: u32,
    pub backdrop_blurs: u32,
    pub backdrops_reused: u32,
    pub groups: u32,
    pub group_passes: u32,
    pub content_blurs: u32,
    pub image_bytes_uploaded: u64,
    pub images: u32,
    pub surfaces: u32,
}

type Scissor = [u32; 4];
type SurfaceKey = (SurfaceId, BufferRef);

#[derive(Clone, Copy, PartialEq)]
struct Area {
    origin: (i32, i32),
    size: (u32, u32),
}

impl Area {
    fn scissor(&self, rect: Option<Rect>) -> Option<Scissor> {
        let Some(rect) = rect else {
            return Some([0, 0, self.size.0, self.size.1]);
        };
        let Area { origin, size } = self.within(&rect, f32::round, f32::round)?;
        Some([
            (origin.0 - self.origin.0) as u32,
            (origin.1 - self.origin.1) as u32,
            size.0,
            size.1,
        ])
    }

    fn shifted(&self, (x, y): (i32, i32)) -> Area {
        Area {
            origin: (self.origin.0 - x, self.origin.1 - y),
            size: self.size,
        }
    }

    fn cover(&self, rect: &Rect) -> Option<Area> {
        self.within(rect, f32::floor, f32::ceil)
    }

    fn within(&self, rect: &Rect, low: fn(f32) -> f32, high: fn(f32) -> f32) -> Option<Area> {
        let max = rect.max();
        let span = |min: f32, max: f32, origin: i32, size: u32| {
            let (start, end) = (origin as f32, origin as f32 + size as f32);
            let lo = low(min).clamp(start, end) as i32;
            let hi = high(max).clamp(start, end) as i32;
            (lo, hi.saturating_sub(lo).max(0) as u32)
        };
        let (x, width) = span(rect.origin.x, max.x, self.origin.0, self.size.0);
        let (y, height) = span(rect.origin.y, max.y, self.origin.1, self.size.1);
        (width > 0 && height > 0).then_some(Area {
            origin: (x, y),
            size: (width, height),
        })
    }

    fn rect(&self) -> Rect {
        Rect::new(
            self.origin.0 as f32,
            self.origin.1 as f32,
            self.size.0 as f32,
            self.size.1 as f32,
        )
    }

    fn surface(&self) -> [f32; 4] {
        [
            self.size.0 as f32,
            self.size.1 as f32,
            self.origin.0 as f32,
            self.origin.1 as f32,
        ]
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Kernel {
    levels: u32,
    spread: f32,
}

impl Kernel {
    fn new(radius: f32) -> Kernel {
        if radius < 0.05 {
            return Kernel::default();
        }
        let levels = (radius.log2().ceil().max(1.0) as u32).min(MAX_BLUR_LEVELS);
        Kernel {
            levels,
            spread: (radius / (1u32 << levels) as f32).min(1.0),
        }
    }

    fn reach(self) -> f32 {
        if self.levels == 0 {
            0.0
        } else {
            (3u32 << self.levels) as f32
        }
    }

    fn result_level(self) -> u32 {
        u32::from(self.levels >= 4)
    }

    fn shift(self) -> u32 {
        if self.levels == 0 {
            0
        } else {
            self.result_level() + 1
        }
    }

    fn state(self) -> (u32, u32) {
        (self.levels, self.spread.to_bits())
    }
}

fn texels(origin: (f32, f32), size: (f32, f32), dims: (u32, u32)) -> [f32; 4] {
    let (width, height) = (dims.0 as f32, dims.1 as f32);
    [
        origin.0 / width,
        origin.1 / height,
        size.0 / width,
        size.1 / height,
    ]
}

fn texel_bounds(origin: (f32, f32), size: (f32, f32), dims: (u32, u32), inset: f32) -> [f32; 4] {
    texels(
        (origin.0 + inset, origin.1 + inset),
        (origin.0 + size.0 - inset, origin.1 + size.1 - inset),
        dims,
    )
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct Mapping {
    map: [f32; 4],
    bounds: [f32; 4],
}

impl Mapping {
    fn new(origin: (f32, f32), size: (u32, u32), used: (u32, u32), dims: (u32, u32)) -> Mapping {
        let scale = |used: u32, size: u32, dims: u32| used as f32 / (size as f32 * dims as f32);
        Mapping {
            map: [
                origin.0,
                origin.1,
                scale(used.0, size.0, dims.0),
                scale(used.1, size.1, dims.1),
            ],
            bounds: texel_bounds((0.0, 0.0), (used.0 as f32, used.1 as f32), dims, 0.0),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Layer {
    matrix: [f32; 4],
    placement: [f32; 4],
    surface: [f32; 4],
    radii: [f32; 4],
}

impl Layer {
    fn new(transform: &Affine, opacity: f32, area: Area, scale: f32, radii: Corners) -> Layer {
        Layer {
            matrix: matrix(transform),
            placement: [transform.tx, transform.ty, opacity, scale],
            surface: area.surface(),
            radii: radii.to_array(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DrawRecord {
    layer: Layer,
    fill: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct EffectRecord {
    layer: Layer,
    rect: [f32; 4],
    inverse: [f32; 4],
    bounds: [f32; 4],
    content: Mapping,
    backdrop: Mapping,
    margin: [f32; 4],
    params: [f32; 16],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RegionRecord {
    rect: [f32; 4],
    bounds: [f32; 4],
    options: [f32; 4],
}

#[derive(Clone, Copy, PartialEq)]
struct Record {
    page: u32,
    row: u32,
}

impl Record {
    fn vertices(self) -> Range<u32> {
        self.row * 4..self.row * 4 + 4
    }
}

#[derive(Default)]
struct Records {
    bytes: Vec<u8>,
}

impl Records {
    fn push<T: Pod>(&mut self, record: &T) -> Record {
        let record = bytes_of(record);
        let mut start = self.bytes.len();
        let page = start - start % PAGE_BYTES;
        if start + record.len() > page + PAGE_BYTES {
            start = page + PAGE_BYTES;
            self.bytes.resize(start, 0);
        }
        self.bytes.extend_from_slice(record);
        Record {
            page: (start - start % PAGE_BYTES) as u32,
            row: (start % PAGE_BYTES / ROW_BYTES) as u32,
        }
    }
}

struct Source {
    origin: (f32, f32),
    dims: (u32, u32),
    valid: Rect,
    zero_outside: bool,
}

struct BlurGeometry {
    origin: (f32, f32),
    size: (u32, u32),
    dims: (u32, u32),
    mapping: Mapping,
}

impl BlurGeometry {
    fn new(source: &Source, area: Rect, kernel: Kernel) -> Option<BlurGeometry> {
        let step = (1u32 << kernel.levels) as f32;
        let area = area.intersect(&source.valid.inflate(kernel.reach()));
        if area.is_empty() {
            return None;
        }
        let max = area.max();
        let origin = (
            (area.origin.x / step).floor() * step,
            (area.origin.y / step).floor() * step,
        );
        let size = (
            (((max.x - origin.0) / step).ceil() * step) as u32,
            (((max.y - origin.1) / step).ceil() * step) as u32,
        );
        let shift = kernel.shift();
        let used = ((size.0 >> shift).max(1), (size.1 >> shift).max(1));
        let dims = (
            used.0.next_multiple_of(BLUR_TEXTURE_STEP),
            used.1.next_multiple_of(BLUR_TEXTURE_STEP),
        );
        Some(BlurGeometry {
            origin,
            size,
            dims,
            mapping: Mapping::new(origin, size, used, dims),
        })
    }

    fn passes(
        &self,
        source: &Source,
        kernel: Kernel,
        (from, to): (Attachment, Attachment),
        records: &mut Records,
        scratch: (u32, u32),
        out: &mut Vec<BlurPass>,
    ) -> Range<u32> {
        let start = out.len() as u32;
        let offset = (
            self.origin.0 - source.origin.0,
            self.origin.1 - source.origin.1,
        );
        let valid = source
            .valid
            .translate(Point::new(-source.origin.0, -source.origin.1));
        let mut region = RegionRecord {
            rect: texels(
                offset,
                (self.size.0 as f32, self.size.1 as f32),
                source.dims,
            ),
            bounds: texel_bounds(
                (valid.origin.x, valid.origin.y),
                (valid.size.width, valid.size.height),
                source.dims,
                0.5,
            ),
            options: [
                kernel.spread,
                f32::from(u8::from(source.zero_outside)),
                0.0,
                0.0,
            ],
        };
        if kernel.levels == 0 {
            out.push(BlurPass {
                kind: BlurKind::Copy,
                record: records.push(&region),
                size: self.size,
                source: from,
                destination: to,
                kept: None,
            });
            return start..start + 1;
        }
        let down = (0..kernel.levels).map(|level| (BlurKind::Down, level));
        let up = (kernel.result_level()..kernel.levels - 1)
            .rev()
            .map(|level| (BlurKind::Up, level));
        let count = kernel.levels + (kernel.levels - 1 - kernel.result_level());
        let mut source = from;
        for (index, (kind, level)) in (0..).zip(down.chain(up)) {
            let size = (
                (self.size.0 >> (level + 1)).max(1),
                (self.size.1 >> (level + 1)).max(1),
            );
            let destination = if index + 1 < count {
                Attachment::Scratch(level)
            } else {
                to
            };
            out.push(BlurPass {
                kind,
                record: records.push(&region),
                size,
                source,
                destination,
                kept: None,
            });
            source = destination;
            let used = (size.0 as f32, size.1 as f32);
            let dims = scratch_dims(scratch, level);
            region = RegionRecord {
                rect: texels((0.0, 0.0), used, dims),
                bounds: texel_bounds((0.0, 0.0), used, dims, 0.5),
                options: [kernel.spread, 0.0, 0.0, 0.0],
            };
        }
        start..out.len() as u32
    }
}

fn scratch_dims(scratch: (u32, u32), level: u32) -> (u32, u32) {
    (
        (scratch.0 >> (level + 1)).max(1),
        (scratch.1 >> (level + 1)).max(1),
    )
}

#[derive(Clone, Copy)]
enum BlurKind {
    Copy,
    Down,
    Up,
}

#[derive(Clone, Copy)]
enum Attachment {
    Scene,
    Backdrop(usize),
    Clean(usize),
    Group(u64),
    GroupBlur(u64),
    Scratch(u32),
}

struct BlurPass {
    kind: BlurKind,
    record: Record,
    size: (u32, u32),
    source: Attachment,
    destination: Attachment,
    kept: Option<(f32, f32)>,
}

#[derive(Clone, Copy)]
enum Content {
    None,
    Group { key: u64, blurred: bool },
    Surface(SurfaceKey),
}

struct DrawBatch {
    image: Option<u64>,
    instances: Range<u32>,
}

enum Step {
    Draw {
        record: Record,
        scissor: Scissor,
        batches: Range<u32>,
    },
    Effect {
        pipeline: u32,
        record: Record,
        content: Content,
        backdrop: Option<usize>,
        scissor: Scissor,
    },
    Snapshot(Range<u32>),
}

enum Pass {
    Raster { key: u64, steps: Range<u32> },
    Blur(Range<u32>),
}

#[derive(Default)]
struct Plan {
    records: Records,
    pending: Vec<Step>,
    steps: Vec<Step>,
    passes: Vec<Pass>,
    snapshots: Vec<BlurPass>,
    blurs: Vec<BlurPass>,
    batches: Vec<DrawBatch>,
    since: Vec<Rect>,
    scale: f32,
    viewport: Rect,
    scratch: (u32, u32),
}

impl Plan {
    fn begin(&mut self, scale: f32, area: Area) {
        self.records.bytes.clear();
        self.pending.clear();
        self.steps.clear();
        self.passes.clear();
        self.snapshots.clear();
        self.blurs.clear();
        self.batches.clear();
        self.since.clear();
        self.scale = scale;
        self.viewport = area.rect();
        let pad = |extent: u32| (extent + SCRATCH_PADDING).next_multiple_of(GROUP_TEXTURE_STEP);
        self.scratch = (pad(area.size.0), pad(area.size.1));
    }

    fn blurs(&self) -> bool {
        let blurred = |pass: &BlurPass| !matches!(pass.kind, BlurKind::Copy);
        self.snapshots.iter().any(blurred) || self.blurs.iter().any(blurred)
    }
}

struct Children<'a> {
    scene: Scene<'a>,
    range: Range<usize>,
    surfaces: &'a Surfaces,
}

#[derive(Clone, Copy, PartialEq)]
struct Reads {
    region: Rect,
    reach: f32,
}

struct Behind {
    base: u64,
    drawn: Vec<Drawn>,
    reads: Option<Reads>,
    open: Option<usize>,
    backdrops: usize,
}

#[derive(Clone, Copy)]
struct Drawn {
    fingerprint: u64,
    extent: Option<Rect>,
    reads: Option<Reads>,
}

struct Placed<'a> {
    draw: &'a EffectDraw,
    transform: Affine,
    opacity: f32,
    scissor: Scissor,
    content: Content,
    quad: Rect,
    mapping: Mapping,
    margin: f32,
}

struct GroupTarget {
    target: Option<Target>,
    blurred: Option<Target>,
    blur: (u32, u32),
    area: Area,
    signature: u64,
    used: u64,
}

struct BackdropTarget {
    target: Option<Target>,
    clean: Option<Target>,
    signature: u64,
}

struct Destination<'a> {
    view: &'a TextureView,
    clear: Clear<'a>,
    scene: Option<&'a Target>,
}

enum Clear<'a> {
    All(ClearColor),
    Within(&'a [Scissor], Record),
}

enum Redraw {
    Unchanged,
    Full,
    Partial(Vec<Scissor>),
}

impl Redraw {
    fn pixels(&self, area: Area) -> u64 {
        let pixels = |size: (u32, u32)| u64::from(size.0) * u64::from(size.1);
        match self {
            Redraw::Unchanged => 0,
            Redraw::Full => pixels(area.size),
            Redraw::Partial(clips) => clips.iter().map(|clip| pixels((clip[2], clip[3]))).sum(),
        }
    }
}

struct Target {
    texture: Texture,
    view: TextureView,
    bind_group: BindGroup,
}

impl Target {
    fn dims(&self) -> (u32, u32) {
        (self.texture.width(), self.texture.height())
    }
}

struct Textures {
    layout: BindGroupLayout,
    sampler: Sampler,
    format: TextureFormat,
}

impl Textures {
    fn bind(&self, device: &Device, label: &str, view: &TextureView) -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some(label),
            layout: &self.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    fn target(
        &self,
        device: &Device,
        label: &str,
        size: (u32, u32),
        format: TextureFormat,
        usage: TextureUsages,
    ) -> Target {
        let usage = usage | TextureUsages::TEXTURE_BINDING;
        let texture = create_texture(device, label, size, format, usage);
        let view = texture.create_view(&TextureViewDescriptor::default());
        let bind_group = self.bind(device, label, &view);
        Target {
            texture,
            view,
            bind_group,
        }
    }

    fn offscreen(&self, device: &Device, label: &str, size: (u32, u32)) -> Target {
        self.target(
            device,
            label,
            size,
            self.format,
            TextureUsages::RENDER_ATTACHMENT,
        )
    }

    fn ensure(
        &self,
        device: &Device,
        slot: &mut Option<Target>,
        label: &str,
        size: (u32, u32),
    ) -> bool {
        if slot.as_ref().is_some_and(|target| target.dims() == size) {
            return false;
        }
        *slot = Some(self.offscreen(device, label, size));
        true
    }
}

struct Slot {
    start: u32,
    capacity: u32,
    revision: u64,
    used: u64,
}

struct Instances {
    buffer: Buffer,
    used: u64,
    slots: FxHashMap<u64, Slot>,
}

impl Instances {
    fn new(device: &Device) -> Self {
        Instances {
            buffer: instance_buffer(device, INSTANCE_MIN_BYTES),
            used: 0,
            slots: FxHashMap::default(),
        }
    }

    fn sync(&mut self, device: &Device, queue: &Queue, list: &DrawList, frame: u64) -> (u32, u64) {
        let bytes: &[u8] = cast_slice(list.instances());
        let uploaded = bytes.len() as u64;
        let len = list.len() as u32;
        if let Some(slot) = self.slots.get_mut(&list.id()) {
            slot.used = frame;
            if slot.revision == list.revision() {
                return (slot.start, 0);
            }
            if len <= slot.capacity {
                slot.revision = list.revision();
                queue.write_buffer(&self.buffer, u64::from(slot.start) * Instance::SIZE, bytes);
                return (slot.start, uploaded);
            }
        }
        let capacity = len.next_power_of_two();
        let offset = self.reserve(device, queue, u64::from(capacity) * Instance::SIZE);
        queue.write_buffer(&self.buffer, offset, bytes);
        let start = (offset / Instance::SIZE) as u32;
        self.slots.insert(
            list.id(),
            Slot {
                start,
                capacity,
                revision: list.revision(),
                used: frame,
            },
        );
        (start, uploaded)
    }

    fn reserve(&mut self, device: &Device, queue: &Queue, bytes: u64) -> u64 {
        let offset = self.used;
        self.used += bytes;
        if self.used > self.buffer.size() {
            let buffer = instance_buffer(device, self.used.next_power_of_two());
            if offset > 0 {
                let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("rouge instances grow"),
                });
                encoder.copy_buffer_to_buffer(&self.buffer, 0, &buffer, 0, offset);
                queue.submit([encoder.finish()]);
            }
            self.buffer = buffer;
        }
        offset
    }

    fn maintain(&mut self, frame: u64) {
        let mut live = 0;
        self.slots.retain(|_, slot| {
            let keep = frame - slot.used < IDLE_FRAMES;
            live += u64::from(keep) * u64::from(slot.capacity) * Instance::SIZE;
            keep
        });
        if self.used > INSTANCE_MIN_BYTES && self.used > live * 2 {
            self.slots.clear();
            self.used = 0;
        }
    }
}

struct CachedImage {
    target: Target,
    bytes: u64,
    used: u64,
}

#[derive(Default)]
struct Images {
    cached: FxHashMap<u64, CachedImage>,
    bytes: u64,
    idle: Vec<(u64, u64)>,
}

impl Images {
    fn evict(&mut self, frame: u64) {
        let bytes = &mut self.bytes;
        self.cached.retain(|_, image| {
            let keep = frame - image.used < IDLE_FRAMES;
            *bytes -= u64::from(!keep) * image.bytes;
            keep
        });
        if self.bytes <= IMAGE_BUDGET_BYTES {
            return;
        }
        self.idle.clear();
        let idle = self.cached.iter().filter(|(_, image)| image.used < frame);
        self.idle.extend(idle.map(|(id, image)| (image.used, *id)));
        self.idle.sort_unstable();
        for (_, id) in &self.idle {
            if self.bytes <= IMAGE_BUDGET_BYTES {
                break;
            }
            if let Some(image) = self.cached.remove(id) {
                self.bytes -= image.bytes;
            }
        }
    }
}

#[derive(Default)]
struct Bound<'a> {
    pipeline: Option<&'a RenderPipeline>,
    groups: [Option<&'a BindGroup>; 3],
    page: Option<u32>,
    scissor: Option<Scissor>,
    instances: bool,
}

impl<'a> Bound<'a> {
    fn pipeline(&mut self, pass: &mut RenderPass<'_>, pipeline: &'a RenderPipeline) {
        if rebind(&mut self.pipeline, pipeline) {
            pass.set_pipeline(pipeline);
        }
    }

    fn group(&mut self, pass: &mut RenderPass<'_>, index: u32, group: &'a BindGroup) {
        if rebind(&mut self.groups[index as usize], group) {
            pass.set_bind_group(index, group, &[]);
        }
    }

    fn page(&mut self, pass: &mut RenderPass<'_>, records: &BindGroup, page: u32) {
        if self.page.replace(page) != Some(page) {
            pass.set_bind_group(1, records, &[page]);
        }
    }

    fn scissor(&mut self, pass: &mut RenderPass<'_>, scissor: Scissor) {
        if self.scissor.replace(scissor) != Some(scissor) {
            pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
        }
    }
}

fn clipped(scissor: Scissor, clips: Option<&[Scissor]>) -> impl Iterator<Item = Scissor> + '_ {
    let all = clips.is_none().then_some(scissor);
    let each = clips.into_iter().flatten().filter_map(move |clip| {
        let (x, y) = (scissor[0].max(clip[0]), scissor[1].max(clip[1]));
        let right = (scissor[0] + scissor[2]).min(clip[0] + clip[2]);
        let bottom = (scissor[1] + scissor[3]).min(clip[1] + clip[3]);
        (right > x && bottom > y).then(|| [x, y, right - x, bottom - y])
    });
    all.into_iter().chain(each)
}

fn rebind<'a, T>(slot: &mut Option<&'a T>, value: &'a T) -> bool {
    let changed = !slot.is_some_and(|bound| ptr::eq(bound, value));
    *slot = Some(value);
    changed
}

pub struct Renderer {
    device: Device,
    queue: Queue,
    textures: Textures,
    records_layout: BindGroupLayout,
    records: Buffer,
    records_group: BindGroup,
    primitive: RenderPipeline,
    fullscreen: [RenderPipeline; 3],
    fill: RenderPipeline,
    effect_layout: PipelineLayout,
    effects: Vec<(&'static str, RenderPipeline)>,
    instances: Instances,
    groups: FxHashMap<u64, GroupTarget>,
    backdrops: Vec<BackdropTarget>,
    scene: Option<Target>,
    scratch: [Option<Target>; MAX_BLUR_LEVELS as usize],
    blank: Target,
    atlas: Option<Target>,
    images: Images,
    surface_binds: FxHashMap<SurfaceKey, BindGroup>,
    plan: Plan,
    damage: Option<Damage>,
    frame: u64,
    stats: RenderStats,
}

impl Renderer {
    pub fn new(device: &Device, queue: &Queue, format: TextureFormat) -> Self {
        let records_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("rouge records layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: BufferSize::new(PAGE_BYTES as u64),
                },
                count: None,
            }],
        });
        let (records, records_group) = records_buffer(device, &records_layout, PAGE_BYTES as u64);
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("rouge texture layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("rouge sampler"),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..Default::default()
        });
        let pipeline_layout = |groups: &[Option<&BindGroupLayout>]| {
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("rouge pipeline layout"),
                bind_group_layouts: groups,
                immediate_size: 0,
            })
        };
        let base_layout = pipeline_layout(&[Some(&layout), Some(&records_layout)]);
        let effect_layout = pipeline_layout(&[Some(&layout), Some(&records_layout), Some(&layout)]);
        let blend = Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let primitive_module = shader_module(device, "rouge primitive", PRIMITIVE_WGSL);
        let primitive = create_pipeline(
            device,
            &PipelineDesc {
                layout: &base_layout,
                module: &primitive_module,
                fragment: "fs_main",
                format,
                blend,
                buffers: &[Some(VertexBufferLayout {
                    array_stride: Instance::SIZE,
                    step_mode: VertexStepMode::Instance,
                    attributes: &Instance::ATTRIBUTES,
                })],
            },
        );
        let fullscreen_module = shader_module(device, "rouge fullscreen", FULLSCREEN_WGSL);
        let fullscreen = |fragment| {
            create_pipeline(
                device,
                &PipelineDesc {
                    layout: &base_layout,
                    module: &fullscreen_module,
                    fragment,
                    format,
                    blend: None,
                    buffers: &[],
                },
            )
        };
        let textures = Textures {
            layout,
            sampler,
            format,
        };
        let blank = textures.offscreen(device, "rouge blank", (1, 1));

        Renderer {
            device: device.clone(),
            queue: queue.clone(),
            records_layout,
            records,
            records_group,
            primitive,
            fill: fullscreen("fs_fill"),
            fullscreen: ["fs_blit", "fs_down", "fs_up"].map(fullscreen),
            effect_layout,
            effects: Vec::new(),
            instances: Instances::new(device),
            groups: FxHashMap::default(),
            backdrops: Vec::new(),
            scene: None,
            scratch: Default::default(),
            blank,
            atlas: None,
            images: Images::default(),
            surface_binds: FxHashMap::default(),
            plan: Plan::default(),
            damage: None,
            frame: 0,
            stats: RenderStats::default(),
            textures,
        }
    }

    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    fn sync_image(&mut self, image: &ImageData) {
        if let Some(cached) = self.images.cached.get_mut(&image.id()) {
            cached.used = self.frame;
            return;
        }
        let format = match image.format() {
            ImageFormat::Rgba8 => TextureFormat::Rgba8Unorm,
            ImageFormat::Alpha8 => TextureFormat::R8Unorm,
        };
        let size = (image.width(), image.height());
        let target = self.textures.target(
            &self.device,
            "rouge image",
            size,
            format,
            TextureUsages::COPY_DST,
        );
        let bytes_per_row = size.0 * image.format().bytes_per_pixel();
        write_rows(
            &self.queue,
            &target.texture,
            0,
            image.pixels(),
            bytes_per_row,
        );
        self.stats.image_bytes_uploaded += image.byte_len();
        self.images.bytes += image.byte_len();
        self.images.cached.insert(
            image.id(),
            CachedImage {
                target,
                bytes: image.byte_len(),
                used: self.frame,
            },
        );
    }

    pub fn upload_atlas(&mut self, uploads: &AtlasUploads) {
        if !uploads.is_empty() {
            self.write_atlas(uploads.height, uploads.start, &uploads.rows);
        }
    }

    fn write_atlas(&mut self, height: u32, start: u32, rows: &[u8]) {
        let kept = self.atlas.take();
        let kept = kept.filter(|target| target.texture.height() == height);
        let target = kept.unwrap_or_else(|| {
            self.textures.target(
                &self.device,
                "rouge glyph atlas",
                (Atlas::WIDTH, height),
                TextureFormat::R8Unorm,
                TextureUsages::COPY_DST,
            )
        });
        if !rows.is_empty() {
            write_rows(&self.queue, &target.texture, start, rows, Atlas::WIDTH);
        }
        self.atlas = Some(target);
    }

    fn sync_list(&mut self, list: &DrawList) -> u32 {
        let (start, uploaded) = self
            .instances
            .sync(&self.device, &self.queue, list, self.frame);
        self.stats.instance_bytes_uploaded += uploaded;
        start
    }

    fn sync_scratch(&mut self, scratch: (u32, u32)) {
        for (level, slot) in (0..).zip(&mut self.scratch) {
            let dims = scratch_dims(scratch, level);
            self.textures
                .ensure(&self.device, slot, "rouge blur scratch", dims);
        }
    }

    fn sync_records(&mut self, records: &Records) {
        let needed = records.bytes.len().next_multiple_of(PAGE_BYTES) as u64;
        if needed > self.records.size() {
            (self.records, self.records_group) = records_buffer(
                &self.device,
                &self.records_layout,
                needed.next_power_of_two(),
            );
        }
        self.queue.write_buffer(&self.records, 0, &records.bytes);
    }

    fn effect_pipeline(&mut self, wgsl: &'static str) -> u32 {
        if let Some(index) = self.effects.iter().position(|(key, _)| ptr::eq(*key, wgsl)) {
            return index as u32;
        }
        let source = format!("{EFFECT_WGSL}\n{wgsl}");
        let module = shader_module(&self.device, "rouge effect", &source);
        let pipeline = create_pipeline(
            &self.device,
            &PipelineDesc {
                layout: &self.effect_layout,
                module: &module,
                fragment: "fs_main",
                format: self.textures.format,
                blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                buffers: &[],
            },
        );
        let index = self.effects.len() as u32;
        self.effects.push((wgsl, pipeline));
        index
    }

    fn plan_items(
        &mut self,
        scene: Scene<'_>,
        range: Range<usize>,
        area: Area,
        mut behind: Option<&mut Behind>,
        plan: &mut Plan,
        surfaces: &Surfaces,
    ) -> Range<u32> {
        let mark = plan.pending.len();
        let mut index = range.start;
        while index < range.end {
            let next = scene.next(index);
            let shown = match &scene.commands[index] {
                Command::Draw(draw) => {
                    self.plan_layer(scene.lists, draw, area, behind.as_deref_mut(), plan)
                }
                Command::Surface(draw) => {
                    self.plan_surface(draw, area, plan, surfaces);
                    true
                }
                Command::Group(group) => {
                    let children = Children {
                        scene,
                        range: index + 1..next,
                        surfaces,
                    };
                    self.plan_group(group, children, area, behind.as_deref_mut(), plan)
                }
            };
            if let Some(behind) = behind.as_deref_mut() {
                let reads = behind.reads.take();
                if shown {
                    let mut hasher = FxHasher::default();
                    scene.fingerprint(index, &mut hasher);
                    let extent = scene.extent(index, plan.scale);
                    behind.drawn.push(Drawn {
                        fingerprint: hasher.finish(),
                        extent,
                        reads,
                    });
                    plan.since.extend(extent);
                }
            }
            index = next;
        }
        let start = plan.steps.len() as u32;
        let Plan { pending, steps, .. } = plan;
        steps.extend(pending.drain(mark..));
        start..steps.len() as u32
    }

    fn plan_layer(
        &mut self,
        lists: &DrawLists,
        draw: &LayerDraw,
        area: Area,
        behind: Option<&mut Behind>,
        plan: &mut Plan,
    ) -> bool {
        let Some(scissor) = area.scissor(draw.scissor).filter(|_| draw.opacity > 0.0) else {
            return false;
        };
        let list = draw.list.and_then(|key| lists.get(key));
        let start = list
            .filter(|list| !list.is_empty())
            .map(|list| self.sync_list(list));
        for batch in list.map_or(&[][..], DrawList::batches) {
            if let Some(image) = &batch.image {
                self.sync_image(image);
            }
        }
        if let Some(backdrop) = &draw.backdrop {
            let margin = backdrop.effect.margin();
            let placed = Placed {
                draw: backdrop,
                transform: draw.transform,
                opacity: draw.opacity,
                scissor,
                content: Content::None,
                quad: device_rect(
                    &draw.transform,
                    &backdrop.bounds.inflate(margin),
                    plan.scale,
                ),
                mapping: Mapping::default(),
                margin,
            };
            self.plan_effect(placed, area, behind, plan);
        }
        let extent = list
            .and_then(DrawList::extent)
            .map(|extent| device_rect(&draw.transform, &extent, plan.scale).inflate(1.0));
        let bounded = match (draw.scissor, extent) {
            (Some(scissor), Some(extent)) => Some(scissor.intersect(&extent)),
            (scissor, extent) => scissor.or(extent),
        };
        if let (Some(start), Some(list), Some(scissor)) = (start, list, area.scissor(bounded)) {
            let record = plan.records.push(&DrawRecord {
                layer: Layer::new(&draw.transform, draw.opacity, area, plan.scale, draw.radii),
                fill: draw.fill,
            });
            let first = plan.batches.len() as u32;
            plan.batches
                .extend(list.batches().iter().map(|batch| DrawBatch {
                    image: batch.image.as_ref().map(ImageData::id),
                    instances: start + batch.instances.start..start + batch.instances.end,
                }));
            plan.pending.push(Step::Draw {
                record,
                scissor,
                batches: first..plan.batches.len() as u32,
            });
        }
        true
    }

    fn plan_surface(
        &mut self,
        draw: &SurfaceDraw,
        area: Area,
        plan: &mut Plan,
        surfaces: &Surfaces,
    ) {
        let Some(scissor) = area.scissor(draw.scissor).filter(|_| draw.opacity > 0.0) else {
            return;
        };
        let Some(latched) = surfaces.latched(draw.id) else {
            return;
        };
        let key = (draw.id, latched.buffer);
        if let Entry::Vacant(entry) = self.surface_binds.entry(key) {
            let view = latched
                .texture
                .create_view(&TextureViewDescriptor::default());
            entry.insert(self.textures.bind(&self.device, "rouge surface", &view));
        }
        let dims = (latched.texture.width(), latched.texture.height());
        let region = latched.region;
        let origin = (region.origin.x, region.origin.y);
        let size = (region.size.width, region.size.height);
        let [x, y, width, height] = texels(origin, size, dims);
        let map = match latched.origin {
            Origin::TopLeft => [x, y, width, height],
            Origin::BottomLeft => [x, y + height, width, -height],
        };
        let sample = EffectDraw {
            corner_radii: draw.radii,
            ..EffectDraw::new(
                &SurfaceSample {
                    opaque: latched.alpha == AlphaMode::Opaque,
                },
                draw.bounds,
            )
        };
        let placed = Placed {
            draw: &sample,
            transform: draw.transform,
            opacity: draw.opacity,
            scissor,
            content: Content::Surface(key),
            quad: device_rect(&draw.transform, &draw.bounds, plan.scale).inflate(1.0),
            mapping: Mapping {
                map,
                bounds: texel_bounds(origin, size, dims, 0.5),
            },
            margin: sample.effect.margin(),
        };
        self.plan_effect(placed, area, None, plan);
    }

    fn plan_group(
        &mut self,
        group: &Group,
        children: Children<'_>,
        area: Area,
        behind: Option<&mut Behind>,
        plan: &mut Plan,
    ) -> bool {
        let local = area.shifted(group.offset);
        let Some(cover) = local.cover(&group.bounds).filter(|_| group.opacity > 0.0) else {
            return false;
        };
        let Some(scissor) = area.scissor(group.scissor) else {
            return false;
        };
        let needed = (
            cover.size.0.next_multiple_of(GROUP_TEXTURE_STEP),
            cover.size.1.next_multiple_of(GROUP_TEXTURE_STEP),
        );
        let signature = group.signature;
        let (textures, device) = (&self.textures, &self.device);
        let entry = self.groups.entry(group.key).or_insert_with(|| GroupTarget {
            target: None,
            blurred: None,
            blur: (0, 0),
            area: cover,
            signature,
            used: 0,
        });
        let resized = textures.ensure(device, &mut entry.target, "rouge group", needed);
        let fresh = !resized && entry.signature == signature && entry.area == cover;
        entry.area = cover;
        entry.signature = signature;
        entry.used = self.frame;

        let passthrough = EffectDraw::new(&Passthrough, Rect::ZERO);
        let (draw, transform) = group
            .effect
            .as_ref()
            .map_or((&passthrough, Affine::IDENTITY), |effect| {
                (effect, group.transform)
            });
        let kernel = Kernel::new(draw.effect.content_blur() * plan.scale);
        let source = Source {
            origin: (cover.origin.0 as f32, cover.origin.1 as f32),
            dims: needed,
            valid: cover.rect(),
            zero_outside: true,
        };
        let geometry = (kernel.levels > 0)
            .then(|| BlurGeometry::new(&source, cover.rect().inflate(kernel.reach()), kernel))
            .flatten();
        let mut mapping = Mapping::new(source.origin, cover.size, cover.size, needed);
        let mut reach = 0.0;
        let mut reblur = false;
        if let Some(geometry) = &geometry {
            let created = textures.ensure(
                device,
                &mut entry.blurred,
                "rouge group blur",
                geometry.dims,
            );
            reblur = !fresh || created || entry.blur != kernel.state();
            entry.blur = kernel.state();
            (mapping, reach) = (geometry.mapping, kernel.reach());
        } else {
            entry.blurred = None;
        }
        if !fresh {
            let inner = Area {
                origin: cover.origin,
                size: needed,
            };
            let Children {
                scene,
                range,
                surfaces,
            } = children;
            let steps = self.plan_items(scene, range, inner, None, plan, surfaces);
            plan.passes.push(Pass::Raster {
                key: group.key,
                steps,
            });
        }
        if let Some(geometry) = geometry.as_ref().filter(|_| reblur) {
            let Plan {
                records,
                blurs,
                scratch,
                passes,
                ..
            } = plan;
            let slots = (
                Attachment::Group(group.key),
                Attachment::GroupBlur(group.key),
            );
            let range = geometry.passes(&source, kernel, slots, records, *scratch, blurs);
            passes.push(Pass::Blur(range));
        }
        let shift = group.shift();
        mapping.map[0] += shift.x;
        mapping.map[1] += shift.y;
        let placed = Placed {
            draw,
            transform,
            opacity: group.opacity,
            scissor,
            content: Content::Group {
                key: group.key,
                blurred: geometry.is_some(),
            },
            quad: cover
                .rect()
                .translate(shift)
                .inflate(draw.effect.margin() * plan.scale + reach),
            mapping,
            margin: UNBOUNDED_MARGIN,
        };
        self.plan_effect(placed, area, behind, plan);
        true
    }

    fn plan_effect(
        &mut self,
        placed: Placed<'_>,
        area: Area,
        behind: Option<&mut Behind>,
        plan: &mut Plan,
    ) {
        let Placed {
            draw,
            transform,
            opacity,
            scissor,
            content,
            quad,
            mapping,
            margin,
        } = placed;
        let effect = draw.effect;
        let (mut wgsl, mut inputs) = (effect.wgsl(), effect.inputs());
        if inputs.backdrop() && behind.is_none() {
            if !inputs.content() {
                return;
            }
            (wgsl, inputs) = (PASSTHROUGH_WGSL, Inputs::CONTENT);
        }
        let Some(inverse) = transform.inverse() else {
            return;
        };
        let scale = plan.scale;
        let mut backdrop = None;
        let mut backdrop_map = Mapping::default();
        if let Some(behind) = behind.filter(|_| inputs.backdrop()) {
            let kernel = Kernel::new(effect.backdrop_blur() * scale);
            let viewport = Source {
                origin: (0.0, 0.0),
                dims: (
                    plan.viewport.size.width as u32,
                    plan.viewport.size.height as u32,
                ),
                valid: plan.viewport,
                zero_outside: false,
            };
            let region = quad.inflate(kernel.reach()).intersect(&plan.viewport);
            let Some(geometry) = BlurGeometry::new(&viewport, region, kernel) else {
                return;
            };
            let stretch = transform
                .a
                .hypot(transform.b)
                .max(transform.c.hypot(transform.d));
            let slack = if kernel.levels == 0 {
                1.0
            } else {
                (4u32 << kernel.levels) as f32
            };
            let reach = effect
                .backdrop_reach()
                .map_or(f32::INFINITY, |reach| reach * scale * stretch + slack);
            behind.reads = Some(Reads { region, reach });
            let mut hasher = FxHasher::default();
            behind.base.hash(&mut hasher);
            for drawn in &behind.drawn {
                if (drawn.extent).is_none_or(|extent| !extent.intersect(&region).is_empty()) {
                    drawn.fingerprint.hash(&mut hasher);
                }
            }
            (
                geometry.origin.0.to_bits(),
                geometry.origin.1.to_bits(),
                geometry.size,
                kernel.state(),
            )
                .hash(&mut hasher);
            let signature = hasher.finish();
            let index = behind.backdrops;
            behind.backdrops += 1;
            if self.backdrops.len() <= index {
                self.backdrops.push(BackdropTarget {
                    target: None,
                    clean: None,
                    signature,
                });
            }
            let existing = &mut self.backdrops[index];
            let created = self.textures.ensure(
                &self.device,
                &mut existing.target,
                "rouge backdrop",
                geometry.dims,
            ) | self.textures.ensure(
                &self.device,
                &mut existing.clean,
                "rouge clean backdrop",
                geometry.size,
            );
            let cached = !created && existing.signature == signature;
            existing.signature = signature;
            if cached {
                self.stats.backdrops_reused += 1;
            } else {
                let Plan {
                    records,
                    snapshots,
                    scratch,
                    pending,
                    since,
                    ..
                } = plan;
                let start = snapshots.len() as u32;
                let size = (geometry.size.0 as f32, geometry.size.1 as f32);
                let copy = RegionRecord {
                    rect: texels(geometry.origin, size, viewport.dims),
                    bounds: texel_bounds(geometry.origin, size, viewport.dims, 0.0),
                    options: [0.0; 4],
                };
                snapshots.push(BlurPass {
                    kind: BlurKind::Copy,
                    record: records.push(&copy),
                    size: geometry.size,
                    source: Attachment::Scene,
                    destination: Attachment::Clean(index),
                    kept: Some(geometry.origin),
                });
                let copied = Source {
                    origin: geometry.origin,
                    dims: geometry.size,
                    valid: viewport.valid,
                    zero_outside: false,
                };
                let slots = (Attachment::Clean(index), Attachment::Backdrop(index));
                let range = geometry.passes(&copied, kernel, slots, records, *scratch, snapshots);
                let range = start..range.end;
                let clear = !since
                    .iter()
                    .any(|drawn| !drawn.intersect(&region).is_empty());
                let open = behind.open.filter(|_| clear);
                if let Some(Step::Snapshot(passes)) = open.and_then(|open| pending.get_mut(open)) {
                    passes.end = range.end;
                } else {
                    behind.open = Some(pending.len());
                    since.clear();
                    pending.push(Step::Snapshot(range));
                }
            }
            backdrop = Some(index);
            backdrop_map = geometry.mapping;
        }

        let record = plan.records.push(&EffectRecord {
            layer: Layer::new(&transform, opacity, area, scale, draw.corner_radii),
            rect: quad.to_array(),
            inverse: matrix(&inverse),
            bounds: draw.bounds.to_array(),
            content: mapping,
            backdrop: backdrop_map,
            margin: [margin, 0.0, 0.0, 0.0],
            params: effect.uniforms(),
        });
        let pipeline = self.effect_pipeline(wgsl);
        plan.pending.push(Step::Effect {
            pipeline,
            record,
            content,
            backdrop,
            scissor,
        });
    }

    fn attachment<'a>(&'a self, slot: Attachment, scene: Option<&'a Target>) -> Option<&'a Target> {
        match slot {
            Attachment::Scene => scene,
            Attachment::Backdrop(index) => self.backdrops.get(index)?.target.as_ref(),
            Attachment::Clean(index) => self.backdrops.get(index)?.clean.as_ref(),
            Attachment::Group(key) => self.groups.get(&key)?.target.as_ref(),
            Attachment::GroupBlur(key) => self.groups.get(&key)?.blurred.as_ref(),
            Attachment::Scratch(level) => self.scratch.get(level as usize)?.as_ref(),
        }
    }

    fn encode_blurs(
        &self,
        encoder: &mut CommandEncoder,
        passes: &[BlurPass],
        scene: Option<&Target>,
        clips: Option<&[Scissor]>,
        stats: &mut RenderStats,
    ) {
        for pass in passes {
            stats.backdrop_blurs += u32::from(matches!(pass.source, Attachment::Scene));
            stats.content_blurs += u32::from(matches!(pass.source, Attachment::Group(_)));
            let source = self.attachment(pass.source, scene);
            let destination = self.attachment(pass.destination, scene);
            let (Some(source), Some(destination)) = (source, destination) else {
                continue;
            };
            let kept = pass.kept.zip(clips);
            let load = if kept.is_some() {
                LoadOp::Load
            } else {
                LoadOp::Clear(ClearColor::TRANSPARENT)
            };
            let mut render = begin_pass(encoder, &destination.view, load);
            render.set_viewport(0.0, 0.0, pass.size.0 as f32, pass.size.1 as f32, 0.0, 1.0);
            render.set_pipeline(&self.fullscreen[pass.kind as usize]);
            render.set_bind_group(0, &source.bind_group, &[]);
            render.set_bind_group(1, &self.records_group, &[pass.record.page]);
            let Some((origin, clips)) = kept else {
                render.draw(pass.record.vertices(), 0..1);
                continue;
            };
            let bounds = Rect::new(0.0, 0.0, pass.size.0 as f32, pass.size.1 as f32);
            for clip in clips {
                let rect = Rect::new(
                    clip[0] as f32 - origin.0,
                    clip[1] as f32 - origin.1,
                    clip[2] as f32,
                    clip[3] as f32,
                )
                .intersect(&bounds);
                if rect.is_empty() {
                    continue;
                }
                render.set_scissor_rect(
                    rect.origin.x as u32,
                    rect.origin.y as u32,
                    rect.size.width as u32,
                    rect.size.height as u32,
                );
                render.draw(pass.record.vertices(), 0..1);
            }
        }
    }

    fn encode(
        &self,
        encoder: &mut CommandEncoder,
        destination: Destination<'_>,
        steps: Range<u32>,
        plan: &Plan,
        stats: &mut RenderStats,
    ) {
        let Some(atlas) = self.atlas.as_ref().map(|atlas| &atlas.bind_group) else {
            return;
        };
        let Destination { view, clear, scene } = destination;
        let blank = &self.blank.bind_group;
        let load = match clear {
            Clear::All(color) => LoadOp::Clear(color),
            Clear::Within(..) => LoadOp::Load,
        };
        let mut pass = begin_pass(encoder, view, load);
        let mut bound = Bound::default();
        let clips = match clear {
            Clear::All(_) => None,
            Clear::Within(clips, fill) => {
                for clip in clips {
                    bound.scissor(&mut pass, *clip);
                    bound.pipeline(&mut pass, &self.fill);
                    bound.group(&mut pass, 0, blank);
                    bound.page(&mut pass, &self.records_group, fill.page);
                    pass.draw(fill.vertices(), 0..1);
                }
                Some(clips)
            }
        };
        for step in &plan.steps[steps.start as usize..steps.end as usize] {
            match step {
                Step::Snapshot(passes) => {
                    drop(pass);
                    let passes = &plan.snapshots[passes.start as usize..passes.end as usize];
                    self.encode_blurs(encoder, passes, scene, clips, stats);
                    pass = begin_pass(encoder, view, LoadOp::Load);
                    bound = Bound::default();
                    stats.backdrop_passes += 1;
                }
                Step::Draw {
                    record,
                    scissor,
                    batches,
                } => {
                    bound.pipeline(&mut pass, &self.primitive);
                    bound.page(&mut pass, &self.records_group, record.page);
                    if !bound.instances {
                        pass.set_vertex_buffer(0, self.instances.buffer.slice(..));
                        bound.instances = true;
                    }
                    for scissor in clipped(*scissor, clips) {
                        bound.scissor(&mut pass, scissor);
                        for batch in &plan.batches[batches.start as usize..batches.end as usize] {
                            let texture = match batch.image {
                                Some(id) => match self.images.cached.get(&id) {
                                    Some(image) => &image.target.bind_group,
                                    None => continue,
                                },
                                None => atlas,
                            };
                            bound.group(&mut pass, 0, texture);
                            pass.draw(record.vertices(), batch.instances.clone());
                            stats.images += u32::from(batch.image.is_some());
                            stats.draw_calls += 1;
                        }
                    }
                    stats.layers += 1;
                }
                Step::Effect {
                    pipeline,
                    record,
                    content,
                    backdrop,
                    scissor,
                } => {
                    let texture = match content {
                        Content::None => blank,
                        Content::Group { key, blurred } => {
                            let slot = if *blurred {
                                Attachment::GroupBlur(*key)
                            } else {
                                Attachment::Group(*key)
                            };
                            self.attachment(slot, None)
                                .map_or(blank, |target| &target.bind_group)
                        }
                        Content::Surface(key) => match self.surface_binds.get(key) {
                            Some(texture) => texture,
                            None => continue,
                        },
                    };
                    let backdrop = backdrop
                        .and_then(|index| self.attachment(Attachment::Backdrop(index), None))
                        .map_or(blank, |target| &target.bind_group);
                    bound.pipeline(&mut pass, &self.effects[*pipeline as usize].1);
                    bound.group(&mut pass, 0, texture);
                    bound.page(&mut pass, &self.records_group, record.page);
                    bound.group(&mut pass, 2, backdrop);
                    for scissor in clipped(*scissor, clips) {
                        bound.scissor(&mut pass, scissor);
                        pass.draw(record.vertices(), 0..1);
                        stats.draw_calls += 1;
                    }
                    stats.groups += u32::from(matches!(content, Content::Group { .. }));
                    stats.surfaces += u32::from(matches!(content, Content::Surface(_)));
                }
            }
        }
    }

    pub fn render(
        &mut self,
        target: &TextureView,
        viewport: Viewport,
        clear: Color,
        scene: Scene<'_>,
        surfaces: &Surfaces,
    ) {
        self.stats = RenderStats::default();
        self.frame += 1;
        if self.atlas.is_none() {
            self.write_atlas(INITIAL_HEIGHT, 0, &[]);
        }

        let area = Area {
            origin: (0, 0),
            size: (viewport.width.max(1), viewport.height.max(1)),
        };
        let mut plan = take(&mut self.plan);
        plan.begin(viewport.scale, area);
        let mut base = FxHasher::default();
        (
            area.size,
            viewport.scale.to_bits(),
            clear.to_array().map(f32::to_bits),
        )
            .hash(&mut base);
        let mut behind = Behind {
            base: base.finish(),
            drawn: Vec::new(),
            reads: None,
            open: None,
            backdrops: 0,
        };
        let main = self.plan_items(
            scene,
            0..scene.commands.len(),
            area,
            Some(&mut behind),
            &mut plan,
            surfaces,
        );
        let frame = self.frame;
        let maintain = frame.is_multiple_of(MAINTENANCE_FRAMES);
        if maintain {
            self.groups
                .retain(|_, group| frame - group.used < IDLE_FRAMES);
            self.surface_binds
                .retain(|(id, buffer), _| surfaces.holds(*id, buffer.set));
        }
        self.backdrops.truncate(behind.backdrops);
        let blit = plan.records.push(&RegionRecord {
            rect: [0.0, 0.0, 1.0, 1.0],
            bounds: [0.0, 0.0, 1.0, 1.0],
            options: [0.0; 4],
        });
        let premultiplied = [clear.r, clear.g, clear.b, 1.0].map(|c| c * clear.a);
        let fill = plan.records.push(&RegionRecord {
            rect: [0.0, 0.0, 1.0, 1.0],
            bounds: [0.0, 0.0, 1.0, 1.0],
            options: premultiplied,
        });
        self.sync_records(&plan.records);
        if plan.blurs() {
            self.sync_scratch(plan.scratch);
        }
        let created = self
            .textures
            .ensure(&self.device, &mut self.scene, "rouge scene", area.size);
        let key = DamageKey {
            size: area.size,
            scale: viewport.scale.to_bits(),
            clear: clear.to_array().map(f32::to_bits),
        };
        let damage = Damage::new(key, behind.drawn);
        let previous = self.damage.take().filter(|_| !created);
        let redraw = damage.since(previous.as_ref(), area);
        self.damage = Some(damage);
        let scene = self.scene.as_ref();

        let descriptor = CommandEncoderDescriptor {
            label: Some("rouge frame"),
        };
        let mut encoder = self.device.create_command_encoder(&descriptor);
        let mut stats = self.stats;
        stats.damaged = redraw.pixels(area);
        for pass in &plan.passes {
            match pass {
                Pass::Raster { key, steps } => {
                    let Some(target) = self.attachment(Attachment::Group(*key), None) else {
                        continue;
                    };
                    let destination = Destination {
                        view: &target.view,
                        clear: Clear::All(ClearColor::TRANSPARENT),
                        scene: None,
                    };
                    self.encode(&mut encoder, destination, steps.clone(), &plan, &mut stats);
                    stats.group_passes += 1;
                }
                Pass::Blur(passes) => {
                    let passes = &plan.blurs[passes.start as usize..passes.end as usize];
                    self.encode_blurs(&mut encoder, passes, None, None, &mut stats);
                }
            }
        }
        let [r, g, b, a] = premultiplied.map(f64::from);
        let clear = match &redraw {
            Redraw::Unchanged => None,
            Redraw::Full => Some(Clear::All(ClearColor { r, g, b, a })),
            Redraw::Partial(clips) => Some(Clear::Within(clips, fill)),
        };
        if let (Some(scene), Some(clear)) = (scene, clear) {
            let destination = Destination {
                view: &scene.view,
                clear,
                scene: Some(scene),
            };
            self.encode(&mut encoder, destination, main, &plan, &mut stats);
        }
        if let Some(scene) = scene {
            let mut pass = begin_pass(&mut encoder, target, LoadOp::Clear(ClearColor::TRANSPARENT));
            pass.set_pipeline(&self.fullscreen[BlurKind::Copy as usize]);
            pass.set_bind_group(0, &scene.bind_group, &[]);
            pass.set_bind_group(1, &self.records_group, &[blit.page]);
            pass.draw(blit.vertices(), 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.stats = stats;
        if maintain {
            self.images.evict(frame);
            self.instances.maintain(frame);
        }
        self.plan = plan;
    }
}

pub fn device_rect(transform: &Affine, rect: &Rect, scale: f32) -> Rect {
    let rect = transform.apply_rect(rect);
    Rect::new(
        rect.origin.x * scale,
        rect.origin.y * scale,
        rect.size.width * scale,
        rect.size.height * scale,
    )
}

fn matrix(transform: &Affine) -> [f32; 4] {
    [transform.a, transform.b, transform.c, transform.d]
}

fn shader_module(device: &Device, label: &str, source: &str) -> ShaderModule {
    device.create_shader_module(ShaderModuleDescriptor {
        label: Some(label),
        source: ShaderSource::Wgsl(format!("{COMMON_WGSL}\n{source}").into()),
    })
}

fn instance_buffer(device: &Device, size: u64) -> Buffer {
    device.create_buffer(&BufferDescriptor {
        label: Some("rouge instances"),
        size,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

fn records_buffer(device: &Device, layout: &BindGroupLayout, size: u64) -> (Buffer, BindGroup) {
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("rouge records"),
        size,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("rouge records"),
        layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::Buffer(BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: BufferSize::new(PAGE_BYTES as u64),
            }),
        }],
    });
    (buffer, bind_group)
}

fn write_rows(queue: &Queue, texture: &Texture, y: u32, rows: &[u8], bytes_per_row: u32) {
    queue.write_texture(
        TexelCopyTextureInfo {
            origin: Origin3d { x: 0, y, z: 0 },
            ..texture.as_image_copy()
        },
        rows,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(bytes_per_row),
            rows_per_image: None,
        },
        Extent3d {
            width: texture.width(),
            height: rows.len() as u32 / bytes_per_row,
            depth_or_array_layers: 1,
        },
    );
}

#[derive(Clone, Copy, PartialEq)]
struct DamageKey {
    size: (u32, u32),
    scale: u32,
    clear: [u32; 4],
}

struct Damage {
    key: DamageKey,
    drawn: Vec<Drawn>,
}

impl Damage {
    fn new(key: DamageKey, drawn: Vec<Drawn>) -> Self {
        Damage { key, drawn }
    }

    fn readers(&self) -> impl Iterator<Item = (usize, Reads)> + '_ {
        (self.drawn.iter().enumerate()).filter_map(|(index, drawn)| Some((index, drawn.reads?)))
    }

    fn since(&self, previous: Option<&Damage>, area: Area) -> Redraw {
        let Some(previous) = previous.filter(|previous| {
            previous.key == self.key && previous.readers().count() == self.readers().count()
        }) else {
            return Redraw::Full;
        };
        let Some(rects) = changed(&previous.drawn, &self.drawn) else {
            return Redraw::Full;
        };
        let before: Vec<_> = previous.readers().collect();
        let after: Vec<_> = self.readers().collect();
        let clips: Vec<Scissor> = settle(rects, &before, &after, area.rect())
            .iter()
            .filter_map(|rect| area.cover(rect))
            .map(|cover| {
                [
                    cover.origin.0 as u32,
                    cover.origin.1 as u32,
                    cover.size.0,
                    cover.size.1,
                ]
            })
            .collect();
        if clips.is_empty() {
            Redraw::Unchanged
        } else if clips == [[0, 0, area.size.0, area.size.1]] {
            Redraw::Full
        } else {
            Redraw::Partial(clips)
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Depth {
    Under,
    Was(usize),
    Is(usize),
}

impl Depth {
    fn behind(self, (was, is): (usize, usize)) -> bool {
        match self {
            Depth::Under => true,
            Depth::Was(index) => index < was,
            Depth::Is(index) => index < is,
        }
    }
}

fn changed(before: &[Drawn], after: &[Drawn]) -> Option<Vec<(Rect, Depth)>> {
    let mut rects = Vec::new();
    let kept_before = unmatched(before, after, Depth::Was, &mut rects);
    let kept_after = unmatched(after, before, Depth::Is, &mut rects);
    (kept_before == kept_after).then_some(rects)
}

fn unmatched(
    from: &[Drawn],
    to: &[Drawn],
    depth: fn(usize) -> Depth,
    rects: &mut Vec<(Rect, Depth)>,
) -> Vec<u64> {
    let mut counts: FxHashMap<u64, usize> = FxHashMap::default();
    for drawn in to {
        *counts.entry(drawn.fingerprint).or_default() += 1;
    }
    let mut kept = Vec::new();
    for (index, drawn) in from.iter().enumerate() {
        match counts.get_mut(&drawn.fingerprint) {
            Some(count) if *count > 0 => {
                *count -= 1;
                kept.push(drawn.fingerprint);
            }
            _ => rects.extend(drawn.extent.map(|extent| (extent, depth(index)))),
        }
    }
    kept
}

fn settle(
    mut rects: Vec<(Rect, Depth)>,
    before: &[(usize, Reads)],
    after: &[(usize, Reads)],
    full: Rect,
) -> Vec<Rect> {
    let readers: Vec<((usize, usize), Reads)> = (before.iter().zip(after))
        .map(|((was, _), (is, reads))| ((*was, *is), *reads))
        .collect();
    for ((_, was), (_, reads)) in before.iter().zip(after) {
        if was.region != reads.region {
            rects.extend([(was.region, Depth::Under), (reads.region, Depth::Under)]);
        }
    }
    let mut owners = vec![None; rects.len()];
    let mut seen = vec![0; readers.len()];
    let mut pieces = vec![0; readers.len()];
    let mut grew = true;
    while grew {
        grew = false;
        for (reader, (at, reads)) in readers.iter().enumerate() {
            while seen[reader] < rects.len() && pieces[reader] <= MAX_DAMAGE_RECTS {
                let ((rect, depth), owner) = (rects[seen[reader]], owners[seen[reader]]);
                seen[reader] += 1;
                if owner == Some(reader)
                    || !depth.behind(*at)
                    || rect.intersect(&reads.region).is_empty()
                {
                    continue;
                }
                let piece = if reads.reach.is_finite() {
                    rect.inflate(reads.reach).intersect(&reads.region)
                } else {
                    reads.region
                };
                let covered = (rects.iter())
                    .any(|(other, depth)| *depth == Depth::Under && other.union(&piece) == *other);
                if !covered {
                    pieces[reader] += 1;
                    let piece = if pieces[reader] > MAX_DAMAGE_RECTS {
                        reads.region
                    } else {
                        piece
                    };
                    rects.push((piece, Depth::Under));
                    owners.push(Some(reader));
                    grew = true;
                }
            }
        }
    }
    let mut merged: Vec<Rect> = Vec::new();
    for rect in rects.iter().map(|(rect, _)| rect.intersect(&full)) {
        if rect.is_empty() {
            continue;
        }
        let mut rect = rect;
        while let Some(index) = (merged.iter())
            .position(|other| !other.inflate(DAMAGE_SLACK).intersect(&rect).is_empty())
        {
            rect = rect.union(&merged.swap_remove(index));
        }
        merged.push(rect);
    }
    if merged.len() > MAX_DAMAGE_RECTS {
        let bounds = merged
            .iter()
            .copied()
            .reduce(|bounds, rect| bounds.union(&rect));
        merged = bounds.into_iter().collect();
    }
    merged
}
