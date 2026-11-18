fn effect(in: EffectInput) -> vec4<f32> {
    let content = sample_content(in.local);
    let phase = fract(param(0).x / param(0).y);
    let offset = in.local.x / param(0).z - (phase * 3.0 - 1.0);
    let band = exp(-offset * offset * 8.0);
    let glint = param(1) * (band * content.a);
    return glint + content * (1.0 - glint.a);
}
