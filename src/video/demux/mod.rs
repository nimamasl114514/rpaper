pub mod mp4;

pub struct DemuxedSample {
    pub data: Vec<u8>,
    #[allow(dead_code)]
    pub timestamp: u64,
}

#[allow(dead_code)]
pub trait Demuxer {
    fn next_sample(&mut self) -> Option<DemuxedSample>;
    fn duration_ms(&self) -> u64;
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    /// 视频采样总数（不含 SPS/PPS），用于进度展示
    fn sample_count(&self) -> usize;
    /// 当前已读采样序号（0-based，不含 SPS/PPS）
    fn current_sample_index(&self) -> usize;
}
