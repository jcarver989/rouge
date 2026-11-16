@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) bounds: vec4<f32>,
    @location(2) @interpolate(flat) options: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VsOut {
    let corner = select_record(vertex_index);
    let rect = row(0u);
    var out: VsOut;
    out.position = vec4<f32>(corner * 2.0 - vec2<f32>(1.0), 0.0, 1.0);
    out.uv = rect.xy + vec2<f32>(corner.x, 1.0 - corner.y) * rect.zw;
    out.bounds = row(1u);
    out.options = row(2u);
    return out;
}

fn tap(in: VsOut, uv: vec2<f32>) -> vec4<f32> {
    return sample_within(source, source_sampler, uv, in.bounds, vec2<f32>(0.0), in.options.y > 0.5);
}

@fragment
fn fs_blit(in: VsOut) -> @location(0) vec4<f32> {
    return tap(in, in.uv);
}

@fragment
fn fs_down(in: VsOut) -> @location(0) vec4<f32> {
    let h = in.options.x / vec2<f32>(textureDimensions(source));
    var sum = tap(in, in.uv) * 4.0;
    sum += tap(in, in.uv + vec2<f32>(-h.x, -h.y));
    sum += tap(in, in.uv + vec2<f32>(h.x, -h.y));
    sum += tap(in, in.uv + vec2<f32>(-h.x, h.y));
    sum += tap(in, in.uv + vec2<f32>(h.x, h.y));
    return sum / 8.0;
}

@fragment
fn fs_up(in: VsOut) -> @location(0) vec4<f32> {
    let h = 0.5 * in.options.x / vec2<f32>(textureDimensions(source));
    var sum = tap(in, in.uv + vec2<f32>(-h.x * 2.0, 0.0));
    sum += tap(in, in.uv + vec2<f32>(-h.x, h.y)) * 2.0;
    sum += tap(in, in.uv + vec2<f32>(0.0, h.y * 2.0));
    sum += tap(in, in.uv + vec2<f32>(h.x, h.y)) * 2.0;
    sum += tap(in, in.uv + vec2<f32>(h.x * 2.0, 0.0));
    sum += tap(in, in.uv + vec2<f32>(h.x, -h.y)) * 2.0;
    sum += tap(in, in.uv + vec2<f32>(0.0, -h.y * 2.0));
    sum += tap(in, in.uv + vec2<f32>(-h.x, -h.y)) * 2.0;
    return sum / 12.0;
}

@fragment
fn fs_fill(in: VsOut) -> @location(0) vec4<f32> {
    return in.options;
}
