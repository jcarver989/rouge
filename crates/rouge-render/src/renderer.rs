use crate::effect::{EffectDraw, PASSTHROUGH_WGSL, Passthrough, SurfaceSample};
use crate::gpu::{PipelineDesc, begin_pass, create_pipeline, create_texture};
use crate::scene::{DrawList, Instance};
use crate::surface::{AlphaMode, BufferRef, Origin, SurfaceId, Surfaces};
use crate::{Affine, Atlas, Color, Corners, ImageData, ImageFormat, Inputs, Point, Rect};
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
const IMAGE_BUDGET_BYTES: u64 = 256 << 20;
const INSTANCE_MIN_BYTES: u64 = 64 << 10;

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

#[derive(Clone, Copy)]
pub struct LayerDraw<'a> {
    pub list: &'a DrawList,
    pub transform: Affine,
    pub opacity: f32,
    pub fill: [f32; 4],
    pub radii: Corners,
    pub backdrop: Option<EffectDraw>,
    pub scissor: Option<Rect>,
}

impl<'a> LayerDraw<'a> {
    pub fn new(list: &'a DrawList) -> Self {
        LayerDraw {
            list,
            transform: Affine::IDENTITY,
            opacity: 1.0,
            fill: [0.0; 4],
            radii: Corners::ZERO,
            backdrop: None,
            scissor: None,
        }
    }
}

pub struct Group<'a> {
    pub key: u64,
    pub signature: u64,
    pub bounds: Rect,
    pub opacity: f32,
    pub scissor: Option<Rect>,
    pub transform: Affine,
    pub effect: Option<EffectDraw>,
    pub items: Vec<Item<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceDraw {
    pub id: SurfaceId,
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
            bounds,
            transform: Affine::IDENTITY,
            opacity: 1.0,
            radii: Corners::ZERO,
            scissor: None,
        }
    }
}

pub enum Item<'a> {
    Layer(LayerDraw<'a>),
    Group(Group<'a>),
    Surface(SurfaceDraw),
}

impl<'a> From<LayerDraw<'a>> for Item<'a> {
    fn from(draw: LayerDraw<'a>) -> Self {
        Item::Layer(draw)
    }
}

impl Item<'_> {
    pub fn extent(&self, scale: f32) -> Option<Rect> {
        let (reach, scissor) = match self {
            Item::Layer(draw) => {
                let list = draw
                    .list
                    .extent()
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
            Item::Surface(draw) => (
                device_rect(&draw.transform, &draw.bounds, scale).inflate(1.0),
                draw.scissor,
            ),
            Item::Group(group) => {
                let spread = group.effect.map_or(0.0, |effect| {
                    let blur = Kernel::new(effect.effect.content_blur() * scale);
                    effect.effect.margin() * scale + Kernel::reach(blur)
                });
                (group.bounds.inflate(spread), group.scissor)
            }
        };
        let reach = scissor.map_or(reach, |scissor| scissor.intersect(&reach));
        (!reach.is_empty()).then_some(reach)
    }

    pub fn fingerprint<T: Hasher>(&self, hasher: &mut T) {
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
        match self {
            Item::Layer(draw) => {
                (0u8, draw.list.id(), draw.list.revision()).hash(hasher);
                affine(&draw.transform, hasher);
                draw.opacity.to_bits().hash(hasher);
                draw.fill.map(f32::to_bits).hash(hasher);
                draw.radii.to_array().map(f32::to_bits).hash(hasher);
                rect(draw.scissor, hasher);
                effect(&draw.backdrop, hasher);
            }
            Item::Surface(draw) => {
                (2u8, draw.id, draw.opacity.to_bits()).hash(hasher);
                rect(Some(draw.bounds), hasher);
                affine(&draw.transform, hasher);
                draw.radii.to_array().map(f32::to_bits).hash(hasher);
                rect(draw.scissor, hasher);
            }
            Item::Group(group) => {
                (1u8, group.key, group.signature, group.opacity.to_bits()).hash(hasher);
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
    origin: (u32, u32),
    size: (u32, u32),
}

impl Area {
    fn scissor(&self, rect: Option<Rect>) -> Option<Scissor> {
        let Some(rect) = rect else {
            return Some([0, 0, self.size.0, self.size.1]);
        };
        let Area { origin, size } = self.within(&rect, f32::round, f32::round)?;
        Some([
            origin.0 - self.origin.0,
            origin.1 - self.origin.1,
            size.0,
            size.1,
        ])
    }

    fn cover(&self, rect: &Rect) -> Option<Area> {
        self.within(rect, f32::floor, f32::ceil)
    }

    fn within(&self, rect: &Rect, low: fn(f32) -> f32, high: fn(f32) -> f32) -> Option<Area> {
        let max = rect.max();
        let span = |min: f32, max: f32, origin: u32, size: u32| {
            let lo = low(min).clamp(origin as f32, (origin + size) as f32) as u32;
            let hi = high(max).clamp(origin as f32, (origin + size) as f32) as u32;
            (lo, hi.saturating_sub(lo))
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
        [self.size.0, self.size.1, self.origin.0, self.origin.1].map(|v| v as f32)
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Kernel {
    levels: u32,
    spread: f32,
}

impl Kernel {
    fn new(radius: f32) -> Option<Kernel> {
        if radius < 0.05 {
            return None;
        }
        let levels = (radius.log2().ceil().max(1.0) as u32).min(MAX_BLUR_LEVELS);
        Some(Kernel {
            levels,
            spread: (radius / (1u32 << levels) as f32).min(1.0),
        })
    }

    fn reach(kernel: Option<Kernel>) -> f32 {
        kernel.map_or(0.0, |kernel| (3u32 << kernel.levels) as f32)
    }

    fn result_level(self) -> u32 {
        u32::from(self.levels >= 4)
    }

    fn state(kernel: Option<Kernel>) -> (u32, u32) {
        kernel.map_or((0, 0), |kernel| (kernel.levels, kernel.spread.to_bits()))
    }
}

#[derive(Clone, Copy)]
struct Window {
    origin: (f32, f32),
    size: (f32, f32),
    dims: (u32, u32),
}

impl Window {
    fn new(region: Rect, dims: (u32, u32)) -> Window {
        Window {
            origin: (region.origin.x, region.origin.y),
            size: (region.size.width, region.size.height),
            dims,
        }
    }

    fn texels((width, height): (u32, u32), dims: (u32, u32)) -> Window {
        Window {
            origin: (0.0, 0.0),
            size: (width as f32, height as f32),
            dims,
        }
    }

    fn scaled(&self, low: (f32, f32), high: (f32, f32)) -> [f32; 4] {
        let (width, height) = (self.dims.0 as f32, self.dims.1 as f32);
        [
            low.0 / width,
            low.1 / height,
            high.0 / width,
            high.1 / height,
        ]
    }

    fn rect(&self) -> [f32; 4] {
        self.scaled(self.origin, self.size)
    }

    fn bounds(&self) -> [f32; 4] {
        let (x, y) = self.origin;
        self.scaled((x, y), (x + self.size.0, y + self.size.1))
    }

    fn inset(&self) -> [f32; 4] {
        let (x, y) = self.origin;
        self.scaled(
            (x + 0.5, y + 0.5),
            (x + self.size.0 - 0.5, y + self.size.1 - 0.5),
        )
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct Mapping {
    map: [f32; 4],
    bounds: [f32; 4],
}

impl Mapping {
    fn new(origin: (f32, f32), size: (u32, u32), texels: Window) -> Mapping {
        let scale = |used: f32, size: u32, dims: u32| used / (size as f32 * dims as f32);
        Mapping {
            map: [
                origin.0,
                origin.1,
                scale(texels.size.0, size.0, texels.dims.0),
                scale(texels.size.1, size.1, texels.dims.1),
            ],
            bounds: texels.bounds(),
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
    fn new(source: &Source, area: Rect, kernel: Option<Kernel>) -> Option<BlurGeometry> {
        let levels = kernel.map_or(0, |kernel| kernel.levels);
        let step = (1u32 << levels) as f32;
        let area = area.intersect(&source.valid.inflate(Kernel::reach(kernel)));
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
        let shift = kernel.map_or(0, |kernel| kernel.result_level() + 1);
        let used = ((size.0 >> shift).max(1), (size.1 >> shift).max(1));
        let dims = (
            used.0.next_multiple_of(BLUR_TEXTURE_STEP),
            used.1.next_multiple_of(BLUR_TEXTURE_STEP),
        );
        Some(BlurGeometry {
            origin,
            size,
            dims,
            mapping: Mapping::new(origin, size, Window::texels(used, dims)),
        })
    }

    fn passes(&self, source: &Source, kernel: Option<Kernel>, plan: &mut Plan) -> Range<u32> {
        let start = plan.blur_passes.len() as u32;
        let window = Window {
            origin: (
                self.origin.0 - source.origin.0,
                self.origin.1 - source.origin.1,
            ),
            size: (self.size.0 as f32, self.size.1 as f32),
            dims: source.dims,
        };
        let valid = Window::new(
            source
                .valid
                .translate(Point::new(-source.origin.0, -source.origin.1)),
            source.dims,
        );
        let mut region = RegionRecord {
            rect: window.rect(),
            bounds: valid.inset(),
            options: [
                kernel.map_or(0.0, |kernel| kernel.spread),
                f32::from(u8::from(source.zero_outside)),
                0.0,
                0.0,
            ],
        };
        let Some(kernel) = kernel else {
            let record = plan.records.push(&region);
            plan.blur_passes.push(BlurPass {
                kind: BlurKind::Copy,
                record,
                level: None,
                size: self.size,
            });
            return start..start + 1;
        };
        let down = (0..kernel.levels).map(|level| (BlurKind::Down, level));
        let up = (kernel.result_level()..kernel.levels - 1)
            .rev()
            .map(|level| (BlurKind::Up, level));
        let count = kernel.levels + (kernel.levels - 1 - kernel.result_level());
        for (index, (kind, level)) in (0..).zip(down.chain(up)) {
            let size = (
                (self.size.0 >> (level + 1)).max(1),
                (self.size.1 >> (level + 1)).max(1),
            );
            let record = plan.records.push(&region);
            plan.blur_passes.push(BlurPass {
                kind,
                record,
                level: (index + 1 < count).then_some(level),
                size,
            });
            let scratch = Window::texels(size, plan.scratch_dims(level));
            region = RegionRecord {
                rect: scratch.rect(),
                bounds: scratch.inset(),
                options: [kernel.spread, 0.0, 0.0, 0.0],
            };
        }
        start..plan.blur_passes.len() as u32
    }
}

#[derive(Clone, Copy)]
enum BlurKind {
    Copy,
    Down,
    Up,
}

struct BlurPass {
    kind: BlurKind,
    record: Record,
    level: Option<u32>,
    size: (u32, u32),
}

#[derive(Clone, Copy)]
enum BlurTarget {
    Backdrop(usize),
    Group(u64),
}

struct Blur {
    target: BlurTarget,
    passes: Range<u32>,
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
    Blur(Blur),
}

#[derive(Default)]
struct Plan {
    records: Records,
    pending: Vec<Step>,
    steps: Vec<Step>,
    passes: Vec<Pass>,
    snapshots: Vec<Blur>,
    blur_passes: Vec<BlurPass>,
    batches: Vec<DrawBatch>,
    since: Vec<Rect>,
    frames: Vec<(u64, usize)>,
    cursor: usize,
    scale: f32,
    viewport: Rect,
    scratch: (u32, u32),
    snapshot: bool,
    blurs: bool,
}

impl Plan {
    fn begin(&mut self, scale: f32, area: Area) {
        self.records.bytes.clear();
        self.pending.clear();
        self.steps.clear();
        self.passes.clear();
        self.snapshots.clear();
        self.blur_passes.clear();
        self.batches.clear();
        self.since.clear();
        self.frames.clear();
        self.cursor = 0;
        self.scale = scale;
        self.viewport = area.rect();
        let pad = |extent: u32| (extent + SCRATCH_PADDING).next_multiple_of(GROUP_TEXTURE_STEP);
        self.scratch = (pad(area.size.0), pad(area.size.1));
        self.snapshot = false;
        self.blurs = false;
    }

    fn scratch_dims(&self, level: u32) -> (u32, u32) {
        (
            (self.scratch.0 >> (level + 1)).max(1),
            (self.scratch.1 >> (level + 1)).max(1),
        )
    }
}

struct Behind {
    hasher: FxHasher,
    open: Option<usize>,
    backdrops: usize,
}

#[derive(Clone, Copy)]
struct GroupContent {
    key: u64,
    cover: Rect,
    blurred: bool,
    mapping: Mapping,
    reach: f32,
}

#[derive(Clone, Copy)]
enum Sampled {
    Nothing,
    Group(GroupContent),
    Surface { key: SurfaceKey, mapping: Mapping },
}

struct Placed<'a> {
    draw: &'a EffectDraw,
    transform: Affine,
    opacity: f32,
    scissor: Scissor,
    content: Sampled,
}

struct Blurred {
    target: Target,
    state: (u32, u32),
}

struct GroupTarget {
    target: Target,
    area: Area,
    signature: u64,
    blurred: Option<Blurred>,
    used: u64,
}

struct BackdropTarget {
    target: Target,
    signature: u64,
}

struct Destination<'a> {
    view: &'a TextureView,
    load: LoadOp<ClearColor>,
    scene: Option<&'a Target>,
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
    device: Device,
    layout: BindGroupLayout,
    sampler: Sampler,
    format: TextureFormat,
}

impl Textures {
    fn bind(&self, label: &str, view: &TextureView) -> BindGroup {
        self.device.create_bind_group(&BindGroupDescriptor {
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
        label: &str,
        size: (u32, u32),
        format: TextureFormat,
        usage: TextureUsages,
    ) -> Target {
        let usage = usage | TextureUsages::TEXTURE_BINDING;
        let texture = create_texture(&self.device, label, size, format, usage);
        let view = texture.create_view(&TextureViewDescriptor::default());
        let bind_group = self.bind(label, &view);
        Target {
            texture,
            view,
            bind_group,
        }
    }

    fn offscreen(&self, label: &str, size: (u32, u32)) -> Target {
        self.target(label, size, self.format, TextureUsages::RENDER_ATTACHMENT)
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
    effect_layout: PipelineLayout,
    effect_index: FxHashMap<&'static str, u32>,
    effects: Vec<RenderPipeline>,
    instances: Instances,
    groups: FxHashMap<u64, GroupTarget>,
    backdrops: Vec<BackdropTarget>,
    scene: Option<Target>,
    scratch: Vec<Target>,
    blank: Target,
    atlas: Option<Target>,
    images: Images,
    surface_binds: FxHashMap<SurfaceKey, BindGroup>,
    plan: Plan,
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
            device: device.clone(),
            layout,
            sampler,
            format,
        };
        let blank = textures.offscreen("rouge blank", (1, 1));

        Renderer {
            device: device.clone(),
            queue: queue.clone(),
            records_layout,
            records,
            records_group,
            primitive,
            fullscreen: ["fs_blit", "fs_down", "fs_up"].map(fullscreen),
            effect_layout,
            effect_index: FxHashMap::default(),
            effects: Vec::new(),
            instances: Instances::new(device),
            groups: FxHashMap::default(),
            backdrops: Vec::new(),
            scene: None,
            scratch: Vec::new(),
            blank,
            atlas: None,
            images: Images::default(),
            surface_binds: FxHashMap::default(),
            plan: Plan::default(),
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
        let target = self
            .textures
            .target("rouge image", size, format, TextureUsages::COPY_DST);
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

    fn sync_atlas(&mut self, atlas: &mut Atlas) {
        let size = atlas.size();
        let mut rows = atlas.take_dirty_rows();
        if self
            .atlas
            .as_ref()
            .is_none_or(|target| target.texture.height() != size.1)
        {
            let usage = TextureUsages::COPY_DST;
            let target =
                self.textures
                    .target("rouge glyph atlas", size, TextureFormat::R8Unorm, usage);
            self.atlas = Some(target);
            rows = Some((0, size.1));
        }
        if let (Some(target), Some((start, end))) = (&self.atlas, rows) {
            write_rows(
                &self.queue,
                &target.texture,
                start,
                atlas.rows(start, end),
                size.0,
            );
        }
    }

    fn sync_list(&mut self, list: &DrawList) -> u32 {
        let (start, uploaded) = self
            .instances
            .sync(&self.device, &self.queue, list, self.frame);
        self.stats.instance_bytes_uploaded += uploaded;
        start
    }

    fn sync_scratch(&mut self, plan: &Plan) {
        let stale = self
            .scratch
            .first()
            .is_none_or(|level| level.dims() != plan.scratch_dims(0));
        if stale {
            let levels = (0..MAX_BLUR_LEVELS).map(|level| {
                self.textures
                    .offscreen("rouge blur scratch", plan.scratch_dims(level))
            });
            self.scratch = levels.collect();
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
        if let Some(index) = self.effect_index.get(wgsl) {
            return *index;
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
        self.effects.push(pipeline);
        self.effect_index.insert(wgsl, index);
        index
    }

    fn plan_items(
        &mut self,
        items: &[Item<'_>],
        area: Area,
        mut behind: Option<&mut Behind>,
        plan: &mut Plan,
        surfaces: &Surfaces,
    ) -> Range<u32> {
        let mark = plan.pending.len();
        for item in items {
            let group = plan.cursor;
            let shown = match item {
                Item::Layer(draw) => self.plan_layer(draw, area, behind.as_deref_mut(), plan),
                Item::Surface(draw) => {
                    self.plan_surface(draw, area, plan, surfaces);
                    true
                }
                Item::Group(child) => {
                    let (frames, span) = plan.frames[group];
                    plan.cursor += 1;
                    let shown =
                        self.plan_group(child, area, behind.as_deref_mut(), plan, surfaces, frames);
                    plan.cursor = group + span;
                    shown
                }
            };
            if let Some(behind) = behind.as_deref_mut().filter(|_| shown) {
                item.fingerprint(&mut behind.hasher);
                match item {
                    Item::Surface(draw) => surfaces
                        .latched(draw.id)
                        .map(|latched| latched.frame)
                        .hash(&mut behind.hasher),
                    Item::Group(_) => plan.frames[group].0.hash(&mut behind.hasher),
                    Item::Layer(_) => {}
                }
                plan.since.extend(item.extent(plan.scale));
            }
        }
        let start = plan.steps.len() as u32;
        let Plan { pending, steps, .. } = plan;
        steps.extend(pending.drain(mark..));
        start..steps.len() as u32
    }

    fn plan_layer(
        &mut self,
        draw: &LayerDraw<'_>,
        area: Area,
        behind: Option<&mut Behind>,
        plan: &mut Plan,
    ) -> bool {
        let Some(scissor) = area.scissor(draw.scissor).filter(|_| draw.opacity > 0.0) else {
            return false;
        };
        let start = (!draw.list.is_empty()).then(|| self.sync_list(draw.list));
        for batch in draw.list.batches() {
            if let Some(image) = &batch.image {
                self.sync_image(image);
            }
        }
        if let Some(backdrop) = &draw.backdrop {
            let placed = Placed {
                draw: backdrop,
                transform: draw.transform,
                opacity: draw.opacity,
                scissor,
                content: Sampled::Nothing,
            };
            self.plan_effect(placed, area, behind, plan);
        }
        if let Some(start) = start {
            let record = plan.records.push(&DrawRecord {
                layer: Layer::new(&draw.transform, draw.opacity, area, plan.scale, draw.radii),
                fill: draw.fill,
            });
            let first = plan.batches.len() as u32;
            plan.batches
                .extend(draw.list.batches().iter().map(|batch| DrawBatch {
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
            entry.insert(self.textures.bind("rouge surface", &view));
        }
        let dims = (latched.texture.width(), latched.texture.height());
        let window = Window::new(latched.region, dims);
        let [x, y, width, height] = window.rect();
        let map = match latched.origin {
            Origin::TopLeft => [x, y, width, height],
            Origin::BottomLeft => [x, window.bounds()[3], width, -height],
        };
        let opaque = latched.alpha == AlphaMode::Opaque;
        let sample = EffectDraw {
            corner_radii: draw.radii,
            ..EffectDraw::new(&SurfaceSample { opaque }, draw.bounds)
        };
        let placed = Placed {
            draw: &sample,
            transform: draw.transform,
            opacity: draw.opacity,
            scissor,
            content: Sampled::Surface {
                key,
                mapping: Mapping {
                    map,
                    bounds: window.inset(),
                },
            },
        };
        self.plan_effect(placed, area, None, plan);
    }

    fn plan_group(
        &mut self,
        group: &Group<'_>,
        area: Area,
        behind: Option<&mut Behind>,
        plan: &mut Plan,
        surfaces: &Surfaces,
        frames: u64,
    ) -> bool {
        let Some(cover) = area.cover(&group.bounds).filter(|_| group.opacity > 0.0) else {
            return false;
        };
        let Some(scissor) = area.scissor(group.scissor) else {
            return false;
        };
        let needed = (
            cover.size.0.next_multiple_of(GROUP_TEXTURE_STEP),
            cover.size.1.next_multiple_of(GROUP_TEXTURE_STEP),
        );
        let mut hasher = FxHasher::default();
        (group.signature, frames).hash(&mut hasher);
        let signature = hasher.finish();
        let fresh = self.groups.get(&group.key).is_some_and(|existing| {
            existing.target.dims() == needed
                && existing.signature == signature
                && existing.area == cover
        });
        if !fresh {
            let inner = Area {
                origin: cover.origin,
                size: needed,
            };
            let steps = self.plan_items(&group.items, inner, None, plan, surfaces);
            plan.passes.push(Pass::Raster {
                key: group.key,
                steps,
            });
        }

        let textures = &self.textures;
        let entry = match self.groups.entry(group.key) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(GroupTarget {
                target: textures.offscreen("rouge group", needed),
                area: cover,
                signature,
                blurred: None,
                used: 0,
            }),
        };
        if entry.target.dims() != needed {
            entry.target = textures.offscreen("rouge group", needed);
            entry.blurred = None;
        }
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
        let reach = Kernel::reach(kernel);
        let source = Source {
            origin: (cover.origin.0 as f32, cover.origin.1 as f32),
            dims: needed,
            valid: cover.rect(),
            zero_outside: true,
        };
        let geometry =
            kernel.and_then(|_| BlurGeometry::new(&source, cover.rect().inflate(reach), kernel));
        let mut content = GroupContent {
            key: group.key,
            cover: cover.rect(),
            blurred: geometry.is_some(),
            mapping: Mapping::new(
                source.origin,
                cover.size,
                Window::texels(cover.size, needed),
            ),
            reach: 0.0,
        };
        if let Some(geometry) = geometry {
            let state = Kernel::state(kernel);
            let keep = fresh
                && entry.blurred.as_ref().is_some_and(|blurred| {
                    blurred.target.dims() == geometry.dims && blurred.state == state
                });
            match &mut entry.blurred {
                Some(blurred) if blurred.target.dims() == geometry.dims => blurred.state = state,
                blurred => {
                    *blurred = Some(Blurred {
                        target: textures.offscreen("rouge group blur", geometry.dims),
                        state,
                    });
                }
            }
            if !keep {
                let passes = geometry.passes(&source, kernel, plan);
                plan.passes.push(Pass::Blur(Blur {
                    target: BlurTarget::Group(group.key),
                    passes,
                }));
                plan.blurs = true;
            }
            (content.mapping, content.reach) = (geometry.mapping, reach);
        } else {
            entry.blurred = None;
        }
        let placed = Placed {
            draw,
            transform,
            opacity: group.opacity,
            scissor,
            content: Sampled::Group(content),
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
        let (scale, margin) = (plan.scale, effect.margin());
        let (quad, content_map, source, bounded) = match content {
            Sampled::Group(group) => (
                group.cover.inflate(margin * scale + group.reach),
                group.mapping,
                Content::Group {
                    key: group.key,
                    blurred: group.blurred,
                },
                UNBOUNDED_MARGIN,
            ),
            Sampled::Surface { key, mapping } => (
                device_rect(&transform, &draw.bounds, scale).inflate(1.0),
                mapping,
                Content::Surface(key),
                margin,
            ),
            Sampled::Nothing => (
                device_rect(&transform, &draw.bounds.inflate(margin), scale),
                Mapping::default(),
                Content::None,
                margin,
            ),
        };

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
            let region = quad
                .inflate(Kernel::reach(kernel))
                .intersect(&plan.viewport);
            let Some(geometry) = BlurGeometry::new(&viewport, region, kernel) else {
                return;
            };
            let mut hasher = behind.hasher.clone();
            (
                geometry.origin.0.to_bits(),
                geometry.origin.1.to_bits(),
                geometry.size,
                Kernel::state(kernel),
            )
                .hash(&mut hasher);
            let signature = hasher.finish();
            let index = behind.backdrops;
            behind.backdrops += 1;
            let cached = match self.backdrops.get_mut(index) {
                Some(existing) if existing.target.dims() == geometry.dims => {
                    let cached = existing.signature == signature;
                    existing.signature = signature;
                    cached
                }
                Some(existing) => {
                    existing.target = self.textures.offscreen("rouge backdrop", geometry.dims);
                    existing.signature = signature;
                    false
                }
                None => {
                    self.backdrops.push(BackdropTarget {
                        target: self.textures.offscreen("rouge backdrop", geometry.dims),
                        signature,
                    });
                    false
                }
            };
            if cached {
                self.stats.backdrops_reused += 1;
            } else {
                let passes = geometry.passes(&viewport, kernel, plan);
                plan.snapshots.push(Blur {
                    target: BlurTarget::Backdrop(index),
                    passes,
                });
                let end = plan.snapshots.len() as u32;
                let clear = !plan
                    .since
                    .iter()
                    .any(|drawn| !drawn.intersect(&region).is_empty());
                let open = behind.open.filter(|_| clear);
                if let Some(Step::Snapshot(blurs)) =
                    open.and_then(|open| plan.pending.get_mut(open))
                {
                    blurs.end = end;
                } else {
                    behind.open = Some(plan.pending.len());
                    plan.since.clear();
                    plan.pending.push(Step::Snapshot(end - 1..end));
                }
                plan.snapshot = true;
                plan.blurs |= kernel.is_some();
            }
            backdrop = Some(index);
            backdrop_map = geometry.mapping;
        }

        let record = plan.records.push(&EffectRecord {
            layer: Layer::new(&transform, opacity, area, scale, draw.corner_radii),
            rect: quad.to_array(),
            inverse: matrix(&inverse),
            bounds: draw.bounds.to_array(),
            content: content_map,
            backdrop: backdrop_map,
            margin: [bounded, 0.0, 0.0, 0.0],
            params: effect.uniforms(),
        });
        let pipeline = self.effect_pipeline(wgsl);
        plan.pending.push(Step::Effect {
            pipeline,
            record,
            content: source,
            backdrop,
            scissor,
        });
    }

    fn encode_blur(
        &self,
        encoder: &mut CommandEncoder,
        blur: &Blur,
        scene: Option<&Target>,
        plan: &Plan,
        stats: &mut RenderStats,
    ) {
        let (source, target) = match blur.target {
            BlurTarget::Backdrop(index) => {
                stats.backdrop_blurs += 1;
                (
                    scene,
                    self.backdrops.get(index).map(|backdrop| &backdrop.target),
                )
            }
            BlurTarget::Group(key) => {
                stats.content_blurs += 1;
                let group = self.groups.get(&key);
                (
                    group.map(|group| &group.target),
                    group
                        .and_then(|group| group.blurred.as_ref())
                        .map(|blurred| &blurred.target),
                )
            }
        };
        let (Some(mut source), Some(target)) = (source, target) else {
            return;
        };
        let passes = &plan.blur_passes[blur.passes.start as usize..blur.passes.end as usize];
        for pass in passes {
            let destination = match pass.level {
                Some(level) => &self.scratch[level as usize],
                None => target,
            };
            let mut render = begin_pass(
                encoder,
                &destination.view,
                LoadOp::Clear(ClearColor::TRANSPARENT),
            );
            render.set_viewport(0.0, 0.0, pass.size.0 as f32, pass.size.1 as f32, 0.0, 1.0);
            render.set_pipeline(&self.fullscreen[pass.kind as usize]);
            render.set_bind_group(0, &source.bind_group, &[]);
            render.set_bind_group(1, &self.records_group, &[pass.record.page]);
            render.draw(pass.record.vertices(), 0..1);
            source = destination;
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
        let Destination { view, load, scene } = destination;
        let blank = &self.blank.bind_group;
        let mut pass = begin_pass(encoder, view, load);
        let mut bound = Bound::default();
        for step in &plan.steps[steps.start as usize..steps.end as usize] {
            match step {
                Step::Snapshot(blurs) => {
                    drop(pass);
                    for blur in &plan.snapshots[blurs.start as usize..blurs.end as usize] {
                        self.encode_blur(encoder, blur, scene, plan, stats);
                    }
                    pass = begin_pass(encoder, view, LoadOp::Load);
                    bound = Bound::default();
                    stats.backdrop_passes += 1;
                }
                Step::Draw {
                    record,
                    scissor,
                    batches,
                } => {
                    bound.scissor(&mut pass, *scissor);
                    bound.pipeline(&mut pass, &self.primitive);
                    bound.page(&mut pass, &self.records_group, record.page);
                    if !bound.instances {
                        pass.set_vertex_buffer(0, self.instances.buffer.slice(..));
                        bound.instances = true;
                    }
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
                        Content::Group { key, blurred } => match self.groups.get(key) {
                            Some(group) => match (&group.blurred, blurred) {
                                (Some(blurred), true) => &blurred.target.bind_group,
                                _ => &group.target.bind_group,
                            },
                            None => blank,
                        },
                        Content::Surface(key) => match self.surface_binds.get(key) {
                            Some(texture) => texture,
                            None => continue,
                        },
                    };
                    let backdrop = backdrop
                        .and_then(|index| self.backdrops.get(index))
                        .map_or(blank, |backdrop| &backdrop.target.bind_group);
                    bound.scissor(&mut pass, *scissor);
                    bound.pipeline(&mut pass, &self.effects[*pipeline as usize]);
                    bound.group(&mut pass, 0, texture);
                    bound.page(&mut pass, &self.records_group, record.page);
                    bound.group(&mut pass, 2, backdrop);
                    pass.draw(record.vertices(), 0..1);
                    stats.groups += u32::from(matches!(content, Content::Group { .. }));
                    stats.surfaces += u32::from(matches!(content, Content::Surface(_)));
                    stats.draw_calls += 1;
                }
            }
        }
    }

    pub fn render(
        &mut self,
        target: &TextureView,
        viewport: Viewport,
        clear: Color,
        items: &[Item<'_>],
        atlas: &mut Atlas,
        surfaces: &Surfaces,
    ) {
        self.stats = RenderStats::default();
        self.frame += 1;
        self.sync_atlas(atlas);

        let area = Area {
            origin: (0, 0),
            size: (viewport.width.max(1), viewport.height.max(1)),
        };
        let mut plan = take(&mut self.plan);
        plan.begin(viewport.scale, area);
        hash_frames(items, surfaces, &mut plan.frames);
        let mut behind = Behind {
            hasher: FxHasher::default(),
            open: None,
            backdrops: 0,
        };
        (
            area.size,
            viewport.scale.to_bits(),
            clear.to_array().map(f32::to_bits),
        )
            .hash(&mut behind.hasher);
        let main = self.plan_items(items, area, Some(&mut behind), &mut plan, surfaces);
        let frame = self.frame;
        self.groups.retain(|_, group| group.used == frame);
        self.surface_binds
            .retain(|(id, buffer), _| surfaces.holds(*id, buffer.set));
        self.backdrops.truncate(behind.backdrops);
        let blit = plan.records.push(&RegionRecord {
            rect: [0.0, 0.0, 1.0, 1.0],
            bounds: [0.0, 0.0, 1.0, 1.0],
            options: [0.0; 4],
        });
        self.sync_records(&plan.records);
        if plan.blurs {
            self.sync_scratch(&plan);
        }
        if plan.snapshot
            && self
                .scene
                .as_ref()
                .is_none_or(|scene| scene.dims() != area.size)
        {
            self.scene = Some(self.textures.offscreen("rouge scene", area.size));
        }
        let scene = self.scene.as_ref().filter(|_| plan.snapshot);

        let descriptor = CommandEncoderDescriptor {
            label: Some("rouge frame"),
        };
        let mut encoder = self.device.create_command_encoder(&descriptor);
        let mut stats = self.stats;
        for pass in &plan.passes {
            match pass {
                Pass::Raster { key, steps } => {
                    let Some(group) = self.groups.get(key) else {
                        continue;
                    };
                    let destination = Destination {
                        view: &group.target.view,
                        load: LoadOp::Clear(ClearColor::TRANSPARENT),
                        scene: None,
                    };
                    self.encode(&mut encoder, destination, steps.clone(), &plan, &mut stats);
                    stats.group_passes += 1;
                }
                Pass::Blur(blur) => self.encode_blur(&mut encoder, blur, None, &plan, &mut stats),
            }
        }
        let clear = [clear.r, clear.g, clear.b, 1.0].map(|c| f64::from(c * clear.a));
        let destination = Destination {
            view: scene.map_or(target, |scene| &scene.view),
            load: LoadOp::Clear(ClearColor {
                r: clear[0],
                g: clear[1],
                b: clear[2],
                a: clear[3],
            }),
            scene,
        };
        self.encode(&mut encoder, destination, main, &plan, &mut stats);

        if let Some(scene) = scene {
            let mut pass = begin_pass(&mut encoder, target, LoadOp::Clear(ClearColor::TRANSPARENT));
            pass.set_pipeline(&self.fullscreen[BlurKind::Copy as usize]);
            pass.set_bind_group(0, &scene.bind_group, &[]);
            pass.set_bind_group(1, &self.records_group, &[blit.page]);
            pass.draw(blit.vertices(), 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.stats = stats;
        self.images.evict(frame);
        self.instances.maintain(frame);
        self.plan = plan;
    }
}

fn hash_frames(items: &[Item<'_>], surfaces: &Surfaces, frames: &mut Vec<(u64, usize)>) -> u64 {
    let mut hasher = FxHasher::default();
    for item in items {
        match item {
            Item::Surface(draw) => surfaces
                .latched(draw.id)
                .map(|latched| latched.frame)
                .hash(&mut hasher),
            Item::Group(group) => {
                let index = frames.len();
                frames.push((0, 0));
                let hash = hash_frames(&group.items, surfaces, frames);
                frames[index] = (hash, frames.len() - index);
                hash.hash(&mut hasher);
            }
            Item::Layer(_) => {}
        }
    }
    hasher.finish()
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
