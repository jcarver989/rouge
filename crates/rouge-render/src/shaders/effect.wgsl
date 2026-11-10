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
};

fn to_local(device: vec2<f32>) -> vec2<f32> {
    let d = device - layer.translation_opacity.xy * scale_factor();
    return transform(layer.inverse, d) / scale_factor() - layer.bounds.xy;
}

fn sample(texture: texture_2d<f32>, at: sampler, local: vec2<f32>, map: vec4<f32>, bounds: vec4<f32>, zero_outside: bool) -> vec4<f32> {
    let uv = (to_device((local + layer.bounds.xy) * scale_factor()) - map.xy) * map.zw;
    let texel = 0.5 / vec2<f32>(textureDimensions(texture));
    let inside = step(bounds.xy, uv) * step(uv, bounds.zw);
    let keep = select(1.0, inside.x * inside.y, zero_outside);
    return textureSampleLevel(texture, at, clamp(uv, bounds.xy + texel, bounds.zw - texel), 0.0) * keep;
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VsOut {
    let unit = vec2<f32>(f32(vertex_index & 1u), f32((vertex_index >> 1u) & 1u));
    let device = layer.rect.xy + unit * layer.rect.zw;
    var out: VsOut;
    out.position = device_to_clip(device);
    out.local = to_local(device);
    return out;
}

fn param(index: i32) -> vec4<f32> {
    return layer.params[index];
}

fn shape_sdf(local: vec2<f32>) -> f32 {
    let half_size = layer.bounds.zw * 0.5;
    let p = local - half_size;
    let radius = min(pick_radius(p, layer.radii), min(half_size.x, half_size.y));
    return sd_rounded_rect(p, half_size, radius);
}

fn sample_content(local: vec2<f32>) -> vec4<f32> {
    return sample(content_texture, content_sampler, local, layer.content_map, layer.content_bounds, true);
}

fn sample_backdrop(local: vec2<f32>) -> vec4<f32> {
    return sample(backdrop_texture, backdrop_sampler, local, layer.backdrop_map, layer.backdrop_bounds, false);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let span = pixel_span(in.local);
    let size = layer.bounds.zw;
    let margin = vec2<f32>(layer.translation_opacity.w);
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
