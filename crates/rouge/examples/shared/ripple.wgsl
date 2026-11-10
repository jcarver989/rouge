fn effect(in: EffectInput) -> vec4<f32> {
    let away = in.local - in.size * 0.5;
    let distance = max(length(away), 1e-4);
    let wave = sin(distance * param(0).x) * param(0).y;
    return sample_content(in.local + away / distance * wave);
}
