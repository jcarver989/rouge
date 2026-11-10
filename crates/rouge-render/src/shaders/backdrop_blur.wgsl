fn effect(in: EffectInput) -> vec4<f32> {
    return sample_backdrop(in.local) * in.coverage;
}
