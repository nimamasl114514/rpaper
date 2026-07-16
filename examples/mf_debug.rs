//! MF 调试工具 — 打印视频文件的 media type、stride、buffer 信息
use std::os::windows::ffi::OsStrExt;
use windows::core::PCWSTR;
use windows::Win32::Media::MediaFoundation::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = if args.len() > 1 {
        std::path::PathBuf::from(&args[1])
    } else {
        std::path::PathBuf::from(r"C:\Users\wwww\Documents\QQ20260714-212139.mp4")
    };

    println!("文件: {}", path.display());

    let path_wide: Vec<u16> = path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        MFStartup(MF_VERSION, MFSTARTUP_LITE).unwrap();

        let reader = MFCreateSourceReaderFromURL(PCWSTR(path_wide.as_ptr()), None).unwrap();
        let stream_idx = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

        for i in 0..8u32 {
            let _ = reader.SetStreamSelection(i, false);
        }
        reader.SetStreamSelection(stream_idx, true).unwrap();

        // 打印 native media type
        let native_mt = reader.GetNativeMediaType(stream_idx, 0).unwrap();
        let packed_size = native_mt.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
        let width = (packed_size & 0xFFFFFFFF) as u32;
        let height = (packed_size >> 32) as u32;
        println!("Native: {}x{}", width, height);

        let native_subtype = native_mt.GetGUID(&MF_MT_SUBTYPE).unwrap_or(GUID::zeroed());
        println!("Native subtype: {:?} ({})", native_subtype, guid_to_string(&native_subtype));

        // 试每种输出格式
        for (name, subtype) in &[
            ("NV12", MFVideoFormat_NV12),
            ("YUY2", MFVideoFormat_YUY2),
            ("RGB32", MFVideoFormat_RGB32),
            ("ARGB32", MFVideoFormat_ARGB32),
        ] {
            let mt = create_output_type(width, height, *subtype);
            match reader.SetCurrentMediaType(stream_idx, None, &mt) {
                Ok(()) => {
                    println!("\n{} 设置成功!", name);

                    // 读取实际 output media type
                    let out_mt = reader.GetCurrentMediaType(stream_idx).unwrap();

                    // stride
                    let stride = out_mt.GetUINT32(&MF_MT_DEFAULT_STRIDE);
                    println!("  MF_MT_DEFAULT_STRIDE: {:?}", stride);

                    // 实际 subtype
                    let actual_sub = out_mt.GetGUID(&MF_MT_SUBTYPE).unwrap_or(GUID::zeroed());
                    println!("  实际 subtype: {}", guid_to_string(&actual_sub));

                    // frame size
                    let fs = out_mt.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
                    println!("  Frame size: {}x{}", fs & 0xFFFFFFFF, fs >> 32);

                    // 读一帧看 buffer 大小
                    let mut actual_stream: u32 = 0;
                    let mut stream_flags: u32 = 0;
                    let mut timestamp: i64 = 0;
                    let mut sample_opt: Option<IMFSample> = None;

                    let r = reader.ReadSample(
                        stream_idx, 0,
                        Some(&mut actual_stream),
                        Some(&mut stream_flags),
                        Some(&mut timestamp),
                        Some(&mut sample_opt),
                    );

                    if r.is_ok() {
                        if let Some(sample) = sample_opt {
                            let buf = sample.ConvertToContiguousBuffer().unwrap();
                            let mut ptr: *mut u8 = std::ptr::null_mut();
                            let mut max_len: u32 = 0;
                            let mut cur_len: u32 = 0;
                            buf.Lock(&mut ptr, Some(&mut max_len), Some(&mut cur_len)).unwrap();

                            println!("  Buffer max_len: {}, cur_len: {}", max_len, cur_len);

                            // 计算期望的紧凑大小
                            let expected = match *name {
                                "NV12" => width * height * 3 / 2,
                                "YUY2" => width * height * 2,
                                "RGB32" | "ARGB32" => width * height * 4,
                                _ => 0,
                            };
                            println!("  紧凑大小: {}, 差值: {}", expected, cur_len as i64 - expected as i64);

                            // stride 反推
                            if height > 0 {
                                let h = height as usize;
                                let calc_stride = match *name {
                                    "NV12" => cur_len as usize * 2 / (h * 3),
                                    _ => cur_len as usize / h,
                                };
                                println!("  反推 stride: {}", calc_stride);
                            }

                            // 打印前 32 字节
                            let preview = std::slice::from_raw_parts(ptr, 32.min(cur_len as usize));
                            print!("  前32字节: ");
                            for b in preview {
                                print!("{:02x} ", b);
                            }
                            println!();

                            let _ = buf.Unlock();
                        } else {
                            println!("  (无 sample)");
                        }
                    }

                    // 关闭流，换下一个格式
                    break; // 只用第一个成功的格式
                }
                Err(e) => {
                    println!("\n{} 失败: {}", name, e);
                }
            }
        }

        MFShutdown();
    }
}

use windows::core::GUID;

unsafe fn create_output_type(width: u32, height: u32, subtype: GUID) -> IMFMediaType {
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

fn guid_to_string(g: &GUID) -> String {
    if *g == MFVideoFormat_NV12 {
        "NV12".into()
    } else if *g == MFVideoFormat_YUY2 {
        "YUY2".into()
    } else if *g == MFVideoFormat_RGB32 {
        "RGB32".into()
    } else if *g == MFVideoFormat_ARGB32 {
        "ARGB32".into()
    } else if *g == MFVideoFormat_H264 || *g == MFVideoFormat_H264_ES {
        "H264".into()
    } else if *g == MFVideoFormat_HEVC || *g == MFVideoFormat_HEVC_ES {
        "HEVC".into()
    } else {
        format!("{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            g.data1, g.data2, g.data3,
            g.data4[0], g.data4[1],
            g.data4[2], g.data4[3], g.data4[4], g.data4[5], g.data4[6], g.data4[7])
    }
}
