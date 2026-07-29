//! 色彩转换性能基准 — 对比标量与 SSE4.1 路径的吞吐量
//!
//! 用法:
//!   cargo run --release --example color_bench
//!
//! 预期:
//!   - 标量版本 ~3-5 ms/帧 (1920×1080)
//!   - SSE4.1 版本 ~1-2 ms/帧
//!   - 加速比 ~2-4×
//!   - 两次输出逐字节一致

use std::time::Instant;

use rpaper::video::color::{yuv420_to_rgba, yuv420_to_rgba_scalar};

fn main() {
    // 1920×1080 是壁纸引擎最常见分辨率，coded_w 等于 visible_w（已是 16 的倍数）
    let visible_w = 1920usize;
    let visible_h = 1080usize;
    let coded_w = (visible_w + 15) & !15; // 1920 已对齐
    let frame_size = visible_w * visible_h * 4;

    // 构造一帧非平凡 YUV 数据：渐变色 + 行间变化
    let mut y = vec![0u8; coded_w * visible_h];
    let mut u = vec![0u8; (coded_w / 2) * (visible_h / 2)];
    let mut v = vec![0u8; (coded_w / 2) * (visible_h / 2)];
    for row in 0..visible_h {
        for col in 0..coded_w {
            y[row * coded_w + col] = ((row as u32 * 7 + col as u32 * 3) & 0xFF) as u8;
        }
    }
    for row in 0..visible_h / 2 {
        for col in 0..coded_w / 2 {
            u[row * coded_w / 2 + col] = ((row as u32 * 11 + col as u32 * 5 + 30) & 0xFF) as u8;
            v[row * coded_w / 2 + col] = ((row as u32 * 13 + col as u32 * 9 + 60) & 0xFF) as u8;
        }
    }

    let mut rgba_scalar = vec![0u8; frame_size];
    let mut rgba_simd = vec![0u8; frame_size];

    // ── 先验证两次输出完全一致（防止 bench 跑出错误结果）──
    yuv420_to_rgba_scalar(&y, &u, &v, &mut rgba_scalar, visible_w, visible_h, coded_w);
    yuv420_to_rgba(&y, &u, &v, &mut rgba_simd, visible_w, visible_h, coded_w);
    for i in 0..frame_size {
        assert_eq!(rgba_scalar[i], rgba_simd[i],
            "字节 {i} 不一致: 标量={} SIMD={}", rgba_scalar[i], rgba_simd[i]);
    }
    println!("✓ 标量与 SIMD 输出逐字节一致 ({frame_size} 字节)");

    // ── 性能测试 ──
    let iters = 100;

    // 标量
    let t0 = Instant::now();
    for _ in 0..iters {
        yuv420_to_rgba_scalar(&y, &u, &v, &mut rgba_scalar, visible_w, visible_h, coded_w);
    }
    let scalar_ms = t0.elapsed().as_secs_f64() * 1000.0 / iters as f64;

    // SIMD（自动选择 SSE4.1）
    let t1 = Instant::now();
    for _ in 0..iters {
        yuv420_to_rgba(&y, &u, &v, &mut rgba_simd, visible_w, visible_h, coded_w);
    }
    let simd_ms = t1.elapsed().as_secs_f64() * 1000.0 / iters as f64;

    let speedup = scalar_ms / simd_ms;
    let fps_scalar = 1000.0 / scalar_ms;
    let fps_simd = 1000.0 / simd_ms;

    println!();
    println!("── color_bench 结果 ({visible_w}×{visible_h}, {iters} 次平均) ──");
    println!("标量 : {scalar_ms:>7.3} ms/帧  ({fps_scalar:>6.1} FPS)");
    println!("SIMD : {simd_ms:>7.3} ms/帧  ({fps_simd:>6.1} FPS)");
    println!("加速比: {speedup:>5.2}×");
}
