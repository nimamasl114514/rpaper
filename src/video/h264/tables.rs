//! H.264 解码器查找表与常量定义。
// 常量沿用 H.264 领域命名惯例（如 4x4、16x16），保留小写 x。
#![allow(non_upper_case_globals)]

/// 4x4 Zigzag 扫描顺序（H.264 标准 Table 7-3.5）。
/// 将 4x4 量化系数从频域扫描到空间域，索引 [i] 返回 (row, col)。
pub const ZIGZAG_4x4: [(u8, u8); 16] = [
    (0, 0), (0, 1), (1, 0), (2, 0), (1, 1), (0, 2), (0, 3), (1, 2), (2, 1), (3, 0), (3, 1),
    (2, 2), (1, 3), (2, 3), (3, 2), (3, 3),
];

/// 4x4 Intra 预测模式数量。
#[allow(dead_code)]
pub const INTRA4x4_MODES: u8 = 9;

/// Intra 16x16 预测模式（0-3）。
#[allow(dead_code)]
pub const INTRA16x16_DC: u8 = 0;
#[allow(dead_code)]
pub const INTRA16x16_HOR: u8 = 1;
#[allow(dead_code)]
pub const INTRA16x16_VER: u8 = 2;
#[allow(dead_code)]
pub const INTRA16x16_PLANE: u8 = 3;

/// 色度预测模式（0-3，与 16x16 相同语义）。
#[allow(dead_code)]
pub const CHROMA_DC: u8 = 0;
#[allow(dead_code)]
pub const CHROMA_HOR: u8 = 1;
#[allow(dead_code)]
pub const CHROMA_VER: u8 = 2;
#[allow(dead_code)]
pub const CHROMA_PLANE: u8 = 3;

/// 宏块类型定义。
#[allow(dead_code)]
pub const MB_I_4x4: u8 = 0;
#[allow(dead_code)]
pub const MB_I_16x16: u8 = 1;
#[allow(dead_code)]
pub const MB_I_PCM: u8 = 25;
#[allow(dead_code)]
pub const MB_P_L0_16x16: u8 = 0;
#[allow(dead_code)]
pub const MB_P_L0_L0_16x8: u8 = 1;
#[allow(dead_code)]
pub const MB_P_L0_L0_8x16: u8 = 2;
#[allow(dead_code)]
pub const MB_P_8x8: u8 = 3;
#[allow(dead_code)]
pub const MB_P_8x8REF0: u8 = 4;
#[allow(dead_code)]
pub const MB_P_SKIP: u8 = 0; // 用特殊标志区分

/// 子宏块类型。
#[allow(dead_code)]
pub const SUB_MB_8x8: u8 = 0;
#[allow(dead_code)]
pub const SUB_MB_8x4: u8 = 1;
#[allow(dead_code)]
pub const SUB_MB_4x8: u8 = 2;
#[allow(dead_code)]
pub const SUB_MB_4x4: u8 = 3;

/// 分数像素插值权重（6 抽头滤波器），用于亮度半像素与四分之一像素插值。
#[allow(dead_code)]
pub const LUMA_HALF_PEL_TAPS: [i32; 6] = [1, -5, 20, 20, -5, 1];

/// Coded Block Pattern 映射表 (H.264 标准 Table 9-4(a), I 宏块, 48 条目)。
/// 索引为 me(v) 解码值, 输出 6 位 CBP (高 2 位色度, 低 4 位亮度 8x8 块)。
pub const CBP_INTRA: [u8; 48] = [
    47, 31, 15, 0, 23, 27, 29, 30, 7, 11, 13, 14, 39, 43, 45, 46,
    3, 5, 10, 12, 19, 21, 22, 26, 28, 35, 37, 38, 41, 42, 44, 1,
    2, 4, 8, 17, 18, 20, 24, 6, 9, 25, 33, 34, 36, 40, 48, 16,
];

/// Coded Block Pattern 映射表 (H.264 标准 Table 9-4, P 宏块, 52 条目)。
/// 索引 48-51 是 B 宏块专用 (此处 P 帧路径不会用到)。
pub const CBP_INTER: [u8; 52] = [
    0, 16, 1, 2, 4, 8, 32, 3, 5, 10, 12, 15, 47, 7, 11, 13, 14, 6, 9,
    31, 35, 37, 42, 44, 33, 34, 36, 40, 39, 43, 45, 46, 17, 18, 20, 24,
    19, 21, 22, 25, 26, 28, 23, 27, 29, 30, 41, 43, 42, 47, 47, 47,
];

/// I_16x16 mb_type (1..=24) → CBP 解码。
///
/// mb_type 编码了 Intra16x16 预测模式与 CBP 索引:
/// - pred_mode = (mb_type - 1) % 4    → 0..3
/// - cbp_idx   = (mb_type - 1) / 4    → 0..5
///
/// 返回 (pred_mode, cbp) 其中 cbp 与 me(v) 解码出的格式一致。
pub fn intra16x16_mb_type_to_cbp(mb_type: u8) -> (u8, u8) {
    debug_assert!((1..=24).contains(&mb_type));
    let idx = mb_type - 1;
    let pred_mode = idx % 4;
    let cbp_idx = idx / 4;
    // 标准 Table 7-11 中 CodedBlockPatternLuma 与 CodedBlockPatternChroma 映射
    const LUMA_PAT: [u8; 6] = [0, 0, 0, 15, 15, 15];
    const CHROMA_PAT: [u8; 6] = [0, 1, 2, 0, 1, 2];
    let cbp = (CHROMA_PAT[cbp_idx as usize] << 4) | LUMA_PAT[cbp_idx as usize];
    (pred_mode, cbp)
}

/// 从 CBP 提取色度模式 (0/1/2)。
#[inline]
pub fn cbp_chroma(cbp: u8) -> u8 {
    (cbp >> 4) & 0x3
}

/// 从 CBP 提取亮度 8x8 块掩码 (低 4 位)。
#[inline]
pub fn cbp_luma_8x8(cbp: u8) -> u8 {
    cbp & 0x0F
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cbp_tables_length() {
        assert_eq!(CBP_INTRA.len(), 48);
        assert_eq!(CBP_INTER.len(), 52);
    }

    #[test]
    fn test_intra16x16_cbp_decode() {
        // mb_type=1: pred_mode=0, cbp_idx=0 → cbp=(0<<4)|0=0
        assert_eq!(intra16x16_mb_type_to_cbp(1), (0, 0));
        // mb_type=4: pred_mode=3, cbp_idx=0 → cbp=0
        assert_eq!(intra16x16_mb_type_to_cbp(4), (3, 0));
        // mb_type=13: pred_mode=0, cbp_idx=3 → cbp=(0<<4)|15=15
        assert_eq!(intra16x16_mb_type_to_cbp(13), (0, 15));
        // mb_type=24: pred_mode=3, cbp_idx=5 → cbp=(2<<4)|15=47
        assert_eq!(intra16x16_mb_type_to_cbp(24), (3, 47));
    }
}
