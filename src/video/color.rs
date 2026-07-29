//! 色彩空间转换：YUV 4:2:0 planar (I420) → RGBA8
//!
//! 纯 Rust H.264 解码器的色彩转换模块。H.264 解码器内部输出全范围
//! YUV [0,255]（非 studio range），用 BT.601 系数做定点转换，输出
//! RGBA8 供 wgpu 纹理上传使用。
//!
//! # 转换公式（full-range BT.601）
//! ```text
//! D = U - 128
//! E = V - 128
//! R = clip(Y + 1.402 * E)
//! G = clip(Y - 0.344 * D - 0.714 * E)
//! B = clip(Y + 1.772 * D)
//! ```
//!
//! 标量和 SIMD 路径都用 Q16 定点（系数 ×65536, `>>16`），保证结果
//! 逐字节一致。SIMD 路径用 SSE4.1 的 `_mm_mullo_epi32` 在 i32 通道
//! 做精确乘法，避免 SSE2 `_mm_mulhi_epi16` + `<<2` 的精度损失
//! （该组合最大误差 3，会让黄色 R 通道从 255 变成 254）。

use std::arch::x86_64::*;

/// 检查 width 和 height 是否满足 YUV420 要求：均为正偶数。
pub fn validate_dimensions(width: usize, height: usize) -> bool {
    width > 0 && height > 0 && (width & 1) == 0 && (height & 1) == 0
}

/// 将 i32 钳制到 `[0,255]` 并转换为 `u8`。
#[inline(always)]
fn clip255(x: i32) -> u8 {
    if x < 0 {
        0
    } else if x > 255 {
        255
    } else {
        x as u8
    }
}

/// YUV 4:2:0 planar (I420) 转 RGBA8。
///
/// 自动选择 SSE4.1 加速路径或标量 fallback，两者输出逐字节一致。
/// SSE4.1 在 2006 年后的 Intel / 2009 年后的 AMD 上全部支持，对 64
/// 位 x86 几乎是默认配置。
///
/// # 参数
/// - `y`: Y 平面数据，行跨距 `y_stride`，至少 `y_stride * visible_height` 字节
/// - `u`/`v`: 色度平面，行跨距 `y_stride/2`，至少 `(y_stride/2) * (visible_height/2)` 字节
/// - `rgba`: 输出缓冲区，尺寸 `visible_width * visible_height * 4`，像素布局 R,G,B,A
/// - `visible_width` / `visible_height`: 输出可见尺寸（裁剪后），必须为正偶数
/// - `y_stride`: Y 平面行跨距（编码宽度, 16 的倍数），通常等于 `coded_width`
///
/// 每个输出像素 alpha 固定为 255。
pub fn yuv420_to_rgba(
    y: &[u8],
    u: &[u8],
    v: &[u8],
    rgba: &mut [u8],
    visible_width: usize,
    visible_height: usize,
    y_stride: usize,
) {
    debug_assert!(
        validate_dimensions(visible_width, visible_height),
        "visible_width/height 必须为正偶数: got {visible_width}x{visible_height}"
    );
    debug_assert!(y_stride >= visible_width, "y_stride 必须 >= visible_width");
    let uv_stride = y_stride >> 1;
    debug_assert!(y.len() >= y_stride * visible_height, "Y 平面数据不足");
    debug_assert!(u.len() >= uv_stride * visible_height, "U 平面数据不足");
    debug_assert!(v.len() >= uv_stride * visible_height, "V 平面数据不足");
    debug_assert!(rgba.len() >= visible_width * visible_height * 4, "RGBA 输出缓冲不足");

    if is_x86_feature_detected!("sse4.1") {
        unsafe { yuv420_to_rgba_sse41(y, u, v, rgba, visible_width, visible_height, y_stride) }
    } else {
        yuv420_to_rgba_scalar(y, u, v, rgba, visible_width, visible_height, y_stride)
    }
}

/// 标量版本 — 按 2×2 像素块处理：四个像素共享同一组 UV，把色度读取与
/// 差量预计算摊到 4 个像素上，比逐像素方案减少 3/4 的色度运算。
///
/// 同时作为 SSE4.1 路径的尾部处理（处理 visible_width 不被 8 整除的剩余列）。
///
/// 公开是为给 color_bench example 做性能对比 baseline，正常调用走 `yuv420_to_rgba`。
pub fn yuv420_to_rgba_scalar(
    y: &[u8],
    u: &[u8],
    v: &[u8],
    rgba: &mut [u8],
    visible_width: usize,
    visible_height: usize,
    y_stride: usize,
) {
    let half_w = visible_width >> 1;
    let half_h = visible_height >> 1;
    let uv_stride = y_stride >> 1;
    let out_stride = visible_width * 4;

    for row in 0..half_h {
        let y_row0 = row * 2 * y_stride;
        let y_row1 = y_row0 + y_stride;
        let uv_row = row * uv_stride;
        let out_row0 = row * 2 * out_stride;
        let out_row1 = out_row0 + out_stride;

        for col in 0..half_w {
            let d = u[uv_row + col] as i32 - 128;
            let e = v[uv_row + col] as i32 - 128;

            // Q16 定点差量（>> 16 等价于 / 65536）
            let dr = (91881 * e) >> 16;            // 1.402 * (V-128)
            let dg = (22554 * d + 46802 * e) >> 16; // 0.344*D + 0.714*E
            let db = (116130 * d) >> 16;           // 1.772 * (U-128)

            let y00 = y[y_row0 + col * 2] as i32;
            let y01 = y[y_row0 + col * 2 + 1] as i32;
            let y10 = y[y_row1 + col * 2] as i32;
            let y11 = y[y_row1 + col * 2 + 1] as i32;

            let o0 = out_row0 + col * 8;
            let o1 = out_row1 + col * 8;

            rgba[o0    ] = clip255(y00 + dr);
            rgba[o0 + 1] = clip255(y00 - dg);
            rgba[o0 + 2] = clip255(y00 + db);
            rgba[o0 + 3] = 255;
            rgba[o0 + 4] = clip255(y01 + dr);
            rgba[o0 + 5] = clip255(y01 - dg);
            rgba[o0 + 6] = clip255(y01 + db);
            rgba[o0 + 7] = 255;
            rgba[o1    ] = clip255(y10 + dr);
            rgba[o1 + 1] = clip255(y10 - dg);
            rgba[o1 + 2] = clip255(y10 + db);
            rgba[o1 + 3] = 255;
            rgba[o1 + 4] = clip255(y11 + dr);
            rgba[o1 + 5] = clip255(y11 - dg);
            rgba[o1 + 6] = clip255(y11 + db);
            rgba[o1 + 7] = 255;
        }
    }
}

/// SSE4.1 加速路径：一次处理 2 行 × 8 列 = 16 像素，4 个 UV 对（每 UV 服务 4 像素）。
///
/// # 精确 Q16
/// `_mm_cvtepi16_epi32`（SSE4.1）把 i16x8 拆成两个 i32x4，
/// `_mm_mullo_epi32`（SSE4.1）在 i32 通道做精确乘法，
/// `_mm_srai_epi32(_, 16)` 算术右移 16 位，与标量 Q16 完全等价。
///
/// # RGBA 交织
/// 用 SSE2 的三步 unpack 组合（兼容性最好）：
/// 1. `rg = unpacklo_epi8(r, g)` → [R0,G0,R1,G1,...,R7,G7] (16 字节)
/// 2. `ba = unpacklo_epi8(b, a)` → [B0,A0,B1,A1,...,B7,A7]
/// 3. `out_lo = unpacklo_epi16(rg, ba)` → 前 4 像素 RGBA (16 字节)
/// 4. `out_hi = unpackhi_epi16(rg, ba)` → 后 4 像素 RGBA (16 字节)
#[target_feature(enable = "sse4.1")]
unsafe fn yuv420_to_rgba_sse41(
    y: &[u8],
    u: &[u8],
    v: &[u8],
    rgba: &mut [u8],
    visible_width: usize,
    visible_height: usize,
    y_stride: usize,
) {
    let half_w = visible_width >> 1;
    let half_h = visible_height >> 1;
    let uv_stride = y_stride >> 1;
    let out_stride = visible_width * 4;

    // Q16 系数（与标量完全一致）
    let kr = _mm_set1_epi32(91881);   // 1.402 * 2^16
    let kg_d = _mm_set1_epi32(22554); // 0.344 * 2^16
    let kg_e = _mm_set1_epi32(46802); // 0.714 * 2^16
    let kb = _mm_set1_epi32(116130);  // 1.772 * 2^16
    let zero = _mm_setzero_si128();
    let bias128 = _mm_set1_epi16(128);
    let alpha = _mm_set1_epi8(0xFFu8 as i8);

    // 主循环每次 4 个 UV 对（横向 8 像素），可见宽度 8 的倍数部分走这里
    let main_blocks = half_w / 4;

    for row in 0..half_h {
        let y_row0 = y.as_ptr().add(row * 2 * y_stride);
        let y_row1 = y.as_ptr().add(row * 2 * y_stride + y_stride);
        let u_row = u.as_ptr().add(row * uv_stride);
        let v_row = v.as_ptr().add(row * uv_stride);
        let out_row0 = rgba.as_mut_ptr().add(row * 2 * out_stride);
        let out_row1 = rgba.as_mut_ptr().add(row * 2 * out_stride + out_stride);

        for blk in 0..main_blocks {
            let col = blk * 4; // UV 列起点（4 个 UV 对）

            // ── 加载 4 个 U、4 个 V，复制 2 份 → i16x8 ──
            // 用 from_le_bytes 逐字节读取避免对齐问题（YUV 平面无 4 字节对齐保证）
            let u4 = _mm_cvtsi32_si128(i32::from_le_bytes([
                *u_row.add(col),
                *u_row.add(col + 1),
                *u_row.add(col + 2),
                *u_row.add(col + 3),
            ]));
            let v4 = _mm_cvtsi32_si128(i32::from_le_bytes([
                *v_row.add(col),
                *v_row.add(col + 1),
                *v_row.add(col + 2),
                *v_row.add(col + 3),
            ]));

            // 4 个 u8 → 8 个 u8 模式 [u0,u0,u1,u1,u2,u2,u3,u3]
            // unpacklo_epi8(a, a) 把低 8 字节交织成 [a0,a0,a1,a1,...,a7,a7]
            let u_dup = _mm_unpacklo_epi8(u4, u4);
            let v_dup = _mm_unpacklo_epi8(v4, v4);
            // u8x8 → i16x8
            let u16 = _mm_unpacklo_epi8(u_dup, zero);
            let v16 = _mm_unpacklo_epi8(v_dup, zero);
            // D = U - 128, E = V - 128
            let d = _mm_sub_epi16(u16, bias128);
            let e = _mm_sub_epi16(v16, bias128);

            // ── 在 i32 通道计算 Q16 差量 ──
            // i16x8 → 两个 i32x4（低 4 + 高 4）
            let d_lo = _mm_cvtepi16_epi32(d);
            let d_hi = _mm_cvtepi16_epi32(_mm_srli_si128(d, 8));
            let e_lo = _mm_cvtepi16_epi32(e);
            let e_hi = _mm_cvtepi16_epi32(_mm_srli_si128(e, 8));

            // dr = (E * 91881) >> 16, db = (D * 116130) >> 16
            let dr_lo = _mm_srai_epi32(_mm_mullo_epi32(e_lo, kr), 16);
            let dr_hi = _mm_srai_epi32(_mm_mullo_epi32(e_hi, kr), 16);
            let db_lo = _mm_srai_epi32(_mm_mullo_epi32(d_lo, kb), 16);
            let db_hi = _mm_srai_epi32(_mm_mullo_epi32(d_hi, kb), 16);

            // dg = (D*22554 + E*46802) >> 16
            let dg_d_lo = _mm_mullo_epi32(d_lo, kg_d);
            let dg_d_hi = _mm_mullo_epi32(d_hi, kg_d);
            let dg_e_lo = _mm_mullo_epi32(e_lo, kg_e);
            let dg_e_hi = _mm_mullo_epi32(e_hi, kg_e);
            let dg_lo = _mm_srai_epi32(_mm_add_epi32(dg_d_lo, dg_e_lo), 16);
            let dg_hi = _mm_srai_epi32(_mm_add_epi32(dg_d_hi, dg_e_hi), 16);

            // i32x4 → i16x8（packs 饱和到 [-32768, 32767]，差量值域 [-256, 256] 不会触发）
            let dr16 = _mm_packs_epi32(dr_lo, dr_hi);
            let dg16 = _mm_packs_epi32(dg_lo, dg_hi);
            let db16 = _mm_packs_epi32(db_lo, db_hi);

            // ── 加载 2 行 × 8 个 Y 像素 ──
            let y0_8 = _mm_loadl_epi64(y_row0.add(col * 2) as *const __m128i);
            let y1_8 = _mm_loadl_epi64(y_row1.add(col * 2) as *const __m128i);
            let y0 = _mm_unpacklo_epi8(y0_8, zero);
            let y1 = _mm_unpacklo_epi8(y1_8, zero);

            // R = Y + dr, G = Y - dg, B = Y + db
            let r0 = _mm_add_epi16(y0, dr16);
            let g0 = _mm_sub_epi16(y0, dg16);
            let b0 = _mm_add_epi16(y0, db16);
            let r1 = _mm_add_epi16(y1, dr16);
            let g1 = _mm_sub_epi16(y1, dg16);
            let b1 = _mm_add_epi16(y1, db16);

            // packus: i16x8 → u8x16, 顺便完成 clip 到 [0, 255]
            // 低 8 字节有效（高位用 zero 填充）
            let r0_u8 = _mm_packus_epi16(r0, zero);
            let g0_u8 = _mm_packus_epi16(g0, zero);
            let b0_u8 = _mm_packus_epi16(b0, zero);
            let r1_u8 = _mm_packus_epi16(r1, zero);
            let g1_u8 = _mm_packus_epi16(g1, zero);
            let b1_u8 = _mm_packus_epi16(b1, zero);

            // ── RGBA 交织（SSE2 三步 unpack 组合）──
            // rg = [R0,G0,R1,G1,...,R7,G7] (16 字节, 8 个 i16)
            // ba = [B0,255,B1,255,...,B7,255]
            let rg0 = _mm_unpacklo_epi8(r0_u8, g0_u8);
            let ba0 = _mm_unpacklo_epi8(b0_u8, alpha);
            let rg1 = _mm_unpacklo_epi8(r1_u8, g1_u8);
            let ba1 = _mm_unpacklo_epi8(b1_u8, alpha);

            // out_lo = [R0,G0,B0,255, R1,G1,B1,255, R2,G2,B2,255, R3,G3,B3,255]
            // out_hi = [R4,G4,B4,255, R5,G5,B5,255, R6,G6,B6,255, R7,G7,B7,255]
            let out0_lo = _mm_unpacklo_epi16(rg0, ba0);
            let out0_hi = _mm_unpackhi_epi16(rg0, ba0);
            let out1_lo = _mm_unpacklo_epi16(rg1, ba1);
            let out1_hi = _mm_unpackhi_epi16(rg1, ba1);

            // 写入两行 RGBA 输出（每行 8 像素 = 32 字节 = 2 个 128-bit store）
            let out0_ptr = out_row0.add(col * 8) as *mut __m128i;
            let out1_ptr = out_row1.add(col * 8) as *mut __m128i;
            _mm_storeu_si128(out0_ptr, out0_lo);
            _mm_storeu_si128(out0_ptr.add(1), out0_hi);
            _mm_storeu_si128(out1_ptr, out1_lo);
            _mm_storeu_si128(out1_ptr.add(1), out1_hi);
        }

        // ── 尾部：剩余 < 8 列走标量 2×2 块 ──
        let tail_start = main_blocks * 4;
        if tail_start < half_w {
            let y_row0_off = row * 2 * y_stride;
            let y_row1_off = y_row0_off + y_stride;
            let uv_row_off = row * uv_stride;
            let out_row0_off = row * 2 * out_stride;
            let out_row1_off = out_row0_off + out_stride;

            for col in tail_start..half_w {
                let d = u[uv_row_off + col] as i32 - 128;
                let e = v[uv_row_off + col] as i32 - 128;
                let dr = (91881 * e) >> 16;
                let dg = (22554 * d + 46802 * e) >> 16;
                let db = (116130 * d) >> 16;

                let y00 = y[y_row0_off + col * 2] as i32;
                let y01 = y[y_row0_off + col * 2 + 1] as i32;
                let y10 = y[y_row1_off + col * 2] as i32;
                let y11 = y[y_row1_off + col * 2 + 1] as i32;

                let o0 = out_row0_off + col * 8;
                let o1 = out_row1_off + col * 8;
                rgba[o0    ] = clip255(y00 + dr);
                rgba[o0 + 1] = clip255(y00 - dg);
                rgba[o0 + 2] = clip255(y00 + db);
                rgba[o0 + 3] = 255;
                rgba[o0 + 4] = clip255(y01 + dr);
                rgba[o0 + 5] = clip255(y01 - dg);
                rgba[o0 + 6] = clip255(y01 + db);
                rgba[o0 + 7] = 255;
                rgba[o1    ] = clip255(y10 + dr);
                rgba[o1 + 1] = clip255(y10 - dg);
                rgba[o1 + 2] = clip255(y10 + db);
                rgba[o1 + 3] = 255;
                rgba[o1 + 4] = clip255(y11 + dr);
                rgba[o1 + 5] = clip255(y11 - dg);
                rgba[o1 + 6] = clip255(y11 + db);
                rgba[o1 + 7] = 255;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一帧纯色 YUV 数据
    fn make_solid_frame(w: usize, h: usize, y_val: u8, u_val: u8, v_val: u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let coded_w = (w + 15) & !15;
        let y = vec![y_val; coded_w * h];
        let uv = vec![u_val; (coded_w / 2) * (h / 2)];
        let v_plane = vec![v_val; (coded_w / 2) * (h / 2)];
        (y, uv, v_plane)
    }

    /// 纯色 YUV 对应的 RGBA 参考值（与标量 Q16 一致）
    fn solid_rgba(y_val: u8, u_val: u8, v_val: u8) -> [u8; 4] {
        let d = u_val as i32 - 128;
        let e = v_val as i32 - 128;
        let dr = (91881 * e) >> 16;
        let dg = (22554 * d + 46802 * e) >> 16;
        let db = (116130 * d) >> 16;
        let yy = y_val as i32;
        [clip255(yy + dr), clip255(yy - dg), clip255(yy + db), 255]
    }

    #[test]
    fn solid_frame_16x16_white() {
        let (y, u, v) = make_solid_frame(16, 16, 255, 128, 128);
        let mut rgba = vec![0u8; 16 * 16 * 4];
        yuv420_to_rgba(&y, &u, &v, &mut rgba, 16, 16, 16);
        let expected = solid_rgba(255, 128, 128);
        for px in rgba.chunks_exact(4) {
            assert_eq!(px, expected);
        }
    }

    #[test]
    fn solid_frame_32x16_yellow() {
        // 黄色: Y=226, U=0, V=149 (BT.601 full-range)
        let (y, u, v) = make_solid_frame(32, 16, 226, 0, 149);
        let mut rgba = vec![0u8; 32 * 16 * 4];
        yuv420_to_rgba(&y, &u, &v, &mut rgba, 32, 16, 32);
        let expected = solid_rgba(226, 0, 149);
        for px in rgba.chunks_exact(4) {
            assert_eq!(px, expected);
        }
    }

    #[test]
    fn non_aligned_width_uses_tail() {
        // 宽度 22 = 主循环 16 (2 个 8 列块) + 尾部 6 (3 个 2 列块)
        let w = 22;
        let h = 16;
        let (y, u, v) = make_solid_frame(w, h, 128, 100, 200);
        let mut rgba = vec![0u8; w * h * 4];
        yuv420_to_rgba(&y, &u, &v, &mut rgba, w, h, (w + 15) & !15);
        let expected = solid_rgba(128, 100, 200);
        for (i, px) in rgba.chunks_exact(4).enumerate() {
            assert_eq!(px, expected, "像素 {i} 不匹配");
        }
    }

    #[test]
    fn sse41_matches_scalar() {
        // 构造非平凡 YUV 图案，确保 SSE4.1 与标量输出逐字节一致
        let w = 64;
        let h = 32;
        let coded_w = (w + 15) & !15;
        let mut y = vec![0u8; coded_w * h];
        let mut u = vec![0u8; (coded_w / 2) * (h / 2)];
        let mut v = vec![0u8; (coded_w / 2) * (h / 2)];
        for row in 0..h {
            for col in 0..coded_w {
                y[row * coded_w + col] = ((row * 7 + col * 3) & 0xFF) as u8;
            }
        }
        for row in 0..h / 2 {
            for col in 0..coded_w / 2 {
                u[row * coded_w / 2 + col] = ((row * 11 + col * 5 + 30) & 0xFF) as u8;
                v[row * coded_w / 2 + col] = ((row * 13 + col * 9 + 60) & 0xFF) as u8;
            }
        }

        let mut rgba_simd = vec![0u8; w * h * 4];
        let mut rgba_scalar = vec![0u8; w * h * 4];
        unsafe { yuv420_to_rgba_sse41(&y, &u, &v, &mut rgba_simd, w, h, coded_w) };
        yuv420_to_rgba_scalar(&y, &u, &v, &mut rgba_scalar, w, h, coded_w);

        for i in 0..(w * h * 4) {
            assert_eq!(rgba_simd[i], rgba_scalar[i],
                "字节 {i} 不一致 (SSE4.1={}, 标量={})", rgba_simd[i], rgba_scalar[i]);
        }
    }

    #[test]
    fn odd_width_with_tail_matches_scalar() {
        // 宽度 22 包含尾部，确保 SIMD+尾部 与 标量 一致
        let w = 22;
        let h = 16;
        let coded_w = (w + 15) & !15;
        let mut y = vec![0u8; coded_w * h];
        let mut u = vec![0u8; (coded_w / 2) * (h / 2)];
        let mut v = vec![0u8; (coded_w / 2) * (h / 2)];
        for row in 0..h {
            for col in 0..coded_w {
                y[row * coded_w + col] = ((row * 31 + col * 17) & 0xFF) as u8;
            }
        }
        for row in 0..h / 2 {
            for col in 0..coded_w / 2 {
                u[row * coded_w / 2 + col] = ((row * 19 + col * 23 + 50) & 0xFF) as u8;
                v[row * coded_w / 2 + col] = ((row * 29 + col * 11 + 70) & 0xFF) as u8;
            }
        }

        let mut rgba_simd = vec![0u8; w * h * 4];
        let mut rgba_scalar = vec![0u8; w * h * 4];
        unsafe { yuv420_to_rgba_sse41(&y, &u, &v, &mut rgba_simd, w, h, coded_w) };
        yuv420_to_rgba_scalar(&y, &u, &v, &mut rgba_scalar, w, h, coded_w);

        for i in 0..(w * h * 4) {
            assert_eq!(rgba_simd[i], rgba_scalar[i],
                "字节 {i} 不一致 (SSE4.1={}, 标量={})", rgba_simd[i], rgba_scalar[i]);
        }
    }
}
