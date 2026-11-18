fn effect(in: EffectInput) -> vec4<f32> {
    let column = param(0);
    let left = max(column.y, (in.size.x - column.x) * 0.5);
    let below = smoothstep(column.z, column.z + column.w, in.local.y);
    let reading = smoothstep(left, left + 68.0, in.local.x) * (1.0 - below);
    return sample_content(in.local) * (1.0 - 0.85 * reading);
}
