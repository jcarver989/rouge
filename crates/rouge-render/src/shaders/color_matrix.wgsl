fn effect(in: EffectInput) -> vec4<f32> {
    let color = sample_content(in.local);
    if (color.a <= 0.0) {
        return vec4<f32>(0.0);
    }
    let straight = vec4<f32>(color.rgb / color.a, 1.0);
    let mapped = vec3<f32>(dot(param(0), straight), dot(param(1), straight), dot(param(2), straight));
    return vec4<f32>(clamp(mapped, vec3<f32>(0.0), vec3<f32>(1.0)) * color.a, color.a);
}
