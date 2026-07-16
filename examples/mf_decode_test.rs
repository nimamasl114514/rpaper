//! MF 解码调试 — 解码一帧并保存为 PNG，打印关键参数
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

        // 获取原生尺寸
        let native_mt = reader.GetNativeMediaType(stream_idx, 0).unwrap();
        let packed_size = native_mt.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
        let width = (packed_size & 0xFFFFFFFF) as u32;
        let height = (packed_size >> 32) as u32;
        println!("视频: {}x{}", width, height);

        // 设置 NV12 输出
        let nv12 = create_output_type(width, height, MFVideoFormat_NV12);
        reader.SetCurrentMediaType(stream_idx, None, &nv12).unwrap();

        // 读 output media type
        let out_mt = reader.GetCurrentMediaType(stream_idx).unwrap();
        let stride_attr = out_mt.GetUINT32(&MF_MT_DEFAULT_STRIDE);
        println!("MF_MT_DEFAULT_STRIDE: {:?}", stride_attr);

        // 跳几帧到关键帧
        for _ in 0..5 {
            let mut s: u32 = 0;
            let mut f: u32 = 0;
            let mut t: i64 = 0;
            let mut sample: Option<IMFSample> = None;
            let _ = reader.ReadSample(stream_idx, 0, Some(&mut s), Some(&mut f), Some(&mut t), Some(&mut sample));
        }

        // 读关键帧
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

        println!("Buffer: max_len={}, cur_len={}", max_len, cur_len);

        let w = width as usize;
        let h = height as usize;
        let buf_len = cur_len as usize;

        // 反推 stride
        let stride = buf_len * 2 / (h * 3);
        println!("反推 stride: {} (width={})", stride, w);
        println!("每行 padding: {} 字节", stride - w);

        let raw = std::slice::from_raw_parts(data_ptr, buf_len);

        // 用反推的 stride 转换
        let rgba = convert_nv12_to_rgba(raw, w, h, stride);

        // 打印前 16 像素
        print!("RGBA 前16像素: ");
        for i in 0..16 {
            let o = i * 4;
            print!("({},{},{},{}) ", rgba[o], rgba[o+1], rgba[o+2], rgba[o+3]);
        }
        println!();

        // 打印中间一行的前 8 像素
        let mid = h / 2;
        print!("第{}行前8像素: ", mid);
        for i in 0..8 {
            let o = (mid * w + i) * 4;
            print!("({},{},{},{}) ", rgba[o], rgba[o+1], rgba[o+2], rgba[o+3]);
        }
        println!();

        // 保存为 PPM (简单无依赖)
        let ppm_path = r"C:\Users\wwww\Documents\lingxi-claw\20260715-18-35-54-219\debug_frame.ppm";
        let mut f = std::fs::File::create(ppm_path).unwrap();
        write!(f, "P6\n{} {}\n255\n", w, h).unwrap();
        for i in 0..(w * h) {
            let o = i * 4;
            f.write_all(&[rgba[o], rgba[o+1], rgba[o+2]]).unwrap();
        }
        println!("已保存 PPM: {}", ppm_path);

        // 也保存原始 NV12 数据前 256 字节
        let raw_path = r"C:\Users\wwww\Documents\lingxi-claw\20260715-18-35-54-219\debug_raw_nv12.bin";
        std::fs::write(raw_path, &raw[..256.min(raw.len())]).unwrap();
        println!("已保存原始 NV12 前256字节: {}", raw_path);

        let _ = buffer.Unlock();
        MFShutdown();
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

fn convert_nv12_to_rgba(raw: &[u8], w: usize, h: usize, stride: usize) -> Vec<u8> {
    let y_plane_size = stride * h;
    let mut rgba = vec![0u8; w * h * 4];

    for j in 0..h {
        let y_row = j * stride;
        for i in 0..w {
            let y_idx = y_row + i;
            if y_idx >= raw.len() { continue; }
            let y = raw[y_idx] as f32;

            let uv_row = (j / 2) * stride;
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

    rgba
}
