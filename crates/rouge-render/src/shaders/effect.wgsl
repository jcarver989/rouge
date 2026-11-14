const ROW_RECT: u32 = 4u;
const ROW_INVERSE: u32 = 5u;
const ROW_BOUNDS: u32 = 6u;
const ROW_CONTENT_MAP: u32 = 7u;
const ROW_CONTENT_BOUNDS: u32 = 8u;
const ROW_BACKDROP_MAP: u32 = 9u;
const ROW_BACKDROP_BOUNDS: u32 = 10u;
const ROW_MARGIN: u32 = 11u;
const ROW_PARAMS: u32 = 12u;

@group(0) @binding(0) var content_texture: texture_2d<f32>;
@group(0) @binding(1) var content_sampler: sampler;
@group(2) @binding(0) var backdrop_texture: texture_2d<f32>;
@group(2) @binding(1) var backdrop_sampler: sampler;

struct EffectInput {
    local: vec2<f32>,
    size: vec2<f32>,
    uv: vec2<f32>,
    distance: f32,
    coverage: f32,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) record: u32,
};

fn to_local(device: vec2<f32>) -> vec2<f32> {
    let d = device - row(ROW_PLACEMENT).xy * scale_factor();
    return transform(row(ROW_INVERSE), d) / scale_factor() - row(ROW_BOUNDS).xy;
}

fn sample(texture: texture_2d<f32>, at: sampler, local: vec2<f32>, map: vec4<f32>, bounds: vec4<f32>, zero_outside: bool) -> vec4<f32> {
    let uv = (to_device((local + row(ROW_BOUNDS).xy) * scale_factor()) - map.xy) * map.zw;
    let texel = 0.5 / vec2<f32>(textureDimensions(texture));
    return sample_within(texture, at, uv, bounds, texel, zero_outside);
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VsOut {
    let unit = select_record(vertex_index);
    let rect = row(ROW_RECT);
    let device = rect.xy + unit * rect.zw;
    var out: VsOut;
    out.position = device_to_clip(device);
    out.local = to_local(device);
    out.record = record;
    return out;
}

fn param(index: i32) -> vec4<f32> {
    return row(ROW_PARAMS + u32(index));
}

fn shape_sdf(local: vec2<f32>) -> f32 {
    let half_size = row(ROW_BOUNDS).zw * 0.5;
    let p = local - half_size;
    let radius = min(pick_radius(p, row(ROW_RADII)), min(half_size.x, half_size.y));
    return sd_rounded_rect(p, half_size, radius);
}

fn sample_content(local: vec2<f32>) -> vec4<f32> {
    return sample(content_texture, content_sampler, local, row(ROW_CONTENT_MAP), row(ROW_CONTENT_BOUNDS), true);
}

fn sample_backdrop(local: vec2<f32>) -> vec4<f32> {
    return sample(backdrop_texture, backdrop_sampler, local, row(ROW_BACKDROP_MAP), row(ROW_BACKDROP_BOUNDS), false);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    record = in.record;
    let span = pixel_span(in.local);
    let size = row(ROW_BOUNDS).zw;
    let margin = vec2<f32>(row(ROW_MARGIN).x);
    if (any(in.local < -margin) || any(in.local > size + margin)) {
        discard;
    }
    var input: EffectInput;
    input.local = in.local;
    input.size = size;
    input.uv = in.local / max(size, vec2<f32>(1e-6));
    input.distance = shape_sdf(in.local);
    input.coverage = clamp(0.5 - input.distance / span, 0.0, 1.0);
    return effect(input) * layer_opacity();
}
