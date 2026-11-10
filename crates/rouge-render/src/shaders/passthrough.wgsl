fn effect(in: EffectInput) -> vec4<f32> {
    return sample_content(in.local);
}
