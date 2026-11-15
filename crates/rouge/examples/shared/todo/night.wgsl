fn ns_hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn ns_hash3(p: vec2<f32>) -> vec3<f32> {
    let q = vec3<f32>(
        dot(p, vec2<f32>(127.1, 311.7)),
        dot(p, vec2<f32>(269.5, 183.3)),
        dot(p, vec2<f32>(419.2, 371.9)),
    );
    return fract(sin(q) * 43758.5453);
}

fn ns_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = ns_hash(i);
    let b = ns_hash(i + vec2<f32>(1.0, 0.0));
    let c = ns_hash(i + vec2<f32>(0.0, 1.0));
    let d = ns_hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn ns_fbm(start: vec2<f32>) -> f32 {
    var p = start;
    var value = 0.0;
    var amplitude = 0.5;
    for (var octave = 0; octave < 5; octave++) {
        value += amplitude * ns_noise(p);
        p = p * 2.03 + vec2<f32>(1.3, 7.1);
        amplitude *= 0.5;
    }
    return value;
}

fn ns_stars(px: vec2<f32>, cell: f32, chance: f32, size: f32, t: f32, seed: f32) -> vec3<f32> {
    let id = floor(px / cell + seed);
    let r = ns_hash3(id);
    if (r.x > chance) {
        return vec3<f32>(0.0);
    }
    let center = (id + 0.15 + 0.7 * r.yz - seed) * cell;
    let d = length(px - center);
    let magnitude = pow(ns_hash(id + 7.3), 4.0);
    let radius = size * (0.55 + 1.3 * magnitude);
    let core = exp(-d * d / (radius * radius));
    let halo = exp(-d / (radius * 4.0)) * 0.12 * magnitude * (1.0 - smoothstep(0.04 * cell, 0.14 * cell, d));
    let amp = 0.12 + 0.45 * magnitude;
    let twinkle = 1.0 - amp * (0.5 + 0.5 * sin(t * (0.6 + 2.2 * r.y) + r.z * 40.0));
    let tint = mix(vec3<f32>(0.7, 0.8, 1.0), vec3<f32>(1.0, 0.87, 0.7), ns_hash(id + 3.1));
    return tint * (core + halo) * (0.18 + 1.1 * magnitude) * twinkle;
}

fn ns_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ab = b - a;
    let h = clamp(dot(p - a, ab) / max(dot(ab, ab), 0.0001), 0.0, 1.0);
    return length(p - a - ab * h);
}

fn ns_constellations(local: vec2<f32>, size: vec2<f32>, t: f32) -> vec3<f32> {
    var stars = array<vec2<f32>, 25>(
        vec2<f32>(0.0, 0.35), vec2<f32>(0.18, 0.3), vec2<f32>(0.33, 0.36), vec2<f32>(0.47, 0.45),
        vec2<f32>(0.52, 0.66), vec2<f32>(0.78, 0.72), vec2<f32>(0.8, 0.5),
        vec2<f32>(0.0, 0.2), vec2<f32>(0.25, 0.55), vec2<f32>(0.48, 0.35), vec2<f32>(0.72, 0.62),
        vec2<f32>(1.0, 0.3),
        vec2<f32>(0.25, 0.12), vec2<f32>(0.7, 0.18), vec2<f32>(0.42, 0.5), vec2<f32>(0.5, 0.47),
        vec2<f32>(0.58, 0.44), vec2<f32>(0.32, 0.86), vec2<f32>(0.76, 0.83), vec2<f32>(0.48, 0.0),
        vec2<f32>(0.5, 0.0), vec2<f32>(0.5, 0.33), vec2<f32>(0.48, 0.95), vec2<f32>(0.1, 0.22),
        vec2<f32>(0.9, 0.46),
    );
    var lines = array<vec2<i32>, 23>(
        vec2<i32>(0, 1), vec2<i32>(1, 2), vec2<i32>(2, 3), vec2<i32>(3, 4), vec2<i32>(4, 5),
        vec2<i32>(5, 6), vec2<i32>(6, 3),
        vec2<i32>(7, 8), vec2<i32>(8, 9), vec2<i32>(9, 10), vec2<i32>(10, 11),
        vec2<i32>(19, 12), vec2<i32>(19, 13), vec2<i32>(12, 14), vec2<i32>(13, 16), vec2<i32>(14, 15),
        vec2<i32>(15, 16), vec2<i32>(14, 17), vec2<i32>(16, 18),
        vec2<i32>(20, 21), vec2<i32>(21, 22), vec2<i32>(21, 23), vec2<i32>(21, 24),
    );
    var shapes = array<vec4<i32>, 4>(
        vec4<i32>(0, 7, 0, 7), vec4<i32>(7, 5, 7, 4), vec4<i32>(12, 8, 11, 8), vec4<i32>(20, 5, 19, 4),
    );
    var places = array<vec3<f32>, 4>(
        vec3<f32>(0.12, 0.2, 0.24), vec3<f32>(0.88, 0.18, 0.17), vec3<f32>(0.11, 0.56, 0.28),
        vec3<f32>(0.89, 0.55, 0.24),
    );

    let period = 26.0;
    let cycle = t / period;
    let lit = i32(floor(cycle)) % 4;
    let phase = fract(cycle);
    let shown = smoothstep(0.0, 0.14, phase) * (1.0 - smoothstep(0.8, 1.0, phase));
    let start = 0.14;
    let trace = 2.2 / period;
    let since = (phase - start - trace) * period;
    let pulse = select(0.0, exp(-since * 2.4) * (1.0 - exp(-since * 30.0)), since > 0.0);

    var light = vec3<f32>(0.0);
    for (var k = 0; k < 4; k++) {
        let shape = shapes[k];
        let place = places[k];
        let center = place.xy * size;
        let span = place.z * size.y;
        let boost = select(0.0, shown, k == lit);
        let flare = select(0.0, pulse, k == lit);
        for (var i = 0; i < shape.y; i++) {
            let at = center + (stars[shape.x + i] - vec2<f32>(0.5)) * span;
            let d = length(local - at);
            let core = exp(-d * d / 1.4);
            let halo = exp(-d / 6.0) * (0.18 * boost + 0.35 * flare);
            light += vec3<f32>(0.84, 0.89, 1.0) * (core * (0.35 + 0.65 * boost + 0.4 * flare) + halo);
        }
    }

    let shape = shapes[lit];
    let place = places[lit];
    let center = place.xy * size;
    let span = place.z * size.y;
    let drawn = smoothstep(0.0, 1.0, clamp((phase - start) / trace, 0.0, 1.0)) * f32(shape.w);
    var line = 0.0;
    for (var j = 0; j < shape.w; j++) {
        let progress = clamp(drawn - f32(j), 0.0, 1.0);
        if (progress <= 0.0) {
            continue;
        }
        let segment = lines[shape.z + j];
        let a = center + (stars[segment.x] - vec2<f32>(0.5)) * span;
        let b = center + (stars[segment.y] - vec2<f32>(0.5)) * span;
        let gap = min(7.0 / max(length(b - a), 1.0), 0.45);
        let start = mix(a, b, gap);
        let end = mix(a, b, mix(gap, 1.0 - gap, progress));
        let d = ns_segment(local, start, end);
        line = max(line, exp(-d * d / 0.5));
    }
    return light + vec3<f32>(0.66, 0.75, 1.0) * line * shown * (0.3 + 0.35 * pulse);
}

fn effect(in: EffectInput) -> vec4<f32> {
    let t = param(0).x + param(0).y;
    let uv = in.uv;
    let aspect = in.size.x / max(in.size.y, 1.0);
    let scale = scale_factor();
    let px = in.local * scale;

    let zenith = vec3<f32>(0.010, 0.014, 0.034);
    let horizon = vec3<f32>(0.045, 0.062, 0.125);
    var color = mix(zenith, horizon, pow(uv.y, 1.8));

    let p = (uv - vec2<f32>(0.5, 0.5)) * vec2<f32>(aspect, 1.0);
    let dir = normalize(vec2<f32>(1.0, -0.62));
    let along = dot(p, dir);
    let across = dot(p, vec2<f32>(-dir.y, dir.x)) + 0.05 * sin(along * 2.3) + 0.08;
    let band = exp(-across * across / 0.035);
    let core = exp(-across * across / 0.006);
    let dust = ns_fbm(vec2<f32>(along * 3.0, across * 8.0) + 2.0);
    let lanes = smoothstep(0.42, 0.72, ns_fbm(vec2<f32>(along * 5.0, across * 16.0) + 9.0));
    let glow = band * (0.3 + 0.7 * dust) * (1.0 - 0.7 * lanes * core);
    color += vec3<f32>(0.26, 0.29, 0.42) * glow * 0.24;
    color += vec3<f32>(0.46, 0.36, 0.31) * core * dust * (1.0 - lanes) * 0.1;

    var stars = ns_stars(px, 7.0 * scale, 0.55 * band, 0.45 * scale, t, 0.0) * band * 0.6;
    stars += ns_stars(px, 22.0 * scale, 0.32, 0.55 * scale, t, 3.7);
    stars += ns_stars(px, 64.0 * scale, 0.5, 0.75 * scale, t, 11.3);
    stars += ns_stars(px, 190.0 * scale, 0.6, 1.0 * scale, t, 23.9);
    color += stars * (1.0 - 0.65 * smoothstep(0.6, 0.95, uv.y));
    color += ns_constellations(in.local, in.size, t);

    let period = 13.0;
    let round = floor(t / period);
    let since = t - round * period;
    let roll = ns_hash3(vec2<f32>(round, 17.0));
    if (since < 1.1 && roll.x > 0.3) {
        let progress = since / 1.1;
        let start = vec2<f32>(mix(0.2, 0.95, roll.y), mix(0.04, 0.4, roll.z)) * in.size;
        let dir = normalize(vec2<f32>(-1.0, 0.45 + 0.35 * roll.x));
        let head = start + dir * progress * 300.0;
        let tail = 110.0 * sin(progress * 3.14159);
        let back = clamp(dot(in.local - head, -dir), 0.0, tail);
        let gap = length(in.local - (head - dir * back));
        let fade = (1.0 - back / max(tail, 0.001)) * sin(progress * 3.14159);
        color += vec3<f32>(0.88, 0.92, 1.0) * exp(-gap * gap / 0.5) * fade * 0.7;
    }

    color += vec3<f32>(0.06, 0.07, 0.15) * smoothstep(0.5, 0.95, uv.y) * 0.6;

    let x = uv.x * aspect;
    let peaks = ns_fbm(vec2<f32>(x * 1.1, 4.0));
    let ridge = 0.955 - 0.11 * pow(peaks, 1.6) - 0.012 * ns_fbm(vec2<f32>(x * 9.0, 9.0));
    color += vec3<f32>(0.12, 0.11, 0.2) * exp(-max(ridge - uv.y, 0.0) * 30.0) * 0.35;
    let edge = 1.5 / in.size.y;
    let land = smoothstep(ridge - edge, ridge + edge, uv.y);
    color = mix(color, vec3<f32>(0.008, 0.01, 0.02), land);

    return vec4<f32>(color, 1.0) * in.coverage;
}
