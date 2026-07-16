//! 调试 detect_layout 函数
use std::os::windows::ffi::OsStrExt;
use windows::core::{GUID, PCWSTR};
use windows::Win32::Media::MediaFoundation::*;

#[path = "../src/mf_decoder.rs"]
mod mf_decoder;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = if args.len() > 1 {
        std::path::PathBuf::from(&args[1])
    } else {
        std::path::PathBuf::from(r"C:\Users\wwww\Documents\QQ20260715-163423.mp4")
    };

    let path_wide: Vec<u16> = path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        MFStartup(MF_VERSION, MFSTARTUP_LITE).unwrap();
        let reader = MFCreateSourceReaderFromURL(PCWSTR(path_wide.as_ptr()), None).unwrap();
        let stream_idx = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        for i in 0..8u32 { let _ = reader.SetStreamSelection(i, false); }
        reader.SetStreamSelection(stream_idx, true).unwrap();

        let mt = reader.GetNativeMediaType(stream_idx, 0).unwrap();
        let packed = mt.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
        let rw = (packed & 0xFFFFFFFF) as u32;
        let rh = (packed >> 32) as u32;
        println!("Reported: {}x{}", rw, rh);

        let nv12 = mf_decoder::create_output_type(rw, rh, MFVideoFormat_NV12);
        reader.SetCurrentMediaType(stream_idx, None, &nv12).unwrap();

        // Skip frames + read one
        let mut last_buf: Vec<u8> = Vec::new();
        for _ in 0..15 {
            let mut s: u32 = 0; let mut f: u32 = 0; let mut t: i64 = 0;
            let mut sample: Option<IMFSample> = None;
            let _ = reader.ReadSample(stream_idx, 0, Some(&mut s), Some(&mut f), Some(&mut t), Some(&mut sample));
            if let Some(sample) = sample {
                if let Ok(buf) = sample.ConvertToContiguousBuffer() {
                    let mut p: *mut u8 = std::ptr::null_mut();
                    let mut mx: u32 = 0; let mut cl: u32 = 0;
                    if buf.Lock(&mut p, Some(&mut mx), Some(&mut cl)).is_ok() {
                        last_buf = std::slice::from_raw_parts(p, cl as usize).to_vec();
                        let _ = buf.Unlock();
                    }
                }
            }
        }

        println!("Buffer size: {}", last_buf.len());
        let (w, h, s) = mf_decoder::detect_layout(&last_buf, rw, rh, &MFVideoFormat_NV12);
        println!("detect_layout result: {}x{} stride={}", w, h, s);
        
        let _ = MFShutdown();
    }
}
