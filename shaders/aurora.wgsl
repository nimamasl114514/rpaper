// Aurora 极光流动着色器 — 最终版
// 多层波浪极光 + 星空 + 地平线辉光

struct Uniforms {
    resolution: vec2<f32>,
    time: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0),  vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
    );
    return vec4<f32>(p[vi], 0.0, 1.0);
}

fn hash(p: vec2<f32>) -> f32 {
    var p2 = fract(p * vec2<f32>(0.1031, 0.1030));
    p2 += dot(p2, p2.yx + 33.33);
    return fract((p2.x + p2.y) * p2.x);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    var f = 1.0;
    for (var i = 0; i < 4; i++) {
        v += a * noise(p * f);
        f *= 2.0;
        a *= 0.5;
    }
    return v;
}

// 极光波浪函数 — 模拟极光幕布的起伏
fn aurora_wave(uv: vec2<f32>, t: f32, freq: f32, speed: f32, offset: f32) -> f32 {
    let n = fbm(vec2<f32>(uv.x * freq + t * speed, offset));
    return n;
}

@fragment
fn fs_main(@builtin(position) fc: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = fc.xy / u.resolution;
    var p = uv;
    p.x *= u.resolution.x / u.resolution.y;
    let t = u.time * 0.05;

    // === 星空背景 ===
    let star_uv = p * 3.0;
    let star_n = hash(floor(star_uv));
    let star = step(0.985, star_n) * (0.5 + 0.5 * sin(u.time * 2.0 + star_n * 100.0));
    let star2 = step(0.995, hash(floor(star_uv * 2.0))) * 0.6;
    let stars = (star + star2) * smoothstep(0.3, 1.0, uv.y);

    // 深空背景渐变
    var bg = mix(
        vec3<f32>(0.02, 0.01, 0.06),
        vec3<f32>(0.01, 0.02, 0.04),
        uv.y
    );

    // === 三层极光 ===
    // 第一层: 绿色主极光
    let wave1 = aurora_wave(p, t, 2.0, 1.0, 0.0);
    let aurora1_pos = 0.3 + wave1 * 0.2;
    let aurora1_band = exp(-pow((p.y - aurora1_pos) * 8.0, 2.0));
    let aurora1_color = vec3<f32>(0.1, 0.9, 0.4) * aurora1_band;
    // 极光内部的竖直纹理
    let aurora1_streaks = fbm(vec2<f32>(p.x * 15.0 + t * 3.0, p.y * 5.0));
    var aurora1 = aurora1_color * (0.5 + aurora1_streaks * 0.5);

    // 第二层: 青色极光
    let wave2 = aurora_wave(p, t * 1.3, 3.0, 0.7, 5.0);
    let aurora2_pos = 0.45 + wave2 * 0.15;
    let aurora2_band = exp(-pow((p.y - aurora2_pos) * 10.0, 2.0));
    let aurora2_streaks = fbm(vec2<f32>(p.x * 20.0 + t * 4.0, p.y * 6.0));
    var aurora2 = vec3<f32>(0.2, 0.7, 1.0) * aurora2_band * (0.5 + aurora2_streaks * 0.5);

    // 第三层: 紫粉色极光（最远）
    let wave3 = aurora_wave(p, t * 0.8, 1.5, 0.5, 10.0);
    let aurora3_pos = 0.6 + wave3 * 0.1;
    let aurora3_band = exp(-pow((p.y - aurora3_pos) * 12.0, 2.0));
    var aurora3 = vec3<f32>(0.8, 0.2, 0.9) * aurora3_band * 0.5;

    // === 地平线辉光 ===
    let horizon = exp(-pow((p.y - 0.05) * 4.0, 2.0)) * 0.3;
    let horizon_color = vec3<f32>(0.05, 0.15, 0.1) * horizon;

    // === 合成 ===
    var color = bg;
    color += horizon_color;
    color += aurora1;
    color += aurora2 * 0.7;
    color += aurora3 * 0.5;
    color += vec3<f32>(stars);

    // 整体微微提升亮度
    color *= 1.1;

    // 底部渐暗
    color *= 0.3 + 0.7 * smoothstep(0.0, 0.3, uv.y);

    return vec4<f32>(color, 1.0);
}
