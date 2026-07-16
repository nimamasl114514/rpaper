//! MF 解码调试 — 直接保存 Y plane 为图片，跳过 UV 转换
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

        let nv12 = create_output_type(width, height, MFVideoFormat_NV12);
        reader.SetCurrentMediaType(stream_idx, None, &nv12).unwrap();

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
        let stride = buf_len * 2 / (h * 3);
        println!("buf_len={}, stride={}, w={}, h={}", buf_len, stride, w, h);

        let raw = std::slice::from_raw_parts(data_ptr, buf_len);

        // 1. 保存 Y plane，用 stride=1152 (反推值)
        let y_plane_size = stride * h;
        let ppm1 = r"C:\Users\wwww\Documents\lingxi-claw\20260715-18-35-54-219\debug_y_1152.ppm";
        let mut f = std::fs::File::create(ppm1).unwrap();
        write!(f, "P5\n{} {}\n255\n", w, h).unwrap(); // P5 = grayscale
        for j in 0..h {
            let row = j * stride;
            for i in 0..w {
                f.write_all(&[raw[row + i]]).unwrap();
            }
        }
        println!("Saved Y (stride={}): {}", stride, ppm1);

        // 2. 保存 Y plane，用 stride=1144 (紧凑)
        let ppm2 = r"C:\Users\wwww\Documents\lingxi-claw\20260715-18-35-54-219\debug_y_1144.ppm";
        let mut f = std::fs::File::create(ppm2).unwrap();
        write!(f, "P5\n{} {}\n255\n", w, h).unwrap();
        for j in 0..h {
            let row = j * w; // 紧凑 stride = width
            if row + w > buf_len { break; }
            f.write_all(&raw[row..row + w]).unwrap();
        }
        println!("Saved Y (stride={}): {}", w, ppm2);

        // 3. 保存 Y plane，用 stride=1920（假设 width/height 被互换了）
        let ppm3 = r"C:\Users\wwww\Documents\lingxi-claw\20260715-18-35-54-219\debug_y_1920.ppm";
        let mut f = std::fs::File::create(ppm3).unwrap();
        write!(f, "P5\n{} {}\n255\n", 1920, 1144).unwrap();
        for j in 0..1144 {
            let row = j * 1920;
            if row + 1920 > buf_len { break; }
            f.write_all(&raw[row..row + 1920]).unwrap();
        }
        println!("Saved Y (stride=1920, 1920x1144): {}", ppm3);

        // 4. 保存整个 raw buffer 为 binary
        let bin_path = r"C:\Users\wwww\Documents\lingxi-claw\20260715-18-35-54-219\debug_full_nv12.bin";
        std::fs::write(bin_path, raw).unwrap();
        println!("Saved raw NV12: {} ({} bytes)", bin_path, buf_len);

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
