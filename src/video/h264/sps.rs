use crate::video::h264::bitstream::BitReader;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Sps {
    pub profile_idc: u8,
    pub level_idc: u8,
    pub seq_parameter_set_id: u32,
    pub chroma_format_idc: u32,
    pub log2_max_frame_num_minus4: u32,
    pub pic_order_cnt_type: u32,
    pub log2_max_pic_order_cnt_lsb_minus4: Option<u32>,
    pub num_ref_frames: u32,
    pub gaps_in_frame_num_value_allowed_flag: bool,
    pub pic_width_in_mbs_minus1: u32,
    pub pic_height_in_map_units_minus1: u32,
    pub frame_mbs_only_flag: bool,
    pub mb_width: u32,
    pub mb_height: u32,
    pub width: u32,
    pub height: u32,
    pub direct_8x8_inference_flag: bool,
    pub crop_left: u32,
    pub crop_right: u32,
    pub crop_top: u32,
    pub crop_bottom: u32,
}

impl Sps {
    pub fn parse(nal_data: &[u8]) -> Result<Self, String> {
        let mut br = BitReader::new(&nal_data[1..]);

        let profile_idc = br.read_bits(8) as u8;
        let _constraint_set0 = br.read_bit();
        let _constraint_set1 = br.read_bit();
        let _constraint_set2 = br.read_bit();
        let _constraint_set3 = br.read_bit();
        let _constraint_set4 = br.read_bit();
        let _constraint_set5 = br.read_bit();
        let _reserved = br.read_bits(2);
        let level_idc = br.read_bits(8) as u8;
        let seq_parameter_set_id = br.read_ue();

        let chroma_format_idc = if profile_idc == 100
            || profile_idc == 110
            || profile_idc == 122
            || profile_idc == 244
            || profile_idc == 44
            || profile_idc == 83
            || profile_idc == 86
            || profile_idc == 118
            || profile_idc == 128
        {
            let chroma = br.read_ue();
            if chroma == 3 {
                let _separate_colour_plane_flag = br.read_bit();
            }
            let _bit_depth_luma = br.read_ue();
            let _bit_depth_chroma = br.read_ue();
            let _qpprime_y_zero_transform_bypass = br.read_bool();
            let _seq_scaling_matrix_present_flag = br.read_bool();
            chroma
        } else {
            1
        };

        let log2_max_frame_num_minus4 = br.read_ue();
        let pic_order_cnt_type = br.read_ue();
        let log2_max_pic_order_cnt_lsb_minus4 = if pic_order_cnt_type == 0 {
            Some(br.read_ue())
        } else {
            None
        };
        let num_ref_frames = br.read_ue();
        let gaps_in_frame_num_value_allowed_flag = br.read_bool();
        let pic_width_in_mbs_minus1 = br.read_ue();
        let pic_height_in_map_units_minus1 = br.read_ue();
        let frame_mbs_only_flag = br.read_bool();

        let mb_width = pic_width_in_mbs_minus1 + 1;
        let mb_height =
            (pic_height_in_map_units_minus1 + 1) * (2 - frame_mbs_only_flag as u32);

        // H.264 §7.3.2.1.1: mb_adaptive_frame_field_flag 仅在 !frame_mbs_only_flag 时存在;
        // direct_8x8_inference_flag 总是存在 (1 bit)。
        if !frame_mbs_only_flag {
            let _mb_adaptive_frame_field_flag = br.read_bool();
        }
        let direct_8x8_inference_flag = br.read_bool();

        let mut crop_left = 0u32;
        let mut crop_right = 0u32;
        let mut crop_top = 0u32;
        let mut crop_bottom = 0u32;
        let frame_cropping_flag = br.read_bool();
        if frame_cropping_flag {
            let crop_left_ue = br.read_ue();
            let crop_right_ue = br.read_ue();
            let crop_top_ue = br.read_ue();
            let crop_bottom_ue = br.read_ue();
            // H.264 §7.4.2.1.1 crop 公式:
            //   crop_unit_x = sub_width_c
            //   crop_unit_y = sub_height_c × (2 − frame_mbs_only_flag)
            let sub_w_c = if chroma_format_idc == 0 { 1 } else { 2 };
            let sub_h_c = if chroma_format_idc == 0 { 1 } else { 2 };
            let crop_unit_x = sub_w_c;
            let crop_unit_y = sub_h_c * (2 - frame_mbs_only_flag as u32);
            crop_left = crop_left_ue * crop_unit_x;
            crop_right = crop_right_ue * crop_unit_x;
            crop_top = crop_top_ue * crop_unit_y;
            crop_bottom = crop_bottom_ue * crop_unit_y;
        }

        let width = mb_width * 16 - crop_left - crop_right;
        let height = mb_height * 16 - crop_top - crop_bottom;

        Ok(Sps {
            profile_idc,
            level_idc,
            seq_parameter_set_id,
            chroma_format_idc,
            log2_max_frame_num_minus4,
            pic_order_cnt_type,
            log2_max_pic_order_cnt_lsb_minus4,
            num_ref_frames,
            gaps_in_frame_num_value_allowed_flag,
            pic_width_in_mbs_minus1,
            pic_height_in_map_units_minus1,
            frame_mbs_only_flag,
            mb_width,
            mb_height,
            width,
            height,
            direct_8x8_inference_flag,
            crop_left,
            crop_right,
            crop_top,
            crop_bottom,
        })
    }
}