struct Pattern {
    colors: array<vec4<f32>, 4>,
    flip: vec4<f32>,
};

@group(0) @binding(0) var<uniform> pattern: Pattern;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VsOut;
    out.position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var v = in.uv.y;
    if (pattern.flip.x > 0.5) {
        v = 1.0 - v;
    }
    let column = select(0u, 1u, in.uv.x >= 0.5);
    let row = select(0u, 2u, v >= 0.5);
    return pattern.colors[row + column];
}
