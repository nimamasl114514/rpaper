//! H.264 帧内预测（Intra Prediction）模块。
//!
//! 实现 H.264 标准 §8.3 帧内预测：
//! - 亮度 4x4 Intra 预测（9 种模式，§8.3.1）
//! - 亮度 16x16 Intra 预测（4 种模式，§8.3.3）
//! - 色度 8x8 Intra 预测（4 种模式，§8.3.4）
//!
//! 参考像素不可用时使用 128（中性灰）填充。

/// 亮度 4x4 Intra 预测（H.264 标准 §8.3.1，表 8-2）。
///
/// * `dst` - 输出 4x4 块，16 个 u8，行优先排列
/// * `x` / `y` - 块左上角在帧中的位置（像素坐标）
/// * `ref_y` - 参考帧亮度平面（整帧 u8 数据）
/// * `stride` - 帧的宽度（像素）
/// * `mode` - 预测模式：0=Vertical, 1=Horizontal, 2=DC,
///   3=Diagonal_Down_Left, 4=Diagonal_Down_Right,
///   5=Vertical_Right, 6=Horizontal_Down,
///   7=Vertical_Left, 8=Horizontal_Up
pub fn intra4x4(dst: &mut [u8], x: usize, y: usize, ref_y: &[u8], stride: usize, mode: u8) {
    match mode {
        0 => intra4x4_vertical(dst, ref_y, stride, x, y),
        1 => intra4x4_horizontal(dst, ref_y, stride, x, y),
        2 => intra4x4_dc(dst, ref_y, stride, x, y),
        3 => intra4x4_diagonal_down_left(dst, ref_y, stride, x, y),
        4 => intra4x4_diagonal_down_right(dst, ref_y, stride, x, y),
        5 => intra4x4_vertical_right(dst, ref_y, stride, x, y),
        6 => intra4x4_horizontal_down(dst, ref_y, stride, x, y),
        7 => intra4x4_vertical_left(dst, ref_y, stride, x, y),
        8 => intra4x4_horizontal_up(dst, ref_y, stride, x, y),
        _ => {}
    }
}

/// 亮度 16x16 Intra 预测（H.264 标准 §8.3.3）。
///
/// * `dst` - 输出 16x16 块，256 个 u8，行优先排列
/// * `mode` - 预测模式：0=Vertical, 1=Horizontal, 2=DC, 3=Plane
pub fn intra16x16(dst: &mut [u8], x: usize, y: usize, ref_y: &[u8], stride: usize, mode: u8) {
    match mode {
        0 => intra16x16_vertical(dst, ref_y, stride, x, y),
        1 => intra16x16_horizontal(dst, ref_y, stride, x, y),
        2 => intra16x16_dc(dst, ref_y, stride, x, y),
        3 => intra16x16_plane(dst, ref_y, stride, x, y),
        _ => {}
    }
}

/// 色度 8x8 Intra 预测（H.264 标准 §8.3.4）。
///
/// 色度块大小为亮度的一半（4:2:0 下为 8x8），
/// 使用与 Intra16x16 相同的 4 种模式。
///
/// * `dst` - 输出 8x8 块，64 个 u8，行优先排列
/// * `mode` - 预测模式：0=Vertical, 1=Horizontal, 2=DC, 3=Plane
#[allow(dead_code)]
pub fn intra_chroma(dst: &mut [u8], x: usize, y: usize, ref_plane: &[u8], stride: usize, mode: u8) {
    match mode {
        0 => intra_chroma_vertical(dst, ref_plane, stride, x, y),
        1 => intra_chroma_horizontal(dst, ref_plane, stride, x, y),
        2 => intra_chroma_dc(dst, ref_plane, stride, x, y),
        3 => intra_chroma_plane(dst, ref_plane, stride, x, y),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 将 i32 值裁剪到 [0, 255] 并转为 u8。
#[inline]
fn clip(x: i32) -> u8 {
    x.clamp(0, 255) as u8
}

/// 获取 4x4 块的参考像素。
///
/// 返回 13 个 u8：
/// - 索引 0..8：上方像素 A-H（p[x+0,y-1] .. p[x+7,y-1]）
/// - 索引 8..12：左方像素 I-L（p[x-1,y+0] .. p[x-1,y+3]）
/// - 索引 12：左上像素 M（p[x-1,y-1]）
///
/// 越界或不可用时填充 128。
fn get_ref_4x4(ref_y: &[u8], stride: usize, x: usize, y: usize) -> [u8; 13] {
    let mut r = [128u8; 13];

    // A-H：上方参考像素
    if y > 0 {
        let row_base = (y - 1) * stride;
        for (i, r_item) in r.iter_mut().enumerate().take(8) {
            let px = x + i;
            if px < stride {
                *r_item = ref_y[row_base + px];
            }
        }
    }

    // I-L：左方参考像素
    if x > 0 {
        for i in 0..4 {
            let py = y + i;
            r[8 + i] = ref_y[py * stride + x - 1];
        }
    }

    // M：左上参考像素
    if x > 0 && y > 0 {
        r[12] = ref_y[(y - 1) * stride + x - 1];
    }

    r
}

// ---------------------------------------------------------------------------
// 4x4 模式实现（H.264 标准表 8-2）
// ---------------------------------------------------------------------------

/// Mode 0: Vertical
fn intra4x4_vertical(dst: &mut [u8], ref_y: &[u8], stride: usize, x: usize, y: usize) {
    if y > 0 {
        let row_base = (y - 1) * stride;
        for row in 0..4 {
            for col in 0..4 {
                dst[row * 4 + col] = ref_y[row_base + x + col];
            }
        }
    } else {
        dst.fill(128);
    }
}

/// Mode 1: Horizontal
fn intra4x4_horizontal(dst: &mut [u8], ref_y: &[u8], stride: usize, x: usize, y: usize) {
    if x > 0 {
        for row in 0..4 {
            let val = ref_y[(y + row) * stride + x - 1];
            for col in 0..4 {
                dst[row * 4 + col] = val;
            }
        }
    } else {
        dst.fill(128);
    }
}

/// Mode 2: DC
fn intra4x4_dc(dst: &mut [u8], ref_y: &[u8], stride: usize, x: usize, y: usize) {
    let mut sum = 0u32;
    let mut count = 0u32;

    if y > 0 {
        let row_base = (y - 1) * stride;
        for i in 0..4 {
            sum += ref_y[row_base + x + i] as u32;
            count += 1;
        }
    }
    if x > 0 {
        for i in 0..4 {
            sum += ref_y[(y + i) * stride + x - 1] as u32;
            count += 1;
        }
    }

    let dc = (sum + count / 2).checked_div(count).map_or(128, |v| v as u8);

    dst.fill(dc);
}

/// Mode 3: Diagonal Down-Left
fn intra4x4_diagonal_down_left(dst: &mut [u8], ref_y: &[u8], stride: usize, x: usize, y: usize) {
    let r = get_ref_4x4(ref_y, stride, x, y);
    let a = r[0] as i32;
    let b = r[1] as i32;
    let c = r[2] as i32;
    let d = r[3] as i32;
    let e = r[4] as i32;
    let f = r[5] as i32;
    let g = r[6] as i32;
    let h = r[7] as i32;

    dst[0] = clip((a + c + 2 * b + 2) >> 2);
    dst[1] = clip((b + d + 2 * c + 2) >> 2);
    dst[2] = clip((c + e + 2 * d + 2) >> 2);
    dst[3] = clip((d + f + 2 * e + 2) >> 2);
    dst[4] = clip((b + d + 2 * c + 2) >> 2);
    dst[5] = clip((c + e + 2 * d + 2) >> 2);
    dst[6] = clip((d + f + 2 * e + 2) >> 2);
    dst[7] = clip((e + g + 2 * f + 2) >> 2);
    dst[8] = clip((c + e + 2 * d + 2) >> 2);
    dst[9] = clip((d + f + 2 * e + 2) >> 2);
    dst[10] = clip((e + g + 2 * f + 2) >> 2);
    dst[11] = clip((f + h + 2 * g + 2) >> 2);
    dst[12] = clip((d + f + 2 * e + 2) >> 2);
    dst[13] = clip((e + g + 2 * f + 2) >> 2);
    dst[14] = clip((f + h + 2 * g + 2) >> 2);
    dst[15] = clip((g + 3 * h + 2) >> 2);
}

/// Mode 4: Diagonal Down-Right
fn intra4x4_diagonal_down_right(dst: &mut [u8], ref_y: &[u8], stride: usize, x: usize, y: usize) {
    let r = get_ref_4x4(ref_y, stride, x, y);
    let a = r[0] as i32;
    let b = r[1] as i32;
    let c = r[2] as i32;
    let d = r[3] as i32;
    let e = r[4] as i32;
    let i = r[8] as i32;
    let j = r[9] as i32;
    let k = r[10] as i32;
    let m = r[12] as i32;

    dst[0] = clip((a + 2 * m + i + 2) >> 2);
    dst[1] = clip((a + 2 * b + c + 2) >> 2);
    dst[2] = clip((b + 2 * c + d + 2) >> 2);
    dst[3] = clip((c + 2 * d + e + 2) >> 2);
    dst[4] = clip((i + 2 * m + a + 2) >> 2);
    dst[5] = clip((a + 2 * m + i + 2) >> 2);
    dst[6] = clip((a + 2 * b + c + 2) >> 2);
    dst[7] = clip((b + 2 * c + d + 2) >> 2);
    dst[8] = clip((j + 2 * i + m + 2) >> 2);
    dst[9] = clip((i + 2 * m + a + 2) >> 2);
    dst[10] = clip((a + 2 * m + i + 2) >> 2);
    dst[11] = clip((a + 2 * b + c + 2) >> 2);
    dst[12] = clip((k + 2 * j + i + 2) >> 2);
    dst[13] = clip((j + 2 * i + m + 2) >> 2);
    dst[14] = clip((i + 2 * m + a + 2) >> 2);
    dst[15] = clip((a + 2 * m + i + 2) >> 2);
}

/// Mode 5: Vertical-Right
fn intra4x4_vertical_right(dst: &mut [u8], ref_y: &[u8], stride: usize, x: usize, y: usize) {
    let r = get_ref_4x4(ref_y, stride, x, y);
    let a = r[0] as i32;
    let b = r[1] as i32;
    let c = r[2] as i32;
    let d = r[3] as i32;
    let i = r[8] as i32;
    let j = r[9] as i32;
    let k = r[10] as i32;
    let m = r[12] as i32;

    dst[0] = clip((m + a + 1) >> 1);
    dst[1] = clip((a + b + 1) >> 1);
    dst[2] = clip((b + c + 1) >> 1);
    dst[3] = clip((c + d + 1) >> 1);
    dst[4] = clip((i + 2 * m + a + 2) >> 2);
    dst[5] = clip((m + 2 * a + b + 2) >> 2);
    dst[6] = clip((a + 2 * b + c + 2) >> 2);
    dst[7] = clip((b + 2 * c + d + 2) >> 2);
    dst[8] = clip((j + 2 * i + m + 2) >> 2);
    dst[9] = clip((m + a + 1) >> 1);
    dst[10] = clip((a + b + 1) >> 1);
    dst[11] = clip((b + c + 1) >> 1);
    dst[12] = clip((k + 2 * j + i + 2) >> 2);
    dst[13] = clip((i + 2 * m + a + 2) >> 2);
    dst[14] = clip((m + 2 * a + b + 2) >> 2);
    dst[15] = clip((a + 2 * b + c + 2) >> 2);
}

/// Mode 6: Horizontal-Down
fn intra4x4_horizontal_down(dst: &mut [u8], ref_y: &[u8], stride: usize, x: usize, y: usize) {
    let r = get_ref_4x4(ref_y, stride, x, y);
    let a = r[0] as i32;
    let b = r[1] as i32;
    let c = r[2] as i32;
    let i = r[8] as i32;
    let j = r[9] as i32;
    let k = r[10] as i32;
    let l = r[11] as i32;
    let m = r[12] as i32;

    dst[0] = clip((m + i + 1) >> 1);
    dst[1] = clip((i + j + 1) >> 1);
    dst[2] = clip((j + k + 1) >> 1);
    dst[3] = clip((k + l + 1) >> 1);
    dst[4] = clip((i + 2 * m + a + 2) >> 2);
    dst[5] = clip((m + 2 * i + j + 2) >> 2);
    dst[6] = clip((i + 2 * j + k + 2) >> 2);
    dst[7] = clip((j + 2 * k + l + 2) >> 2);
    dst[8] = clip((b + 2 * a + m + 2) >> 2);
    dst[9] = clip((m + i + 1) >> 1);
    dst[10] = clip((i + j + 1) >> 1);
    dst[11] = clip((j + k + 1) >> 1);
    dst[12] = clip((c + 2 * b + a + 2) >> 2);
    dst[13] = clip((i + 2 * m + a + 2) >> 2);
    dst[14] = clip((m + 2 * i + j + 2) >> 2);
    dst[15] = clip((i + 2 * j + k + 2) >> 2);
}

/// Mode 7: Vertical-Left
fn intra4x4_vertical_left(dst: &mut [u8], ref_y: &[u8], stride: usize, x: usize, y: usize) {
    let r = get_ref_4x4(ref_y, stride, x, y);
    let a = r[0] as i32;
    let b = r[1] as i32;
    let c = r[2] as i32;
    let d = r[3] as i32;
    let e = r[4] as i32;
    let f = r[5] as i32;
    let g = r[6] as i32;
    dst[0] = clip((a + b + 1) >> 1);
    dst[1] = clip((b + c + 1) >> 1);
    dst[2] = clip((c + d + 1) >> 1);
    dst[3] = clip((d + e + 1) >> 1);
    dst[4] = clip((a + 2 * b + c + 2) >> 2);
    dst[5] = clip((b + 2 * c + d + 2) >> 2);
    dst[6] = clip((c + 2 * d + e + 2) >> 2);
    dst[7] = clip((d + 2 * e + f + 2) >> 2);
    dst[8] = clip((b + c + 1) >> 1);
    dst[9] = clip((c + d + 1) >> 1);
    dst[10] = clip((d + e + 1) >> 1);
    dst[11] = clip((e + f + 1) >> 1);
    dst[12] = clip((b + 2 * c + d + 2) >> 2);
    dst[13] = clip((c + 2 * d + e + 2) >> 2);
    dst[14] = clip((d + 2 * e + f + 2) >> 2);
    dst[15] = clip((e + 2 * f + g + 2) >> 2);
}

/// Mode 8: Horizontal-Up
fn intra4x4_horizontal_up(dst: &mut [u8], ref_y: &[u8], stride: usize, x: usize, y: usize) {
    let r = get_ref_4x4(ref_y, stride, x, y);
    let i = r[8] as i32;
    let j = r[9] as i32;
    let k = r[10] as i32;
    let l = r[11] as i32;

    dst[0] = clip((i + j + 1) >> 1);
    dst[1] = clip((j + k + 1) >> 1);
    dst[2] = clip((k + l + 1) >> 1);
    dst[3] = l as u8;
    dst[4] = clip((i + 2 * j + k + 2) >> 2);
    dst[5] = clip((j + 2 * k + l + 2) >> 2);
    dst[6] = clip((k + 2 * l + l + 2) >> 2);
    dst[7] = l as u8;
    dst[8] = clip((j + k + 1) >> 1);
    dst[9] = clip((k + l + 1) >> 1);
    dst[10] = l as u8;
    dst[11] = l as u8;
    dst[12] = clip((j + 2 * k + l + 2) >> 2);
    dst[13] = clip((k + 2 * l + l + 2) >> 2);
    dst[14] = l as u8;
    dst[15] = l as u8;
}

// ---------------------------------------------------------------------------
// 16x16 模式实现（H.264 标准 §8.3.3）
// ---------------------------------------------------------------------------

/// Mode 0: Vertical
fn intra16x16_vertical(dst: &mut [u8], ref_y: &[u8], stride: usize, x: usize, y: usize) {
    if y > 0 {
        let row_base = (y - 1) * stride;
        for row in 0..16 {
            for col in 0..16 {
                dst[row * 16 + col] = ref_y[row_base + x + col];
            }
        }
    } else {
        dst.fill(128);
    }
}

/// Mode 1: Horizontal
fn intra16x16_horizontal(dst: &mut [u8], ref_y: &[u8], stride: usize, x: usize, y: usize) {
    if x > 0 {
        for row in 0..16 {
            let val = ref_y[(y + row) * stride + x - 1];
            for col in 0..16 {
                dst[row * 16 + col] = val;
            }
        }
    } else {
        dst.fill(128);
    }
}

/// Mode 2: DC
fn intra16x16_dc(dst: &mut [u8], ref_y: &[u8], stride: usize, x: usize, y: usize) {
    let mut sum = 0u32;
    let mut count = 0u32;

    if y > 0 {
        let row_base = (y - 1) * stride;
        for i in 0..16 {
            sum += ref_y[row_base + x + i] as u32;
            count += 1;
        }
    }
    if x > 0 {
        for i in 0..16 {
            sum += ref_y[(y + i) * stride + x - 1] as u32;
            count += 1;
        }
    }

    let dc = (sum + count / 2).checked_div(count).map_or(128, |v| v as u8);

    dst.fill(dc);
}

/// Mode 3: Plane（H.264 标准 §8.3.3.4）
fn intra16x16_plane(dst: &mut [u8], ref_y: &[u8], stride: usize, x: usize, y: usize) {
    // 获取 16 个上方参考像素和 16 个左方参考像素
    let mut above = [128i32; 16];
    let mut left = [128i32; 16];

    if y > 0 {
        let row_base = (y - 1) * stride;
        for i in 0..16 {
            above[i] = ref_y[row_base + x + i] as i32;
        }
    }
    if x > 0 {
        for i in 0..16 {
            left[i] = ref_y[(y + i) * stride + x - 1] as i32;
        }
    }

    // M：左上角参考像素，用于 H/V 的最后一项
    let m = if x > 0 && y > 0 {
        ref_y[(y - 1) * stride + x - 1] as i32
    } else {
        128
    };

    // H = Σ(i=0..7) (i+1) * (above[8+i] - above[6-i])，其中 above[-1] = M
    let mut h = 0i32;
    for i in 0..7 {
        h += (i as i32 + 1) * (above[8 + i] - above[6 - i]);
    }
    h += 8 * (above[15] - m);

    // V = Σ(j=0..7) (j+1) * (left[8+j] - left[6-j])，其中 left[-1] = M
    let mut v = 0i32;
    for j in 0..7 {
        v += (j as i32 + 1) * (left[8 + j] - left[6 - j]);
    }
    v += 8 * (left[15] - m);

    let a = 16 * (left[15] + above[15]);
    let b = (5 * h + 32) >> 6;
    let c = (5 * v + 32) >> 6;

    for row in 0..16 {
        let y_off = (row as i32 - 7) * c;
        for col in 0..16 {
            let val = a + b * (col as i32 - 7) + y_off + 16;
            dst[row * 16 + col] = clip(val >> 5);
        }
    }
}

// ---------------------------------------------------------------------------
// 色度 8x8 模式实现（H.264 标准 §8.3.4）
// ---------------------------------------------------------------------------

/// Mode 0: Vertical
#[allow(dead_code)]
fn intra_chroma_vertical(dst: &mut [u8], ref_plane: &[u8], stride: usize, x: usize, y: usize) {
    if y > 0 {
        let row_base = (y - 1) * stride;
        for row in 0..8 {
            for col in 0..8 {
                dst[row * 8 + col] = ref_plane[row_base + x + col];
            }
        }
    } else {
        dst.fill(128);
    }
}

/// Mode 1: Horizontal
#[allow(dead_code)]
fn intra_chroma_horizontal(dst: &mut [u8], ref_plane: &[u8], stride: usize, x: usize, y: usize) {
    if x > 0 {
        for row in 0..8 {
            let val = ref_plane[(y + row) * stride + x - 1];
            for col in 0..8 {
                dst[row * 8 + col] = val;
            }
        }
    } else {
        dst.fill(128);
    }
}

/// Mode 2: DC
#[allow(dead_code)]
fn intra_chroma_dc(dst: &mut [u8], ref_plane: &[u8], stride: usize, x: usize, y: usize) {
    let mut sum = 0u32;
    let mut count = 0u32;

    if y > 0 {
        let row_base = (y - 1) * stride;
        for i in 0..8 {
            sum += ref_plane[row_base + x + i] as u32;
            count += 1;
        }
    }
    if x > 0 {
        for i in 0..8 {
            sum += ref_plane[(y + i) * stride + x - 1] as u32;
            count += 1;
        }
    }

    let dc = (sum + count / 2).checked_div(count).map_or(128, |v| v as u8);

    dst.fill(dc);
}

/// Mode 3: Plane（H.264 标准 §8.3.4.4）
#[allow(dead_code)]
fn intra_chroma_plane(dst: &mut [u8], ref_plane: &[u8], stride: usize, x: usize, y: usize) {
    let mut above = [128i32; 8];
    let mut left = [128i32; 8];

    if y > 0 {
        let row_base = (y - 1) * stride;
        for i in 0..8 {
            above[i] = ref_plane[row_base + x + i] as i32;
        }
    }
    if x > 0 {
        for i in 0..8 {
            left[i] = ref_plane[(y + i) * stride + x - 1] as i32;
        }
    }

    // M：左上角参考像素，用于 H/V 的最后一项（i=3 时 above[2-3] = M）
    let m = if x > 0 && y > 0 {
        ref_plane[(y - 1) * stride + x - 1] as i32
    } else {
        128
    };

    // H = Σ(i=0..3) (i+1) * (above[4+i] - above[2-i])，i=3 时 above[-1] = M
    let mut h = 0i32;
    for i in 0..3 {
        h += (i as i32 + 1) * (above[4 + i] - above[2 - i]);
    }
    h += 4 * (above[7] - m);

    // V = Σ(j=0..3) (j+1) * (left[4+j] - left[2-j])，j=3 时 left[-1] = M
    let mut v = 0i32;
    for j in 0..3 {
        v += (j as i32 + 1) * (left[4 + j] - left[2 - j]);
    }
    v += 4 * (left[7] - m);

    let a = 16 * (left[7] + above[7]);
    let b = (17 * h + 16) >> 5;
    let c = (17 * v + 16) >> 5;

    for row in 0..8 {
        let y_off = (row as i32 - 3) * c;
        for col in 0..8 {
            let val = a + b * (col as i32 - 3) + y_off + 16;
            dst[row * 8 + col] = clip(val >> 5);
        }
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个简单的测试帧（32x16），填充指定梯度。
    fn make_frame() -> Vec<u8> {
        let w = 32;
        let h = 16;
        let mut buf = vec![0u8; w * h];
        for row in 0..h {
            for col in 0..w {
                buf[row * w + col] = (row * 16 + col * 2) as u8;
            }
        }
        buf
    }

    // ---- 4x4 模式测试 ----

    #[test]
    fn test_intra4x4_vertical() {
        let frame = make_frame();
        let mut dst = [0u8; 16];
        // 块在 (4, 4)，上方像素来自 frame[3*32+4..7]
        intra4x4(&mut dst, 4, 4, &frame, 32, 0);
        for row in 0..4 {
            for col in 0..4 {
                let expected = frame[3 * 32 + 4 + col];
                assert_eq!(
                    dst[row * 4 + col],
                    expected,
                    "Vertical: dst[{},{}] should equal above pixel",
                    row,
                    col
                );
            }
        }
    }

    #[test]
    fn test_intra4x4_vertical_top_edge() {
        let frame = make_frame();
        let mut dst = [0u8; 16];
        // y=0 时无上方参考，全部为 128
        intra4x4(&mut dst, 4, 0, &frame, 32, 0);
        for &v in &dst {
            assert_eq!(v, 128);
        }
    }

    #[test]
    fn test_intra4x4_horizontal() {
        let frame = make_frame();
        let mut dst = [0u8; 16];
        intra4x4(&mut dst, 4, 4, &frame, 32, 1);
        for row in 0..4 {
            let expected = frame[(4 + row) * 32 + 3];
            for col in 0..4 {
                assert_eq!(dst[row * 4 + col], expected);
            }
        }
    }

    #[test]
    fn test_intra4x4_horizontal_left_edge() {
        let frame = make_frame();
        let mut dst = [0u8; 16];
        intra4x4(&mut dst, 0, 4, &frame, 32, 1);
        for &v in &dst {
            assert_eq!(v, 128);
        }
    }

    #[test]
    fn test_intra4x4_dc() {
        let frame = make_frame();
        let mut dst = [0u8; 16];
        intra4x4(&mut dst, 4, 4, &frame, 32, 2);
        // 上方 4 个 + 左方 4 个取均值
        let mut sum = 0u32;
        for i in 0..4 {
            sum += frame[3 * 32 + 4 + i] as u32;
        }
        for i in 0..4 {
            sum += frame[(4 + i) * 32 + 3] as u32;
        }
        let dc = ((sum + 4) / 8) as u8; // count=8
        for &v in &dst {
            assert_eq!(v, dc);
        }
    }

    #[test]
    fn test_intra4x4_dc_corner() {
        let frame = make_frame();
        let mut dst = [0u8; 16];
        // 左上角 (0,0)，无参考像素，DC=128
        intra4x4(&mut dst, 0, 0, &frame, 32, 2);
        for &v in &dst {
            assert_eq!(v, 128);
        }
    }

    #[test]
    fn test_intra4x4_ddl() {
        // 用已知值验证 DDL 模式
        let w = 16;
        let h = 8;
        let mut frame = vec![128u8; w * h];
        // 设置上方参考像素 A-H = 10,20,30,40,50,60,70,80
        for i in 0..8 {
            frame[3 * w + 4 + i] = (10 * (i + 1)) as u8;
        }
        let mut dst = [0u8; 16];
        intra4x4(&mut dst, 4, 4, &frame, w, 3);
        // 验证第一个像素：dst[0] = (A + C + 2*B + 2) >> 2 = (10+30+40+2)>>2 = 82>>2 = 20
        assert_eq!(dst[0], 20);
        // dst[3] = (D + F + 2*E + 2) >> 2 = (40+60+100+2)>>2 = 202>>2 = 50
        assert_eq!(dst[3], 50);
        // dst[15] = (G + 3*H + 2) >> 2 = (70+240+2)>>2 = 312>>2 = 78
        assert_eq!(dst[15], 78);
    }

    #[test]
    fn test_intra4x4_ddr() {
        let w = 16;
        let h = 8;
        let mut frame = vec![128u8; w * h];
        // A-D = 10,20,30,40; E = 50; I-L = 100,110,120,130; M = 5
        frame[3 * w + 4] = 10; // A
        frame[3 * w + 5] = 20; // B
        frame[3 * w + 6] = 30; // C
        frame[3 * w + 7] = 40; // D
        frame[3 * w + 8] = 50; // E
        frame[4 * w + 3] = 100; // I
        frame[5 * w + 3] = 110; // J
        frame[6 * w + 3] = 120; // K
        frame[3 * w + 3] = 5; // M
        let mut dst = [0u8; 16];
        intra4x4(&mut dst, 4, 4, &frame, w, 4);
        // dst[0] = (A + 2*M + I + 2) >> 2 = (10+10+100+2)>>2 = 122>>2 = 30
        assert_eq!(dst[0], 30);
        // dst[1] = (A + 2*B + C + 2) >> 2 = (10+40+30+2)>>2 = 82>>2 = 20
        assert_eq!(dst[1], 20);
    }

    #[test]
    fn test_intra4x4_vertical_right() {
        let w = 16;
        let h = 8;
        let mut frame = vec![128u8; w * h];
        frame[3 * w + 4] = 10; // A
        frame[3 * w + 5] = 20; // B
        frame[3 * w + 6] = 30; // C
        frame[3 * w + 7] = 40; // D
        frame[4 * w + 3] = 100; // I
        frame[5 * w + 3] = 110; // J
        frame[6 * w + 3] = 120; // K
        frame[3 * w + 3] = 5; // M
        let mut dst = [0u8; 16];
        intra4x4(&mut dst, 4, 4, &frame, w, 5);
        // dst[0] = (M + A + 1) >> 1 = (5+10+1)>>1 = 8
        assert_eq!(dst[0], 8);
        // dst[1] = (A + B + 1) >> 1 = (10+20+1)>>1 = 15
        assert_eq!(dst[1], 15);
        // dst[4] = (I + 2*M + A + 2) >> 2 = (100+10+10+2)>>2 = 122>>2 = 30
        assert_eq!(dst[4], 30);
    }

    #[test]
    fn test_intra4x4_horizontal_down() {
        let w = 16;
        let h = 8;
        let mut frame = vec![128u8; w * h];
        frame[3 * w + 4] = 10; // A
        frame[3 * w + 5] = 20; // B
        frame[3 * w + 6] = 30; // C
        frame[4 * w + 3] = 100; // I
        frame[5 * w + 3] = 110; // J
        frame[6 * w + 3] = 120; // K
        frame[7 * w + 3] = 130; // L
        frame[3 * w + 3] = 5; // M
        let mut dst = [0u8; 16];
        intra4x4(&mut dst, 4, 4, &frame, w, 6);
        // dst[0] = (M + I + 1) >> 1 = (5+100+1)>>1 = 53
        assert_eq!(dst[0], 53);
        // dst[1] = (I + J + 1) >> 1 = (100+110+1)>>1 = 105
        assert_eq!(dst[1], 105);
    }

    #[test]
    fn test_intra4x4_vertical_left() {
        let w = 16;
        let h = 8;
        let mut frame = vec![128u8; w * h];
        // A-H = 10,20,30,40,50,60,70,80
        for i in 0..8 {
            frame[3 * w + 4 + i] = (10 * (i + 1)) as u8;
        }
        let mut dst = [0u8; 16];
        intra4x4(&mut dst, 4, 4, &frame, w, 7);
        // dst[0] = (A + B + 1) >> 1 = (10+20+1)>>1 = 15
        assert_eq!(dst[0], 15);
        // dst[4] = (A + 2*B + C + 2) >> 2 = (10+40+30+2)>>2 = 82>>2 = 20
        assert_eq!(dst[4], 20);
    }

    #[test]
    fn test_intra4x4_horizontal_up() {
        let w = 16;
        let h = 8;
        let mut frame = vec![128u8; w * h];
        // I-L = 100,110,120,130
        frame[4 * w + 3] = 100;
        frame[5 * w + 3] = 110;
        frame[6 * w + 3] = 120;
        frame[7 * w + 3] = 130;
        let mut dst = [0u8; 16];
        intra4x4(&mut dst, 4, 4, &frame, w, 8);
        // dst[0] = (I + J + 1) >> 1 = (100+110+1)>>1 = 105
        assert_eq!(dst[0], 105);
        // dst[3] = L = 130
        assert_eq!(dst[3], 130);
        // dst[10] = L = 130
        assert_eq!(dst[10], 130);
    }

    // ---- 16x16 模式测试 ----

    #[test]
    fn test_intra16x16_vertical() {
        let frame = make_frame();
        let mut dst = [0u8; 256];
        intra16x16(&mut dst, 0, 8, &frame, 32, 0);
        for row in 0..16 {
            for col in 0..16 {
                let expected = frame[7 * 32 + col];
                assert_eq!(dst[row * 16 + col], expected);
            }
        }
    }

    #[test]
    fn test_intra16x16_dc() {
        let frame = make_frame();
        let mut dst = [0u8; 256];
        intra16x16(&mut dst, 0, 8, &frame, 32, 2);
        // 只有上方参考可用（x=0 无左方）
        let mut sum = 0u32;
        for i in 0..16 {
            sum += frame[7 * 32 + i] as u32;
        }
        let dc = ((sum + 8) / 16) as u8;
        for &v in &dst {
            assert_eq!(v, dc);
        }
    }

    #[test]
    fn test_intra16x16_plane() {
        // 构造一个简单的渐变帧，验证 Plane 模式不 panic
        let w = 64;
        let h = 64;
        let frame: Vec<u8> = (0..(w * h)).map(|i| (i % 256) as u8).collect();
        let mut dst = [0u8; 256];
        intra16x16(&mut dst, 20, 20, &frame, w, 3);
        // 不验证具体值，只验证不 panic
    }

    // ---- 色度模式测试 ----

    #[test]
    fn test_intra_chroma_vertical() {
        let w = 16;
        let h = 8;
        let mut frame = vec![128u8; w * h];
        for i in 0..8 {
            frame[3 * w + 4 + i] = (20 * (i + 1)) as u8;
        }
        let mut dst = [0u8; 64];
        intra_chroma(&mut dst, 4, 4, &frame, w, 0);
        for row in 0..8 {
            for col in 0..8 {
                let expected = frame[3 * w + 4 + col];
                assert_eq!(dst[row * 8 + col], expected);
            }
        }
    }

    #[test]
    fn test_intra_chroma_dc() {
        let w = 16;
        let h = 8;
        let frame = vec![128u8; w * h];
        let mut dst = [0u8; 64];
        intra_chroma(&mut dst, 0, 0, &frame, w, 2);
        for &v in &dst {
            assert_eq!(v, 128);
        }
    }

    #[test]
    fn test_intra_chroma_plane() {
        let w = 32;
        let h = 16;
        let frame: Vec<u8> = (0..(w * h)).map(|i| (i % 256) as u8).collect();
        let mut dst = [0u8; 64];
        intra_chroma(&mut dst, 8, 4, &frame, w, 3);
        // 不验证具体值，只验证不 panic
    }

    #[test]
    fn test_all_modes_at_corner() {
        // 验证左上角 (0,0) 所有模式不 panic
        let frame = vec![128u8; 32 * 16];
        let mut dst4 = [0u8; 16];
        for mode in 0..=8 {
            intra4x4(&mut dst4, 0, 0, &frame, 32, mode);
        }
        let mut dst16 = [0u8; 256];
        for mode in 0..=3 {
            intra16x16(&mut dst16, 0, 0, &frame, 32, mode);
        }
        let mut dst_chroma = [0u8; 64];
        for mode in 0..=3 {
            intra_chroma(&mut dst_chroma, 0, 0, &frame, 32, mode);
        }
    }
}