//! Media Foundation 内置视频解码器 (v3)
//!
//! 核心思路: MF 报告的 width/height 可能与 buffer 实际布局不一致
//! (有旋转元数据时宽高互换，H.264 宏块对齐导致高度也有 padding)。
//! 所以不信任 MF 报告的尺寸，直接从 buffer 长度反推:
//!   NV12: buf_len = stride * height * 1.5
//!   → stride = 从 MF 报告的两个维度中找能整除的那个
//!   → height = buf_len / (stride * 1.5)
//! 再用行相关性验证哪个维度是真正的 stride。

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::os::windows::ffi::OsStrExt;
use windows::core::{GUID, PCWSTR};
use windows::Win32::Media::MediaFoundation::*;

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
                .map_err(|e| format!("MFStartup 失败: {e}"))?;
        }

        let reader = unsafe {
            MFCreateSourceReaderFromURL(PCWSTR(path_wide.as_ptr()), None)
                .map_err(|e| format!("无法打开视频文件: {e}"))?
        };

        let stream_idx = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

        unsafe {
            for i in 0..8u32 {
                let _ = reader.SetStreamSelection(i, false);
            }
            reader.SetStreamSelection(stream_idx, true)
                .map_err(|e| format!("SetStreamSelection 失败: {e}"))?;
        }

        // 获取 MF 报告的尺寸 (可能是旋转后的)
        let (reported_w, reported_h) = unsafe {
            let mt = reader.GetNativeMediaType(stream_idx, 0)
                .map_err(|e| format!("GetNativeMediaType 失败: {e}"))?;
            let packed = mt.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
            ((packed & 0xFFFFFFFF) as u32, (packed >> 32) as u32)
        };

        if reported_w == 0 || reported_h == 0 {
            return Err("无法获取视频分辨率".into());
        }

        // 设置 NV12 输出
        let out_subtype = unsafe {
            let nv12 = create_output_type(reported_w, reported_h, MFVideoFormat_NV12);
            match reader.SetCurrentMediaType(stream_idx, None, &nv12) {
                Ok(()) => MFVideoFormat_NV12,
                Err(_) => {
                    let yuy2 = create_output_type(reported_w, reported_h, MFVideoFormat_YUY2);
                    match reader.SetCurrentMediaType(stream_idx, None, &yuy2) {
                        Ok(()) => MFVideoFormat_YUY2,
                        Err(_) => {
                            let rgb32 = create_output_type(reported_w, reported_h, MFVideoFormat_RGB32);
                            reader.SetCurrentMediaType(stream_idx, None, &rgb32)
                                .map_err(|_| "视频解码器初始化失败: 不支持 NV12/YUY2/RGB32 输出格式".to_string())?;
                            MFVideoFormat_RGB32
                        }
                    }
                }
            }
        };

        // 读几帧跳过可能的损坏帧，拿到一个稳定的关键帧
        let mut last_buf: Vec<u8> = Vec::new();
        for _ in 0..15 {
            let mut actual_stream: u32 = 0;
            let mut stream_flags: u32 = 0;
            let mut timestamp: i64 = 0;
            let mut sample_opt: Option<IMFSample> = None;

            let r = unsafe {
                reader.ReadSample(
                    stream_idx, 0,
                    Some(&mut actual_stream),
                    Some(&mut stream_flags),
                    Some(&mut timestamp),
                    Some(&mut sample_opt),
                )
            };
            if r.is_err() { break; }
            if stream_flags & 0x2 != 0 { break; } // EOF

            if let Some(sample) = sample_opt {
                if let Ok(buffer) = unsafe { sample.ConvertToContiguousBuffer() } {
                    let mut p: *mut u8 = std::ptr::null_mut();
                    let mut mx: u32 = 0;
                    let mut cl: u32 = 0;
                    if unsafe { buffer.Lock(&mut p, Some(&mut mx), Some(&mut cl)) }.is_ok() {
                        let slice = unsafe { std::slice::from_raw_parts(p, cl as usize) };
                        last_buf = slice.to_vec();
                        unsafe { let _ = buffer.Unlock(); }
                    }
                }
            }
        }

        if last_buf.is_empty() {
            return Err("无法读取视频帧数据".into());
        }

        // 从 buffer 数据反推真实布局
        let (real_w, real_h, stride) = detect_layout(&last_buf, reported_w, reported_h, &out_subtype);

        eprintln!("[MF] 报告尺寸: {}x{}, 实际布局: {}x{} (stride={}), buf_len={}",
            reported_w, reported_h, real_w, real_h, stride, last_buf.len());

        let frame_slot: FrameSlot = Arc::new(Mutex::new(None));
        let slot_clone = frame_slot.clone();
        let path_buf = path.to_path_buf();
        let frame_size = (real_w * real_h * 4) as usize;

        let join_handle = thread::spawn(move || {
            unsafe { let _ = MFStartup(MF_VERSION, MFSTARTUP_LITE); }

            let path_wide: Vec<u16> = path_buf.as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            let mut reader = match unsafe {
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
                let nv12 = create_output_type(reported_w, reported_h, MFVideoFormat_NV12);
                if reader.SetCurrentMediaType(stream_idx, None, &nv12).is_ok() {
                    true
                } else {
                    let yuy2 = create_output_type(reported_w, reported_h, MFVideoFormat_YUY2);
                    if reader.SetCurrentMediaType(stream_idx, None, &yuy2).is_ok() {
                        true
                    } else {
                        let rgb32 = create_output_type(reported_w, reported_h, MFVideoFormat_RGB32);
                        reader.SetCurrentMediaType(stream_idx, None, &rgb32).is_ok()
                    }
                }
            };

            if !set_ok { return; }

            // 外层循环: EOF 时重建 reader 实现循环播放
            'outer: loop {
                let mut actual_stream: u32 = 0;
                let mut stream_flags: u32 = 0;
                let mut timestamp: i64 = 0;

                loop {
                    let mut sample_opt: Option<IMFSample> = None;

                    let result = unsafe {
                        reader.ReadSample(
                        stream_idx, 0,
                        Some(&mut actual_stream),
                        Some(&mut stream_flags),
                        Some(&mut timestamp),
                        Some(&mut sample_opt),
                    )
                };

                    if result.is_err() { break 'outer; }
                    if stream_flags & 0x2 != 0 {
                        break; // EOF, 内层 break → 外层重建 reader
                    }

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
                let mut cur_len: u32 = 0;
                if unsafe { buffer.Lock(&mut data_ptr, Some(&mut max_len), Some(&mut cur_len)) }.is_err() {
                    continue;
                }

                let raw = unsafe { std::slice::from_raw_parts(data_ptr, cur_len as usize) };
                let rgba = convert_to_rgba(raw, real_w, real_h, &out_subtype, stride);

                unsafe { let _ = buffer.Unlock(); }

                    if rgba.len() == frame_size {
                        let mut slot = slot_clone.lock().unwrap();
                        *slot = Some(rgba);
                    }
                }
                // 重建 reader 循环播放
                drop(reader);
                
                let new_reader = match unsafe {
                    MFCreateSourceReaderFromURL(PCWSTR(path_wide.as_ptr()), None)
                } {
                    Ok(r) => r,
                    Err(_) => break,
                };
                reader = new_reader;
                
                unsafe {
                    for i in 0..8u32 {
                        let _ = reader.SetStreamSelection(i, false);
                    }
                    let _ = reader.SetStreamSelection(stream_idx, true);
                }
                
                let re_ok = unsafe {
                    let nv12 = create_output_type(reported_w, reported_h, MFVideoFormat_NV12);
                    if reader.SetCurrentMediaType(stream_idx, None, &nv12).is_ok() {
                        true
                    } else {
                        let yuy2 = create_output_type(reported_w, reported_h, MFVideoFormat_YUY2);
                        if reader.SetCurrentMediaType(stream_idx, None, &yuy2).is_ok() {
                            true
                        } else {
                            let rgb32 = create_output_type(reported_w, reported_h, MFVideoFormat_RGB32);
                            reader.SetCurrentMediaType(stream_idx, None, &rgb32).is_ok()
                        }
                    }
                };
                if !re_ok { break; }
            }

            unsafe { let _ = MFShutdown(); }
        });

        Ok(Self {
            width: real_w,
            height: real_h,
            frame_slot,
            _join_handle: join_handle,
        })
    }
}

/// 从 buffer 数据和 MF 报告的尺寸，检测真实的 buffer 布局
///
/// NV12: buf_len = stride * height * 3/2
/// 策略: 对 reported_w 和 reported_h 两个值，分别作为 stride 候选，
/// 用 buf_len * 2 / (stride * 3) 算出对应的高度，再用行相关性验证。
pub fn detect_layout(buf: &[u8], reported_w: u32, reported_h: u32, subtype: &GUID) -> (u32, u32, usize) {
    let buf_len = buf.len();
    let bpp_factor = bytes_per_pixel_factor(subtype); // NV12=1.5, YUY2=2, RGB32=4

    // 候选 stride: 对 reported_w 和 reported_h 分别做对齐
    let mut candidates: Vec<(u32, u32, usize)> = Vec::new(); // (width, height, stride)

    for &dim in &[reported_w, reported_h] {
        for &align in &[32usize, 16, 8, 4, 2, 1] {
            let stride = ((dim as usize + align - 1) / align) * align;
            if stride == 0 { continue; }

            // height = buf_len / (stride * factor)
            let factor_times_1000 = (bpp_factor * 1000.0) as usize;
            let height = buf_len * 1000 / (stride * factor_times_1000);

            if height == 0 { continue; }
            // 验证: stride * height * factor == buf_len
            let check = stride * height * factor_times_1000 / 1000;
            if check != buf_len { continue; }

            // 另一个维度应该接近 reported 的另一个值
            let other = if dim == reported_w { reported_h } else { reported_w };
            if (height as i32 - other as i32).abs() > 64 {
                // 高度偏差太大，可能是错误的 stride
                // 但对于有 padding 的视频，偏差可能正好是宏块大小 (16)
                // 所以放宽到 64
            }

            candidates.push((dim, height as u32, stride));
        }
    }

    // 去重
    candidates.sort();
    candidates.dedup();

    if candidates.is_empty() {
        let stride = ((reported_w as usize + 31) / 32) * 32;
        return (reported_w, reported_h, stride);
    }

    if candidates.len() == 1 {
        return candidates[0];
    }

    // 多个候选: 用 Y plane 行相关性选最佳
    let mut best = candidates[0];
    let mut best_corr = -1.0f64;

    for &(w, h, stride) in &candidates {
        let corr = avg_row_correlation(buf, w as usize, h as usize, stride, subtype);
        eprintln!("[MF] 候选: {}x{} stride={} → corr={:.4}", w, h, stride, corr);
        if corr > best_corr {
            best_corr = corr;
            best = (w, h, stride);
        }
    }

    eprintln!("[MF] 选定: {}x{} stride={} (corr={:.4})", best.0, best.1, best.2, best_corr);
    best
}

/// 计算 Y plane 相邻行的平均皮尔逊相关系数
fn avg_row_correlation(buf: &[u8], w: usize, h: usize, stride: usize, subtype: &GUID) -> f64 {
    if h < 4 || stride == 0 { return 0.0; }

    // Y plane 的起始位置和行间距
    let (y_start, y_stride) = if *subtype == MFVideoFormat_RGB32 {
        (0, stride) // RGB32: Y is not separate, use raw rows
    } else {
        (0, stride) // NV12/YUY2: Y plane starts at beginning
    };

    let sample_w = w.min(200);
    let mut corrs = Vec::new();

    for &r in &[50usize, 200, 500, 1000] {
        if r + 1 >= h { continue; }
        let off0 = y_start + r * y_stride;
        let off1 = y_start + (r + 1) * y_stride;
        if off0 + sample_w > buf.len() || off1 + sample_w > buf.len() { continue; }

        let row0 = &buf[off0..off0 + sample_w];
        let row1 = &buf[off1..off1 + sample_w];
        let c = pearson_corr(row0, row1);
        if !c.is_nan() {
            corrs.push(c);
        }
    }

    if corrs.is_empty() { 0.0 } else { corrs.iter().sum::<f64>() / corrs.len() as f64 }
}

fn pearson_corr(a: &[u8], b: &[u8]) -> f64 {
    let n = a.len().min(b.len());
    if n < 2 { return f64::NAN; }
    let n = n as f64;
    let mut sa = 0.0f64;
    let mut sb = 0.0f64;
    for i in 0..n as usize {
        sa += a[i] as f64;
        sb += b[i] as f64;
    }
    let ma = sa / n;
    let mb = sb / n;
    let mut num = 0.0;
    let mut da = 0.0;
    let mut db = 0.0;
    for i in 0..n as usize {
        let av = a[i] as f64 - ma;
        let bv = b[i] as f64 - mb;
        num += av * bv;
        da += av * av;
        db += bv * bv;
    }
    let den = (da * db).sqrt();
    if den == 0.0 { f64::NAN } else { num / den }
}

fn bytes_per_pixel_factor(subtype: &GUID) -> f64 {
    if *subtype == MFVideoFormat_NV12 { 1.5 }
    else if *subtype == MFVideoFormat_YUY2 { 2.0 }
    else if *subtype == MFVideoFormat_RGB32 { 4.0 }
    else { 1.5 }
}

pub unsafe fn create_output_type(width: u32, height: u32, subtype: GUID) -> IMFMediaType {
    let mt = MFCreateMediaType().unwrap();
    let _ = mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video);
    let _ = mt.SetGUID(&MF_MT_SUBTYPE, &subtype);
    let packed = (height as u64) << 32 | width as u64;
    let _ = mt.SetUINT64(&MF_MT_FRAME_SIZE, packed);
    let fps_packed = (1u64) << 32 | 30u64;
    let _ = mt.SetUINT64(&MF_MT_FRAME_RATE, fps_packed);
    let _ = mt.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32);
    let _ = mt.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64) << 32 | 1u64);
    mt
}

fn convert_to_rgba(raw: &[u8], width: u32, height: u32, subtype: &GUID, stride: usize) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let rgba_size = w * h * 4;
    let mut rgba = vec![0u8; rgba_size];

    let min_stride = if *subtype == MFVideoFormat_NV12 { w }
        else if *subtype == MFVideoFormat_YUY2 { w * 2 }
        else { w * 4 };
    let s = stride.max(min_stride);

    if *subtype == MFVideoFormat_RGB32 {
        for j in 0..h {
            let row_start = j * s;
            for i in 0..w {
                let src = row_start + i * 4;
                let dst = (j * w + i) * 4;
                if src + 3 < raw.len() {
                    rgba[dst] = raw[src + 2];
                    rgba[dst + 1] = raw[src + 1];
                    rgba[dst + 2] = raw[src];
                    rgba[dst + 3] = 255;
                }
            }
        }
    } else if *subtype == MFVideoFormat_NV12 {
        let y_plane_size = s * h;
        for j in 0..h {
            let y_row = j * s;
            for i in 0..w {
                let y_idx = y_row + i;
                if y_idx >= raw.len() { continue; }
                let y = raw[y_idx] as f32;
                let uv_row = (j / 2) * s;
                let uv_idx = uv_row + (i / 2) * 2;
                let uv_abs = y_plane_size + uv_idx;
                if uv_abs + 1 >= raw.len() { continue; }
                let u = raw[uv_abs] as f32 - 128.0;
                let v = raw[uv_abs + 1] as f32 - 128.0;
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
            let row_start = j * s;
            for i in (0..w).step_by(2) {
                let src = row_start + i * 2;
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
    }
    rgba
}
