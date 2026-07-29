//! 纯 Rust 视频解码器包装器
//!
//! 组合 Mp4Demuxer + H264Decoder + YUV→RGBA 色彩转换，
//! 在后台线程中持续解码并将 RGBA 帧写入共享缓冲区。
//! 同时对外暴露播放状态 (Loading/Playing/Error) 与进度，供 UI 展示。

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::video::demux::mp4::Mp4Demuxer;
use crate::video::demux::Demuxer;
use crate::video::h264::h264::H264Decoder;
use crate::video::color::yuv420_to_rgba;

/// 三缓冲池 — 解码线程与 UI 线程之间零拷贝传递 RGBA 帧
///
/// 三个缓冲分时复用：
/// - `ready`：最新解码完成的帧，等 UI 上传纹理
/// - `pool[0]`：UI 已用完归还的缓冲，待解码线程复用
/// - `pool[1]`：解码线程当前正在写入的工作缓冲（不在 Mutex 内，由解码线程独占）
///
/// 每帧路径：
/// 1. 解码线程：从 pool 拿一个 buf，yuv420_to_rgba 写入
/// 2. 解码线程：把 buf 提交为 ready，旧 ready 归还 pool
/// 3. UI 线程：take ready，write_texture 上传 GPU，return_buf 归还
///
/// 关键：每帧零 clone，只搬 Vec 的指针。8MB RGBA 帧不再每帧拷贝。
pub struct FrameSlot {
    inner: Arc<Mutex<FrameSlotInner>>,
}

struct FrameSlotInner {
    /// 已就绪帧，等 UI 消费
    ready: Option<Vec<u8>>,
    /// 空闲缓冲池（UI 归还 / 解码线程 acquire）
    pool: Vec<Vec<u8>>,
}

impl FrameSlot {
    /// 创建池，预分配 2 个 buf 作为空闲缓冲。
    /// 运行时稳态：1 个 ready + 1 个 pool + 1 个解码线程工作 = 3 个，互不冲突。
    pub fn new(buf_size: usize) -> Self {
        let inner = FrameSlotInner {
            ready: None,
            pool: vec![vec![0u8; buf_size], vec![0u8; buf_size]],
        };
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    /// 消费者（UI 线程）：取走 ready 帧，用完必须 return_buf 归还。
    pub fn take(&self) -> Option<Vec<u8>> {
        let mut inner = self.inner.lock().unwrap();
        inner.ready.take()
    }

    /// 消费者：归还用完的帧到 pool。
    pub fn return_buf(&self, buf: Vec<u8>) {
        let mut inner = self.inner.lock().unwrap();
        inner.pool.push(buf);
    }
}

impl Clone for FrameSlot {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// 视频解码状态 — 给 UI 显示用
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderState {
    /// 已启动但首帧尚未解出
    Loading,
    /// 正在播放（首帧已出）
    Playing,
    /// 解码线程遇到不可恢复错误
    Error,
}

/// 状态快照 — 一次性读取，避免多次加锁
#[derive(Debug, Clone, Copy)]
pub struct DecoderStatus {
    pub state: DecoderState,
    /// 当前采样序号 (0-based，不含 SPS/PPS)
    pub current: usize,
    /// 总采样数
    pub total: usize,
}

pub struct VideoDecoder {
    pub width: u32,
    pub height: u32,
    pub frame_slot: FrameSlot,
    pub shutdown: Arc<AtomicBool>,
    pub join_handle: Option<thread::JoinHandle<()>>,
    /// 状态共享给主线程读取
    state: Arc<Mutex<DecoderState>>,
    progress: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
}

impl VideoDecoder {
    pub fn open(path: &Path) -> Result<Self, String> {
        // 1. 用 Mp4Demuxer 打开文件
        let mut demuxer = Mp4Demuxer::from_path(path)?;
        let width = demuxer.width();
        let height = demuxer.height();
        let total = demuxer.sample_count();

        // 2. 创建 H264Decoder
        let mut decoder = H264Decoder::new();

        // 3. 先传入 SPS 和 PPS 初始化解码器
        // Mp4Demuxer::next_sample() 第一次返回 SPS，第二次返回 PPS
        if let Some(sps_sample) = demuxer.next_sample() {
            let _ = decoder.decode(&sps_sample.data);
        }
        if let Some(pps_sample) = demuxer.next_sample() {
            let _ = decoder.decode(&pps_sample.data);
        }

        // 用 SPS 的实际编码尺寸（含裁剪），而非 demuxer 的 tkhd 显示尺寸
        // SPS 尺寸才是 YUV 平面和 RGBA 缓冲区的正确大小
        let width = if decoder.width() > 0 { decoder.width() } else { width };
        let height = if decoder.height() > 0 { decoder.height() } else { height };

        // 4. 创建三缓冲池（每帧 8MB，3 个 = 24MB 上限）
        let frame_size = width as usize * height as usize * 4;
        let frame_slot = FrameSlot::new(frame_size);

        let shutdown = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(DecoderState::Loading));
        let progress = Arc::new(AtomicUsize::new(0));
        let total = Arc::new(AtomicUsize::new(total));

        let slot_clone = frame_slot.inner.clone(); // 共享内部 Mutex
        let shutdown_clone = shutdown.clone();
        let state_clone = state.clone();
        let progress_clone = progress.clone();

        // 5. 启动后台解码线程
        let join_handle = thread::spawn(move || {
            // 解码线程独占的工作缓冲：从 pool 拿一个，写完后 swap 出去
            let mut working = {
                let mut inner = slot_clone.lock().unwrap();
                inner.pool.pop().unwrap_or_else(|| vec![0u8; frame_size])
            };

            loop {
                if shutdown_clone.load(Ordering::Acquire) {
                    break;
                }

                match demuxer.next_sample() {
                    Some(sample) => {
                        // 循环重置时 current_sample 回到 0，UI 进度也跟着回到 0
                        progress_clone.store(demuxer.current_sample_index(), Ordering::Relaxed);

                        match decoder.decode(&sample.data) {
                            Ok(Some(frame)) => {
                                // 输出 rgba 缓冲按可见尺寸分配 (decoder.width × decoder.height × 4),
                                // YUV 平面行跨距为编码宽度 (frame.coded_width, 16 的倍数)。
                                // 仅迭代可见尺寸, 跳过 padding 区域。
                                yuv420_to_rgba(
                                    &frame.y,
                                    &frame.u,
                                    &frame.v,
                                    &mut working,
                                    frame.width,
                                    frame.height,
                                    frame.coded_width,
                                );

                                // 提交 working 为 ready，同时 acquire 一个新 working 继续工作。
                                // 一次 lock 完成两件事，避免重复加锁。
                                // 旧 ready 归还 pool，防止 UI 没及时消费时丢缓冲。
                                let next = {
                                    let mut inner = slot_clone.lock().unwrap();
                                    if let Some(old) = inner.ready.take() {
                                        inner.pool.push(old);
                                    }
                                    inner.ready = Some(working);
                                    inner.pool.pop().unwrap_or_else(|| vec![0u8; frame_size])
                                };
                                working = next;

                                // 首帧已出 → 切到 Playing（一次性，后续保持）
                                if let Ok(mut s) = state_clone.lock() {
                                    if *s == DecoderState::Loading {
                                        *s = DecoderState::Playing;
                                    }
                                }
                            }
                            Ok(None) => {
                                // SPS/PPS/SEI 等非帧数据，跳过
                            }
                            Err(_) => {
                                // 解码错误，跳过当前帧（不致命，状态保持）
                            }
                        }
                    }
                    None => {
                        // 解复用器返回空（理论上不会发生，Mp4Demuxer 内部循环）
                        thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
            }
        });

        Ok(VideoDecoder {
            width,
            height,
            frame_slot,
            shutdown,
            join_handle: Some(join_handle),
            state,
            progress,
            total,
        })
    }

    /// 一次性读取状态快照 — 给 UI 定时刷新用
    pub fn status(&self) -> DecoderStatus {
        let state = self.state.lock()
            .map(|s| *s)
            .unwrap_or(DecoderState::Error);
        let current = self.progress.load(Ordering::Relaxed);
        let total = self.total.load(Ordering::Relaxed);
        DecoderStatus { state, current, total }
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}
