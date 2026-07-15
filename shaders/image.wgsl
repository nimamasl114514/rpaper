// 图片壁纸着色器 — cover 模式缩放 + 轻微呼吸效果

struct Uniforms {
    resolution: vec2<f32>,
    time: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

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
    let tex_size = textureDimensions(tex);
    let tex_aspect = f32(tex_size.x) / f32(tex_size.y);
    let screen_aspect = u.resolution.x / u.resolution.y;

    // cover 模式: 图片填满屏幕，可能裁切
    var uv = screen_uv;
    if (tex_aspect > screen_aspect) {
        // 图片更宽，裁切左右
        let scale = screen_aspect / tex_aspect;
        uv.x = (screen_uv.x - 0.5) * scale + 0.5;
    } else {
        // 图片更高，裁切上下
        let scale = tex_aspect / screen_aspect;
        uv.y = (screen_uv.y - 0.5) * scale + 0.5;
    }

    var color = textureSample(tex, samp, uv);

    // 轻微呼吸效果
    let breathe = 1.0 + 0.03 * sin(u.time * 0.5);
    color = vec4<f32>(color.rgb * breathe, color.a);

    return color;
}
