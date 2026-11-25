const ROW_MATRIX: u32 = 0u;
const ROW_PLACEMENT: u32 = 1u;
const ROW_SURFACE: u32 = 2u;
const ROW_RADII: u32 = 3u;

@group(1) @binding(0) var<uniform> records: array<vec4<f32>, 1024>;

var<private> record: u32;

fn row(index: u32) -> vec4<f32> {
    return records[record + index];
}

fn select_record(vertex_index: u32) -> vec2<f32> {
    record = vertex_index >> 2u;
    return vec2<f32>(f32(vertex_index & 1u), f32((vertex_index >> 1u) & 1u));
}

fn scale_factor() -> f32 {
    return row(ROW_PLACEMENT).w;
}

fn layer_opacity() -> f32 {
    return row(ROW_PLACEMENT).z;
}

fn transform(m: vec4<f32>, p: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(m.x * p.x + m.z * p.y, m.y * p.x + m.w * p.y);
}

fn device_to_clip(device: vec2<f32>) -> vec4<f32> {
    let surface = row(ROW_SURFACE);
    let ndc = (device - surface.zw) / surface.xy * 2.0 - vec2<f32>(1.0);
    return vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
}

fn to_device(local_px: vec2<f32>) -> vec2<f32> {
    return transform(row(ROW_MATRIX), local_px) + row(ROW_PLACEMENT).xy * scale_factor();
}

fn rect_coverage(p: vec2<f32>, half_size: vec2<f32>, radius: f32, span: f32) -> f32 {
    return clamp(0.5 - sd_rounded_rect(p, half_size, radius) / span, 0.0, 1.0);
}

fn pixel_span(local: vec2<f32>) -> f32 {
    return max(length(fwidth(local)) * 0.70710678, 1e-5);
}

fn premultiply(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

fn pick_radius(p: vec2<f32>, r: vec4<f32>) -> f32 {
    if (p.x < 0.0) {
        if (p.y < 0.0) { return r.x; }
        return r.w;
    }
    if (p.y < 0.0) { return r.y; }
    return r.z;
}

fn sd_rounded_rect(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

fn clip_coverage(frag: vec2<f32>, clip: vec4<f32>, pixel: vec2<f32>) -> f32 {
    let lo = clamp((frag - clip.xy) / pixel + vec2<f32>(0.5), vec2<f32>(0.0), vec2<f32>(1.0));
    let hi = clamp((clip.zw - frag) / pixel + vec2<f32>(0.5), vec2<f32>(0.0), vec2<f32>(1.0));
    return lo.x * lo.y * hi.x * hi.y;
}

fn sample_within(texture: texture_2d<f32>, at: sampler, uv: vec2<f32>, bounds: vec4<f32>, inset: vec2<f32>, zero_outside: bool) -> vec4<f32> {
    let inside = step(bounds.xy, uv) * step(uv, bounds.zw);
    let keep = select(1.0, inside.x * inside.y, zero_outside);
    return textureSampleLevel(texture, at, clamp(uv, bounds.xy + inset, bounds.zw - inset), 0.0) * keep;
}
