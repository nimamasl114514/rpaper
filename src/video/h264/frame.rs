/// 解码后的视频帧（YUV 4:2:0 planar）
///
/// YUV 平面按**编码尺寸**（mb_width*16 × mb_height*16, 恒为 16 的倍数）分配,
/// 因为 H.264 切片解码器需对每个宏块（含 padding 区域）写入残差与预测。
/// 输出阶段（YUV→RGBA）仅迭代 `visible_width × visible_height` 区域。
pub struct DecodedFrame {
    pub y: Vec<u8>,      // 亮度平面 (coded_width * coded_height)
    pub u: Vec<u8>,      // 色度 U 平面 (coded_width/2 * coded_height/2)
    pub v: Vec<u8>,      // 色度 V 平面 (coded_width/2 * coded_height/2)
    /// 编码尺寸（16 的倍数）, 用于切片解码器写入和去块滤波
    pub coded_width: usize,
    pub coded_height: usize,
    /// 可见尺寸（裁剪后）, 用于输出色彩转换
    pub width: usize,
    pub height: usize,
}

impl DecodedFrame {
    /// 创建新的解码帧。
    ///
    /// * `coded_width` / `coded_height` - 编码尺寸（必须为 16 的倍数）
    /// * `visible_width` / `visible_height` - 裁剪后的可见尺寸
    pub fn new(coded_width: usize, coded_height: usize, visible_width: usize, visible_height: usize) -> Self {
        DecodedFrame {
            y: vec![128u8; coded_width * coded_height],
            u: vec![128u8; coded_width / 2 * coded_height / 2],
            v: vec![128u8; coded_width / 2 * coded_height / 2],
            coded_width,
            coded_height,
            width: visible_width,
            height: visible_height,
        }
    }
}
