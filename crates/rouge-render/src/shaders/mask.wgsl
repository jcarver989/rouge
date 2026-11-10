fn effect(in: EffectInput) -> vec4<f32> {
    let options = param(0);
    let geometry = param(1);
    var along = 0.0;
    if (options.x < 0.5) {
        let axis = (geometry.zw - geometry.xy) * in.size;
        let offset = (in.uv - geometry.xy) * in.size;
        along = dot(offset, axis) / max(dot(axis, axis), 1e-6);
    } else if (options.x < 1.5) {
        let reach = max(geometry.z * min(in.size.x, in.size.y), 1e-6);
        along = length((in.uv - geometry.xy) * in.size) / reach;
    } else {
        along = 1.0 - in.coverage;
    }
    let span = max(options.w - options.z, 1e-6);
    var alpha = 1.0 - clamp((along - options.z) / span, 0.0, 1.0);
    if (options.y > 0.5) {
        alpha = 1.0 - alpha;
    }
    return sample_content(in.local) * alpha;
}
