// 粒子着色器 — 浮动光点 + 发光效果

struct Global {
    resolution: vec2<f32>,
    time: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> g: Global;

struct Particle {
    @location(0) pos: vec2<f32>,
    @location(1) vel: vec2<f32>,
    @location(2) size: f32,
    @location(3) hue: f32,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) hue: f32,
    @location(2) size: f32,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, p: Particle) -> VertexOut {
    // 画一个小 quad（两个三角形）
    var offsets = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0),  vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
    );

    let off = offsets[vi] * p.size;
    let screen_pos = p.pos + off;

    // 转成 NDC
    let ndc = vec2<f32>(
        screen_pos.x / g.resolution.x * 2.0 - 1.0,
        1.0 - screen_pos.y / g.resolution.y * 2.0,
    );

    var out: VertexOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = offsets[vi];
    out.hue = p.hue;
    out.size = p.size;
    return out;
}

fn hsv2rgb(h: f32, s: f32, v: f32) -> vec3<f32> {
    let c = v * s;
    let x = c * (1.0 - abs(fract(h * 6.0) * 2.0 - 1.0));
    let m = v - c;

    var rgb: vec3<f32>;
    if (h < 1.0 / 6.0) {
        rgb = vec3<f32>(c, x, 0.0);
    } else if (h < 2.0 / 6.0) {
        rgb = vec3<f32>(x, c, 0.0);
    } else if (h < 3.0 / 6.0) {
        rgb = vec3<f32>(0.0, c, x);
    } else if (h < 4.0 / 6.0) {
        rgb = vec3<f32>(0.0, x, c);
    } else if (h < 5.0 / 6.0) {
        rgb = vec3<f32>(x, 0.0, c);
    } else {
        rgb = vec3<f32>(c, 0.0, x);
    }
    return rgb + vec3<f32>(m);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let d = length(in.uv);
    if (d > 1.0) {
        discard;
    }

    // 发光效果：中心亮，边缘渐隐
    let glow = 1.0 - d;
    let glow2 = glow * glow;

    // 基于时间和 hue 的颜色变化
    let h = fract(in.hue + g.time * 0.02);
    let base_color = hsv2rgb(h, 0.7, 1.0);

    let alpha = glow2;
    let color = base_color * glow2 * 2.0;

    return vec4<f32>(color, alpha);
}
