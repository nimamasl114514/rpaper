//! H.264 Baseline Profile 帧间预测模块。
//!
//! 实现亮度分量的运动补偿：
//! - 整数像素直接拷贝
//! - 1/4 像素精度的 6 抽头 FIR 滤波器（H.264 标准 §8.4.2.2）
//! - 双线性分数像素插值

/// 帧间预测：获取运动补偿块。
///
/// 根据运动矢量从参考帧中提取预测块，支持 1/4 像素精度的分数插值。
///
/// * `dst` - 输出缓冲区（行优先），长度至少 `block_w * block_h`
/// * `ref_frame` - 参考帧 Y 平面（完整帧的 u8 数据）
/// * `frame_width` / `frame_height` - 参考帧尺寸
/// * `mv_x` / `mv_y` - 运动矢量（1/4 像素精度）
/// * `block_x` / `block_y` - 目标块在帧中的位置（整数像素）
/// * `block_w` / `block_h` - 块大小（4 或 8）
#[allow(clippy::too_many_arguments)]
pub fn inter_predict(
    dst: &mut [u8],
    ref_frame: &[u8],
    frame_width: usize,
    frame_height: usize,
    mv_x: i32,
    mv_y: i32,
    block_x: usize,
    block_y: usize,
    block_w: usize,
    block_h: usize,
) {
    // 参考帧中的位置（1/4 像素精度）
    let ref_x = block_x as i32 * 4 + mv_x;
    let ref_y = block_y as i32 * 4 + mv_y;

    // 整数像素位置（isize 以支持负坐标的边界处理）
    let int_x = (ref_x >> 2) as isize;
    let int_y = (ref_y >> 2) as isize;
    let frac_x = (ref_x & 3) as u8;
    let frac_y = (ref_y & 3) as u8;
    let fw = frame_width as isize;
    let fh = frame_height as isize;

    if frac_x == 0 && frac_y == 0 {
        // 整数像素：直接拷贝，边界外的像素以 0 填充
        for row in 0..block_h {
            let dst_off = row * block_w;
            let py = int_y + row as isize;
            for col in 0..block_w {
                let px = int_x + col as isize;
                dst[dst_off + col] = if px >= 0 && px < fw && py >= 0 && py < fh {
                    ref_frame[(py * fw + px) as usize]
                } else {
                    0
                };
            }
        }
    } else if frac_y == 0 {
        // 仅水平分数像素
        for row in 0..block_h {
            let py = int_y + row as isize;
            let dst_off = row * block_w;
            for col in 0..block_w {
                let px = int_x + col as isize;
                dst[dst_off + col] = hpel_h(ref_frame, py, px, frac_x, frame_width, frame_height);
            }
        }
    } else if frac_x == 0 {
        // 仅垂直分数像素
        for row in 0..block_h {
            let py = int_y + row as isize;
            let dst_off = row * block_w;
            for col in 0..block_w {
                let px = int_x + col as isize;
                dst[dst_off + col] =
                    hpel_v(ref_frame, px, py, frac_y, frame_width, frame_height);
            }
        }
    } else {
        // 双线性插值（分数像素）
        for row in 0..block_h {
            let py = int_y + row as isize;
            let dst_off = row * block_w;
            for col in 0..block_w {
                let px = int_x + col as isize;
                dst[dst_off + col] =
                    bilinear_hpel(ref_frame, px, py, frac_x, frac_y, frame_width, frame_height);
            }
        }
    }
}

/// 水平半像素插值（6 抽头 FIR 滤波器，H.264 标准 §8.4.2.2.1）。
///
/// `y` 为行号（isize），`x` 为列号（isize），允许负值表示边界外。
fn hpel_h(
    ref_frame: &[u8],
    y: isize,
    x: isize,
    frac: u8,
    stride: usize,
    height: usize,
) -> u8 {
    if y < 0 || y >= height as isize {
        return 0;
    }
    let row_start = y as usize * stride;
    let p = get_h_samples(ref_frame, row_start, x, stride);

    match frac {
        0 => p[2],
        1 => {
            let val = (p[0] as i32 + p[5] as i32)
                + 5 * (p[1] as i32 + p[4] as i32)
                + 20 * (p[2] as i32 + p[3] as i32)
                + 16;
            (val >> 5).clamp(0, 255) as u8
        }
        2 => {
            let val = (p[0] as i32 + p[5] as i32)
                - 5 * (p[1] as i32 + p[4] as i32)
                + 20 * (p[2] as i32 + p[3] as i32)
                + 16;
            (val >> 5).clamp(0, 255) as u8
        }
        3 => {
            let val = (p[0] as i32 + p[5] as i32)
                + 5 * (p[1] as i32 + p[4] as i32)
                + 20 * (p[2] as i32 + p[3] as i32)
                + 16;
            (val >> 5).clamp(0, 255) as u8
        }
        _ => p[2],
    }
}

/// 垂直半像素插值（6 抽头 FIR 滤波器）。
///
/// `x` 为列号（isize），`y` 为行号（isize），允许负值表示边界外。
fn hpel_v(
    ref_frame: &[u8],
    x: isize,
    y: isize,
    frac: u8,
    stride: usize,
    height: usize,
) -> u8 {
    let p = get_v_samples(ref_frame, x, y, stride, height);

    match frac {
        0 => p[2],
        1 => {
            let val = (p[0] as i32 + p[5] as i32)
                + 5 * (p[1] as i32 + p[4] as i32)
                + 20 * (p[2] as i32 + p[3] as i32)
                + 16;
            (val >> 5).clamp(0, 255) as u8
        }
        2 => {
            let val = (p[0] as i32 + p[5] as i32)
                - 5 * (p[1] as i32 + p[4] as i32)
                + 20 * (p[2] as i32 + p[3] as i32)
                + 16;
            (val >> 5).clamp(0, 255) as u8
        }
        3 => {
            let val = (p[0] as i32 + p[5] as i32)
                + 5 * (p[1] as i32 + p[4] as i32)
                + 20 * (p[2] as i32 + p[3] as i32)
                + 16;
            (val >> 5).clamp(0, 255) as u8
        }
        _ => p[2],
    }
}

/// 双线性分数像素插值：先水平 6 抽头再垂直 6 抽头。
///
/// 水平插值产生 6 个中间值（不取整），垂直插值汇总后一次取整。
///
/// `x` 为列号（isize），`y` 为行号（isize），允许负值。
fn bilinear_hpel(
    ref_frame: &[u8],
    x: isize,
    y: isize,
    frac_x: u8,
    frac_y: u8,
    stride: usize,
    height: usize,
) -> u8 {
    // 水平插值：对 6 行分别计算
    let mut h_vals = [0i32; 6];
    for (i, h_val) in h_vals.iter_mut().enumerate() {
        let sy = (y + (i as isize - 2))
            .max(0)
            .min(height as isize - 1) as usize;
        let row_start = sy * stride;
        let p = get_h_samples(ref_frame, row_start, x, stride);
        *h_val = match frac_x {
            0 => p[2] as i32,
            1 => {
                (p[0] as i32 + p[5] as i32)
                    + 5 * (p[1] as i32 + p[4] as i32)
                    + 20 * (p[2] as i32 + p[3] as i32)
            }
            2 => {
                (p[0] as i32 + p[5] as i32)
                    - 5 * (p[1] as i32 + p[4] as i32)
                    + 20 * (p[2] as i32 + p[3] as i32)
            }
            3 => {
                (p[0] as i32 + p[5] as i32)
                    + 5 * (p[1] as i32 + p[4] as i32)
                    + 20 * (p[2] as i32 + p[3] as i32)
            }
            _ => p[2] as i32,
        };
    }

    // 垂直插值：对 6 个中间值做 6 抽头，最后统一取整 (>> 10)
    let val = match frac_y {
        0 => h_vals[2],
        1 => {
            h_vals[0] + h_vals[5]
                + 5 * (h_vals[1] + h_vals[4])
                + 20 * (h_vals[2] + h_vals[3])
                + 512
        }
        2 => {
            h_vals[0] + h_vals[5]
                - 5 * (h_vals[1] + h_vals[4])
                + 20 * (h_vals[2] + h_vals[3])
                + 512
        }
        3 => {
            h_vals[0] + h_vals[5]
                + 5 * (h_vals[1] + h_vals[4])
                + 20 * (h_vals[2] + h_vals[3])
                + 512
        }
        _ => h_vals[2],
    };

    (val >> 10).clamp(0, 255) as u8
}

/// 获取 6 个水平像素样本（用于亮度 6 抽头 FIR 滤波器）。
///
/// 返回 `[x, x+1, x+2, x+3, x+4, x+5]`，超出边界的像素以 0 填充。
fn get_h_samples(ref_frame: &[u8], row_start: usize, x: isize, stride: usize) -> [u8; 6] {
    let get = |i: isize| -> u8 {
        let xi = x + i;
        if xi < 0 || xi >= stride as isize {
            0
        } else {
            ref_frame[row_start + xi as usize]
        }
    };
    [get(0), get(1), get(2), get(3), get(4), get(5)]
}

/// 获取 6 个垂直像素样本（用于亮度 6 抽头 FIR 滤波器）。
///
/// 返回 `[y, y+1, y+2, y+3, y+4, y+5]` 列 `x` 的值，超出边界的像素以 0 填充。
fn get_v_samples(
    ref_frame: &[u8],
    x: isize,
    y: isize,
    stride: usize,
    height: usize,
) -> [u8; 6] {
    let h = height as isize;
    let w = stride as isize;
    let get = |i: isize| -> u8 {
        let yi = y + i;
        if yi < 0 || yi >= h || x < 0 || x >= w {
            0
        } else {
            ref_frame[yi as usize * stride + x as usize]
        }
    };
    [get(0), get(1), get(2), get(3), get(4), get(5)]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建一个 16x16 的参考帧，填充已知模式
    fn make_ref_frame(w: usize, h: usize) -> Vec<u8> {
        let mut v = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                v[y * w + x] = (x + y * w) as u8;
            }
        }
        v
    }

    #[test]
    fn test_integer_pixel_copy() {
        let ref_frame = make_ref_frame(16, 16);
        let mut dst = [0u8; 16];
        inter_predict(
            &mut dst,
            &ref_frame,
            16,
            16,
            0,  // mv_x = 0
            0,  // mv_y = 0
            2,  // block_x
            2,  // block_y
            4,  // block_w
            4,  // block_h
        );
        // 直接拷贝 ref_frame[2..6][2..6]
        for row in 0..4 {
            for col in 0..4 {
                let expected = ref_frame[(2 + row) * 16 + (2 + col)];
                assert_eq!(dst[row * 4 + col], expected);
            }
        }
    }

    #[test]
    fn test_half_pixel_horizontal() {
        // 水平 1/2 像素插值（mv_x = 2，即 1/2 像素）
        let ref_frame = make_ref_frame(16, 16);
        let mut dst = [0u8; 16];
        inter_predict(
            &mut dst,
            &ref_frame,
            16,
            16,
            2,  // mv_x = 2 (1/2 pixel)
            0,  // mv_y = 0
            2,  // block_x
            2,  // block_y
            4,  // block_w
            4,  // block_h
        );
        // 验证不 panic
    }

    #[test]
    fn test_half_pixel_vertical() {
        let ref_frame = make_ref_frame(16, 16);
        let mut dst = [0u8; 16];
        inter_predict(
            &mut dst,
            &ref_frame,
            16,
            16,
            0,
            2,  // mv_y = 2 (1/2 pixel)
            2,
            2,
            4,
            4,
        );
        // 验证不 panic
    }

    #[test]
    fn test_quarter_pixel_bilinear() {
        let ref_frame = make_ref_frame(16, 16);
        let mut dst = [0u8; 16];
        inter_predict(
            &mut dst,
            &ref_frame,
            16,
            16,
            1,  // mv_x = 1 (1/4 pixel)
            3,  // mv_y = 3 (3/4 pixel)
            2,
            2,
            4,
            4,
        );
        // 验证不 panic
    }

    #[test]
    fn test_boundary_clamp() {
        // 块在边界外：mv_x = -16 导致参考位置完全在左边界外
        let ref_frame = make_ref_frame(16, 16);
        let mut dst = [0u8; 16];
        inter_predict(
            &mut dst,
            &ref_frame,
            16,
            16,
            -16, // mv_x 负值，导致参考位置超出左边界
            0,
            0, // block_x = 0
            2,
            4,
            4,
        );
        // 所有像素应为 0（边界外）
        for &v in &dst {
            assert_eq!(v, 0, "boundary pixels should be 0");
        }
    }

    #[test]
    fn test_eight_by_eight_block() {
        let ref_frame = make_ref_frame(32, 32);
        let mut dst = [0u8; 64];
        inter_predict(
            &mut dst,
            &ref_frame,
            32,
            32,
            0,
            0,
            4,
            4,
            8,
            8,
        );
        for row in 0..8 {
            for col in 0..8 {
                let expected = ref_frame[(4 + row) * 32 + (4 + col)];
                assert_eq!(dst[row * 8 + col], expected);
            }
        }
    }

    #[test]
    fn test_get_h_samples_edge() {
        let ref_frame = make_ref_frame(16, 16);
        // x=0, 取 [0,1,2,3,4,5]
        let samples = get_h_samples(&ref_frame, 0, 0, 16);
        assert_eq!(samples, [0, 1, 2, 3, 4, 5]);

        // x=14, 取 [14,15,0,0,0,0]（最后两个超出边界）
        let samples = get_h_samples(&ref_frame, 0, 14, 16);
        assert_eq!(samples[0], 14);
        assert_eq!(samples[1], 15);
        assert_eq!(samples[2], 0);
        assert_eq!(samples[3], 0);
    }

    #[test]
    fn test_get_v_samples_edge() {
        let ref_frame = make_ref_frame(16, 16);
        // y=14, x=3, 取 [14*16+3, 15*16+3, 0, 0, 0, 0]
        let samples = get_v_samples(&ref_frame, 3, 14, 16, 16);
        assert_eq!(samples[0], (14 * 16 + 3) as u8);
        assert_eq!(samples[1], (15 * 16 + 3) as u8);
        assert_eq!(samples[2], 0);
        assert_eq!(samples[3], 0);
    }
}