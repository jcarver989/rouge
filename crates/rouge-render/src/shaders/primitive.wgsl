@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var atlas_sampler: sampler;

const ROW_FILL: u32 = 4u;
const KIND: u32 = 3u;
const QUAD: u32 = 0u;
const SHADOW: u32 = 1u;
const GLYPH: u32 = 2u;
const IMAGE: u32 = 3u;
const LAYER_FILL: u32 = 4u;
const LAYER_RADII: u32 = 8u;
const ALPHA_MASK: u32 = 16u;

struct Instance {
    @location(0) bounds: vec4<f32>,
    @location(1) clip: vec4<f32>,
    @location(2) radii: vec4<f32>,
    @location(3) params: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) border_color: vec4<f32>,
    @location(6) flags: u32,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) rect: vec4<f32>,
    @location(1) @interpolate(flat) clip: vec4<f32>,
    @location(2) @interpolate(flat) color: vec4<f32>,
    @location(3) @interpolate(flat) border_color: vec4<f32>,
    @location(4) @interpolate(flat) radii: vec4<f32>,
    @location(5) @interpolate(flat) params: vec4<f32>,
    @location(6) @interpolate(flat) kind: u32,
    @location(7) local: vec2<f32>,
    @location(8) texel: vec2<f32>,
    @location(9) @interpolate(flat) opacity: f32,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, instance: Instance) -> VsOut {
    let unit = select_record(vertex_index);
    let kind = instance.flags & KIND;
    var origin = instance.bounds.xy * scale_factor();
    var size = instance.bounds.zw * scale_factor();
    var params = instance.params * scale_factor();
    var margin = vec2<f32>(1.0);
    if (kind == SHADOW) {
        params.x = max(params.x, 0.01);
        margin = vec2<f32>(3.0 * params.x + 1.0);
    } else if (kind == GLYPH) {
        origin = round(origin);
        size = instance.params.zw;
        margin = vec2<f32>(0.0);
    } else if (kind == QUAD) {
        let edges = snap(vec4<f32>(origin, (instance.bounds.xy + instance.bounds.zw) * scale_factor()));
        let border = select(vec4<f32>(0.0), max(snap(params), vec4<f32>(1.0)), params > vec4<f32>(0.0));
        let solid = instance.params.wx + instance.params.yz >= instance.bounds.zw;
        origin = edges.xy;
        size = select(edges.zw - edges.xy, border.wx + border.yz, solid);
        params = border;
    }
    let position = origin - margin + unit * (size + 2.0 * margin);
    let clip = snap(vec4<f32>(instance.clip.xy, instance.clip.xy + instance.clip.zw) * scale_factor());

    var out: VsOut;
    out.position = device_to_clip(to_device(position));
    out.local = position;
    out.texel = instance.params.xy + unit * instance.params.zw;
    out.rect = vec4<f32>(origin + size * 0.5, size * 0.5);
    out.clip = clip;
    out.color = premultiply(instance.color);
    if ((instance.flags & LAYER_FILL) != 0u) {
        out.color = row(ROW_FILL);
    }
    out.border_color = premultiply(instance.border_color);
    if (kind == IMAGE) {
        out.texel = instance.params.xy + (position - origin) / max(size, vec2<f32>(1e-5)) * instance.params.zw;
        out.border_color = instance.params;
        params = vec4<f32>(select(0.0, 1.0, (instance.flags & ALPHA_MASK) != 0u), 0.0, 0.0, 0.0);
    }
    var radii = instance.radii;
    if ((instance.flags & LAYER_RADII) != 0u) {
        radii = max(row(ROW_RADII) + instance.radii, vec4<f32>(0.0));
    }
    out.radii = radii * scale_factor();
    out.params = params;
    out.kind = kind;
    out.opacity = layer_opacity();
    return out;
}

fn snap(v: vec4<f32>) -> vec4<f32> {
    return floor(v + 0.5);
}

fn gaussian(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (sqrt(6.283185307) * sigma);
}

fn erf2(v: vec2<f32>) -> vec2<f32> {
    let s = sign(v);
    let a = abs(v);
    let r1 = 1.0 + (0.278393 + (0.230389 + (0.000972 + 0.078108 * a) * a) * a) * a;
    let r2 = r1 * r1;
    return s - s / (r2 * r2);
}

fn blur_along_x(x: f32, y: f32, sigma: f32, corner: f32, half_size: vec2<f32>) -> f32 {
    let delta = min(half_size.y - corner - abs(y), 0.0);
    let curved = half_size.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    let integral = 0.5 + 0.5 * erf2((x + vec2<f32>(-curved, curved)) * (sqrt(0.5) / sigma));
    return integral.y - integral.x;
}

fn shadow_coverage(p: vec2<f32>, half_size: vec2<f32>, corner: f32, sigma: f32) -> f32 {
    let low = p.y - half_size.y;
    let high = p.y + half_size.y;
    let start = clamp(-3.0 * sigma, low, high);
    let end = clamp(3.0 * sigma, low, high);
    let step = (end - start) / 4.0;
    var y = start + step * 0.5;
    var alpha = 0.0;
    for (var i = 0; i < 4; i += 1) {
        alpha += blur_along_x(p.x, p.y - y, sigma, corner, half_size) * gaussian(y, sigma) * step;
        y += step;
    }
    return clamp(alpha, 0.0, 1.0);
}

fn quad_color(in: VsOut, p: vec2<f32>, half_size: vec2<f32>, radius: f32, span: f32) -> vec4<f32> {
    let coverage = rect_coverage(p, half_size, radius, span);
    var color = in.color;
    let b = in.params;
    if (b.x + b.y + b.z + b.w > 0.0) {
        let inner_center = vec2<f32>(b.w - b.y, b.x - b.z) * 0.5;
        let inner_half = half_size - vec2<f32>(b.w + b.y, b.x + b.z) * 0.5;
        var border_coverage = 1.0;
        if (inner_half.x > 0.0 && inner_half.y > 0.0) {
            let ip = p - inner_center;
            let bx = select(b.y, b.w, ip.x < 0.0);
            let by = select(b.z, b.x, ip.y < 0.0);
            let inner_radius = clamp(
                pick_radius(ip, in.radii) - max(bx, by),
                0.0,
                min(inner_half.x, inner_half.y),
            );
            let inner = sd_rounded_rect(ip, inner_half, inner_radius);
            border_coverage = clamp(inner / span + 0.5, 0.0, 1.0);
        }
        color = in.border_color * border_coverage
            + color * (1.0 - in.border_color.a * border_coverage);
    }
    return color * coverage;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let span = pixel_span(in.local);
    let visible = clip_coverage(in.local, in.clip) * in.opacity;
    if (in.kind == GLYPH) {
        let uv = in.texel / vec2<f32>(textureDimensions(atlas));
        return in.color * textureSampleLevel(atlas, atlas_sampler, uv, 0.0).r * visible;
    }
    let half_size = in.rect.zw;
    let p = in.local - in.rect.xy;
    let radius = min(pick_radius(p, in.radii), min(half_size.x, half_size.y));
    if (in.kind == SHADOW) {
        return in.color * shadow_coverage(p, half_size, radius, in.params.x) * visible;
    }
    if (in.kind == IMAGE) {
        let source = in.border_color;
        let half_texel = 0.5 / vec2<f32>(textureDimensions(atlas));
        let uv = clamp(in.texel, source.xy + half_texel, source.xy + source.zw - half_texel);
        let sample = textureSampleLevel(atlas, atlas_sampler, uv, 0.0);
        let coverage = rect_coverage(p, half_size, radius, span);
        let color = select(sample * in.color, in.color * sample.r, in.params.x > 0.5);
        return color * coverage * visible;
    }
    return quad_color(in, p, half_size, radius, span) * visible;
}
