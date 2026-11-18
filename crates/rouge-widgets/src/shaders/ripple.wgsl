fn effect(in: EffectInput) -> vec4<f32> {
    let progress = clamp(param(0).x / param(0).y, 0.0, 1.0);
    let center = param(0).zw;
    let reach = length(max(center, in.size - center));
    let radius = reach * (1.0 - (1.0 - progress) * (1.0 - progress));
    let inside = 1.0 - smoothstep(radius - 1.0, radius + 1.0, distance(in.local, center));
    let ink = param(1) * (inside * (1.0 - progress) * in.coverage);
    return ink + sample_content(in.local) * (1.0 - ink.a);
}
