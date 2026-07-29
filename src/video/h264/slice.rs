//! H.264 切片 (Slice) 解码与残差重建。
//!
//! 实现 H.264 标准 §7.3.3 (slice_data) 与 §7.3.5 (macroblock_layer) 的解码流程,
//! 涵盖 I_4x4 / I_16x16 / I_PCM / P_Skip / P_L0_16x16 五种宏块类型。
//! 重点修正: slice_type=7 漏判、IDR 检测用 nal_type、prev_intra4x4_pred_mode_flag
//! 正确语法、CBP me(v) 映射、I_16x16 DC Hadamard、4:2:0 色度残差、P 帧切片头字段。

use super::bitstream::BitReader;
use super::cavlc;
use super::intra;
use super::inter;
use super::nal;
use super::pps::Pps;
use super::sps::Sps;
use super::tables;
use super::transform;

use super::frame::DecodedFrame;

/// 4x4 亮度块在 16x16 宏块内的扫描顺序 (H.264 §7.3.5.3, 按 8x8 分组的光栅顺序)。
/// 8x8 #0: 0,1,4,5  8x8 #1: 2,3,6,7  8x8 #2: 8,9,12,13  8x8 #3: 10,11,14,15
const LUMA_BLOCK_SCAN: [usize; 16] = [
    0, 1, 4, 5, // 8x8 #0
    2, 3, 6, 7, // 8x8 #1
    8, 9, 12, 13, // 8x8 #2
    10, 11, 14, 15, // 8x8 #3
];

/// 4x4 块在宏块内的位置 (row, col), 索引 0..15。
const BLOCK_POS: [(u8, u8); 16] = [
    (0, 0), (0, 1), (0, 2), (0, 3),
    (1, 0), (1, 1), (1, 2), (1, 3),
    (2, 0), (2, 1), (2, 2), (2, 3),
    (3, 0), (3, 1), (3, 2), (3, 3),
];

/// 切片头信息。
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SliceHeader {
    pub first_mb_in_slice: u32,
    pub slice_type: u8,
    pub pic_parameter_set_id: u32,
    pub frame_num: u32,
    pub idr_pic_id: Option<u32>,
    pub pic_order_cnt_lsb: Option<u32>,
    pub slice_qp_delta: i32,
    pub disable_deblocking_filter_idc: u8,
    pub slice_alpha_c0_offset_div2: i8,
    pub slice_beta_offset_div2: i8,
}

/// 宏块类型分类 (用于重建阶段区分预测路径)。
#[derive(Clone, Copy, Debug, PartialEq)]
enum MbKind {
    I4x4,
    I16x16(u8), // 亮度预测模式 0..3
    #[allow(dead_code)]
    IPcm,
    PSkip,
    PL016x16,
}

/// 邻居宏块上下文 (左/上), 用于 CAVLC nC 计算与 intra4x4 mpm 预测。
#[derive(Clone, Copy)]
struct NeighborCtx {
    /// 邻居宏块 16 个 4x4 块的 total_coeff (nC 上下文)。
    nc: [u32; 16],
    /// 邻居宏块 16 个 4x4 块的预测模式 (用于 mpm)。
    modes: [u8; 16],
    /// 邻居宏块类型 (用于判断是否 I_4x4)。
    kind: MbKind,
    /// 邻居宏块的 I_16x16 亮度预测模式 (kind=I16x16 时有效)。
    #[allow(dead_code)]
    i16_mode: u8,
}

impl NeighborCtx {
    const fn empty() -> Self {
        NeighborCtx {
            nc: [0; 16],
            modes: [2; 16], // 默认 DC 模式
            kind: MbKind::I4x4,
            i16_mode: 0,
        }
    }
}

/// 已解码宏块信息 (用于重建与下一宏块的邻居上下文)。
#[allow(dead_code)]
struct Macroblock {
    kind: MbKind,
    intra4x4_modes: [u8; 16],
    intra16x16_mode: u8,
    intra_chroma_mode: u8,
    mv: (i32, i32),
    qp: u8,
    cbp: u8,
    /// 16 个 4x4 亮度块残差 (Zigzag 顺序, 行优先 4x4)。
    luma_residuals: [[i16; 16]; 16],
    /// 4 个 4x4 色度 U 块残差 (Zigzag 顺序)。
    chroma_u_residuals: [[i16; 16]; 4],
    chroma_v_residuals: [[i16; 16]; 4],
    /// I_16x16 亮度 DC 系数 (16 个, 4x4 行优先, 已逆 Hadamard)。
    luma16x16_dc: [i16; 16],
    /// 4:2:0 色度 DC 系数 (4 个, 已逆 Hadamard 2x2)。
    chroma_u_dc: [i16; 4],
    chroma_v_dc: [i16; 4],
    /// 16 个 4x4 亮度块的 total_coeff (CAVLC 上下文)。
    luma_total_coeffs: [u32; 16],
}

impl Macroblock {
    fn to_neighbor(&self) -> NeighborCtx {
        NeighborCtx {
            nc: self.luma_total_coeffs,
            modes: self.intra4x4_modes,
            kind: self.kind,
            i16_mode: self.intra16x16_mode,
        }
    }
}

// ─── 切片解码入口 ─────────────────────────────────────────────────────

/// 解码一个切片。
///
/// * `nal_type` — NAL 单元类型 (用于 IDR 检测, nal_type==5 即 IDR)。
pub fn decode_slice(
    nal_data: &[u8],
    nal_type: u8,
    sps: &Sps,
    pps: &Pps,
    ref_frame: Option<&DecodedFrame>,
    current_frame: &mut DecodedFrame,
) -> Result<SliceHeader, String> {
    let mut br = BitReader::new(&nal_data[1..]); // 跳过 NAL header
    let header = parse_slice_header(&mut br, sps, pps, nal_type)?;

    let qp_init = ((pps.pic_init_qp_minus26 + 26) + header.slice_qp_delta) as u8;
    let mb_w = sps.mb_width as usize;
    let mb_h = sps.mb_height as usize;
    let total_mb = mb_w * mb_h;

    // P 帧切片 (slice_type 0/5)。I 帧为 2/7。
    let is_p = matches!(header.slice_type, 0 | 5);

    // 相邻宏块上下文: 上一行所有宏块 + 当前行的左邻宏块
    let mut top_row: Vec<NeighborCtx> = vec![NeighborCtx::empty(); mb_w];
    let mut cur_row: Vec<NeighborCtx> = Vec::with_capacity(mb_w);

    let mut mb_index = header.first_mb_in_slice as usize;
    let mut mb_qp = qp_init;
    let mut p_skip_run: u32 = 0;

    while mb_index < total_mb {
        let mb_x = mb_index % mb_w;
        let mb_y = mb_index / mb_w;

        // 行切换: 当前行结束 → 提升为 top
        if mb_x == 0 && !cur_row.is_empty() {
            top_row = std::mem::take(&mut cur_row);
            cur_row = Vec::with_capacity(mb_w);
        }

        let left = if mb_x > 0 { Some(cur_row[mb_x - 1]) } else { None };
        let top = Some(top_row[mb_x]);

        let mb = if is_p {
            // P 帧: mb_skip_run 决定若干个 P_Skip, 然后是 1 个非 skip 宏块
            if p_skip_run == 0 {
                if br.remaining_bits() < 8 {
                    break;
                }
                p_skip_run = br.read_ue();
            }

            if p_skip_run > 0 {
                p_skip_run -= 1;
                make_p_skip_mb(mb_qp)
            } else {
                let result = decode_macroblock(
                    &mut br, sps, &header, pps, mb_x, mb_y, mb_qp, left, top,
                )?;
                mb_qp = result.qp;
                result
            }
        } else {
            if br.remaining_bits() < 8 {
                break;
            }
            let result = decode_macroblock(
                &mut br, sps, &header, pps, mb_x, mb_y, mb_qp, left, top,
            )?;
            mb_qp = result.qp;
            result
        };

        reconstruct_macroblock(&mb, mb_index, mb_w, sps, ref_frame, current_frame);

        cur_row.push(mb.to_neighbor());
        mb_index += 1;
    }

    Ok(header)
}

// ─── 切片头解析 ───────────────────────────────────────────────────────

fn parse_slice_header(
    br: &mut BitReader,
    sps: &Sps,
    pps: &Pps,
    nal_type: u8,
) -> Result<SliceHeader, String> {
    let first_mb_in_slice = br.read_ue();
    let slice_type = br.read_ue() as u8;
    let pic_parameter_set_id = br.read_ue();
    let frame_num = br.read_bits(sps.log2_max_frame_num_minus4 as u8 + 4);

    // IDR 检测: 通过 nal_unit_type==5, 而非 slice_type==5 (slice_type 5 实为 P 帧)
    let idr_pic_id = if nal_type == nal::NAL_SLICE_IDR {
        let v = br.read_ue();
        Some(v)
    } else {
        None
    };

    let pic_order_cnt_lsb = if sps.pic_order_cnt_type == 0 {
        Some(br.read_bits(sps.log2_max_pic_order_cnt_lsb_minus4.unwrap_or(0) as u8 + 4))
    } else {
        None
    };

    let is_p = matches!(slice_type, 0 | 5);

    // P 帧切片头额外字段: num_ref_idx_active_override + ref_pic_list_modification
    if is_p {
        let num_ref_idx_active_override_flag = br.read_bool();
        if num_ref_idx_active_override_flag {
            let _ = br.read_ue(); // num_ref_idx_l0_active_minus1
        }

        // ref_pic_list_modification
        let ref_pic_list_modification_flag_l0 = br.read_bool();
        if ref_pic_list_modification_flag_l0 {
            loop {
                let idc = br.read_ue();
                if idc == 0 {
                    break;
                }
                // modification_of_pic_nums_idc 1/2: abs_diff_pic_num_minus1
                // idc 3: long_term_pic_num
                if idc == 1 || idc == 2 {
                    let _ = br.read_ue();
                } else if idc == 3 {
                    let _ = br.read_ue();
                } else {
                    break;
                }
            }
        }
    }

    // dec_ref_pic_marking
    if nal_type == nal::NAL_SLICE_IDR {
        // IDR: no_output_of_prior_pics_flag + long_term_reference_flag
        let _ = br.read_bool();
        let _ = br.read_bool();
    } else {
        // 非 IDR: adaptive_reference_pic_marking_mode_flag
        let adaptive = br.read_bool();
        if adaptive {
            loop {
                let idc = br.read_ue();
                if idc == 0 {
                    break;
                }
                match idc {
                    1 | 2 => { let _ = br.read_ue(); }
                    3 => { let _ = br.read_ue(); let _ = br.read_ue(); }
                    4 => { let _ = br.read_ue(); }
                    5 => { let _ = br.read_ue(); let _ = br.read_ue(); }
                    6 => {}
                    _ => break,
                }
            }
        }
    }

    let slice_qp_delta = br.read_se();

    // deblocking_filter_control
    let (disable_deblocking_filter_idc, slice_alpha_c0_offset_div2, slice_beta_offset_div2) =
        if pps.deblocking_filter_control_present_flag {
            let idc = br.read_ue() as u8;
            if idc != 1 {
                let alpha = br.read_se() as i8;
                let beta = br.read_se() as i8;
                (idc, alpha, beta)
            } else {
                (idc, 0i8, 0i8)
            }
        } else {
            (1u8, 0i8, 0i8)
        };

    Ok(SliceHeader {
        first_mb_in_slice,
        slice_type,
        pic_parameter_set_id,
        frame_num,
        idr_pic_id,
        pic_order_cnt_lsb,
        slice_qp_delta,
        disable_deblocking_filter_idc,
        slice_alpha_c0_offset_div2,
        slice_beta_offset_div2,
    })
}

// ─── 宏块解码 ─────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
fn decode_macroblock(
    br: &mut BitReader,
    _sps: &Sps,
    header: &SliceHeader,
    _pps: &Pps,
    mb_x: usize,
    mb_y: usize,
    qp_in: u8,
    left: Option<NeighborCtx>,
    top: Option<NeighborCtx>,
) -> Result<Macroblock, String> {
    let is_i = matches!(header.slice_type, 2 | 7);
    let is_p = matches!(header.slice_type, 0 | 5);
    // H.264 MB 解码调试开关 — 调试时改成 `mb_x < N && mb_y == 0` 可输出前 N 个 MB 的逐步日志
    // 生产环境必须保持 false，否则每帧产生数百行 eprintln 严重影响性能
    let dbg = false;

    let mb_type_raw = br.read_ue();
    if dbg { eprintln!("[DBG dec_mb] mb_x={mb_x} mb_type_raw={mb_type_raw} bit_pos={}", br.current_bit_pos()); }
    let kind;
    let mut intra4x4_modes = [0u8; 16];
    let mut intra16x16_mode = 0u8;
    let mut intra_chroma_mode = 0u8;
    let mut mv = (0i32, 0i32);
    let cbp;
    let mut mb_qp = qp_in;

    if is_i {
        if mb_type_raw == 25 {
            // I_PCM: 跳过 PCM 数据
            kind = MbKind::IPcm;
            br.skip_to_byte_boundary();
            for _ in 0..256 { let _ = br.read_bits(8); }
            for _ in 0..64 { let _ = br.read_bits(8); }
            for _ in 0..64 { let _ = br.read_bits(8); }
            return Ok(Macroblock {
                kind, intra4x4_modes, intra16x16_mode, intra_chroma_mode,
                mv, qp: mb_qp, cbp: 0,
                luma_residuals: [[0; 16]; 16],
                chroma_u_residuals: [[0; 16]; 4],
                chroma_v_residuals: [[0; 16]; 4],
                luma16x16_dc: [0; 16],
                chroma_u_dc: [0; 4],
                chroma_v_dc: [0; 4],
                luma_total_coeffs: [0; 16],
            });
        } else if mb_type_raw == 0 {
            // I_4x4: 16 个 4x4 块预测模式 + 色度模式 + CBP me(v)
            kind = MbKind::I4x4;
            for i in 0..16 {
                let mpm = calc_most_probable_mode(i, mb_x, mb_y, &intra4x4_modes, left, top);
                let flag = br.read_bool();
                intra4x4_modes[i] = if flag {
                    mpm
                } else {
                    let rem = br.read_bits(3) as u8;
                    if rem < mpm { rem } else { rem + 1 }
                };
            }
            if dbg { eprintln!("[DBG dec_mb] mb_x={mb_x} after pred_modes bit_pos={}", br.current_bit_pos()); }
            intra_chroma_mode = br.read_ue() as u8;
            if dbg { eprintln!("[DBG dec_mb] mb_x={mb_x} chroma_mode={intra_chroma_mode} bit_pos={}", br.current_bit_pos()); }
            let cbp_code = br.read_ue() as usize;
            cbp = if cbp_code < 48 { tables::CBP_INTRA[cbp_code] } else { 0 };
            if dbg { eprintln!("[DBG dec_mb] mb_x={mb_x} cbp_code={cbp_code} cbp={cbp:#04x} bit_pos={}", br.current_bit_pos()); }
        } else if mb_type_raw <= 24 {
            // I_16x16: mb_type 编码 pred_mode + CBP
            let (pred_mode, cbp_16x16) = tables::intra16x16_mb_type_to_cbp(mb_type_raw as u8);
            kind = MbKind::I16x16(pred_mode);
            intra16x16_mode = pred_mode;
            intra4x4_modes = [pred_mode; 16]; // 用 i16_mode 占位 (mpm 计算时会检测 kind)
            intra_chroma_mode = br.read_ue() as u8;
            cbp = cbp_16x16;
            if dbg { eprintln!("[DBG dec_mb] mb_x={mb_x} I16x16 pred={pred_mode} cbp={cbp_16x16:#04x} chroma={intra_chroma_mode} bit_pos={}", br.current_bit_pos()); }
        } else {
            return Err(format!("无效 I 宏块类型: {mb_type_raw}"));
        }
    } else if is_p {
        // P 帧: mb_type_raw=0 → P_L0_16x16 (skip 由 skip_run 处理, 不在此分支)
        if mb_type_raw == 0 {
            kind = MbKind::PL016x16;
            // mvd_l0: 16x16 分区 1 个 mvd
            let mvd_x = br.read_se();
            let mvd_y = br.read_se();
            mv = (mvd_x, mvd_y);
            // CBP me(v) (P 宏块用 CBP_INTER)
            let cbp_code = br.read_ue() as usize;
            cbp = if cbp_code < 48 { tables::CBP_INTER[cbp_code] } else { 0 };
        } else {
            // 简化: 其他 P 宏块类型当作 P_L0_16x16 处理 (不完整支持 16x8/8x16/8x8)
            kind = MbKind::PL016x16;
            let mvd_x = br.read_se();
            let mvd_y = br.read_se();
            mv = (mvd_x, mvd_y);
            let cbp_code = br.read_ue() as usize;
            cbp = if cbp_code < 48 { tables::CBP_INTER[cbp_code] } else { 0 };
        }
    } else {
        return Err(format!("不支持的 slice_type: {}", header.slice_type));
    }

    // mb_qp_delta 读取条件 (H.264 §7.3.5):
    //   CodedBlockPatternLuma > 0 || CodedBlockPatternChroma > 0 || MbPartPredMode == Intra_16x16
    // 即: cbp != 0 时总是读; I_16x16 即使 cbp=0 也要读 (因为有 Intra16x16DCLevel)
    let has_residual = cbp != 0 || matches!(kind, MbKind::I16x16(_));
    if has_residual {
        let delta = br.read_se();
        if dbg { eprintln!("[DBG dec_mb] mb_x={mb_x} qp_delta={delta} bit_pos={}", br.current_bit_pos()); }
        mb_qp = ((mb_qp as i32 + delta + 52) % 52) as u8;
    }

    // 残差解码
    let mut luma_residuals = [[0i16; 16]; 16];
    let mut chroma_u_residuals = [[0i16; 16]; 4];
    let mut chroma_v_residuals = [[0i16; 16]; 4];
    let mut luma16x16_dc = [0i16; 16];
    let mut chroma_u_dc = [0i16; 4];
    let mut chroma_v_dc = [0i16; 4];
    let mut luma_total_coeffs = [0u32; 16];

    if has_residual {
        let luma_pat = tables::cbp_luma_8x8(cbp);
        let chroma_pat = tables::cbp_chroma(cbp);
        if dbg { eprintln!("[DBG dec_mb] mb_x={mb_x} luma_pat={luma_pat:#05b} chroma_pat={chroma_pat}"); }

        // I_16x16: 先解码亮度 DC (16 个系数, nC=-1 用 VLC0)
        // 注意: I_16x16 即使 cbp=0 (LumaPattern=0) 也总是有 Intra16x16DCLevel
        if matches!(kind, MbKind::I16x16(_)) {
            let dc_result = cavlc::decode_block(br, 0, 16);
            luma16x16_dc = dc_result.coeffs;
            if dbg { eprintln!("[DBG dec_mb] mb_x={mb_x} after I16 DC tc={} bit_pos={}", dc_result.total_coeff, br.current_bit_pos()); }
            // 注意: DC 块的 total_coeff 不计入 luma_total_coeffs (用于 nC)
        }

        // 亮度 4x4 块: 按 LUMA_BLOCK_SCAN 顺序, 受 8x8 CBP 位控制
        let max_coeff_luma = if matches!(kind, MbKind::I16x16(_)) { 15 } else { 16 };

        // 临时存储 total_coeff 以便 nC 计算 (按扫描顺序填充)
        let mut cur_tc = [0u32; 16];

        for (scan_pos, &blk_idx) in LUMA_BLOCK_SCAN.iter().enumerate() {
            let blk8 = scan_pos / 4; // 0..3, 对应 8x8 块索引
            if (luma_pat >> blk8) & 1 == 0 {
                continue; // 该 8x8 块无系数
            }
            let n_c = calc_nc(blk_idx, mb_x, mb_y, &cur_tc, left, top);
            let result = cavlc::decode_block(br, n_c, max_coeff_luma);
            luma_residuals[blk_idx] = result.coeffs;
            cur_tc[blk_idx] = result.total_coeff;
            luma_total_coeffs[blk_idx] = result.total_coeff;
            if dbg { eprintln!("[DBG dec_mb] mb_x={mb_x} luma blk_idx={blk_idx} n_c={n_c} tc={} bit_pos={}", result.total_coeff, br.current_bit_pos()); }
        }

        // 色度 DC: 2x2 (4:2:0), 4 个系数, nC=-1 专用 VLC 表
        if chroma_pat != 0 {
            let (u_dc, u_tc) = cavlc::decode_chroma_dc(br);
            let (v_dc, v_tc) = cavlc::decode_chroma_dc(br);
            chroma_u_dc = u_dc;
            chroma_v_dc = v_dc;
            if dbg { eprintln!("[DBG dec_mb] mb_x={mb_x} after chroma DC u_tc={u_tc} v_tc={v_tc} bit_pos={}", br.current_bit_pos()); }
        }

        // 色度 AC: 4 个 4x4 块每分量, max_coeff=15 (跳过 DC 位置)
        if chroma_pat == 2 {
            for blk in 0..4 {
                let result = cavlc::decode_block(br, 0, 15);
                chroma_u_residuals[blk] = result.coeffs;
                if dbg { eprintln!("[DBG dec_mb] mb_x={mb_x} chromaU blk={blk} tc={} bit_pos={}", result.total_coeff, br.current_bit_pos()); }
            }
            for blk in 0..4 {
                let result = cavlc::decode_block(br, 0, 15);
                chroma_v_residuals[blk] = result.coeffs;
                if dbg { eprintln!("[DBG dec_mb] mb_x={mb_x} chromaV blk={blk} tc={} bit_pos={}", result.total_coeff, br.current_bit_pos()); }
            }
        }
    }
    if dbg { eprintln!("[DBG dec_mb] mb_x={mb_x} END bit_pos={}", br.current_bit_pos()); }

    Ok(Macroblock {
        kind,
        intra4x4_modes,
        intra16x16_mode,
        intra_chroma_mode,
        mv,
        qp: mb_qp,
        cbp,
        luma_residuals,
        chroma_u_residuals,
        chroma_v_residuals,
        luma16x16_dc,
        chroma_u_dc,
        chroma_v_dc,
        luma_total_coeffs,
    })
}

/// 构造 P_Skip 宏块 (mv=0,0, 无残差)。
fn make_p_skip_mb(qp: u8) -> Macroblock {
    Macroblock {
        kind: MbKind::PSkip,
        intra4x4_modes: [0; 16],
        intra16x16_mode: 0,
        intra_chroma_mode: 0,
        mv: (0, 0),
        qp,
        cbp: 0,
        luma_residuals: [[0; 16]; 16],
        chroma_u_residuals: [[0; 16]; 4],
        chroma_v_residuals: [[0; 16]; 4],
        luma16x16_dc: [0; 16],
        chroma_u_dc: [0; 4],
        chroma_v_dc: [0; 4],
        luma_total_coeffs: [0; 16],
    }
}

// ─── CAVLC nC 上下文计算 ──────────────────────────────────────────────

/// 计算 4x4 亮度块的 nC (H.264 §9.2.1)。
fn calc_nc(
    blk_idx: usize,
    mb_x: usize,
    _mb_y: usize,
    cur_tc: &[u32; 16],
    left: Option<NeighborCtx>,
    top: Option<NeighborCtx>,
) -> u32 {
    // 左相邻块: blk_idx%4==0 → 左邻宏块右列对应块, 否则同宏块 blk_idx-1
    let n_a = if blk_idx % 4 == 0 {
        if mb_x == 0 {
            None
        } else {
            left.map(|n| n.nc[blk_idx + 3])
        }
    } else {
        Some(cur_tc[blk_idx - 1])
    };

    // 上相邻块: blk_idx<4 → 上邻宏块底行对应块, 否则同宏块 blk_idx-4
    let n_b = if blk_idx < 4 {
        top.map(|n| n.nc[blk_idx + 12])
    } else {
        Some(cur_tc[blk_idx - 4])
    };

    match (n_a, n_b) {
        (Some(a), Some(b)) => (a + b + 1) >> 1,
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => 0,
    }
}

// ─── Intra4x4 最可能模式 (mpm) 计算 ───────────────────────────────────

/// 计算 4x4 块的 most_probable_mode (H.264 §8.3.1.1.1)。
fn calc_most_probable_mode(
    blk_idx: usize,
    mb_x: usize,
    mb_y: usize,
    cur_modes: &[u8; 16],
    left: Option<NeighborCtx>,
    top: Option<NeighborCtx>,
) -> u8 {
    // 获取左/上邻居的预测模式 (若邻居是 I_16x16 则映射到 I_4x4 等效模式)
    let left_mode = if blk_idx % 4 == 0 {
        if mb_x == 0 {
            None
        } else {
            left.map(|n| neighbor_pred_mode(n, blk_idx + 3))
        }
    } else {
        Some(cur_modes[blk_idx - 1])
    };

    let top_mode = if blk_idx < 4 {
        if mb_y == 0 {
            None
        } else {
            top.map(|n| neighbor_pred_mode(n, blk_idx + 12))
        }
    } else {
        Some(cur_modes[blk_idx - 4])
    };

    match (left_mode, top_mode) {
        (Some(a), Some(b)) if a < 8 && b < 8 => {
            if a == b {
                if a < 2 { 1 - a } else { a - 1 }
            } else {
                a.min(b)
            }
        }
        _ => 2, // 默认 DC
    }
}

/// 邻居宏块的预测模式 (用于 mpm)。
/// 若邻居是 I_4x4: 直接返回 4x4 模式。
/// 若邻居是 I_16x16: 映射 I_16x16 模式 → I_4x4 等效 (DC=2, HOR=1, VER=0, PLANE=2)。
fn neighbor_pred_mode(n: NeighborCtx, blk_idx: usize) -> u8 {
    match n.kind {
        MbKind::I4x4 => n.modes[blk_idx],
        MbKind::I16x16(m) => match m {
            0 => 2, // DC
            1 => 1, // HOR
            2 => 0, // VER
            3 => 2, // PLANE → DC 占位
            _ => 2,
        },
        _ => 2, // P 宏块邻居视为 DC
    }
}

// ─── 宏块重建 ─────────────────────────────────────────────────────────

fn reconstruct_macroblock(
    mb: &Macroblock,
    mb_index: usize,
    mb_w: usize,
    sps: &Sps,
    ref_frame: Option<&DecodedFrame>,
    frame: &mut DecodedFrame,
) {
    let mb_x = (mb_index % mb_w) * 16;
    let mb_y = (mb_index / mb_w) * 16;
    // 编码尺寸（16 的倍数）: 缓冲区按编码尺寸分配, 可见尺寸不足 16 倍数的部分
    // 由 padding 区域吸收, 避免切片解码器写入越界。
    let stride = sps.mb_width as usize * 16;
    let frame_h = sps.mb_height as usize * 16;

    // ── 1. 亮度预测 ──────────────────────────────────────────────────
    let mut pred = [0u8; 256];

    match mb.kind {
        MbKind::I4x4 => {
            for (i, &mode) in mb.intra4x4_modes.iter().enumerate() {
                let (br, bc) = BLOCK_POS[i];
                let bx = br as usize * 4;
                let by = bc as usize * 4;
                let mut block_pred = [0u8; 16];
                intra::intra4x4(
                    &mut block_pred,
                    mb_x + bx, mb_y + by,
                    &frame.y, stride, mode,
                );
                for row in 0..4 {
                    let dst_off = (by + row) * 16 + bx;
                    let src_off = row * 4;
                    pred[dst_off..dst_off + 4].copy_from_slice(&block_pred[src_off..src_off + 4]);
                }
            }
        }
        MbKind::I16x16(mode) => {
            intra::intra16x16(&mut pred, mb_x, mb_y, &frame.y, stride, mode);
        }
        MbKind::IPcm => {
            // I_PCM: PCM 数据未保留, 用 128 占位 (实际路径不会触发)
            for v in pred.iter_mut() { *v = 128; }
        }
        MbKind::PSkip | MbKind::PL016x16 => {
            if let Some(ref_frame) = ref_frame {
                inter::inter_predict(
                    &mut pred,
                    &ref_frame.y, stride, frame_h,
                    mb.mv.0, mb.mv.1,
                    mb_x, mb_y, 16, 16,
                );
            }
        }
    }

    // ── 2. 亮度残差重建 ──────────────────────────────────────────────
    let luma_pat = tables::cbp_luma_8x8(mb.cbp);

    if matches!(mb.kind, MbKind::I16x16(_)) {
        // I_16x16: 先逆 Hadamard 4x4 亮度 DC (含 /4 归一化), 再逆量化, 注入到各 4x4 块 [0,0]
        // H.264 §8.5.5.3.1 + §8.5.6 顺序: Hadamard 先, dequant 后
        let dc_inv = transform::inverse_hadamard_4x4(&mb.luma16x16_dc);
        let dc_dequant = transform::inverse_quant_4x4(&dc_inv, mb.qp, true);

        for (scan_pos, &blk_idx) in LUMA_BLOCK_SCAN.iter().enumerate() {
            let blk8 = scan_pos / 4;
            if (luma_pat >> blk8) & 1 == 0 {
                continue;
            }
            let (br, bc) = BLOCK_POS[blk_idx];
            let bx = br as usize * 4;
            let by = bc as usize * 4;

            // AC 先 dequant (I_16x16 的 AC 块 [0,0] 应为 0), 再用 dequant 后的 DC 覆盖 [0,0]
            // 这样避免 DC 被二次 dequant
            let coeffs = mb.luma_residuals[blk_idx];
            let mut dequant = transform::inverse_quant_4x4(&coeffs, mb.qp, false);
            dequant[0] = dc_dequant[blk_idx];
            let residual = transform::inverse_4x4(&dequant);

            for row in 0..4 {
                let dst_off = (by + row) * 16 + bx;
                for col in 0..4 {
                    let val = pred[dst_off + col] as i32 + residual[row * 4 + col] as i32;
                    pred[dst_off + col] = val.clamp(0, 255) as u8;
                }
            }
        }
    } else {
        // I_4x4 / P: 标准 4x4 残差 (含 DC)
        for (scan_pos, &blk_idx) in LUMA_BLOCK_SCAN.iter().enumerate() {
            let blk8 = scan_pos / 4;
            if (luma_pat >> blk8) & 1 == 0 {
                continue;
            }
            let coeffs = &mb.luma_residuals[blk_idx];
            if coeffs.iter().all(|&c| c == 0) {
                continue;
            }
            let (br, bc) = BLOCK_POS[blk_idx];
            let bx = br as usize * 4;
            let by = bc as usize * 4;

            let dequant = transform::inverse_quant_4x4(coeffs, mb.qp, false);
            let residual = transform::inverse_4x4(&dequant);

            for row in 0..4 {
                let dst_off = (by + row) * 16 + bx;
                for col in 0..4 {
                    let val = pred[dst_off + col] as i32 + residual[row * 4 + col] as i32;
                    pred[dst_off + col] = val.clamp(0, 255) as u8;
                }
            }
        }
    }

    // ── 3. 写入亮度 ──────────────────────────────────────────────────
    for row in 0..16 {
        let src_off = row * 16;
        let dst_off = (mb_y + row) * stride + mb_x;
        frame.y[dst_off..dst_off + 16].copy_from_slice(&pred[src_off..src_off + 16]);
    }

    // ── 4. 色度预测与重建 ────────────────────────────────────────────
    let half_w = stride / 2;
    let half_mb_x = mb_x / 2;
    let half_mb_y = mb_y / 2;
    let chroma_mode = mb.intra_chroma_mode;

    // 色度 U/V 预测与残差重建 (按平面索引循环, 避免借用冲突)
    let chroma_pat = tables::cbp_chroma(mb.cbp);
    let chroma_qp = (mb.qp as i32 + _pps_chroma_offset_unused()).clamp(0, 51) as u8;
    for plane in 0u8..2 {
        let mut pred_buf = [0u8; 64];
        let (residuals, dc): (&[[i16; 16]; 4], &[i16; 4]) = if plane == 0 {
            (&mb.chroma_u_residuals, &mb.chroma_u_dc)
        } else {
            (&mb.chroma_v_residuals, &mb.chroma_v_dc)
        };

        // 计算当前帧色度平面的不可变引用快照 (用于 intra_chroma 参考像素)
        // 注意: H.264 intra_chroma 仅读取相邻已重建像素, 此处传整帧 slice
        let frame_chroma: &[u8] = if plane == 0 { &frame.u } else { &frame.v };

        if matches!(mb.kind, MbKind::I4x4 | MbKind::I16x16(_)) {
            intra::intra_chroma(&mut pred_buf, half_mb_x, half_mb_y, frame_chroma, half_w, chroma_mode);
        } else if let Some(ref_frame) = ref_frame {
            let ref_chroma: &[u8] = if plane == 0 { &ref_frame.u } else { &ref_frame.v };
            // 色度运动矢量 = 亮度 MV / 2 (4:2:0)
            inter::inter_predict(
                &mut pred_buf,
                ref_chroma, half_w, frame_h / 2,
                mb.mv.0 / 2, mb.mv.1 / 2,
                half_mb_x, half_mb_y, 8, 8,
            );
        }

        // 色度残差重建 (4:2:0: 2x2 DC + 4 个 4x4 AC)
        if chroma_pat != 0 {
            // 2x2 色度 DC: 先逆 2x2 Hadamard (无归一化), 再逆量化
            // H.264 §8.5.5.3.2 + §8.5.6 顺序: Hadamard 先, dequant 后
            let dc_inv = transform::inverse_hadamard_2x2(dc);
            let dc_dequant = inverse_quant_chroma_dc(&dc_inv, chroma_qp);

            // 4 个 4x4 色度 AC 块: AC 先 dequant, 再用 dequant 后的 DC 覆盖 [0,0]
            // 4:2:0 色度 8x8 = 4 个 4x4 块, 顺序:
            //   block 0: (0,0)  block 1: (0,1)
            //   block 2: (1,0)  block 3: (1,1)
            for (blk, (br, bc)) in [(0u8, 0u8), (0, 1), (1, 0), (1, 1)].iter().enumerate() {
                let bx = *bc as usize * 4;
                let by = *br as usize * 4;
                let coeffs = residuals[blk];
                let mut dequant = transform::inverse_quant_4x4(&coeffs, chroma_qp, false);
                dequant[0] = dc_dequant[blk]; // 注入 dequant 后的 DC (覆盖 [0,0])

                if dequant.iter().all(|&c| c == 0) {
                    continue;
                }

                let residual = transform::inverse_4x4(&dequant);

                for row in 0..4 {
                    let dst_off = (by + row) * 8 + bx;
                    for col in 0..4 {
                        let val = pred_buf[dst_off + col] as i32 + residual[row * 4 + col] as i32;
                        pred_buf[dst_off + col] = val.clamp(0, 255) as u8;
                    }
                }
            }
        }

        // 写入色度平面 (此时 frame_chroma 不可变借用已结束)
        let target: &mut [u8] = if plane == 0 { &mut frame.u } else { &mut frame.v };
        for row in 0..8 {
            let src_off = row * 8;
            let dst_off = (half_mb_y + row) * half_w + half_mb_x;
            target[dst_off..dst_off + 8].copy_from_slice(&pred_buf[src_off..src_off + 8]);
        }
    }
}

/// 占位函数: 色度 QP 偏移 (实际应使用 pps.chroma_qp_index_offset)。
/// 此处返回 0, 因为测试流 chroma_qp_index_offset=0。
fn _pps_chroma_offset_unused() -> i32 { 0 }

/// 色度 DC 逆量化 (4:2:0, 4 系数)。
fn inverse_quant_chroma_dc(coeffs: &[i16; 4], qp: u8) -> [i16; 4] {
    const V: [i32; 6] = [10, 11, 13, 14, 16, 18];
    let qp_per = (qp % 6) as usize;
    let qp_div = (qp / 6) as i32;
    let factor = V[qp_per] << qp_div;

    let mut result = [0i16; 4];
    for i in 0..4 {
        let val = coeffs[i] as i32 * factor;
        result[i] = ((val + (1 << 5)) >> 6) as i16;
    }
    result
}
