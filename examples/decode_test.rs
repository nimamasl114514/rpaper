//! 纯 Rust H.264 解码器测试
//! 用法: decode_test <mp4路径>
//! 验证: MP4 解析 → SPS/PPS → NAL 提取 → 切片解码 → 帧输出
use rpaper::video::decoder::VideoDecoder;
use rpaper::video::demux::Demuxer;
use rpaper::video::demux::mp4::Mp4Demuxer;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from(r"C:\Users\wwww\Documents\QQ20260714-212139.mp4")
    };

    println!("=== 纯 Rust H.264 解码器测试 ===");
    println!("文件: {}", path.display());
    println!("文件大小: {} 字节", std::fs::metadata(&path).unwrap().len());
    println!();

    // 测试 1: 打开解码器（会启动后台线程）
    println!("[1/4] 打开 VideoDecoder...");
    let decoder = match VideoDecoder::open(&path) {
        Ok(d) => {
            println!("  ✓ 成功打开");
            println!("  视频尺寸: {}x{}", d.width, d.height);
            println!("  shutdown 标志: {}", d.shutdown.load(Ordering::Acquire));
            d
        }
        Err(e) => {
            println!("  ✗ 失败: {e}");
            std::process::exit(1);
        }
    };

    // 测试 2: 等待后台线程解码几帧
    println!();
    println!("[2/4] 等待后台解码线程产出帧...");
    let mut got_frame = false;
    for i in 0..30 {
        std::thread::sleep(Duration::from_millis(100));
        // take 一帧出来检查，检查完归还 pool 供解码线程复用
        if let Some(frame_data) = decoder.frame_slot.take() {
            println!("  ✓ 第 {i} 次检查: 获得帧数据");
            println!("  帧大小: {} 字节", frame_data.len());
            println!("  期望大小: {} 字节 ({}x{}x4)",
                decoder.width as usize * decoder.height as usize * 4,
                decoder.width, decoder.height);

            // 统计像素分布
            let mut min = u8::MAX;
            let mut max = 0u8;
            let mut sum = 0u64;
            for &b in frame_data.iter() {
                min = min.min(b);
                max = max.max(b);
                sum += b as u64;
            }
            let avg = sum / frame_data.len() as u64;
            println!("  像素范围: [{min}, {max}], 平均值: {avg}");

            // 检查是否全黑/全绿（常见错误模式）
            let all_black = frame_data.iter().all(|&b| b == 0);
            let all_green = frame_data.chunks(4).all(|c| c[0] == 0 && c[1] != 0 && c[2] == 0);
            if all_black {
                println!("  ⚠ 警告: 帧全黑");
            } else if all_green {
                println!("  ⚠ 警告: 帧全绿（YUV→RGBA 转换异常）");
            } else {
                println!("  ✓ 帧数据有变化");
            }

            // 保存前 16 像素用于检查
            print!("  前 16 像素 RGBA: ");
            for i in 0..16 {
                let off = i * 4;
                if off + 3 < frame_data.len() {
                    print!("[{},{},{},{}] ",
                        frame_data[off], frame_data[off+1],
                        frame_data[off+2], frame_data[off+3]);
                }
            }
            println!();

            // 归还到 pool，下一帧解码能复用
            decoder.frame_slot.return_buf(frame_data);
            got_frame = true;
            break;
        }
        if i % 5 == 0 {
            println!("  ... 等待中 ({i}/30)");
        }
    }

    if !got_frame {
        println!("  ✗ 30 次检查未获得帧（解码线程可能卡住或崩溃）");
    }

    // 测试 3: MP4 解析单独验证
    println!();
    println!("[3/4] MP4 容器解析验证...");
    match Mp4Demuxer::from_path(&path) {
        Ok(demuxer) => {
            println!("  ✓ MP4 解析成功");
            println!("  尺寸: {}x{}", demuxer.width(), demuxer.height());
            println!("  时长: {} ms", demuxer.duration_ms());
            println!("  SPS 长度: {}", demuxer.sps().len());
            println!("  PPS 长度: {}", demuxer.pps().len());
            print!("  SPS 完整 ({} 字节): ", demuxer.sps().len());
            for b in demuxer.sps().iter() {
                print!("{:02x} ", b);
            }
            println!();
            print!("  PPS 完整 ({} 字节): ", demuxer.pps().len());
            for b in demuxer.pps().iter() {
                print!("{:02x} ", b);
            }
            println!();
        }
        Err(e) => {
            println!("  ✗ MP4 解析失败: {e}");
        }
    }

    // 测试 4: Drop 清理
    println!();
    println!("[4/4] Drop 清理测试...");
    drop(decoder);
    println!("  ✓ VideoDecoder 已 drop（后台线程应已退出）");

    println!();
    println!("=== 测试完成 ===");
}
