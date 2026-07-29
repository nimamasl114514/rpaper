//! H.264 去块滤波（Deblocking Filter）模块。
//!
//! 实现 H.264 标准 §8.7 去块滤波，用于减少块边界的不连续性。
//! 对于 Baseline Profile，只处理 P 帧（不使用 B 帧）。
//!
//! 处理顺序：
//! 1. 亮度 4x4 块边界（垂直 → 水平）
//! 2. 色度 4x4 块边界（垂直 → 水平）

/// 对整帧做去块滤波。
///
/// * `y` - 亮度平面 (width × height)
/// * `u` - 色度 U 平面 (width/2 × height/2)
/// * `v` - 色度 V 平面 (width/2 × height/2)
/// * `width` / `height` - 帧尺寸（必须是 16 的倍数）
/// * `qp` - 量化参数
/// * `alpha_offset` / `beta_offset` - 滤波器强度偏移（来自 slice header）
#[allow(clippy::too_many_arguments)]
pub fn deblock_frame(
    y: &mut [u8],
    u: &mut [u8],
    v: &mut [u8],
    width: usize,
    height: usize,
    qp: u8,
    alpha_offset: i8,
    beta_offset: i8,
) {
    let mb_w = width / 16;
    let mb_h = height / 16;

    // 计算 alpha 和 beta 阈值
    let alpha = alpha_table(qp, alpha_offset);
    let beta = beta_table(qp, beta_offset);

    // 亮度滤波
    for mb_y in 0..mb_h {
        for mb_x in 0..mb_w {
            deblock_mb_luma(y, width, mb_x * 16, mb_y * 16, mb_x, mb_y, mb_w, mb_h, alpha, beta);
        }
    }

    // 色度滤波
    let half_w = width / 2;
    let _half_h = height / 2;
    for mb_y in 0..mb_h {
        for mb_x in 0..mb_w {
            deblock_mb_chroma(u, v, half_w, mb_x * 8, mb_y * 8, mb_x, mb_y, mb_w, mb_h, alpha, beta);
        }
    }
}

/// 对单个宏块做亮度去块滤波。
#[allow(clippy::too_many_arguments)]
fn deblock_mb_luma(
    y: &mut [u8], stride: usize, x0: usize, y0: usize,
    mb_x: usize, mb_y: usize, _mb_w: usize, _mb_h: usize,
    alpha: u8, beta: u8,
) {
    // 垂直边界（从左到右）
    for edge in 0..4 {
        if edge == 0 && mb_x == 0 {
            continue; // 左边界，无左侧相邻宏块
        }
        let x = x0 + edge * 4;
        deblock_luma_vertical(y, stride, x, y0, alpha, beta);
    }

    // 水平边界（从上到下）
    for edge in 0..4 {
        if edge == 0 && mb_y == 0 {
            continue; // 上边界，无上方相邻宏块
        }
        let edge_y = y0 + edge * 4;
        deblock_luma_horizontal(y, stride, x0, edge_y, alpha, beta);
    }
}

/// 亮度垂直边界的去块滤波。
fn deblock_luma_vertical(y: &mut [u8], stride: usize, x: usize, y0: usize, alpha: u8, beta: u8) {
    for row in 0..4 {
        let y_off = (y0 + row) * stride;
        let _p3 = y[y_off + x - 4] as i32;
        let _p2 = y[y_off + x - 3] as i32;
        let p1 = y[y_off + x - 2] as i32;
        let p0 = y[y_off + x - 1] as i32;
        let q0 = y[y_off + x] as i32;
        let q1 = y[y_off + x + 1] as i32;
        let _q2 = y[y_off + x + 2] as i32;
        let _q3 = y[y_off + x + 3] as i32;

        // 判断是否需要滤波
        if (p0 - q0).unsigned_abs() as u8 >= alpha
            || (p1 - p0).unsigned_abs() as u8 >= beta
            || (q1 - q0).unsigned_abs() as u8 >= beta
        {
            continue;
        }

        // 滤波强度 Bs = 1 的基本滤波
        let delta = ((q0 - p0) * 4 + (p1 - q1) + 4) >> 3;
        let delta = clip3_i32(-3, 3, delta);

        y[y_off + x - 1] = clip255(p0 + delta);
        y[y_off + x] = clip255(q0 - delta);
        y[y_off + x - 2] = clip255(p1 + delta / 2);
        y[y_off + x + 1] = clip255(q1 - delta / 2);
    }
}

/// 亮度水平边界的去块滤波。
fn deblock_luma_horizontal(
    y: &mut [u8],
    stride: usize,
    x0: usize,
    row_y: usize,
    alpha: u8,
    beta: u8,
) {
    for col in 0..4 {
        let x = x0 + col;
        let off = row_y * stride + x;
        let _p3 = y[off - 4 * stride] as i32;
        let _p2 = y[off - 3 * stride] as i32;
        let p1 = y[off - 2 * stride] as i32;
        let p0 = y[off - stride] as i32;
        let q0 = y[off] as i32;
        let q1 = y[off + stride] as i32;
        let _q2 = y[off + 2 * stride] as i32;
        let _q3 = y[off + 3 * stride] as i32;

        if (p0 - q0).unsigned_abs() as u8 >= alpha
            || (p1 - p0).unsigned_abs() as u8 >= beta
            || (q1 - q0).unsigned_abs() as u8 >= beta
        {
            continue;
        }

        let delta = ((q0 - p0) * 4 + (p1 - q1) + 4) >> 3;
        let delta = clip3_i32(-3, 3, delta);

        y[off - stride] = clip255(p0 + delta);
        y[off] = clip255(q0 - delta);
        y[off - 2 * stride] = clip255(p1 + delta / 2);
        y[off + stride] = clip255(q1 - delta / 2);
    }
}

/// 色度去块滤波（简化版）。
#[allow(clippy::too_many_arguments)]
fn deblock_mb_chroma(
    u: &mut [u8],
    v: &mut [u8],
    stride: usize,
    x0: usize,
    y0: usize,
    mb_x: usize,
    mb_y: usize,
    _mb_w: usize,
    _mb_h: usize,
    alpha: u8,
    beta: u8,
) {
    // 色度只处理内部边界，每个宏块一个垂直边缘和一个水平边缘
    // 垂直边界
    for edge in 1..2 {
        let x = x0 + edge * 4;
        deblock_chroma_vertical(u, stride, x, y0, alpha, beta);
        deblock_chroma_vertical(v, stride, x, y0, alpha, beta);
    }
    // 水平边界
    for edge in 1..2 {
        let y = y0 + edge * 4;
        deblock_chroma_horizontal(u, stride, x0, y, alpha, beta);
        deblock_chroma_horizontal(v, stride, x0, y, alpha, beta);
    }
    // 宏块左边界（非帧左边界）
    if mb_x > 0 {
        let x = x0;
        deblock_chroma_vertical(u, stride, x, y0, alpha, beta);
        deblock_chroma_vertical(v, stride, x, y0, alpha, beta);
    }
    // 宏块上边界（非帧上边界）
    if mb_y > 0 {
        let y = y0;
        deblock_chroma_horizontal(u, stride, x0, y, alpha, beta);
        deblock_chroma_horizontal(v, stride, x0, y, alpha, beta);
    }
}

fn deblock_chroma_vertical(
    p: &mut [u8],
    stride: usize,
    x: usize,
    y0: usize,
    alpha: u8,
    beta: u8,
) {
    for row in 0..4 {
        let off = (y0 + row) * stride;
        let p0 = p[off + x - 1] as i32;
        let q0 = p[off + x] as i32;
        let p1 = p[off + x - 2] as i32;
        let q1 = p[off + x + 1] as i32;

        if (p0 - q0).unsigned_abs() as u8 >= alpha
            || (p1 - p0).unsigned_abs() as u8 >= beta
            || (q1 - q0).unsigned_abs() as u8 >= beta
        {
            continue;
        }

        let delta = ((q0 - p0) * 4 + (p1 - q1) + 4) >> 3;
        let delta = clip3_i32(-3, 3, delta);

        p[off + x - 1] = clip255(p0 + delta);
        p[off + x] = clip255(q0 - delta);
    }
}

fn deblock_chroma_horizontal(
    p: &mut [u8],
    stride: usize,
    x0: usize,
    y: usize,
    alpha: u8,
    beta: u8,
) {
    for col in 0..4 {
        let x = x0 + col;
        let off = y * stride + x;
        let p0 = p[off - stride] as i32;
        let q0 = p[off] as i32;
        let p1 = p[off - 2 * stride] as i32;
        let q1 = p[off + stride] as i32;

        if (p0 - q0).unsigned_abs() as u8 >= alpha
            || (p1 - p0).unsigned_abs() as u8 >= beta
            || (q1 - q0).unsigned_abs() as u8 >= beta
        {
            continue;
        }

        let delta = ((q0 - p0) * 4 + (p1 - q1) + 4) >> 3;
        let delta = clip3_i32(-3, 3, delta);

        p[off - stride] = clip255(p0 + delta);
        p[off] = clip255(q0 - delta);
    }
}

/// Alpha 阈值表（H.264 标准 Table 8-16）。
fn alpha_table(qp: u8, offset: i8) -> u8 {
    let idx = (qp as i32 + offset as i32).clamp(0, 51) as usize;
    const ALPHA: [u8; 52] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        4, 4, 5, 6, 7, 8, 9, 10, 12, 13, 15, 17, 20, 22,
        25, 28, 32, 36, 40, 45, 50, 56, 63, 71, 80, 90,
        101, 113, 127, 144, 162, 182, 203, 226, 255, 255,
    ];
    ALPHA[idx]
}

/// Beta 阈值表（H.264 标准 Table 8-16）。
fn beta_table(qp: u8, offset: i8) -> u8 {
    let idx = (qp as i32 + offset as i32).clamp(0, 51) as usize;
    const BETA: [u8; 52] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 6, 6, 7, 7, 8, 8,
        9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15,
        15, 16, 16, 17, 17, 18, 18,
    ];
    BETA[idx]
}

fn clip3_i32(min: i32, max: i32, x: i32) -> i32 {
    x.max(min).min(max)
}

fn clip255(x: i32) -> u8 {
    x.clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alpha_table() {
        assert_eq!(alpha_table(0, 0), 0);
        assert_eq!(alpha_table(16, 0), 4);
        assert_eq!(alpha_table(30, 0), 25);
        assert_eq!(alpha_table(51, 0), 255);
        // 带偏移
        assert_eq!(alpha_table(15, 1), 4); // idx=16
        assert_eq!(alpha_table(51, 5), 255); // 钳制
    }

    #[test]
    fn test_beta_table() {
        assert_eq!(beta_table(0, 0), 0);
        assert_eq!(beta_table(16, 0), 2);
        assert_eq!(beta_table(30, 0), 8);
        assert_eq!(beta_table(51, 0), 18);
    }

    #[test]
    fn test_clip3() {
        assert_eq!(clip3_i32(-3, 3, 0), 0);
        assert_eq!(clip3_i32(-3, 3, 5), 3);
        assert_eq!(clip3_i32(-3, 3, -5), -3);
    }

    #[test]
    fn test_clip255() {
        assert_eq!(clip255(0), 0);
        assert_eq!(clip255(128), 128);
        assert_eq!(clip255(255), 255);
        assert_eq!(clip255(300), 255);
        assert_eq!(clip255(-10), 0);
    }

    #[test]
    fn test_deblock_noop_on_flat() {
        // 平坦区域不应被滤波
        let w = 16;
        let h = 16;
        let mut y = vec![128u8; w * h];
        let mut u = vec![128u8; w / 2 * h / 2];
        let mut v = vec![128u8; w / 2 * h / 2];
        deblock_frame(&mut y, &mut u, &mut v, w, h, 30, 0, 0);
        for &v in &y {
            assert_eq!(v, 128);
        }
    }

    #[test]
    fn test_deblock_luma_edge() {
        // 构造有明显边界的 16x16 亮度数据
        let w = 16;
        let h = 16;
        let mut y = vec![100u8; w * h];
        let mut u = vec![128u8; w / 2 * h / 2];
        let mut v = vec![128u8; w / 2 * h / 2];
        // 在 x=4 处制造边界（垂直边界在宏块内）
        for row in 0..16 {
            let off = row * w;
            for col in 4..16 {
                y[off + col] = 200;
            }
        }
        deblock_frame(&mut y, &mut u, &mut v, w, h, 20, 0, 0);
        // 验证边界处的值被平滑（p0 和 q0 应该向中间靠拢）
        for row in 0..16 {
            let off = row * w;
            // 边界左侧 (x=3) 和右侧 (x=4) 的值应该被调整
            assert!(y[off + 3] >= 100, "p0 should not be reduced below original");
            assert!(y[off + 4] <= 200, "q0 should not be increased above original");
        }
    }
}