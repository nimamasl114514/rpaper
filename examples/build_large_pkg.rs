//! 构造大 .pkg 测试文件 — 验证后台解压不阻塞 UI
//!
//! 用法: cargo run --release --example build_large_pkg -- <output.pkg> [size_mb]
//! 默认生成 50MB 的 .pkg，视频数据用半随机字节（LZ4 压缩率中等，解压耗时约 1-2 秒）
//!
//! 配合 scripts/test_pkg_async.py 使用:
//!   1. 本程序生成 large_test.pkg
//!   2. py 脚本启动 rpaper.exe 加载该 pkg，测量主线程响应性

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = env::args().collect();
    let output = if args.len() >= 2 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("large_test.pkg")
    };
    let size_mb: usize = if args.len() >= 3 {
        args[2].parse().unwrap_or(50)
    } else {
        50
    };

    // project.json
    let project = b"{\"type\":\"video\",\"file\":\"video.mp4\",\"title\":\"Large Test\"}";
    // 视频数据 — 半随机（每 4 字节递增计数器），LZ4 压缩率中等
    // 全 0 数据 LZ4 压缩率极高，解压太快无法验证后台解压；纯随机数据压缩率为 1:1
    // 递增计数器是折中：有一定重复模式但 LZ4 无法完全消除
    let video_size = size_mb * 1024 * 1024;
    let mut video_data = Vec::with_capacity(video_size);
    let mut counter: u32 = 0;
    while video_data.len() + 4 <= video_size {
        video_data.extend_from_slice(&counter.to_le_bytes());
        counter = counter.wrapping_add(1);
    }

    // 音频数据（小，仅占位）
    let audio = b"FAKE_AUDIO_DATA_FOR_TESTING";

    // 构造 PKG (version 1, 无 hash) — 与 pkg.rs 测试中的 build_pkg 逻辑一致
    let entries: Vec<(&str, &[u8])> = vec![
        ("project.json", project),
        ("video.mp4", &video_data),
        ("audio.mp3", audio),
    ];

    let mut compressed_blocks = Vec::new();
    let mut offsets = Vec::new();
    let mut current_offset: u32 = 0;
    for (_, data) in &entries {
        let compressed = lz4_flex::compress(data);
        offsets.push((current_offset, compressed.len() as u32, data.len() as u32));
        current_offset += compressed.len() as u32;
        compressed_blocks.push(compressed);
    }

    let mut buf = Vec::new();
    // Header
    buf.extend_from_slice(b"PKGV0001");
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&(entries.len() as u32).to_le_bytes());

    // Index
    for (i, (path, _)) in entries.iter().enumerate() {
        let path_bytes = path.as_bytes();
        buf.extend_from_slice(&(path_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(path_bytes);
        let (off, csize, usize_) = offsets[i];
        buf.extend_from_slice(&off.to_le_bytes());
        buf.extend_from_slice(&csize.to_le_bytes());
        buf.extend_from_slice(&usize_.to_le_bytes());
    }

    // Data section
    for compressed in compressed_blocks {
        buf.extend_from_slice(&compressed);
    }

    let mut f = File::create(&output).expect("create file");
    f.write_all(&buf).expect("write file");

    println!("已生成 {}", output.display());
    println!("  视频数据: {}MB", size_mb);
    println!("  PKG 总大小: {:.1}MB", buf.len() as f64 / 1024.0 / 1024.0);
    println!("  LZ4 压缩率: {:.1}%", video_size as f64 * 100.0 / buf.len() as f64);
}
