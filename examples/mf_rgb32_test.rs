//! MF 解码调试 — 强制 RGB32 输出，跳过 YUV 转换
use std::os::windows::ffi::OsStrExt;
use std::io::Write;
use windows::core::{GUID, PCWSTR};
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

        let native_mt = reader.GetNativeMediaType(stream_idx, 0).unwrap();
        let packed_size = native_mt.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
        let width = (packed_size & 0xFFFFFFFF) as u32;
        let height = (packed_size >> 32) as u32;
        println!("视频: {}x{}", width, height);

        // 强制 RGB32 输出
        let rgb32 = create_output_type(width, height, MFVideoFormat_RGB32);
        match reader.SetCurrentMediaType(stream_idx, None, &rgb32) {
            Ok(()) => println!("RGB32 设置成功"),
            Err(e) => {
                println!("RGB32 失败: {}, 试 ARGB32", e);
                let argb32 = create_output_type(width, height, MFVideoFormat_ARGB32);
                reader.SetCurrentMediaType(stream_idx, None, &argb32).unwrap();
                println!("ARGB32 设置成功");
            }
        }

        // 读 output media type
        let out_mt = reader.GetCurrentMediaType(stream_idx).unwrap();
        let actual_sub = out_mt.GetGUID(&MF_MT_SUBTYPE).unwrap_or(GUID::zeroed());
        println!("实际 subtype: {:?}", actual_sub);

        let stride_attr = out_mt.GetUINT32(&MF_MT_DEFAULT_STRIDE);
        println!("MF_MT_DEFAULT_STRIDE: {:?}", stride_attr);

        // 跳几帧
        for _ in 0..10 {
            let mut s: u32 = 0;
            let mut f: u32 = 0;
            let mut t: i64 = 0;
            let mut sample: Option<IMFSample> = None;
            let _ = reader.ReadSample(stream_idx, 0, Some(&mut s), Some(&mut f), Some(&mut t), Some(&mut sample));
        }

        // 读一帧
        let mut actual_stream: u32 = 0;
        let mut stream_flags: u32 = 0;
        let mut timestamp: i64 = 0;
        let mut sample_opt: Option<IMFSample> = None;

        reader.ReadSample(
            stream_idx, 0,
            Some(&mut actual_stream),
            Some(&mut stream_flags),
            Some(&mut timestamp),
            Some(&mut sample_opt),
        ).unwrap();

        let sample = sample_opt.expect("no sample");
        let buffer = sample.ConvertToContiguousBuffer().unwrap();

        let mut data_ptr: *mut u8 = std::ptr::null_mut();
        let mut max_len: u32 = 0;
        let mut cur_len: u32 = 0;
        buffer.Lock(&mut data_ptr, Some(&mut max_len), Some(&mut cur_len)).unwrap();

        let w = width as usize;
        let h = height as usize;
        let buf_len = cur_len as usize;
        println!("Buffer: max_len={}, cur_len={}", max_len, cur_len);
        println!("期望紧凑大小: {} (差: {})", w * h * 4, buf_len as i64 - (w * h * 4) as i64);

        let stride = buf_len / h;
        println!("反推 stride: {} (width*4={}, padding={})", stride, w * 4, stride - w * 4);

        let raw = std::slice::from_raw_parts(data_ptr, buf_len);

        // RGB32 在 Windows 上通常是 B8G8R8X8 (BGR 顺序)
        // 转换为 RGBA
        let mut rgba = vec![0u8; w * h * 4];
        for j in 0..h {
            let row_start = j * stride;
            for i in 0..w {
                let src = row_start + i * 4;
                let dst = (j * w + i) * 4;
                if src + 3 < buf_len {
                    rgba[dst]     = raw[src + 2]; // R
                    rgba[dst + 1] = raw[src + 1]; // G
                    rgba[dst + 2] = raw[src];     // B
                    rgba[dst + 3] = 255;          // A
                }
            }
        }

        // 保存 PPM
        let ppm_path = r"C:\Users\wwww\Documents\lingxi-claw\20260715-18-35-54-219\debug_rgb32.ppm";
        let mut f = std::fs::File::create(ppm_path).unwrap();
        write!(f, "P6\n{} {}\n255\n", w, h).unwrap();
        for i in 0..(w * h) {
            let o = i * 4;
            f.write_all(&[rgba[o], rgba[o+1], rgba[o+2]]).unwrap();
        }
        println!("已保存 PPM: {}", ppm_path);

        // 也试试直接 BGR 顺序不 swap
        let ppm2_path = r"C:\Users\wwww\Documents\lingxi-claw\20260715-18-35-54-219\debug_bgr32.ppm";
        let mut f2 = std::fs::File::create(ppm2_path).unwrap();
        write!(f2, "P6\n{} {}\n255\n", w, h).unwrap();
        for j in 0..h {
            let row_start = j * stride;
            for i in 0..w {
                let src = row_start + i * 4;
                f2.write_all(&[raw[src], raw[src+1], raw[src+2]]).unwrap();
            }
        }
        println!("已保存 BGR PPM: {}", ppm2_path);

        let _ = buffer.Unlock();
        let _ = MFShutdown();
    }
}

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
