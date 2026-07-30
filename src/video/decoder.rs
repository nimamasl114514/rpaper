//! 视频解码器包装器
//!
//! 组合 Mp4Demuxer + openh264 Decoder，在后台线程中持续解码。
//! 解码后 YUV 数据打包写入共享缓冲（去掉 stride padding），
//! UI 线程直接上传 GPU 做 YUV→RGB 转换，零 CPU 色彩转换。
//!
//! 大封小切割策略：
//! - 大封: YUV 三平面打包成一个紧凑 Vec（无 stride padding）
//! - 小切割: 上传 GPU 时按条带分块 write_texture，降低单次 PCIe 阻塞

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use openh264::decoder::Decoder as OpenH264Decoder;
use openh264::formats::YUVSource;
use openh264::nal_units;

use crate::video::demux::mp4::Mp4Demuxer;
use crate::video::demux::Demuxer;

/// 双缓冲池 — 解码线程与 UI 线程之间传递打包 YUV 帧
///
/// YUV 打包格式 (I420 planar, 无 stride)：
/// ```text
/// offset 0:              Y 平面  (width * height 字节)
/// offset y_len:          U 平面  (width/2 * height/2 字节)
/// offset y_len + uv_len: V 平面  (width/2 * height/2 字节)
/// total: width * height * 3 / 2 字节
/// ```
///
/// 双缓冲足够：解码速度通常 >> 播放速度，不需要三缓冲。
/// 1080p 每帧 3.1MB × 2 = 6.2MB（比之前三缓冲 RGBA 24.9MB 省 75%）。
pub struct FrameSlot {
    inner: Arc<Mutex<FrameSlotInner>>,
}

struct FrameSlotInner {
    /// 已就绪帧，等 UI 消费
    ready: Option<Vec<u8>>,
    /// 空闲缓冲池
    pool: Vec<Vec<u8>>,
}

impl FrameSlot {
    pub fn new(buf_size: usize) -> Self {
        let inner = FrameSlotInner {
            ready: None,
            // 双缓冲：1 个 pool + 1 个解码线程工作 = 2 个
            pool: vec![vec![0u8; buf_size]],
        };
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    /// UI 线程：取走 ready 帧
    pub fn take(&self) -> Option<Vec<u8>> {
        self.inner.lock().unwrap().ready.take()
    }

    /// UI 线程：归还用完的帧到 pool
    pub fn return_buf(&self, buf: Vec<u8>) {
        self.inner.lock().unwrap().pool.push(buf);
    }
}

impl Clone for FrameSlot {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

/// 视频解码状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderState {
    Loading,
    Playing,
    Error,
}

#[derive(Debug, Clone, Copy)]
pub struct DecoderStatus {
    pub state: DecoderState,
    pub current: usize,
    pub total: usize,
}

pub struct VideoDecoder {
    pub width: u32,
    pub height: u32,
    pub frame_slot: FrameSlot,
    pub shutdown: Arc<AtomicBool>,
    pub join_handle: Option<thread::JoinHandle<()>>,
    state: Arc<Mutex<DecoderState>>,
    progress: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
}

impl VideoDecoder {
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut demuxer = Mp4Demuxer::from_path(path)?;
        let width = demuxer.width();
        let height = demuxer.height();
        let total = demuxer.sample_count();

        let mut decoder = OpenH264Decoder::new()
            .map_err(|e| format!("openh264 初始化失败: {e}"))?;

        // 传入 SPS/PPS 初始化解码器
        let mut real_width = width;
        let mut real_height = height;
        if let Some(sps_sample) = demuxer.next_sample() {
            for nal in nal_units(&sps_sample.data) {
                if let Ok(Some(yuv)) = decoder.decode(nal) {
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

        // YUV420 打包大小 = w*h*3/2（比 RGBA w*h*4 省 62.5%）
        let y_len = width as usize * height as usize;
        let uv_len = y_len / 4;
        let frame_size = y_len + uv_len * 2;
        let frame_slot = FrameSlot::new(frame_size);

        let shutdown = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(DecoderState::Loading));
        let progress = Arc::new(AtomicUsize::new(0));
        let total = Arc::new(AtomicUsize::new(total));

        let slot_clone = frame_slot.inner.clone();
        let shutdown_clone = shutdown.clone();
        let state_clone = state.clone();
        let progress_clone = progress.clone();
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

                        for nal in nal_units(&sample.data) {
                            match decoder.decode(nal) {
                                Ok(Some(yuv)) => {
                                    let (w, h) = yuv.dimensions();
                                    let (y_stride, u_stride, v_stride) = yuv.strides();
                                    let w = w as usize;
                                    let h = h as usize;

                                    let y_len = w * h;
                                    let uv_w = w / 2;
                                    let uv_h = h / 2;
                                    let uv_len = uv_w * uv_h;

                                    // 大封：YUV 三平面打包到紧凑缓冲（去掉 stride padding）
                                    // 逐行拷贝，消除 openh264 的 stride 对齐
                                    let y_src = yuv.y();
                                    let u_src = yuv.u();
                                    let v_src = yuv.v();

                                    // Y 平面
                                    for row in 0..h {
                                        let src_off = row * y_stride;
                                        let dst_off = row * w;
                                        working[dst_off..dst_off + w]
                                            .copy_from_slice(&y_src[src_off..src_off + w]);
                                    }
                                    // U 平面（半分辨率）
                                    for row in 0..uv_h {
                                        let src_off = row * u_stride;
                                        let dst_off = y_len + row * uv_w;
                                        working[dst_off..dst_off + uv_w]
                                            .copy_from_slice(&u_src[src_off..src_off + uv_w]);
                                    }
                                    // V 平面（半分辨率）
                                    for row in 0..uv_h {
                                        let src_off = row * v_stride;
                                        let dst_off = y_len + uv_len + row * uv_w;
                                        working[dst_off..dst_off + uv_w]
                                            .copy_from_slice(&v_src[src_off..src_off + uv_w]);
                                    }

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

                                    if let Ok(mut s) = state_clone.lock() {
                                        if *s == DecoderState::Loading {
                                            *s = DecoderState::Playing;
                                        }
                                    }
                                }
                                Ok(None) => {} // SPS/PPS/SEI
                                Err(_) => {}   // 单帧错误不致命
                            }
                        }
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
