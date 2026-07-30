// 视频壁纸着色器 — GPU 端 YUV→RGB 转换 + cover 模式 + 呼吸效果
// 采样 3 个 R8Unorm 纹理 (Y/U/V)，在 fragment shader 中做 BT.601 转换

struct Uniforms {
    resolution: vec2<f32>,
    time: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var tex_y: texture_2d<f32>;
@group(0) @binding(2) var tex_u: texture_2d<f32>;
@group(0) @binding(3) var tex_v: texture_2d<f32>;
@group(0) @binding(4) var samp: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0),  vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
    );
    return vec4<f32>(p[vi], 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) fc: vec4<f32>) -> @location(0) vec4<f32> {
    let screen_uv = fc.xy / u.resolution;
    let tex_size = textureDimensions(tex_y);
    let tex_aspect = f32(tex_size.x) / f32(tex_size.y);
    let screen_aspect = u.resolution.x / u.resolution.y;

    // cover 模式: 视频填满屏幕，可能裁切
    var uv = screen_uv;
    if (tex_aspect > screen_aspect) {
        let scale = screen_aspect / tex_aspect;
        uv.x = (screen_uv.x - 0.5) * scale + 0.5;
    } else {
        let scale = tex_aspect / screen_aspect;
        uv.y = (screen_uv.y - 0.5) * scale + 0.5;
    }

    // 采样 YUV — R8Unorm 把 [0,255] 映射到 [0,1]
    let y_n = textureSample(tex_y, samp, uv).r;          // Y/255
    let u_n = textureSample(tex_u, samp, uv).r - 0.502;  // (U-128)/255
    let v_n = textureSample(tex_v, samp, uv).r - 0.502;  // (V-128)/255

    // BT.601 full-range YUV→RGB（系数在归一化域不变）
    let r = y_n + 1.402 * v_n;
    let g = y_n - 0.344 * u_n - 0.714 * v_n;
    let b = y_n + 1.772 * u_n;

    var color = vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);

    // 轻微呼吸效果
    let breathe = 1.0 + 0.03 * sin(u.time * 0.5);
    color = vec4<f32>(color.rgb * breathe, color.a);

    return color;
}
