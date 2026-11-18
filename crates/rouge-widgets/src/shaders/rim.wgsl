fn effect(in: EffectInput) -> vec4<f32> {
    let away = in.local - in.size * 0.5;
    let angle = atan2(away.y, away.x) / 6.2831853;
    let behind = fract(param(0).x / param(0).y - angle);
    let tail = pow(1.0 - behind, 3.0);
    let edge = 1.0 - smoothstep(0.0, param(0).z, abs(in.distance));
    let rim = param(1) * (tail * edge * in.coverage);
    return rim + sample_content(in.local) * (1.0 - rim.a);
}
