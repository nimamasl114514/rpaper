use super::sps::Sps;
use super::pps::Pps;
use super::slice;
use super::frame::DecodedFrame;
use super::nal;
use super::deblock;

/// H.264 解码器
pub struct H264Decoder {
    sps: Option<Sps>,
    pps: Option<Pps>,
    ref_frame: Option<DecodedFrame>,  // 参考帧（P 帧使用）
    width: u32,
    height: u32,
}

impl H264Decoder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        H264Decoder {
            sps: None,
            pps: None,
            ref_frame: None,
            width: 0,
            height: 0,
        }
    }

    /// 解码一个 Annex B 格式的 NAL 数据块（可能包含 SPS + PPS + 切片）
    /// 返回解码后的帧（如果是 IDR 或 P 帧）
    pub fn decode(&mut self, data: &[u8]) -> Result<Option<DecodedFrame>, String> {
        let nals = nal::extract_nals(data);
        let mut frame = None;

        for nal_data in &nals {
            let nal_type = nal::nal_unit_type(nal_data);
            match nal_type {
                nal::NAL_SPS => {
                    self.sps = Some(Sps::parse(nal_data)?);
                    if let Some(ref sps) = self.sps {
                        self.width = sps.width;
                        self.height = sps.height;
                    }
                }
                nal::NAL_PPS => {
                    self.pps = Some(Pps::parse(nal_data)?);
                }
                nal::NAL_SLICE_IDR => {
                    // IDR 帧：清除参考帧
                    self.ref_frame = None;
                    let sps = self.sps.as_ref().ok_or("缺少 SPS")?;
                    let pps = self.pps.as_ref().ok_or("缺少 PPS")?;
                    // 编码尺寸 (16 的倍数) 用于 YUV 缓冲分配与切片解码器写入;
                    // 可见尺寸 (裁剪后) 用于输出色彩转换。
                    let coded_w = sps.mb_width as usize * 16;
                    let coded_h = sps.mb_height as usize * 16;
                    let mut decoded = DecodedFrame::new(
                        coded_w, coded_h,
                        sps.width as usize, sps.height as usize,
                    );
                    let header = slice::decode_slice(nal_data, nal_type, sps, pps, None, &mut decoded)?;

                    // 去块滤波: 必须处理所有编码宏块（含 padding 区域）,
                    // 否则边界滤波会漏掉最后一行/列宏块导致边缘伪影。
                    deblock::deblock_frame(
                        &mut decoded.y, &mut decoded.u, &mut decoded.v,
                        decoded.coded_width, decoded.coded_height,
                        (pps.pic_init_qp_minus26 + 26 + header.slice_qp_delta) as u8,
                        header.slice_alpha_c0_offset_div2,
                        header.slice_beta_offset_div2,
                    );

                    self.ref_frame = Some(DecodedFrame {
                        y: decoded.y.clone(),
                        u: decoded.u.clone(),
                        v: decoded.v.clone(),
                        coded_width: decoded.coded_width,
                        coded_height: decoded.coded_height,
                        width: decoded.width,
                        height: decoded.height,
                    });
                    frame = Some(decoded);
                }
                nal::NAL_SLICE_NON_IDR => {
                    // P 帧
                    let sps = self.sps.as_ref().ok_or("缺少 SPS")?;
                    let pps = self.pps.as_ref().ok_or("缺少 PPS")?;
                    let coded_w = sps.mb_width as usize * 16;
                    let coded_h = sps.mb_height as usize * 16;
                    let mut decoded = DecodedFrame::new(
                        coded_w, coded_h,
                        sps.width as usize, sps.height as usize,
                    );
                    let header = slice::decode_slice(
                        nal_data, nal_type, sps, pps,
                        self.ref_frame.as_ref(),
                        &mut decoded,
                    )?;

                    deblock::deblock_frame(
                        &mut decoded.y, &mut decoded.u, &mut decoded.v,
                        decoded.coded_width, decoded.coded_height,
                        (pps.pic_init_qp_minus26 + 26 + header.slice_qp_delta) as u8,
                        header.slice_alpha_c0_offset_div2,
                        header.slice_beta_offset_div2,
                    );

                    self.ref_frame = Some(DecodedFrame {
                        y: decoded.y.clone(),
                        u: decoded.u.clone(),
                        v: decoded.v.clone(),
                        coded_width: decoded.coded_width,
                        coded_height: decoded.coded_height,
                        width: decoded.width,
                        height: decoded.height,
                    });
                    frame = Some(decoded);
                }
                _ => {} // SEI, AUD 等跳过
            }
        }

        Ok(frame)
    }

    #[allow(dead_code)]
    pub fn width(&self) -> u32 { self.width }
    #[allow(dead_code)]
    pub fn height(&self) -> u32 { self.height }
}