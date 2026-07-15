<div align="center">

# Rpaper

GPU 加速的 Windows 动态壁纸引擎

Rust + wgpu (DirectX 12) · 60fps · ~6% CPU · 6.7MB

</div>

## 功能

- **极光效果** — 多层 simplex noise 极光幕布 + 星空 + 地平线辉光
- **粒子效果** — GPU compute shader 驱动的浮动光点系统
- **图片壁纸** — PNG / JPG / BMP / WebP / GIF，cover 铺满 + 呼吸效果
- **视频壁纸** — MP4 / MKV / AVI / WebM / MOV 等，循环播放（需 ffmpeg）
- **壁纸包 (.rwp)** — 打包格式，方便社区制作和分享壁纸
- **背景音乐** — 壁纸包可附带 MP3/WAV/OGG/FLAC 音频，循环播放
- **系统托盘** — 中文菜单，右键切换效果，左键快速切换
- **DPI 感知** — 自动适配高分辨率屏幕，无缩放模糊

## 截图

（运行后补充）

## 快速开始

### 运行

```bash
# 直接运行（默认极光效果）
rpaper.exe

# 指定效果
rpaper.exe particles    # 粒子效果
rpaper.exe image        # 图片模式
rpaper.exe video        # 视频模式
```

### 托盘菜单

| 菜单项 | 说明 |
|--------|------|
| 极光效果 | 切换到 Aurora 壁纸 |
| 粒子效果 | 切换到 Particles 壁纸 |
| 选择图片... | 弹出文件对话框选择图片 |
| 选择视频... | 弹出文件对话框选择视频 |
| 加载壁纸包 (.rwp)... | 导入社区壁纸包 |
| 退出 | 关闭引擎 |

左键单击托盘图标可在极光/粒子之间快速切换。

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

## 技术架构

### 渲染原理

```
Progman (桌面外壳)
  └── WorkerW (壁纸层)
        └── WallpaperChild (子窗口, 拦截 WM_ERASEBKGND)
              └── wgpu Surface (DX12 后端)
```

引擎通过向 Progman 发送 `0x052C` 消息触发系统创建 WorkerW 窗口，在 WorkerW 上创建子窗口承载 GPU 渲染。壁纸出现在桌面图标**下方**，不影响正常桌面操作。

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

## 项目结构

```
rpaper/
├── Cargo.toml
├── WALLPAPER_FORMAT.md       # 壁纸包格式文档
├── src/
│   ├── main.rs               # 入口 + 消息循环 + 托盘交互
│   ├── app.rs                # wgpu 初始化 + 渲染管理 + 壁纸切换
│   ├── desktop.rs            # WorkerW 窗口管理
│   ├── tray.rs               # 系统托盘图标
│   ├── audio.rs              # 背景音乐播放 (rodio)
│   ├── rwp.rs                # 壁纸包解析 (zip + serde)
│   ├── wallpaper.rs          # Wallpaper trait
│   └── wallpapers/
│       ├── aurora.rs         # 极光效果
│       ├── particle.rs       # 粒子效果 (GPU compute)
│       ├── image.rs          # 图片壁纸
│       └── video.rs          # 视频壁纸 (ffmpeg 管道)
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
- ffmpeg（视频壁纸，可选）

## License

MIT
