const RIM_WIDTH: f32 = 1.5;
const SHADOW_REACH: f32 = 22.0;
const SHADOW_DROP: f32 = 6.0;
const CONTACT_REACH: f32 = 5.0;
const CONTACT_DROP: f32 = 1.5;
const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

fn effect(in: EffectInput) -> vec4<f32> {
    let tint = param(0);
    let optics = param(1);
    let finish = param(2);
    let pointer = param(3);
    let shadow_amount = finish.x;
    let texel = 1.0 / scale_factor();

    var shadow = 0.0;
    if (shadow_amount > 0.0) {
        let dropped = shape_sdf(in.local - vec2<f32>(0.0, SHADOW_DROP));
        let soft = clamp(1.0 - dropped / SHADOW_REACH, 0.0, 1.0);
        let grounded = shape_sdf(in.local - vec2<f32>(0.0, CONTACT_DROP));
        let contact = clamp(1.0 - grounded / CONTACT_REACH, 0.0, 1.0);
        shadow = shadow_amount * (0.75 * soft * soft * soft + 0.25 * contact * contact) * (1.0 - in.coverage);
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

    color = mix(vec3<f32>(dot(color, LUMA)), color, finish.y);
    color = mix(color, tint.rgb, tint.a);

    let light = normalize(vec2<f32>(-0.6, -0.8));
    let facing = dot(normal, light);
    let key = max(facing, 0.0);
    let fill = max(-facing, 0.0);
    let glint = 0.15 + 0.85 * key * key + 0.6 * fill * fill;
    let line = clamp(1.0 - depth / RIM_WIDTH, 0.0, 1.0);
    let bloom = rim * rim * rim;
    let sheen = line * glint + bloom * glint * 0.22 + bend * 0.12;

    let away = (in.local - finish.zw) / max(pointer.y, 1.0);
    let spot = exp(-dot(away, away)) * pointer.x;
    let caught = spot * (line * 0.8 + bloom * 0.3 + 0.05);

    color += vec3<f32>(sheen * optics.z + caught);

    let glass = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)) * in.coverage;
    return vec4<f32>(glass, in.coverage + shadow);
}
