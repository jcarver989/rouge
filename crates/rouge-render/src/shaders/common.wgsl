struct Layer {
    matrix: vec4<f32>,
    translation_opacity: vec4<f32>,
    surface: vec4<f32>,
    rect: vec4<f32>,
    fill: vec4<f32>,
    radii: vec4<f32>,
    inverse: vec4<f32>,
    bounds: vec4<f32>,
    content_map: vec4<f32>,
    content_bounds: vec4<f32>,
    backdrop_map: vec4<f32>,
    backdrop_bounds: vec4<f32>,
    scale: vec4<f32>,
    params: array<vec4<f32>, 4>,
};

@group(1) @binding(0) var<uniform> layer: Layer;

fn scale_factor() -> f32 {
    return layer.scale.x;
}

fn transform(m: vec4<f32>, p: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(m.x * p.x + m.z * p.y, m.y * p.x + m.w * p.y);
}

fn device_to_clip(device: vec2<f32>) -> vec4<f32> {
    let ndc = (device - layer.surface.zw) / layer.surface.xy * 2.0 - vec2<f32>(1.0);
    return vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
}

fn to_device(local_px: vec2<f32>) -> vec2<f32> {
    return transform(layer.matrix, local_px) + layer.translation_opacity.xy * scale_factor();
}

fn rect_coverage(p: vec2<f32>, half_size: vec2<f32>, radius: f32, span: f32) -> f32 {
    return clamp(0.5 - sd_rounded_rect(p, half_size, radius) / span, 0.0, 1.0);
}

fn layer_opacity() -> f32 {
    return layer.translation_opacity.z;
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

fn clip_coverage(frag: vec2<f32>, clip: vec4<f32>) -> f32 {
    let lo = clamp(frag - clip.xy + vec2<f32>(0.5), vec2<f32>(0.0), vec2<f32>(1.0));
    let hi = clamp(clip.zw - frag + vec2<f32>(0.5), vec2<f32>(0.0), vec2<f32>(1.0));
    return lo.x * lo.y * hi.x * hi.y;
}
