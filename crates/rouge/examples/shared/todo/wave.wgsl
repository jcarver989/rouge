fn effect(in: EffectInput) -> vec4<f32> {
    let wave = param(0);
    let shape = param(3);
    let away = in.local - wave.zw;
    let reach = max(length(away), 0.001);
    let front = reach - wave.x * shape.x;
    let fade = exp(-wave.x * shape.z) * (1.0 - clamp(wave.x / wave.y, 0.0, 1.0));
    let swell = shape.y * fade * exp(-front * front / (shape.w * shape.w));
    let bent = sample_backdrop(in.local + away / reach * swell * sin(front * 6.2831853 / shape.w));
    let ink = clamp(distance(bent.rgb, param(1).rgb) * 3.0, 0.0, 1.0);
    let flush = clamp(swell * 0.1, 0.0, 1.0) * ink;
    return vec4<f32>(mix(bent.rgb, param(2).rgb, flush), 1.0) * in.coverage;
}
