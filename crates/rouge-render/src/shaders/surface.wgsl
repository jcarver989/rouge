fn effect(in: EffectInput) -> vec4<f32> {
    let map = layer.content_map;
    let limit = layer.content_bounds;
    let uv = clamp(map.xy + in.uv * map.zw, limit.xy, limit.zw);
    var color = textureSampleLevel(content_texture, content_sampler, uv, 0.0);
    if (param(0).x > 0.5) {
        color = vec4<f32>(color.rgb, 1.0);
    }
    return color * in.coverage;
}
