//! 视频解码器包装器
//!
//! 组合 Mp4Demuxer + openh264 Decoder + YUV→RGBA 色彩转换，
//! 在后台线程中持续解码并将 RGBA 帧写入共享缓冲区。
//! 同时对外暴露播放状态 (Loading/Playing/Error) 与进度，供 UI 展示。

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use openh264::decoder::Decoder as OpenH264Decoder;
use openh264::formats::YUVSource;
use openh264::nal_units;

use crate::video::demux::mp4::Mp4Demuxer;
use crate::video::demux::Demuxer;
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

        // 2. 创建 openh264 解码器
        let mut decoder = OpenH264Decoder::new()
            .map_err(|e| format!("openh264 初始化失败: {e}"))?;

        // 3. 先传入 SPS 和 PPS 初始化解码器
        // Mp4Demuxer::next_sample() 第一次返回 SPS，第二次返回 PPS
        // openh264 会自动识别 NAL 类型并缓存 SPS/PPS
        let mut real_width = width;
        let mut real_height = height;
        if let Some(sps_sample) = demuxer.next_sample() {
            for nal in nal_units(&sps_sample.data) {
                if let Ok(Some(yuv)) = decoder.decode(nal) {
                    // SPS 解析后能拿到实际尺寸
                    let (w, h) = yuv.dimensions();
                    real_width = w as u32;
                    real_height = h as u32;
                }
            }
        }
        if let Some(pps_sample) = demuxer.next_sample() {
            for nal in nal_units(&pps_sample.data) {
                let _ = decoder.decode(nal);
            }
        }

        let width = if real_width > 0 { real_width } else { width };
        let height = if real_height > 0 { real_height } else { height };

        // 4. 创建三缓冲池（每帧 8MB，3 个 = 24MB 上限）
        let frame_size = width as usize * height as usize * 4;
        let frame_slot = FrameSlot::new(frame_size);

        let shutdown = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(DecoderState::Loading));
        let progress = Arc::new(AtomicUsize::new(0));
        let total = Arc::new(AtomicUsize::new(total));

        let slot_clone = frame_slot.inner.clone();
        let shutdown_clone = shutdown.clone();
        let state_clone = state.clone();
        let progress_clone = progress.clone();

        // 5. 启动后台解码线程
        let join_handle = thread::spawn(move || {
            // 解码线程独占的工作缓冲
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
                        progress_clone.store(demuxer.current_sample_index(), Ordering::Relaxed);

                        // openh264 decode 返回 Result<Option<YUVBuffer>>
                        // sample.data 可能含多个 NAL，逐个送入
                        let mut got_frame = false;
                        for nal in nal_units(&sample.data) {
                            match decoder.decode(nal) {
                                Ok(Some(yuv)) => {
                                    got_frame = true;
                                    let (w, h) = yuv.dimensions();
                                    let (y_stride, _u_stride, _v_stride) = yuv.strides();

                                    // YUV420 → RGBA，stride 由 openh264 决定
                                    yuv420_to_rgba(
                                        yuv.y(),
                                        yuv.u(),
                                        yuv.v(),
                                        &mut working,
                                        w,
                                        h,
                                        y_stride,
                                    );

                                    // 提交 working 为 ready，拿一个新 working
                                    let next = {
                                        let mut inner = slot_clone.lock().unwrap();
                                        if let Some(old) = inner.ready.take() {
                                            inner.pool.push(old);
                                        }
                                        inner.ready = Some(working);
                                        inner.pool.pop().unwrap_or_else(|| vec![0u8; frame_size])
                                    };
                                    working = next;

                                    // 首帧已出 → Playing
                                    if let Ok(mut s) = state_clone.lock() {
                                        if *s == DecoderState::Loading {
                                            *s = DecoderState::Playing;
                                        }
                                    }
                                }
                                Ok(None) => {
                                    // SPS/PPS/SEI 等，跳过
                                }
                                Err(_) => {
                                    // 单帧解码错误不致命
                                }
                            }
                        }

                        let _ = got_frame; // 不需要额外处理
                    }
                    None => {
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
