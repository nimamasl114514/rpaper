/// H.264 NAL 单元类型
pub const NAL_SLICE_NON_IDR: u8 = 1;
pub const NAL_SLICE_IDR: u8 = 5;
#[allow(dead_code)]
pub const NAL_SEI: u8 = 6;
pub const NAL_SPS: u8 = 7;
pub const NAL_PPS: u8 = 8;
#[allow(dead_code)]
pub const NAL_AUD: u8 = 9;

/// 从 Annex B 格式数据中提取所有 NAL 单元。
///
/// 关键修正:
/// 1. 提取每个 NAL 后立即去除 emulation prevention bytes (`00 00 03` → `00 00`),
///    否则后续 SPS/PPS/Slice 的 bit 解析会全部错位 (H.264 §7.4.1)。
/// 2. 内层循环条件用 `i < data.len()` 而非 `i + 2 < data.len()`,
///    否则当 NAL 是数据末尾且长度 < 3 时会丢弃最后 2 字节
///    (典型场景: PPS 仅 4 字节, 被截断为 2 字节, 导致 deblocking_ctrl 解析错误)。
pub fn extract_nals(data: &[u8]) -> Vec<Vec<u8>> {
    let mut nals = Vec::new();
    let mut i = 0;
    while i + 2 < data.len() {
        if data[i] == 0 && data[i + 1] == 0 {
            if data[i + 2] == 1 {
                i += 3;
                let start = i;
                while i < data.len()
                    && !(i + 2 < data.len()
                        && data[i] == 0
                        && data[i + 1] == 0
                        && (data[i + 2] == 1
                            || (i + 3 < data.len() && data[i + 2] == 0 && data[i + 3] == 1)))
                {
                    i += 1;
                }
                nals.push(remove_emulation_prevention(&data[start..i]));
            } else if data[i + 2] == 0 && i + 3 < data.len() && data[i + 3] == 1 {
                i += 4;
                let start = i;
                while i < data.len()
                    && !(i + 2 < data.len()
                        && data[i] == 0
                        && data[i + 1] == 0
                        && (data[i + 2] == 1
                            || (i + 3 < data.len() && data[i + 2] == 0 && data[i + 3] == 1)))
                {
                    i += 1;
                }
                nals.push(remove_emulation_prevention(&data[start..i]));
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    nals
}

/// 去除 H.264 emulation prevention bytes (§7.4.1): `00 00 03` → `00 00`。
///
/// NAL unit body 中, `00 00 00`, `00 00 01`, `00 00 02`, `00 00 03` 被转义为
/// `00 00 03 00`, `00 00 03 01`, `00 00 03 02`, `00 00 03 03`, 解码时需还原。
fn remove_emulation_prevention(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if i + 2 < data.len() && data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 3 {
            result.push(0);
            result.push(0);
            i += 3; // 跳过转义字节 03
        } else {
            result.push(data[i]);
            i += 1;
        }
    }
    result
}

pub fn nal_unit_type(nal: &[u8]) -> u8 {
    if nal.is_empty() {
        return 0;
    }
    nal[0] & 0x1F
}

#[allow(dead_code)]
pub fn is_idr(nal: &[u8]) -> bool {
    nal_unit_type(nal) == NAL_SLICE_IDR
}

#[allow(dead_code)]
pub fn is_sps(nal: &[u8]) -> bool {
    nal_unit_type(nal) == NAL_SPS
}

#[allow(dead_code)]
pub fn is_pps(nal: &[u8]) -> bool {
    nal_unit_type(nal) == NAL_PPS
}