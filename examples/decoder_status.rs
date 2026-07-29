//! 解码器状态机验证 — 直接调用 VideoDecoder，每秒打印一次状态快照
//!
//! 用法:
//!   cargo run --release --example decoder_status -- <path/to/video.mp4>
//!
//! 预期:
//!   - 第 0~1 秒: state=Loading, progress=0
//!   - 第 1 秒后: state=Playing, progress 递增
//!   - 视频循环后: progress 归零再次递增

use std::env;
use std::path::Path;
use std::thread;
use std::time::Duration;

use rpaper::video::decoder::{DecoderState, VideoDecoder};

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = if args.len() >= 2 {
        Path::new(&args[1])
    } else {
        eprintln!("用法: decoder_status <video.mp4>");
        std::process::exit(1);
    };

    if !path.exists() {
        eprintln!("文件不存在: {}", path.display());
        std::process::exit(1);
    }

    println!("打开视频: {}", path.display());
    let decoder = match VideoDecoder::open(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("VideoDecoder::open 失败: {e}");
            std::process::exit(1);
        }
    };

    println!("解码器初始化完成: {}x{}", decoder.width, decoder.height);
    println!("---- 状态时间线 ----");

    // 跑 6 秒，每 500ms 打印一次状态
    for tick in 0..12 {
        let st = decoder.status();
        let state_str = match st.state {
            DecoderState::Loading => "Loading",
            DecoderState::Playing => "Playing",
            DecoderState::Error  => "Error",
        };
        let pct = if st.total > 0 {
            st.current * 100 / st.total
        } else { 0 };
        println!(
            "[{:>4}ms] state={:<8} progress={}/{} ({}%)",
            tick * 500, state_str, st.current, st.total, pct
        );
        thread::sleep(Duration::from_millis(500));
    }

    println!("---- 验证结束 ----");
    drop(decoder); // 触发 Drop → shutdown 解码线程
    println!("解码器已关闭");
}
