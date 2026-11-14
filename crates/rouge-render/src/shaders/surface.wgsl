fn effect(in: EffectInput) -> vec4<f32> {
    let map = row(ROW_CONTENT_MAP);
    let uv = map.xy + in.uv * map.zw;
    var color = sample_within(content_texture, content_sampler, uv, row(ROW_CONTENT_BOUNDS), vec2<f32>(0.0), false);
    if (param(0).x > 0.5) {
        color = vec4<f32>(color.rgb, 1.0);
    }
    return color * in.coverage;
}
