fn dodge(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    let lit = min(vec3<f32>(1.0), b / max(vec3<f32>(1.0) - s, vec3<f32>(1e-5)));
    return select(lit, vec3<f32>(0.0), b <= vec3<f32>(0.0));
}

fn burn(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    let dark = vec3<f32>(1.0) - min(vec3<f32>(1.0), (vec3<f32>(1.0) - b) / max(s, vec3<f32>(1e-5)));
    return select(dark, vec3<f32>(1.0), b >= vec3<f32>(1.0));
}

fn hard_light(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    let multiply = 2.0 * b * s;
    let screen = vec3<f32>(1.0) - 2.0 * (vec3<f32>(1.0) - b) * (vec3<f32>(1.0) - s);
    return select(screen, multiply, s <= vec3<f32>(0.5));
}

fn soft_light(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    let curve = select(sqrt(b), ((16.0 * b - 12.0) * b + 4.0) * b, b <= vec3<f32>(0.25));
    let dark = b - (vec3<f32>(1.0) - 2.0 * s) * b * (vec3<f32>(1.0) - b);
    let light = b + (2.0 * s - vec3<f32>(1.0)) * (curve - b);
    return select(light, dark, s <= vec3<f32>(0.5));
}

fn mix_colors(mode: i32, b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    switch mode {
        case 0 { return b * s; }
        case 1 { return b + s - b * s; }
        case 2 { return hard_light(s, b); }
        case 3 { return min(b, s); }
        case 4 { return max(b, s); }
        case 5 { return dodge(b, s); }
        case 6 { return burn(b, s); }
        case 7 { return hard_light(b, s); }
        case 8 { return soft_light(b, s); }
        case 9 { return abs(b - s); }
        case 10 { return b + s - 2.0 * b * s; }
        default { return s; }
    }
}

fn effect(in: EffectInput) -> vec4<f32> {
    let source = sample_content(in.local);
    if (source.a <= 0.0) {
        return vec4<f32>(0.0);
    }
    let behind = sample_backdrop(in.local);
    let mode = i32(param(0).x);
    if (mode == 11) {
        return vec4<f32>(source.rgb + behind.rgb * source.a, source.a);
    }
    let b = behind.rgb / max(behind.a, 1e-5);
    let s = source.rgb / source.a;
    let mixed = clamp(mix_colors(mode, b, s), vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(source.rgb * (1.0 - behind.a) + mixed * source.a * behind.a, source.a);
}
