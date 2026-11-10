use crate::effect::{EffectDraw, Passthrough, Shader};
use crate::scene::{DrawList, Instance};
use crate::{Affine, Atlas, Color, Corners, ImageData, ImageFormat, Rect};
use bytemuck::{Pod, Zeroable, bytes_of};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::mem::{size_of, take};
use std::ptr::from_ref;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBinding,
    BufferBindingType, BufferDescriptor, BufferSize, BufferUsages, Color as ClearColor,
    ColorTargetState, ColorWrites, CommandEncoder, CommandEncoderDescriptor, Device, Extent3d,
    FilterMode, FragmentState, LoadOp, MultisampleState, Operations, Origin3d,
    PipelineCompilationOptions, PipelineLayout, PipelineLayoutDescriptor, PrimitiveState,
    PrimitiveTopology, Queue, RenderPass, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp, TexelCopyBufferLayout,
    TexelCopyTextureInfo, Texture, TextureDescriptor, TextureDimension, TextureFormat,
    TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor, TextureViewDimension,
    VertexAttribute, VertexBufferLayout, VertexFormat, VertexState, VertexStepMode,
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
const LAYER_UNIFORM_SIZE: u64 = size_of::<LayerUniform>() as u64;
const IMAGE_IDLE_FRAMES: u64 = 600;
const IMAGE_BUDGET_BYTES: u64 = 256 << 20;

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

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LayerUniform {
    matrix: [f32; 4],
    translation_opacity: [f32; 4],
    target: [f32; 4],
    rect: [f32; 4],
    fill: [f32; 4],
    radii: [f32; 4],
    inverse: [f32; 4],
    bounds: [f32; 4],
    content_map: [f32; 4],
    content_bounds: [f32; 4],
    backdrop_map: [f32; 4],
    backdrop_bounds: [f32; 4],
    scale: [f32; 4],
    params: [f32; 16],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RegionUniform {
    rect: [f32; 4],
    bounds: [f32; 4],
    options: [f32; 4],
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

pub enum Item<'a> {
    Layer(LayerDraw<'a>),
    Group(Group<'a>),
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

    pub fn fingerprint(&self, hasher: &mut DefaultHasher) {
        let rect = |rect: &Option<Rect>, hasher: &mut DefaultHasher| {
            rect.map(|rect| rect.to_array().map(f32::to_bits))
                .hash(hasher);
        };
        let affine = |t: &Affine, hasher: &mut DefaultHasher| {
            [t.a, t.b, t.c, t.d, t.tx, t.ty]
                .map(f32::to_bits)
                .hash(hasher);
        };
        let effect = |effect: &Option<EffectDraw>, hasher: &mut DefaultHasher| {
            effect.is_some().hash(hasher);
            if let Some(effect) = effect {
                effect.effect.shader().hash(hasher);
                effect.effect.values().map(f32::to_bits).hash(hasher);
                effect
                    .corner_radii
                    .to_array()
                    .map(f32::to_bits)
                    .hash(hasher);
                rect(&Some(effect.bounds), hasher);
            }
        };
        match self {
            Item::Layer(draw) => {
                (0u8, from_ref(draw.list) as usize, draw.list.revision()).hash(hasher);
                affine(&draw.transform, hasher);
                draw.opacity.to_bits().hash(hasher);
                draw.fill.map(f32::to_bits).hash(hasher);
                draw.radii.to_array().map(f32::to_bits).hash(hasher);
                rect(&draw.scissor, hasher);
                effect(&draw.backdrop, hasher);
            }
            Item::Group(group) => {
                (1u8, group.key, group.signature, group.opacity.to_bits()).hash(hasher);
                rect(&Some(group.bounds), hasher);
                rect(&group.scissor, hasher);
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
}

type Scissor = [u32; 4];

#[derive(Clone, Copy)]
struct Surface {
    origin: (u32, u32),
    size: (u32, u32),
}

impl Surface {
    fn scissor(&self, rect: Option<Rect>) -> Option<Scissor> {
        let Some(rect) = rect else {
            return Some([0, 0, self.size.0, self.size.1]);
        };
        let Surface { origin, size } = self.within(&rect, f32::round, f32::round)?;
        Some([
            origin.0 - self.origin.0,
            origin.1 - self.origin.1,
            size.0,
            size.1,
        ])
    }

    fn cover(&self, rect: &Rect) -> Option<Surface> {
        self.within(rect, f32::floor, f32::ceil)
    }

    fn within(&self, rect: &Rect, low: fn(f32) -> f32, high: fn(f32) -> f32) -> Option<Surface> {
        let max = rect.max();
        let span = |min: f32, max: f32, origin: u32, size: u32| {
            let lo = low(min).clamp(origin as f32, (origin + size) as f32) as u32;
            let hi = high(max).clamp(origin as f32, (origin + size) as f32) as u32;
            (lo, hi.saturating_sub(lo))
        };
        let (x, width) = span(rect.origin.x, max.x, self.origin.0, self.size.0);
        let (y, height) = span(rect.origin.y, max.y, self.origin.1, self.size.1);
        (width > 0 && height > 0).then_some(Surface {
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

    fn target(&self) -> [f32; 4] {
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

#[derive(Clone, Copy, Default)]
struct Mapping {
    map: [f32; 4],
    bounds: [f32; 4],
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
        let mapping = Mapping {
            map: [
                origin.0,
                origin.1,
                used.0 as f32 / (size.0 as f32 * dims.0 as f32),
                used.1 as f32 / (size.1 as f32 * dims.1 as f32),
            ],
            bounds: [
                0.0,
                0.0,
                used.0 as f32 / dims.0 as f32,
                used.1 as f32 / dims.1 as f32,
            ],
        };
        Some(BlurGeometry {
            origin,
            size,
            dims,
            mapping,
        })
    }

    fn passes(
        &self,
        source: &Source,
        kernel: Option<Kernel>,
        frame: &mut Frame<'_>,
    ) -> Vec<BlurPass> {
        let (width, height) = (source.dims.0 as f32, source.dims.1 as f32);
        let (low, high) = (source.valid.origin, source.valid.max());
        let mut uniform = RegionUniform {
            rect: [
                (self.origin.0 - source.origin.0) / width,
                (self.origin.1 - source.origin.1) / height,
                self.size.0 as f32 / width,
                self.size.1 as f32 / height,
            ],
            bounds: [
                (low.x - source.origin.0 + 0.5) / width,
                (low.y - source.origin.1 + 0.5) / height,
                (high.x - source.origin.0 - 0.5) / width,
                (high.y - source.origin.1 - 0.5) / height,
            ],
            options: [
                kernel.map_or(0.0, |kernel| kernel.spread),
                f32::from(u8::from(source.zero_outside)),
                0.0,
                0.0,
            ],
        };
        let Some(kernel) = kernel else {
            return vec![BlurPass {
                kind: BlurKind::Copy,
                uniform: frame.push(&uniform),
                level: None,
                size: self.size,
            }];
        };
        let down = (0..kernel.levels).map(|level| (BlurKind::Down, level));
        let up = (kernel.result_level()..kernel.levels - 1)
            .rev()
            .map(|level| (BlurKind::Up, level));
        let sequence: Vec<_> = down.chain(up).collect();
        let mut passes = Vec::with_capacity(sequence.len());
        for (index, (kind, level)) in sequence.iter().copied().enumerate() {
            let last = index + 1 == sequence.len();
            let size = (
                (self.size.0 >> (level + 1)).max(1),
                (self.size.1 >> (level + 1)).max(1),
            );
            passes.push(BlurPass {
                kind,
                uniform: frame.push(&uniform),
                level: (!last).then_some(level),
                size,
            });
            let (width, height) = frame.scratch_dims(level);
            let (width, height, used) =
                (width as f32, height as f32, (size.0 as f32, size.1 as f32));
            uniform = RegionUniform {
                rect: [0.0, 0.0, used.0 / width, used.1 / height],
                bounds: [
                    0.5 / width,
                    0.5 / height,
                    (used.0 - 0.5) / width,
                    (used.1 - 0.5) / height,
                ],
                options: [kernel.spread, 0.0, 0.0, 0.0],
            };
        }
        passes
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
    uniform: u32,
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
    passes: Vec<BlurPass>,
}

enum Step<'a> {
    Draw {
        draw: &'a LayerDraw<'a>,
        uniform: u32,
        scissor: Scissor,
    },
    Effect {
        shader: Shader,
        uniform: u32,
        content: Option<(u64, bool)>,
        backdrop: Option<usize>,
        scissor: Scissor,
    },
    Snapshot(Vec<Blur>),
}

enum Pass<'a> {
    Raster { key: u64, steps: Vec<Step<'a>> },
    Blur(Blur),
}

struct Frame<'a> {
    uniforms: Vec<u8>,
    stride: usize,
    passes: Vec<Pass<'a>>,
    scale: f32,
    viewport: Rect,
    scratch: (u32, u32),
    snapshots: bool,
    blurs: bool,
}

impl Frame<'_> {
    fn push<T: Pod>(&mut self, uniform: &T) -> u32 {
        let offset = self.uniforms.len();
        self.uniforms.resize(offset + self.stride, 0);
        self.uniforms[offset..][..size_of::<T>()].copy_from_slice(bytes_of(uniform));
        offset as u32
    }

    fn scratch_dims(&self, level: u32) -> (u32, u32) {
        (
            (self.scratch.0 >> (level + 1)).max(1),
            (self.scratch.1 >> (level + 1)).max(1),
        )
    }
}

struct Behind {
    hasher: DefaultHasher,
    open: Option<usize>,
    since: Vec<Rect>,
    backdrops: usize,
}

struct Applied<'a> {
    draw: &'a EffectDraw,
    transform: Affine,
    opacity: f32,
    scissor: Scissor,
    content: Option<(u64, Rect)>,
}

struct Blurred {
    target: Target,
    state: (u32, u32),
    mapping: Mapping,
    reach: f32,
}

struct GroupTarget {
    target: Target,
    surface: Surface,
    signature: u64,
    blurred: Option<Blurred>,
    used: bool,
}

impl GroupTarget {
    fn mapping(&self) -> Mapping {
        let (width, height) = (
            self.target.texture.width() as f32,
            self.target.texture.height() as f32,
        );
        let (origin, size) = (self.surface.origin, self.surface.size);
        Mapping {
            map: [origin.0 as f32, origin.1 as f32, 1.0 / width, 1.0 / height],
            bounds: [0.0, 0.0, size.0 as f32 / width, size.1 as f32 / height],
        }
    }
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

struct CachedImage {
    target: Target,
    bytes: u64,
    used: u64,
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

pub struct Renderer {
    device: Device,
    queue: Queue,
    format: TextureFormat,
    layer_layout: BindGroupLayout,
    layer_buffer: Buffer,
    layer_bind_group: BindGroup,
    layer_stride: u64,
    texture_layout: BindGroupLayout,
    sampler: Sampler,
    primitive_pipeline: RenderPipeline,
    effect_layout: PipelineLayout,
    effects: HashMap<Shader, RenderPipeline>,
    groups: HashMap<u64, GroupTarget>,
    backdrops: Vec<BackdropTarget>,
    blit_pipeline: RenderPipeline,
    down_pipeline: RenderPipeline,
    up_pipeline: RenderPipeline,
    scene: Option<Target>,
    scratch: Vec<Target>,
    blank: Option<Target>,
    atlas: Option<Target>,
    images: HashMap<u64, CachedImage>,
    frame: u64,
    stats: RenderStats,
}

impl Renderer {
    pub fn new(device: &Device, queue: &Queue, format: TextureFormat) -> Self {
        let layer_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("rouge uniform layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: BufferSize::new(LAYER_UNIFORM_SIZE),
                },
                count: None,
            }],
        });
        let layer_stride = LAYER_UNIFORM_SIZE.next_multiple_of(u64::from(
            device.limits().min_uniform_buffer_offset_alignment,
        ));
        let (layer_buffer, layer_bind_group) =
            create_layer_buffer(device, &layer_layout, 16 * layer_stride);

        let texture_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
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
        let layout = pipeline_layout(&[Some(&texture_layout), Some(&layer_layout)]);
        let effect_layout = pipeline_layout(&[
            Some(&texture_layout),
            Some(&layer_layout),
            Some(&texture_layout),
        ]);
        let primitive = format!("{COMMON_WGSL}\n{PRIMITIVE_WGSL}");
        let blend = Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let instance_size = size_of::<Instance>() as u64;
        let primitive_pipeline = create_pipeline(
            device,
            &layout,
            format,
            &primitive,
            "fs_main",
            blend,
            instance_size,
        );
        let fullscreen =
            |entry| create_pipeline(device, &layout, format, FULLSCREEN_WGSL, entry, None, 0);

        Renderer {
            device: device.clone(),
            queue: queue.clone(),
            format,
            layer_buffer,
            layer_bind_group,
            layer_stride,
            primitive_pipeline,
            effect_layout,
            effects: HashMap::new(),
            groups: HashMap::new(),
            backdrops: Vec::new(),
            blit_pipeline: fullscreen("fs_blit"),
            down_pipeline: fullscreen("fs_down"),
            up_pipeline: fullscreen("fs_up"),
            layer_layout,
            texture_layout,
            sampler,
            scene: None,
            scratch: Vec::new(),
            blank: None,
            atlas: None,
            images: HashMap::new(),
            frame: 0,
            stats: RenderStats::default(),
        }
    }

    fn sync_image(&mut self, image: &ImageData) {
        if let Some(cached) = self.images.get_mut(&image.id()) {
            cached.used = self.frame;
            return;
        }
        let format = match image.format() {
            ImageFormat::Rgba8 => TextureFormat::Rgba8Unorm,
            ImageFormat::Alpha8 => TextureFormat::R8Unorm,
        };
        let size = (image.width(), image.height());
        let target = self.create_target("rouge image", size, format, TextureUsages::COPY_DST);
        let bytes_per_row = size.0 * image.format().bytes_per_pixel();
        write_rows(
            &self.queue,
            &target.texture,
            0,
            image.pixels(),
            bytes_per_row,
        );
        self.stats.image_bytes_uploaded += image.byte_len();
        self.images.insert(
            image.id(),
            CachedImage {
                target,
                bytes: image.byte_len(),
                used: self.frame,
            },
        );
    }

    fn evict_images(&mut self) {
        let frame = self.frame;
        self.images
            .retain(|_, image| frame - image.used < IMAGE_IDLE_FRAMES);
        let mut total: u64 = self.images.values().map(|image| image.bytes).sum();
        while total > IMAGE_BUDGET_BYTES {
            let idle = self.images.iter().filter(|(_, image)| image.used < frame);
            let Some(oldest) = idle.min_by_key(|(_, image)| image.used).map(|(id, _)| *id) else {
                break;
            };
            total -= self.images.remove(&oldest).map_or(0, |image| image.bytes);
        }
    }

    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    fn create_target(
        &self,
        label: &str,
        (width, height): (u32, u32),
        format: TextureFormat,
        usage: TextureUsages,
    ) -> Target {
        let texture = self.device.create_texture(&TextureDescriptor {
            label: Some(label),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: usage | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some(label),
            layout: &self.texture_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        Target {
            texture,
            view,
            bind_group,
        }
    }

    fn offscreen(&self, label: &str, size: (u32, u32), reuse: Option<Target>) -> Target {
        reuse
            .filter(|target| target.dims() == size)
            .unwrap_or_else(|| {
                self.create_target(label, size, self.format, TextureUsages::RENDER_ATTACHMENT)
            })
    }

    fn sync_atlas(&mut self, atlas: &mut Atlas) -> BindGroup {
        let size = atlas.size();
        let dirty = atlas.take_dirty_rows();
        let (target, rows) =
            if let Some(target) = self.atlas.take().filter(|t| t.texture.height() == size.1) {
                (target, dirty)
            } else {
                let usage = TextureUsages::COPY_DST;
                let target =
                    self.create_target("rouge glyph atlas", size, TextureFormat::R8Unorm, usage);
                (target, Some((0, size.1)))
            };
        if let Some((start, end)) = rows {
            let rows = atlas.rows(start, end);
            write_rows(&self.queue, &target.texture, start, rows, size.0);
        }
        let bind_group = target.bind_group.clone();
        self.atlas = Some(target);
        bind_group
    }

    fn sync_scratch(&mut self, frame: &Frame<'_>) {
        let stale = self
            .scratch
            .first()
            .is_none_or(|level| level.dims() != frame.scratch_dims(0));
        if stale {
            let levels = (0..MAX_BLUR_LEVELS)
                .map(|level| self.offscreen("rouge blur scratch", frame.scratch_dims(level), None));
            self.scratch = levels.collect();
        }
    }

    fn ensure_pipeline(&mut self, shader: Shader) {
        if !self.effects.contains_key(&shader) {
            let source = format!("{COMMON_WGSL}\n{EFFECT_WGSL}\n{}", shader.wgsl());
            let blend = Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING);
            let (device, layout) = (&self.device, &self.effect_layout);
            let pipeline =
                create_pipeline(device, layout, self.format, &source, "fs_main", blend, 0);
            self.effects.insert(shader, pipeline);
        }
    }

    fn plan_effect<'a>(
        &mut self,
        applied: Applied<'_>,
        surface: Surface,
        behind: Option<&mut Behind>,
        frame: &mut Frame<'a>,
        steps: &mut Vec<Step<'a>>,
    ) {
        let Applied {
            draw,
            transform,
            opacity,
            scissor,
            content,
        } = applied;
        let effect = draw.effect;
        let mut shader = effect.shader();
        if shader.inputs().backdrop() && behind.is_none() {
            if !shader.inputs().content() {
                return;
            }
            shader = Shader::of::<Passthrough>();
        }
        let Some(inverse) = transform.inverse() else {
            return;
        };
        let (scale, margin) = (frame.scale, effect.margin());
        let (quad, content_map, content) = match content {
            Some((key, cover)) => {
                let Some(group) = self.groups.get(&key) else {
                    return;
                };
                let (mapping, reach) = match &group.blurred {
                    Some(blurred) => (blurred.mapping, blurred.reach),
                    None => (group.mapping(), 0.0),
                };
                (
                    cover.inflate(margin * scale + reach),
                    mapping,
                    Some((key, group.blurred.is_some())),
                )
            }
            None => (
                device_rect(&transform, &draw.bounds.inflate(margin), scale),
                Mapping::default(),
                None,
            ),
        };

        let mut backdrop = None;
        let mut backdrop_map = Mapping::default();
        let reads = shader.inputs().backdrop();
        if let Some(behind) = behind.filter(|_| reads) {
            let kernel = Kernel::new(effect.backdrop_blur() * scale);
            let source = Source {
                origin: (0.0, 0.0),
                dims: (
                    frame.viewport.size.width as u32,
                    frame.viewport.size.height as u32,
                ),
                valid: frame.viewport,
                zero_outside: false,
            };
            let area = quad
                .inflate(Kernel::reach(kernel))
                .intersect(&frame.viewport);
            let Some(geometry) = BlurGeometry::new(&source, area, kernel) else {
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
            let existing =
                (index < self.backdrops.len()).then(|| self.backdrops.swap_remove(index));
            let reusable = existing.filter(|existing| existing.target.dims() == geometry.dims);
            let cached = reusable
                .as_ref()
                .is_some_and(|existing| existing.signature == signature);
            let target =
                self.offscreen("rouge backdrop", geometry.dims, reusable.map(|r| r.target));
            self.backdrops.push(BackdropTarget { target, signature });
            let last = self.backdrops.len() - 1;
            self.backdrops.swap(index, last);
            if cached {
                self.stats.backdrops_reused += 1;
            } else {
                let passes = geometry.passes(&source, kernel, frame);
                let blur = Blur {
                    target: BlurTarget::Backdrop(index),
                    passes,
                };
                let clear = !behind
                    .since
                    .iter()
                    .any(|drawn| !drawn.intersect(&area).is_empty());
                if let Some(Step::Snapshot(blurs)) = behind
                    .open
                    .filter(|_| clear)
                    .and_then(|open| steps.get_mut(open))
                {
                    blurs.push(blur);
                } else {
                    behind.open = Some(steps.len());
                    behind.since.clear();
                    steps.push(Step::Snapshot(vec![blur]));
                }
                frame.snapshots = true;
                frame.blurs |= kernel.is_some();
            }
            backdrop = Some(index);
            backdrop_map = geometry.mapping;
        }

        let mut params = [0.0; 16];
        params.copy_from_slice(effect.uniforms());
        let bounded = if content.is_some() {
            UNBOUNDED_MARGIN
        } else {
            margin
        };
        let uniform = frame.push(&LayerUniform {
            matrix: [transform.a, transform.b, transform.c, transform.d],
            translation_opacity: [transform.tx, transform.ty, opacity, bounded],
            target: surface.target(),
            rect: quad.to_array(),
            fill: [0.0; 4],
            radii: draw.corner_radii.to_array(),
            inverse: [inverse.a, inverse.b, inverse.c, inverse.d],
            bounds: draw.bounds.to_array(),
            content_map: content_map.map,
            content_bounds: content_map.bounds,
            backdrop_map: backdrop_map.map,
            backdrop_bounds: backdrop_map.bounds,
            scale: [scale; 4],
            params,
        });
        self.ensure_pipeline(shader);
        steps.push(Step::Effect {
            shader,
            uniform,
            content,
            backdrop,
            scissor,
        });
    }

    fn plan<'a>(
        &mut self,
        items: &'a [Item<'a>],
        surface: Surface,
        mut behind: Option<&mut Behind>,
        frame: &mut Frame<'a>,
    ) -> Vec<Step<'a>> {
        let mut steps = Vec::with_capacity(items.len());
        for item in items {
            match item {
                Item::Layer(draw) => {
                    let Some(scissor) =
                        surface.scissor(draw.scissor).filter(|_| draw.opacity > 0.0)
                    else {
                        continue;
                    };
                    self.stats.instance_bytes_uploaded += draw.list.sync(&self.device, &self.queue);
                    for image in draw
                        .list
                        .batches()
                        .iter()
                        .filter_map(|batch| batch.image.as_ref())
                    {
                        self.sync_image(image);
                    }
                    if let Some(backdrop) = &draw.backdrop {
                        let applied = Applied {
                            draw: backdrop,
                            transform: draw.transform,
                            opacity: draw.opacity,
                            scissor,
                            content: None,
                        };
                        self.plan_effect(
                            applied,
                            surface,
                            behind.as_deref_mut(),
                            frame,
                            &mut steps,
                        );
                    }
                    if !draw.list.is_empty() {
                        let t = draw.transform;
                        let uniform = frame.push(&LayerUniform {
                            matrix: [t.a, t.b, t.c, t.d],
                            translation_opacity: [t.tx, t.ty, draw.opacity, 0.0],
                            target: surface.target(),
                            fill: draw.fill,
                            radii: draw.radii.to_array(),
                            scale: [frame.scale; 4],
                            ..Zeroable::zeroed()
                        });
                        steps.push(Step::Draw {
                            draw,
                            uniform,
                            scissor,
                        });
                    }
                }
                Item::Group(group) => {
                    let Some(cover) = surface.cover(&group.bounds).filter(|_| group.opacity > 0.0)
                    else {
                        continue;
                    };
                    let Some(scissor) = surface.scissor(group.scissor) else {
                        continue;
                    };
                    let needed = (
                        cover.size.0.next_multiple_of(GROUP_TEXTURE_STEP),
                        cover.size.1.next_multiple_of(GROUP_TEXTURE_STEP),
                    );
                    let existing = self
                        .groups
                        .remove(&group.key)
                        .filter(|g| g.target.dims() == needed);
                    let fresh = existing.as_ref().is_some_and(|g| {
                        g.signature == group.signature
                            && g.surface.origin == cover.origin
                            && g.surface.size == cover.size
                    });
                    let (target, mut blurred) = match existing {
                        Some(existing) => (existing.target, existing.blurred),
                        None => (self.offscreen("rouge group", needed, None), None),
                    };
                    if !fresh {
                        let inner = Surface {
                            origin: cover.origin,
                            size: needed,
                        };
                        let steps = self.plan(&group.items, inner, None, frame);
                        frame.passes.push(Pass::Raster {
                            key: group.key,
                            steps,
                        });
                    }

                    let passthrough = EffectDraw::new(&Passthrough, Rect::ZERO);
                    let (draw, transform) = match &group.effect {
                        Some(effect) => (effect, group.transform),
                        None => (&passthrough, Affine::IDENTITY),
                    };
                    let kernel = Kernel::new(draw.effect.content_blur() * frame.scale);
                    let source = Source {
                        origin: (cover.origin.0 as f32, cover.origin.1 as f32),
                        dims: needed,
                        valid: cover.rect(),
                        zero_outside: true,
                    };
                    let reach = Kernel::reach(kernel);
                    let geometry = kernel.and_then(|_| {
                        BlurGeometry::new(&source, cover.rect().inflate(reach), kernel)
                    });
                    blurred = match geometry {
                        Some(geometry) => {
                            let state = Kernel::state(kernel);
                            let reusable = blurred.filter(|b| b.target.dims() == geometry.dims);
                            let keep = fresh && reusable.as_ref().is_some_and(|b| b.state == state);
                            let target = self.offscreen(
                                "rouge group blur",
                                geometry.dims,
                                reusable.map(|b| b.target),
                            );
                            if !keep {
                                let passes = geometry.passes(&source, kernel, frame);
                                frame.passes.push(Pass::Blur(Blur {
                                    target: BlurTarget::Group(group.key),
                                    passes,
                                }));
                                frame.blurs = true;
                            }
                            Some(Blurred {
                                target,
                                state,
                                mapping: geometry.mapping,
                                reach,
                            })
                        }
                        None => None,
                    };
                    let entry = GroupTarget {
                        target,
                        surface: cover,
                        signature: group.signature,
                        blurred,
                        used: true,
                    };
                    self.groups.insert(group.key, entry);
                    let applied = Applied {
                        draw,
                        transform,
                        opacity: group.opacity,
                        scissor,
                        content: Some((group.key, cover.rect())),
                    };
                    self.plan_effect(applied, surface, behind.as_deref_mut(), frame, &mut steps);
                }
            }
            if let Some(behind) = behind.as_deref_mut() {
                item.fingerprint(&mut behind.hasher);
                behind.since.extend(item.extent(frame.scale));
            }
        }
        steps
    }

    fn encode_blur(
        &self,
        encoder: &mut CommandEncoder,
        blur: &Blur,
        scene: Option<&Target>,
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
                    group.map(|g| &g.target),
                    group.and_then(|g| g.blurred.as_ref()).map(|b| &b.target),
                )
            }
        };
        let (Some(mut source), Some(target)) = (source, target) else {
            return;
        };
        for pass in &blur.passes {
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
            render.set_pipeline(match pass.kind {
                BlurKind::Copy => &self.blit_pipeline,
                BlurKind::Down => &self.down_pipeline,
                BlurKind::Up => &self.up_pipeline,
            });
            render.set_bind_group(0, &source.bind_group, &[]);
            render.set_bind_group(1, &self.layer_bind_group, &[pass.uniform]);
            render.draw(0..3, 0..1);
            source = destination;
        }
    }

    fn encode(
        &self,
        encoder: &mut CommandEncoder,
        destination: Destination<'_>,
        steps: &[Step<'_>],
        atlas: &BindGroup,
        stats: &mut RenderStats,
    ) {
        let Destination { view, load, scene } = destination;
        let begin = |encoder: &mut CommandEncoder, load| {
            let mut pass = begin_pass(encoder, view, load);
            pass.set_pipeline(&self.primitive_pipeline);
            pass.set_bind_group(0, atlas, &[]);
            pass
        };
        let blank = self.blank.as_ref().map(|blank| &blank.bind_group);
        let mut pass = begin(encoder, load);
        for step in steps {
            match step {
                Step::Snapshot(blurs) => {
                    drop(pass);
                    for blur in blurs {
                        self.encode_blur(encoder, blur, scene, stats);
                    }
                    pass = begin(encoder, LoadOp::Load);
                    stats.backdrop_passes += 1;
                }
                Step::Draw {
                    draw,
                    uniform,
                    scissor,
                } => {
                    let buffer = draw.list.buffer();
                    if let Some(instances) = buffer.as_ref() {
                        pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
                        pass.set_bind_group(1, &self.layer_bind_group, &[*uniform]);
                        pass.set_vertex_buffer(0, instances.slice(..));
                        for batch in draw.list.batches() {
                            let image = batch
                                .image
                                .as_ref()
                                .map(|image| self.images.get(&image.id()));
                            let texture = match image {
                                Some(Some(image)) => &image.target.bind_group,
                                Some(None) => continue,
                                None => atlas,
                            };
                            pass.set_bind_group(0, texture, &[]);
                            pass.draw(0..4, batch.instances.clone());
                            stats.images += u32::from(batch.image.is_some());
                            stats.draw_calls += 1;
                        }
                        pass.set_bind_group(0, atlas, &[]);
                        stats.layers += 1;
                    }
                }
                Step::Effect {
                    shader,
                    uniform,
                    content,
                    backdrop,
                    scissor,
                } => {
                    let group =
                        content.and_then(|(key, blurred)| Some((self.groups.get(&key)?, blurred)));
                    let content = group.map(|(group, blurred)| match (&group.blurred, blurred) {
                        (Some(blurred), true) => &blurred.target.bind_group,
                        _ => &group.target.bind_group,
                    });
                    let backdrop = backdrop.and_then(|index| self.backdrops.get(index));
                    let backdrop = backdrop.map(|backdrop| &backdrop.target.bind_group);
                    let (Some(pipeline), Some(content), Some(backdrop)) = (
                        self.effects.get(shader),
                        content.or(blank),
                        backdrop.or(blank),
                    ) else {
                        continue;
                    };
                    pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, content, &[]);
                    pass.set_bind_group(1, &self.layer_bind_group, &[*uniform]);
                    pass.set_bind_group(2, backdrop, &[]);
                    pass.draw(0..4, 0..1);
                    pass.set_pipeline(&self.primitive_pipeline);
                    pass.set_bind_group(0, atlas, &[]);
                    stats.groups += u32::from(group.is_some());
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
    ) {
        self.stats = RenderStats::default();
        self.frame += 1;
        let atlas = self.sync_atlas(atlas);
        if self.blank.is_none() {
            self.blank = Some(self.offscreen("rouge blank", (1, 1), None));
        }

        let surface = Surface {
            origin: (0, 0),
            size: (viewport.width.max(1), viewport.height.max(1)),
        };
        let pad = |extent: u32| (extent + SCRATCH_PADDING).next_multiple_of(GROUP_TEXTURE_STEP);
        let mut frame = Frame {
            uniforms: Vec::new(),
            stride: self.layer_stride as usize,
            passes: Vec::new(),
            scale: viewport.scale,
            viewport: surface.rect(),
            scratch: (pad(surface.size.0), pad(surface.size.1)),
            snapshots: false,
            blurs: false,
        };
        let mut behind = Behind {
            hasher: DefaultHasher::new(),
            open: None,
            since: Vec::new(),
            backdrops: 0,
        };
        (
            surface.size,
            viewport.scale.to_bits(),
            clear.to_array().map(f32::to_bits),
        )
            .hash(&mut behind.hasher);
        let steps = self.plan(items, surface, Some(&mut behind), &mut frame);
        self.groups.retain(|_, group| take(&mut group.used));
        self.backdrops.truncate(behind.backdrops);
        let blit = frame.push(&RegionUniform {
            rect: [0.0, 0.0, 1.0, 1.0],
            bounds: [0.0, 0.0, 1.0, 1.0],
            options: [0.0; 4],
        });
        if frame.uniforms.len() as u64 > self.layer_buffer.size() {
            let size = (frame.uniforms.len() as u64).next_power_of_two();
            (self.layer_buffer, self.layer_bind_group) =
                create_layer_buffer(&self.device, &self.layer_layout, size);
        }
        self.queue
            .write_buffer(&self.layer_buffer, 0, &frame.uniforms);
        if frame.blurs {
            self.sync_scratch(&frame);
        }
        if frame.snapshots {
            let scene = self.scene.take();
            self.scene = Some(self.offscreen("rouge scene", surface.size, scene));
        }
        let scene = self.scene.as_ref().filter(|_| frame.snapshots);

        let descriptor = CommandEncoderDescriptor {
            label: Some("rouge frame"),
        };
        let mut encoder = self.device.create_command_encoder(&descriptor);
        let mut stats = self.stats;
        for pass in &frame.passes {
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
                    self.encode(&mut encoder, destination, steps, &atlas, &mut stats);
                    stats.group_passes += 1;
                }
                Pass::Blur(blur) => self.encode_blur(&mut encoder, blur, None, &mut stats),
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
        self.encode(&mut encoder, destination, &steps, &atlas, &mut stats);

        if let Some(scene) = scene {
            let mut pass = begin_pass(&mut encoder, target, LoadOp::Clear(ClearColor::TRANSPARENT));
            pass.set_pipeline(&self.blit_pipeline);
            pass.set_bind_group(0, &scene.bind_group, &[]);
            pass.set_bind_group(1, &self.layer_bind_group, &[blit]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.stats = stats;
        self.evict_images();
    }
}

pub(crate) fn device_rect(transform: &Affine, rect: &Rect, scale: f32) -> Rect {
    let rect = transform.apply_rect(rect);
    Rect::new(
        rect.origin.x * scale,
        rect.origin.y * scale,
        rect.size.width * scale,
        rect.size.height * scale,
    )
}

pub(crate) fn upload(
    slot: &mut Option<Buffer>,
    device: &Device,
    queue: &Queue,
    bytes: &[u8],
) -> u64 {
    let needed = bytes.len() as u64;
    if needed == 0 {
        return 0;
    }
    let buffer = match slot.take().filter(|buffer| buffer.size() >= needed) {
        Some(buffer) => buffer,
        None => device.create_buffer(&BufferDescriptor {
            label: Some("rouge instances"),
            size: needed.next_power_of_two().max(256),
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
    };
    queue.write_buffer(&buffer, 0, bytes);
    *slot = Some(buffer);
    needed
}

fn begin_pass(
    encoder: &mut CommandEncoder,
    view: &TextureView,
    load: LoadOp<ClearColor>,
) -> RenderPass<'static> {
    encoder
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("rouge pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
        .forget_lifetime()
}

fn create_layer_buffer(
    device: &Device,
    layout: &BindGroupLayout,
    size: u64,
) -> (Buffer, BindGroup) {
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("rouge layers"),
        size,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("rouge layers"),
        layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::Buffer(BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: BufferSize::new(LAYER_UNIFORM_SIZE),
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

fn create_pipeline(
    device: &Device,
    layout: &PipelineLayout,
    format: TextureFormat,
    source: &str,
    fragment: &str,
    blend: Option<BlendState>,
    instance_size: u64,
) -> RenderPipeline {
    let module = device.create_shader_module(ShaderModuleDescriptor {
        label: Some(fragment),
        source: ShaderSource::Wgsl(source.into()),
    });
    let attributes: Vec<_> = (0..instance_size / 16)
        .map(|i| VertexAttribute {
            format: VertexFormat::Float32x4,
            offset: i * 16,
            shader_location: i as u32,
        })
        .collect();
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(layout),
        vertex: VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: instance_size,
                step_mode: VertexStepMode::Instance,
                attributes: &attributes,
            })][..usize::from(instance_size > 0)],
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &module,
            entry_point: Some(fragment),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(ColorTargetState {
                format,
                blend,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
