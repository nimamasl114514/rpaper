<div align="center">

# Rpaper

GPU 加速的 Windows 动态壁纸引擎

Rust + wgpu (DirectX 12) · 60fps · ~6% CPU · 纯 Rust 视频解码

</div>

## 功能

- **极光效果** — 多层 simplex noise 极光幕布 + 星空 + 地平线辉光
- **粒子效果** — GPU compute shader 驱动的浮动光点系统
- **图片壁纸** — PNG / JPG / BMP / WebP / GIF，cover 铺满 + 呼吸效果
- **视频壁纸** — 纯 Rust H.264 解码，支持 MP4/MKV/AVI/WebM/MOV，循环播放
- **壁纸包 (.rwp)** — 打包格式，方便社区制作和分享壁纸
- **Wallpaper Engine .pkg 兼容** — 直接导入 WE 的 video 类型 .pkg 壁纸
- **背景音乐** — 壁纸包可附带 MP3/WAV/OGG/FLAC 音频，循环播放
- **系统壁纸集成** — 启动时保存原壁纸，退出时自动恢复
- **文件关联** — 双击 .rwp 文件直接调起 Rpaper 加载
- **命令行加载** — `rpaper.exe path/to/file.mp4` 一行直接加载
- **系统托盘** — 中文菜单，左键循环切换、双击打开设置、右键弹出菜单
- **视频状态可视化** — 设置窗口实时显示视频解码状态与播放进度
- **DPI 感知** — 自动适配高分辨率屏幕，无缩放模糊

## 截图

（运行后补充）

## 快速开始

### 命令行运行

```bash
# 默认启动（按上次配置或极光效果）
rpaper.exe

# 指定效果
rpaper.exe particles    # 粒子效果
rpaper.exe image        # 图片模式（加载上次图片）
rpaper.exe video        # 视频模式（加载上次视频）

# 直接加载文件 — 自动按扩展名选择类型
rpaper.exe C:\path\to\video.mp4
rpaper.exe C:\path\to\wallpaper.rwp
rpaper.exe C:\path\to\bg.pkg
rpaper.exe C:\path\to\image.png
```

支持的扩展名：

| 类型 | 扩展名 |
|------|--------|
| 图片 | `.png` `.jpg` `.jpeg` `.bmp` `.webp` `.gif` |
| 视频 | `.mp4` `.mkv` `.avi` `.webm` `.mov` `.flv` `.wmv` `.ts` `.m4v` |
| 壁纸包 | `.rwp` (Rpaper 原生) / `.pkg` (Wallpaper Engine, 仅 video 类型) |
| 音频 | `.mp3` `.wav` `.ogg` `.flac` `.m4a` `.aac` |

### 文件关联双击打开

首次运行 release 版本会自动注册 `.rwp` 文件关联（HKCU 级，无需 UAC）。

- 双击任意 `.rwp` 文件 → 自动调起 `rpaper.exe "<path>"` 加载
- ProgID: `Rpaper.WallpaperPackage`
- 图标使用 rpaper.exe 自带图标
- 通过 `SHChangeNotify(SHCNE_ASSOCCHANGED)` 通知 Explorer 立即生效

注册位置（如需手动清理）：

```
HKCU\Software\Classes\.rwp                       = Rpaper.WallpaperPackage
HKCU\Software\Classes\Rpaper.WallpaperPackage    = "Rpaper 壁纸包"
HKCU\Software\Classes\Rpaper.WallpaperPackage\DefaultIcon
HKCU\Software\Classes\Rpaper.WallpaperPackage\shell\open\command  = "rpaper.exe" "%1"
```

debug 版本不自动注册，避免污染开发环境。

### 托盘菜单

| 操作 | 说明 |
|------|------|
| **左键单击** | 在 极光 → 粒子 → 图片(若已加载) → 视频(若已加载) 之间循环切换 |
| **左键双击** | 打开设置窗口 |
| **右键单击** | 弹出中文菜单 |

菜单项：

| 菜单项 | 说明 |
|--------|------|
| 极光效果 | 切换到 Aurora 壁纸 |
| 粒子效果 | 切换到 Particles 壁纸 |
| 选择图片... | 弹出文件对话框选择图片 |
| 选择视频... | 弹出文件对话框选择视频 |
| 加载壁纸包 (.rwp)... | 导入社区壁纸包或 WE .pkg |
| 设置... | 打开设置窗口 |
| 退出 | 关闭引擎 |

## 系统壁纸集成

启动时通过 `IDesktopWallpaper` COM 接口保存原系统壁纸路径，并将桌面背景设为纯黑色，避免桌面图标后方透出旧壁纸影响观感。退出时（无论正常退出或崩溃 Drop）自动恢复原壁纸。

实现要点：
- RAII 守护 `SysWallpaperGuard` — Drop 时必定恢复
- `CoInitializeEx(APARTMENTTHREADED)` 初始化 COM
- `IDesktopWallpaper::GetWallpaper` 读取原壁纸
- `IDesktopWallpaper::SetWallpaper(null, null)` + `SetBackgroundColor(0)` 设为纯色
- `IDesktopWallpaper::SetWallpaper(null, original_path)` 恢复

## 壁纸包格式 (.rwp)

`.rwp` 是 Rpaper 的打包壁纸格式（ZIP），方便制作和分享。

```
my-wallpaper.rwp (ZIP)
├── manifest.json      # 元数据
├── image.png          # 图片 (image 类型)
├── video.mp4          # 视频 (video 类型)
└── audio.mp3          # 可选，背景音乐
```

**manifest.json:**
```json
{
    "name": "北欧极光",
    "type": "shader",
    "author": "your-name",
    "description": "流动的绿色极光",
    "audio": "bgm.mp3"
}
```

支持四种类型：`shader` / `particles` / `image` / `video`，均可附带背景音乐。

制作方法：把文件按结构放好，压缩为 ZIP，后缀改为 `.rwp` 即可。

详细文档见 [WALLPAPER_FORMAT.md](WALLPAPER_FORMAT.md)。

## Wallpaper Engine .pkg 兼容

支持导入 Wallpaper Engine 导出的 `.pkg` 文件，**仅限 `video` 类型**。

支持的格式版本：
- `PKGV0001` / `PKGV0005` magic header
- LZ4 块压缩解压
- 自动读取 `project.json` 元数据
- 自动提取视频文件和音频文件

不支持的类型（需要完整 Wallpaper Engine 渲染器）：
- `scene` 类型（依赖 WE 的 scene 渲染管线）
- `web` / `application` 类型（依赖 WE 的 Chromium 嵌入）

导入方式：
- 命令行：`rpaper.exe path/to/wallpaper.pkg`
- 菜单：托盘右键 → 加载壁纸包 (.rwp)... → 选择 .pkg 文件

如需使用 scene/web 类型壁纸，请在 Wallpaper Engine 编辑器中将壁纸导出为视频文件后导入。

## 技术架构

### 渲染原理

```
Progman (桌面外壳)
  └── WorkerW (壁纸层)
        └── WallpaperChild (子窗口, 拦截 WM_ERASEBKGND)
              └── wgpu Surface (DX12 后端)
```

引擎通过向 Progman 发送 `0x052C` 消息触发系统创建 WorkerW 窗口，在 WorkerW 上创建子窗口承载 GPU 渲染。壁纸出现在桌面图标**下方**，不影响正常桌面操作。

### 视频解码管线

```
.rwp / .pkg / .mp4
        │
        ▼
   Mp4Demuxer        (纯 Rust MP4 解析)
        │
        ▼
   H264Decoder       (纯 Rust H.264 解码, CAVLC + CABAC)
        │
        ▼
   YUV420 → RGBA     (SIMD-friendly 色彩转换)
        │
        ▼
   wgpu Texture      (GPU 上传)
        │
        ▼
   Full-screen Quad  (shader 渲染)
```

无任何外部依赖（不需要 ffmpeg / Media Foundation / LAV）。

### 性能

| 指标 | 数值 |
|------|------|
| 帧率 | 60fps (VSync) |
| CPU | ~6% (24核, 约 0.25 核) |
| 内存 | ~141MB |
| 二进制 | 6.7MB |

### 优化

- 粒子位置更新在 GPU compute shader 中完成，CPU 零数据传输
- Aurora 着色器 4 层 fbm + 优化 hash，三层独立极光幕布
- Fifo 呈现模式自动同步 VSync，不忙等
- 子窗口拦截 `WM_ERASEBKGND` 防止系统背景擦除
- `SetProcessDPIAware` 获取物理分辨率，避免缩放模糊
- 视频解码后台线程 + 共享 frame_slot，渲染线程零拷贝上传

## 项目结构

```
rpaper/
├── Cargo.toml
├── WALLPAPER_FORMAT.md       # 壁纸包格式文档
├── src/
│   ├── main.rs               # 入口 + 消息循环 + 托盘交互 + 命令行解析
│   ├── app.rs                # wgpu 初始化 + 渲染管理 + 壁纸切换
│   ├── desktop.rs            # WorkerW 窗口管理
│   ├── tray.rs               # 系统托盘图标 + 菜单
│   ├── settings.rs           # 设置窗口 (Win32 原生)
│   ├── audio.rs              # 背景音乐播放 (rodio)
│   ├── rwp.rs                # .rwp 壁纸包解析 (zip + serde)
│   ├── pkg.rs                # Wallpaper Engine .pkg 解析 (LZ4)
│   ├── sys_wallpaper.rs      # 系统壁纸保存/恢复 (IDesktopWallpaper COM)
│   ├── error.rs              # 错误分类 + 友好提示
│   ├── config.rs             # 持久化配置
│   ├── wallpaper.rs          # Wallpaper trait
│   ├── wallpapers/
│   │   ├── aurora.rs         # 极光效果
│   │   ├── particle.rs       # 粒子效果 (GPU compute)
│   │   ├── image.rs          # 图片壁纸
│   │   └── video.rs          # 视频壁纸 (纯 Rust 解码管线)
│   └── video/
│       ├── decoder.rs        # 解码器包装 + 状态/进度
│       ├── color.rs          # YUV→RGBA 色彩转换
│       ├── demux/            # 容器解析 (MP4)
│       └── h264/             # H.264 解码 (CAVLC/CABAC/inter/intra/deblock)
└── shaders/
    ├── aurora.wgsl           # 极光着色器
    ├── particle.wgsl         # 粒子渲染着色器
    ├── particle_compute.wgsl # 粒子计算着色器
    └── image.wgsl            # 图片/视频通用着色器
```

## 扩展新壁纸

1. 在 `src/wallpapers/` 下新建模块，实现 `Wallpaper` trait
2. 在 `app.rs` 的 `WallpaperType` 和 `load_file` 中注册
3. 在 `tray.rs` 和 `main.rs` 中添加菜单项
4.（可选）打包为 `.rwp` 格式方便分享

## 编译

```bash
cargo build --release
```

## 环境要求

- Windows 10/11
- DirectX 12 兼容 GPU
- Rust 1.75+（编译）

## License

MIT
