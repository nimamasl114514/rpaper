//! Media Foundation 内置视频解码器
//!
//! 使用 Windows 自带的 MF API 解码视频，无需外部 ffmpeg。
//! 流程: MFStartup → IMFSourceReader → ReadSample → NV12/YUY2 → CPU转RGBA

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::os::windows::ffi::OsStrExt;
use windows::core::{GUID, PCWSTR};
use windows::Win32::Media::MediaFoundation::*;

/// 解码出的 RGBA 帧
pub type FrameSlot = Arc<Mutex<Option<Vec<u8>>>>;

pub struct MfDecoder {
    pub width: u32,
    pub height: u32,
    pub frame_slot: FrameSlot,
    pub _join_handle: thread::JoinHandle<()>,
}

impl MfDecoder {
    pub fn open(path: &Path) -> Result<Self, String> {
        let path_wide: Vec<u16> = path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_LITE)
                .map_err(|e| format!("MFStartup: {e}"))?;
        }

        let reader = unsafe {
            MFCreateSourceReaderFromURL(PCWSTR(path_wide.as_ptr()), None)
                .map_err(|e| format!("MFCreateSourceReaderFromURL: {e}"))?
        };

        let stream_idx = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

        unsafe {
            for i in 0..8u32 {
                let _ = reader.SetStreamSelection(i, false);
            }
            reader.SetStreamSelection(stream_idx, true)
                .map_err(|e| format!("SetStreamSelection: {e}"))?;
        }

        let (width, height) = unsafe {
            let mt = reader.GetNativeMediaType(stream_idx, 0)
                .map_err(|e| format!("GetNativeMediaType: {e}"))?;
            read_dimensions(&mt)
        };

        if width == 0 || height == 0 {
            return Err("无法获取视频分辨率".into());
        }

        // 试 NV12 → YUY2 → RGB32
        let out_subtype = unsafe {
            let nv12 = create_output_type(width, height, MFVideoFormat_NV12);
            match reader.SetCurrentMediaType(stream_idx, None, &nv12) {
                Ok(()) => MFVideoFormat_NV12,
                Err(_) => {
                    let yuy2 = create_output_type(width, height, MFVideoFormat_YUY2);
                    match reader.SetCurrentMediaType(stream_idx, None, &yuy2) {
                        Ok(()) => MFVideoFormat_YUY2,
                        Err(_) => {
                            let rgb32 = create_output_type(width, height, MFVideoFormat_RGB32);
                            reader.SetCurrentMediaType(stream_idx, None, &rgb32)
                                .map_err(|e| format!("SetCurrentMediaType (所有格式均失败): {e}"))?;
                            MFVideoFormat_RGB32
                        }
                    }
                }
            }
        };

        let frame_slot: FrameSlot = Arc::new(Mutex::new(None));
        let slot_clone = frame_slot.clone();
        let frame_size = (width * height * 4) as usize;
        let path_buf = path.to_path_buf();

        let join_handle = thread::spawn(move || {
            unsafe {
                let _ = MFStartup(MF_VERSION, MFSTARTUP_LITE);
            }

            let path_wide: Vec<u16> = path_buf.as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            let reader = match unsafe {
                MFCreateSourceReaderFromURL(PCWSTR(path_wide.as_ptr()), None)
            } {
                Ok(r) => r,
                Err(_) => return,
            };

            let stream_idx = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

            unsafe {
                for i in 0..8u32 {
                    let _ = reader.SetStreamSelection(i, false);
                }
                let _ = reader.SetStreamSelection(stream_idx, true);
            }

            let set_ok = unsafe {
                let nv12 = create_output_type(width, height, MFVideoFormat_NV12);
                if reader.SetCurrentMediaType(stream_idx, None, &nv12).is_ok() {
                    true
                } else {
                    let yuy2 = create_output_type(width, height, MFVideoFormat_YUY2);
                    if reader.SetCurrentMediaType(stream_idx, None, &yuy2).is_ok() {
                        true
                    } else {
                        let rgb32 = create_output_type(width, height, MFVideoFormat_RGB32);
                        reader.SetCurrentMediaType(stream_idx, None, &rgb32).is_ok()
                    }
                }
            };

            if !set_ok { return; }

            // ReadSample 的 6 个参数: stream_index, control_flags, actual_stream_index, stream_flags, timestamp, sample
            // windows 0.52 返回 Result<()>, 通过输出指针拿结果
            let mut actual_stream: u32 = 0;
            let mut stream_flags: u32 = 0;
            let mut timestamp: i64 = 0;

            loop {
                let mut sample_opt: Option<IMFSample> = None;

                let result = unsafe {
                    reader.ReadSample(
                        stream_idx,
                        0, // MF_SOURCE_READER_CONTROL_FLAG_DEFAULT = 0
                        Some(&mut actual_stream),
                        Some(&mut stream_flags),
                        Some(&mut timestamp),
                        Some(&mut sample_opt),
                    )
                };

                if result.is_err() { break; }

                // 检查是否到达 EOF
                // MF_SOURCE_READERF_ENDOFSTREAM = 0x2
                if stream_flags & 0x2 != 0 { break; }

                let sample = match sample_opt {
                    Some(s) => s,
                    None => continue,
                };

                let buffer = unsafe {
                    match sample.ConvertToContiguousBuffer() {
                        Ok(b) => b,
                        Err(_) => continue,
                    }
                };

                let mut data_ptr: *mut u8 = std::ptr::null_mut();
                let mut max_len: u32 = 0;
                let lock_ok = unsafe {
                    buffer.Lock(&mut data_ptr, Some(&mut max_len), None).is_ok()
                };
                if !lock_ok { continue; }

                let raw = unsafe {
                    std::slice::from_raw_parts(data_ptr as *const u8, max_len as usize)
                };

                let rgba = convert_to_rgba(raw, width, height, &out_subtype);

                unsafe { let _ = buffer.Unlock(); }

                if rgba.len() == frame_size {
                    let mut slot = slot_clone.lock().unwrap();
                    *slot = Some(rgba);
                }
            }

            unsafe { let _ = MFShutdown(); }
        });

        Ok(Self {
            width,
            height,
            frame_slot,
            _join_handle: join_handle,
        })
    }
}

/// 从 IMFMediaType 读取宽高
/// MFGetAttributeSize 在 0.52 中不可用，用 GetUINT64 手动解析
unsafe fn read_dimensions(mt: &IMFMediaType) -> (u32, u32) {
    // MF_MT_FRAME_SIZE 存为 UINT64: (height << 32) | width
    let packed = mt.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
    let width = (packed & 0xFFFFFFFF) as u32;
    let height = (packed >> 32) as u32;
    (width, height)
}

/// 创建输出 MediaType
unsafe fn create_output_type(width: u32, height: u32, subtype: GUID) -> IMFMediaType {
    let mt = MFCreateMediaType().unwrap();
    let _ = mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video);
    let _ = mt.SetGUID(&MF_MT_SUBTYPE, &subtype);

    // MFSetAttributeSize: pack (height << 32) | width into UINT64
    let packed = (height as u64) << 32 | width as u64;
    let _ = mt.SetUINT64(&MF_MT_FRAME_SIZE, packed);

    // MFSetAttributeRatio: pack (denominator << 32) | numerator
    let fps_packed = (1u64) << 32 | 30u64;
    let _ = mt.SetUINT64(&MF_MT_FRAME_RATE, fps_packed);

    let _ = mt.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32);
    let _ = mt.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64) << 32 | 1u64);
    mt
}

fn convert_to_rgba(raw: &[u8], width: u32, height: u32, subtype: &GUID) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let rgba_size = w * h * 4;
    let mut rgba = vec![0u8; rgba_size];

    if *subtype == MFVideoFormat_RGB32 {
        for i in 0..(w * h) {
            let src = i * 4;
            let dst = i * 4;
            if src + 3 < raw.len() {
                rgba[dst]     = raw[src + 2]; // R
                rgba[dst + 1] = raw[src + 1]; // G
                rgba[dst + 2] = raw[src];     // B
                rgba[dst + 3] = 255;          // A
            }
        }
    } else if *subtype == MFVideoFormat_NV12 {
        let y_plane = &raw[..w * h];
        let uv_plane = &raw[w * h..];

        for j in 0..h {
            for i in 0..w {
                let y = y_plane[j * w + i] as f32;
                let uv_idx = (j / 2) * w + (i / 2) * 2;
                let u = uv_plane[uv_idx] as f32 - 128.0;
                let v = uv_plane[uv_idx + 1] as f32 - 128.0;

                let r = (y + 1.402 * v).clamp(0.0, 255.0) as u8;
                let g = (y - 0.344 * u - 0.714 * v).clamp(0.0, 255.0) as u8;
                let b = (y + 1.772 * u).clamp(0.0, 255.0) as u8;

                let dst = (j * w + i) * 4;
                rgba[dst] = r;
                rgba[dst + 1] = g;
                rgba[dst + 2] = b;
                rgba[dst + 3] = 255;
            }
        }
    } else if *subtype == MFVideoFormat_YUY2 {
        for j in 0..h {
            for i in (0..w).step_by(2) {
                let src = (j * w + i) * 2;
                if src + 3 >= raw.len() { break; }
                let y0 = raw[src] as f32;
                let u = raw[src + 1] as f32 - 128.0;
                let y1 = raw[src + 2] as f32;
                let v = raw[src + 3] as f32 - 128.0;

                let r0 = (y0 + 1.402 * v).clamp(0.0, 255.0) as u8;
                let g0 = (y0 - 0.344 * u - 0.714 * v).clamp(0.0, 255.0) as u8;
                let b0 = (y0 + 1.772 * u).clamp(0.0, 255.0) as u8;

                let dst0 = (j * w + i) * 4;
                rgba[dst0] = r0;
                rgba[dst0 + 1] = g0;
                rgba[dst0 + 2] = b0;
                rgba[dst0 + 3] = 255;

                if i + 1 < w {
                    let r1 = (y1 + 1.402 * v).clamp(0.0, 255.0) as u8;
                    let g1 = (y1 - 0.344 * u - 0.714 * v).clamp(0.0, 255.0) as u8;
                    let b1 = (y1 + 1.772 * u).clamp(0.0, 255.0) as u8;
                    let dst1 = (j * w + (i + 1)) * 4;
                    rgba[dst1] = r1;
                    rgba[dst1 + 1] = g1;
                    rgba[dst1 + 2] = b1;
                    rgba[dst1 + 3] = 255;
                }
            }
        }
    } else {
        for chunk in rgba.chunks_exact_mut(4) {
            chunk[0] = 0;
            chunk[1] = 0;
            chunk[2] = 0;
            chunk[3] = 255;
        }
    }

    rgba
}
