@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

struct Region {
    rect: vec4<f32>,
    bounds: vec4<f32>,
    options: vec4<f32>,
};

@group(1) @binding(0) var<uniform> region: Region;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VsOut {
    let corner = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    var out: VsOut;
    out.position = vec4<f32>(corner * 2.0 - vec2<f32>(1.0), 0.0, 1.0);
    out.uv = region.rect.xy + vec2<f32>(corner.x, 1.0 - corner.y) * region.rect.zw;
    return out;
}

fn tap(uv: vec2<f32>) -> vec4<f32> {
    let inside = step(region.bounds.xy, uv) * step(uv, region.bounds.zw);
    let keep = max(inside.x * inside.y, 1.0 - region.options.y);
    let at = clamp(uv, region.bounds.xy, region.bounds.zw);
    return textureSampleLevel(source, source_sampler, at, 0.0) * keep;
}

@fragment
fn fs_blit(in: VsOut) -> @location(0) vec4<f32> {
    return tap(in.uv);
}

@fragment
fn fs_down(in: VsOut) -> @location(0) vec4<f32> {
    let h = region.options.x / vec2<f32>(textureDimensions(source));
    var sum = tap(in.uv) * 4.0;
    sum += tap(in.uv + vec2<f32>(-h.x, -h.y));
    sum += tap(in.uv + vec2<f32>(h.x, -h.y));
    sum += tap(in.uv + vec2<f32>(-h.x, h.y));
    sum += tap(in.uv + vec2<f32>(h.x, h.y));
    return sum / 8.0;
}

@fragment
fn fs_up(in: VsOut) -> @location(0) vec4<f32> {
    let h = 0.5 * region.options.x / vec2<f32>(textureDimensions(source));
    var sum = tap(in.uv + vec2<f32>(-h.x * 2.0, 0.0));
    sum += tap(in.uv + vec2<f32>(-h.x, h.y)) * 2.0;
    sum += tap(in.uv + vec2<f32>(0.0, h.y * 2.0));
    sum += tap(in.uv + vec2<f32>(h.x, h.y)) * 2.0;
    sum += tap(in.uv + vec2<f32>(h.x * 2.0, 0.0));
    sum += tap(in.uv + vec2<f32>(h.x, -h.y)) * 2.0;
    sum += tap(in.uv + vec2<f32>(0.0, -h.y * 2.0));
    sum += tap(in.uv + vec2<f32>(-h.x, -h.y)) * 2.0;
    return sum / 12.0;
}
