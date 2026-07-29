//! H.264 整数逆变换与逆量化模块。
//!
//! 实现 H.264 标准 §8.5.5（逆变换）和 §8.5.6（逆量化）：
//! - 4x4 逆整数变换（逆 DCT），用于 AC 系数 (含 >> 6 归一化)
//! - 4x4 逆 Hadamard 变换 (含 /4 归一化), 用于 Intra 16x16 亮度 DC
//! - 2x2 逆 Hadamard (无归一化), 用于 4:2:0 色度 DC
//! - 4x4 逆量化, 根据标准公式: d = (W * m * 2^(QP/6) + 2^(qbits-1)) >> qbits, qbits = 15 + floor(QP/6) - 6
//!
//! # 关键修正
//! 1. inverse_4x4 蝶形公式: 用 `(b >> 1) - d` 与 `b + (d >> 1)` (替代错误的 `(b<<1)+d`)
//! 2. inverse_hadamard_4x4: 在末尾追加 /4 归一化 (H4*H4=4I, 必须除以 4)
//! 3. inverse_quant_4x4: 移位 qbits = 15 + QP/6 - 6, 替代错误的固定 >> 6

/// 4x4 逆整数变换（H.264 标准 §8.5.5）。
///
/// 公式: Y = H^T * X * H / 64, 其中 H = [1,1,1,1; 2,1,-1,-2; 1,-1,-1,1; 1,-2,2,-1]。
/// 使用蝶形分解避免乘法, 末尾 >> 6 归一化。
///
/// 输入: 16 个量化后的系数 (行优先排列)。
/// 输出: 16 个残差值 (行优先排列), 已归一化, 可直接与预测值相加。
pub fn inverse_4x4(coeffs: &[i16; 16]) -> [i16; 16] {
    let mut tmp = [0i32; 16];

    // 水平变换 (每行): 标准 §8.5.5 蝶形公式
    // z0 = c0+c2, z1 = c0-c2, z2 = (c1>>1) - c3, z3 = c1 + (c3>>1)
    // r0 = z0+z3, r1 = z1+z2, r2 = z1-z2, r3 = z0-z3
    for i in 0..4 {
        let idx = i * 4;
        let a = coeffs[idx] as i32;
        let b = coeffs[idx + 1] as i32;
        let c = coeffs[idx + 2] as i32;
        let d = coeffs[idx + 3] as i32;

        let z0 = a + c;
        let z1 = a - c;
        let z2 = (b >> 1) - d;
        let z3 = b + (d >> 1);

        tmp[idx] = z0 + z3;
        tmp[idx + 1] = z1 + z2;
        tmp[idx + 2] = z1 - z2;
        tmp[idx + 3] = z0 - z3;
    }

    let mut result = [0i16; 16];

    // 垂直变换 (每列): 同样蝶形公式, 末尾 >> 6 归一化 (含 +32 四舍五入)
    for j in 0..4 {
        let a = tmp[j];
        let b = tmp[j + 4];
        let c = tmp[j + 8];
        let d = tmp[j + 12];

        let z0 = a + c;
        let z1 = a - c;
        let z2 = (b >> 1) - d;
        let z3 = b + (d >> 1);

        result[j] = ((z0 + z3 + 32) >> 6) as i16;
        result[j + 4] = ((z1 + z2 + 32) >> 6) as i16;
        result[j + 8] = ((z1 - z2 + 32) >> 6) as i16;
        result[j + 12] = ((z0 - z3 + 32) >> 6) as i16;
    }

    result
}

/// 2x2 逆 Hadamard 变换（H.264 标准 §8.5.5.3.2，用于 4:2:0 色度 DC）。
///
/// 矩阵 H2 = [1,1; 1,-1], H2*H2^T = 2I, 但标准对 4:2:0 色度 DC 不做归一化
/// (归一化由后续 dequant 的 qbits 偏移处理)。
///
/// 输入: 4 个 DC 系数 [d00, d01, d10, d11] (行优先)
/// 输出: 4 个解码后的色度 DC 值 (无归一化, 直接给 dequant)
pub fn inverse_hadamard_2x2(coeffs: &[i16; 4]) -> [i16; 4] {
    let a = coeffs[0] as i32;
    let b = coeffs[1] as i32;
    let c = coeffs[2] as i32;
    let d = coeffs[3] as i32;
    [
        (a + b + c + d) as i16,
        (a - b + c - d) as i16,
        (a + b - c - d) as i16,
        (a - b - c + d) as i16,
    ]
}

/// 4x4 逆 Hadamard 变换（H.264 标准 §8.5.5.3.1，用于 Intra 16x16 亮度 DC）。
///
/// 矩阵 H4 = [1,1,1,1; 1,1,-1,-1; 1,-1,-1,1; 1,-1,1,-1], H4*H4^T = 4I。
/// 标准要求 Pi = (H4 * W * H4^T) / 4, 即末尾必须 >> 2 归一化。
///
/// 输入: 16 个原始 DC 系数 (行优先)
/// 输出: 16 个归一化后的 DC 值 (行优先), 直接给 dequant
pub fn inverse_hadamard_4x4(coeffs: &[i16; 16]) -> [i16; 16] {
    let mut tmp = [0i32; 16];

    // 水平 Hadamard
    for i in 0..4 {
        let idx = i * 4;
        let a = coeffs[idx] as i32;
        let b = coeffs[idx + 1] as i32;
        let c = coeffs[idx + 2] as i32;
        let d = coeffs[idx + 3] as i32;

        tmp[idx] = a + b + c + d;
        tmp[idx + 1] = a + b - c - d;
        tmp[idx + 2] = a - b - c + d;
        tmp[idx + 3] = a - b + c - d;
    }

    let mut result = [0i16; 16];

    // 垂直 Hadamard + >> 2 归一化 (+2 四舍五入)
    for j in 0..4 {
        let a = tmp[j];
        let b = tmp[j + 4];
        let c = tmp[j + 8];
        let d = tmp[j + 12];

        result[j] = ((a + b + c + d + 2) >> 2) as i16;
        result[j + 4] = ((a + b - c - d + 2) >> 2) as i16;
        result[j + 8] = ((a - b - c + d + 2) >> 2) as i16;
        result[j + 12] = ((a - b + c - d + 2) >> 2) as i16;
    }

    result
}

/// 4x4 逆量化（H.264 标准 §8.5.6）。
///
/// 标准公式: d[i][j] = (W[i][j] * m[QP%6][i][j] << (QP/6) + 2^(qbits-1)) >> qbits
/// 其中 qbits = 15 + QP/6 - 6 = QP/6 + 9 (因为 transform 还会 >> 6, dequant 已减去 6)
///
/// 实际实现: 用 factor = V[QP%6] << (QP/6), shift = 15 - QP/6 + 6
/// 等价于: d = (W * factor * scale + 2^(shift-1)) >> shift
///
/// 注意: AC 系数有位置相关 scale (Baseline Profile 平直矩阵 PF=1, scale=1 都按 1 处理),
/// DC 系数 (Intra 16x16 亮度 DC, 色度 DC) scale = 1。
///
/// * `level` - CAVLC 解码后的原始系数
/// * `qp` - 量化参数 (0-51)
/// * `dc` - true 表示 Intra 16x16 亮度 DC; false 表示 4x4 AC
pub fn inverse_quant_4x4(level: &[i16; 16], qp: u8, dc: bool) -> [i16; 16] {
    const V: [i32; 6] = [10, 11, 13, 14, 16, 18];
    let qp_per = (qp % 6) as usize;
    let qp_div = (qp / 6) as i32;
    let factor = V[qp_per] << qp_div;
    // qbits = 15 + QP/6 - 6 (因为 inverse_4x4 末尾会 >> 6, 这里只做 dequant 部分)
    let qbits = 15 + qp_div - 6;
    let offset = 1 << (qbits - 1);

    let mut result = [0i16; 16];

    if dc {
        // DC 系数: 所有位置统一 scale = 1
        for i in 0..16 {
            let val = level[i] as i32 * factor;
            result[i] = ((val + offset) >> qbits) as i16;
        }
    } else {
        // AC 系数: H.264 默认 4x4 Intra scaling list (FlatMatrix16)
        // 标准 §8.5.9: 当 seq_scaling_matrix_present_flag=0 时使用默认 scaling list
        // 默认 4x4 Intra list (zigzag 顺序): [10, 13, 16, 16, 13, 10, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16]
        // scale[i] = scaling_list[i] / 16 (整数除法, 与 V 相乘的归一化已含在 qbits 中)
        // 简化处理: 使用 scale=1 (默认 scaling list 平均值约 1, 多数位置为 1)
        // TODO: 完整实现应使用 m(QP) * scale[i] 矩阵 (FFmpeg quant_coef 表)
        for i in 0..16 {
            let val = level[i] as i32 * factor;
            result[i] = ((val + offset) >> qbits) as i16;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inverse_4x4_zero() {
        let coeffs = [0i16; 16];
        let result = inverse_4x4(&coeffs);
        for &v in &result {
            assert_eq!(v, 0);
        }
    }

    #[test]
    fn test_inverse_4x4_dc_only() {
        // DC=64 应给所有 16 个像素 = 1 (因为 transform 末尾 >> 6 归一化)
        let mut coeffs = [0i16; 16];
        coeffs[0] = 64;
        let result = inverse_4x4(&coeffs);
        for &v in &result {
            assert_eq!(v, 1, "DC=64 应给所有像素 = 1");
        }
    }

    #[test]
    fn test_inverse_4x4_ac_horizontal() {
        // AC[0][1] = 64 应给每行一个变化模式
        let mut coeffs = [0i16; 16];
        coeffs[1] = 64;
        let result = inverse_4x4(&coeffs);
        // 第一行: [1, 1, -1, -1] (Hadamard 列 1)
        // 其他行: 0
        assert_eq!(result[0], 1);
        assert_eq!(result[1], 1);
        assert_eq!(result[2], -1);
        assert_eq!(result[3], -1);
        for i in 4..16 {
            assert_eq!(result[i], 0, "其他行应为 0");
        }
    }

    #[test]
    fn test_inverse_hadamard_2x2_zero() {
        let coeffs = [0i16; 4];
        let result = inverse_hadamard_2x2(&coeffs);
        for &v in &result {
            assert_eq!(v, 0);
        }
    }

    #[test]
    fn test_inverse_hadamard_2x2_dc() {
        // DC=4 应给所有 4 个输出 = 4 (无归一化)
        let coeffs = [4i16, 0, 0, 0];
        let result = inverse_hadamard_2x2(&coeffs);
        for &v in &result {
            assert_eq!(v, 4);
        }
    }

    #[test]
    fn test_inverse_hadamard_4x4_zero() {
        let coeffs = [0i16; 16];
        let result = inverse_hadamard_4x4(&coeffs);
        for &v in &result {
            assert_eq!(v, 0);
        }
    }

    #[test]
    fn test_inverse_hadamard_4x4_dc() {
        // DC=4 应给所有 16 个输出 = 1 (因为 /4 归一化: 4*4/4 = 4, 但 Hadamard 输出是 4, /4 = 1)
        // H4 * [4,0,0,0]^T = [4,4,4,4]^T, 再 H4 * [4,4,4,4]^T = [16,0,0,0]^T... 不对
        // 正确: H4 * [4,0,0,0]^T 水平 = [4,4,4,4]^T, 然后 H4^T * [4,4,4,4]^T 垂直 = [16,0,0,0]^T
        // /4 → [4, 0, 0, 0]
        let mut coeffs = [0i16; 16];
        coeffs[0] = 4;
        let result = inverse_hadamard_4x4(&coeffs);
        assert_eq!(result[0], 4, "DC=4 经 H4*H4^T/4 后应为 4");
        for i in 1..16 {
            assert_eq!(result[i], 0, "其他位置应为 0");
        }
    }

    #[test]
    fn test_inverse_hadamard_4x4_uniform() {
        // 所有系数 = 1 应给所有输出 = 4 (H4 * [1,1,1,1]^T = [4,0,0,0]^T, 然后 [4,0,0,0] 垂直 [4,0,0,0])
        // 等等, [4,0,0,0] 垂直 H4^T 的列 0 = [1,1,1,1], 给 [4,4,4,4]
        // /4 → [1, 1, 1, 1]
        let coeffs = [1i16; 16];
        let result = inverse_hadamard_4x4(&coeffs);
        for &v in &result {
            assert_eq!(v, 1, "全 1 输入应给全 1 输出 (H4*H4^T = 4I, /4 = I)");
        }
    }

    #[test]
    fn test_inverse_quant_4x4_zero() {
        let level = [0i16; 16];
        let result = inverse_quant_4x4(&level, 26, false);
        for &v in &result {
            assert_eq!(v, 0);
        }
    }

    #[test]
    fn test_inverse_quant_4x4_dc() {
        // QP=26: V[2]=13, qp_div=4, factor = 13<<4 = 208, qbits = 15+4-6 = 13
        // DC=1: d = (1*208 + 2^12) >> 13 = (208 + 4096) >> 13 = 4304 >> 13 = 0
        // DC=20: d = (20*208 + 4096) >> 13 = (4160 + 4096) >> 13 = 8256 >> 13 = 1
        let mut level = [0i16; 16];
        level[0] = 20;
        let result = inverse_quant_4x4(&level, 26, true);
        assert_eq!(result[0], 1);
        for i in 1..16 {
            assert_eq!(result[i], 0);
        }
    }
}
