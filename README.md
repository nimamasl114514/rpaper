# Rpaper

> GPU 加速的 Windows 动态壁纸引擎 — Rust + wgpu(DX12)，60fps，~6% CPU

Rust 写的 Windows 动态壁纸引擎，自带极光/粒子特效，支持视频和图片壁纸，还能直接导入 Wallpaper Engine 的 .pkg 包。

## 壁纸类型

| 类型 | 说明 | 格式 |
|------|------|------|
| **极光效果** | 多层 simplex noise 极光幕布 + 星空 + 地平线辉光 | 内置 shader |
| **粒子效果** | GPU compute shader 驱动的浮动光点系统，CPU 零负载 | 内置 shader |
| **视频壁纸** | openh264 硬解，全 Profile 支持 | MP4/MKV/AVI/WebM/MOV/FLV/WMV/TS/M4V |
| **图片壁纸** | cover 铺满 + 呼吸动画 | PNG/JPG/BMP/WebP/GIF |
| **壁纸包 (.rwp)** | 自研打包格式，可附带背景音乐 | ZIP + JSON |
| **WE .pkg 兼容** | 直接导入 Wallpaper Engine 导出的 video 类型包 | .pkg (LZ4) |

## 快速开始

```
# 下载 Release 安装包，双击安装
# 或命令行直接跑
rpaper.exe                           # 极光效果（默认）
rpaper.exe particles                  # 粒子效果
rpaper.exe C:\path\video.mp4         # 直接加载文件
rpaper.exe C:\path\wallpaper.rwp     # 加载壁纸包
rpaper.exe C:\path\bg.pkg            # 加载 WE 壁纸
```

## 功能特性

### 渲染与性能

- **wgpu DX12 渲染** — 60fps VSync，GPU compute shader 粒子系统，CPU 占用 ~6%
- **三缓冲零拷贝** — 解码线程 → FrameSlot → UI 线程，每帧只传指针
- **YUV→RGBA SIMD** — SSE4.1 色彩转换，视频帧零开销上传 GPU
- **轻量** — 单 exe 7.4MB（openh264 静态链接），无运行时依赖

### 视频解码

- **openh264 内置** — Cisco H.264 解码器，CABAC + B 帧 + Main/High Profile 全支持
- **纯 Rust MP4 解析** — 自研 demuxer，无 FFmpeg 依赖
- **多格式支持** — 即使扩展名不是 mp4 也能尝试解码

### 桌面集成

- **WorkerW 壁纸层** — 桌面图标下方渲染，不影响正常操作
- **系统壁纸保护** — 启动时保存原壁纸，退出自动恢复
- **文件关联** — 双击 .rwp/.pkg 自动加载，视频/图片右键菜单
- **单实例** — 第二实例自动转发文件路径到已运行的进程

### 操作方式

- **系统托盘** — 左键切壁纸、双击开设置、右键菜单
- **设置窗口** — Win11 Mica 背景 + 卡片布局 + DWM 圆角
- **命令行加载** — 传文件路径或效果名，一行搞定
- **音量控制** — 滑块实时调节壁纸背景音乐
- **暂停/恢复** — 一键暂停动画
- **开机自启** — 注册表持久化

### 壁纸包生态

- **.rwp 格式** — ZIP + JSON，任何人都能制作，门槛极低
- **背景音乐** — 壁纸包可附带 MP3/WAV/OGG/FLAC，循环播放
- **WE 兼容** — 直接导入 Wallpaper Engine 的 .pkg（video 类型），LZ4 解压 + 自动提取

## 对比 Wallpaper Engine

| | Rpaper | Wallpaper Engine |
|--|--------|-----------------|
| 价格 | 免费开源 | 付费 (Steam) |
| 技术栈 | Rust + wgpu | C++ 闭源 |
| 二进制大小 | 7.4MB | ~50MB+ |
| 运行时依赖 | 无 | 无 |
| 着色器壁纸 | 极光/粒子 (WGSL) | 自定义着色器语言 |
| 视频壁纸 | openh264 内置解码 | 内置解码器 |
| 网页壁纸 | 不支持 | 支持 (CEF) |
| 生态 | 待建设 | Steam 创意工坊 |
| 协议 | MIT | 闭源 |

## 技术架构

```
Progman (桌面外壳)
  └── WorkerW (壁纸层)
        └── WallpaperChild (子窗口)
              └── wgpu Surface (DX12)
```

视频解码管线：`MP4 → Demuxer → openh264 → YUV→RGBA(SIMD) → wgpu Texture → 渲染`

## 构建

```bash
cargo build --release
```

需要 Rust 工具链，openh264 编译时自动构建，无需预装。

## 环境要求

- Windows 10/11
- DirectX 12 兼容 GPU
- Rust 1.75+（编译用）

## License

MIT