const RIM_WIDTH: f32 = 1.25;
const SHADOW_REACH: f32 = 22.0;
const SHADOW_DROP: f32 = 6.0;

fn effect(in: EffectInput) -> vec4<f32> {
    let tint = param(0);
    let optics = param(1);
    let shadow_amount = param(2).x;
    let texel = 1.0 / scale_factor();

    var shadow = 0.0;
    if (shadow_amount > 0.0) {
        let dropped = shape_sdf(in.local - vec2<f32>(0.0, SHADOW_DROP));
        let falloff = clamp(1.0 - dropped / SHADOW_REACH, 0.0, 1.0);
        shadow = shadow_amount * falloff * falloff * falloff * (1.0 - in.coverage);
    }
    if (in.coverage <= 0.0 && shadow <= 0.0) {
        discard;
    }

    let e = vec2<f32>(texel, 0.0);
    let gradient = vec2<f32>(
        shape_sdf(in.local + e.xy) - shape_sdf(in.local - e.xy),
        shape_sdf(in.local + e.yx) - shape_sdf(in.local - e.yx),
    );
    let normal = gradient / max(length(gradient), 1e-4 * texel);

    let edge = max(optics.w, texel);
    let depth = max(-in.distance, 0.0);
    let rim = clamp(1.0 - depth / edge, 0.0, 1.0);
    let bend = 1.0 - sqrt(1.0 - rim * rim);

    let offset = -normal * bend * optics.x * edge * 2.0;
    let spread = offset * optics.y;
    let r = sample_backdrop(in.local + offset + spread).r;
    let g = sample_backdrop(in.local + offset).g;
    let b = sample_backdrop(in.local + offset - spread).b;
    var color = vec3<f32>(r, g, b);

    color = mix(color, tint.rgb, tint.a);
    let light = normalize(vec2<f32>(-0.6, -0.8));
    let facing = dot(normal, light);
    let line = clamp(1.0 - depth / RIM_WIDTH, 0.0, 1.0);
    let glint = 0.2 + 0.8 * facing * facing;
    color += vec3<f32>((line * glint + bend * 0.12) * optics.z);

    let glass = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)) * in.coverage;
    return vec4<f32>(glass, in.coverage + shadow);
}
