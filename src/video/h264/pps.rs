use crate::video::h264::bitstream::BitReader;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Pps {
    pub pic_parameter_set_id: u32,
    pub seq_parameter_set_id: u32,
    pub entropy_coding_mode_flag: bool,
    pub pic_order_present_flag: bool,
    pub num_slice_groups_minus1: u32,
    pub num_ref_idx_l0_default_active_minus1: u32,
    pub num_ref_idx_l1_default_active_minus1: u32,
    pub weighted_pred_flag: bool,
    pub weighted_bipred_idc: u32,
    pub pic_init_qp_minus26: i32,
    pub pic_init_qs_minus26: i32,
    pub chroma_qp_index_offset: i32,
    pub deblocking_filter_control_present_flag: bool,
    pub constrained_intra_pred_flag: bool,
    pub redundant_pic_cnt_present_flag: bool,
}

impl Pps {
    pub fn parse(nal_data: &[u8]) -> Result<Self, String> {
        let mut br = BitReader::new(&nal_data[1..]);

        let pic_parameter_set_id = br.read_ue();
        let seq_parameter_set_id = br.read_ue();
        let entropy_coding_mode_flag = br.read_bool();
        let pic_order_present_flag = br.read_bool();
        let num_slice_groups_minus1 = br.read_ue();

        if num_slice_groups_minus1 > 0 {
            let slice_group_map_type = br.read_ue();
            if slice_group_map_type == 0 {
                for _ in 0..=num_slice_groups_minus1 {
                    let _run_length = br.read_ue();
                }
            } else if slice_group_map_type == 2 {
                for _ in 0..=num_slice_groups_minus1 {
                    let _top_left = br.read_ue();
                    let _bottom_right = br.read_ue();
                }
            } else if slice_group_map_type == 3
                || slice_group_map_type == 4
                || slice_group_map_type == 5
            {
                let _slice_group_change_direction_flag = br.read_bool();
                let _slice_group_change_rate_minus1 = br.read_ue();
            } else if slice_group_map_type == 6 {
                let pic_size_in_map_units_minus1 = br.read_ue();
                let n = ((pic_size_in_map_units_minus1 + 1) as f64)
                    .log2()
                    .ceil() as u32;
                for _ in 0..=num_slice_groups_minus1 {
                    let _ = br.read_bits(n as u8);
                }
            }
        }

        let num_ref_idx_l0_default_active_minus1 = br.read_ue();
        let num_ref_idx_l1_default_active_minus1 = br.read_ue();
        let weighted_pred_flag = br.read_bool();
        let weighted_bipred_idc = br.read_bits(2);
        let pic_init_qp_minus26 = br.read_se();
        let pic_init_qs_minus26 = br.read_se();
        let chroma_qp_index_offset = br.read_se();
        let deblocking_filter_control_present_flag = br.read_bool();
        let constrained_intra_pred_flag = br.read_bool();
        let redundant_pic_cnt_present_flag = br.read_bool();

        Ok(Pps {
            pic_parameter_set_id,
            seq_parameter_set_id,
            entropy_coding_mode_flag,
            pic_order_present_flag,
            num_slice_groups_minus1,
            num_ref_idx_l0_default_active_minus1,
            num_ref_idx_l1_default_active_minus1,
            weighted_pred_flag,
            weighted_bipred_idc,
            pic_init_qp_minus26,
            pic_init_qs_minus26,
            chroma_qp_index_offset,
            deblocking_filter_control_present_flag,
            constrained_intra_pred_flag,
            redundant_pic_cnt_present_flag,
        })
    }
}