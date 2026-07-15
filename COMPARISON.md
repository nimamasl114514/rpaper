# Rpaper 与同类动态壁纸项目对比分析

## 对比项目一览

| 项目 | 类型 | 技术栈 | 开源协议 | GitHub Stars |
|------|------|--------|----------|-------------|
| **Rpaper** | 开源 | Rust + wgpu (DirectX 12) | MIT | 新项目 |
| **Wallpaper Engine** | 商业 | C++ (自定义引擎) | 闭源 | N/A (Steam 商店) |
| **Lively Wallpaper** | 开源 | C# / .NET / WPF | GPLv3 | ~4.5k+ |
| **Sucrose Wallpaper Engine** | 开源 | C# / .NET | GPLv3 | ~1k+ |
| **AutoWall** | 开源 | C# / .NET / FFmpeg | MIT | ~800+ |
| **flowy** | 开源 | Rust (CLI 工具) | MIT | 少量 |

## 多维度对比

### 1. 壁纸类型支持

| 特性 | Rpaper | Wallpaper Engine | Lively | Sucrose | AutoWall |
|------|--------|-----------------|--------|---------|----------|
| 着色器 (Shader) | WGSL (内置极光/粒子) | 自定义着色器语言 | HLSL/GLSL | 支持 | 不支持 |
| 视频壁纸 | MP4/MKV/AVI/WebM (需 ffmpeg) | 几乎所有格式 (内置解码器) | MP4/WebM/GIF 等 | 多种格式 | MP4/GIF |
| 图片壁纸 | PNG/JPG/BMP/WebP/GIF | 支持 | 支持 | 支持 | 不支持 |
| 网页壁纸 | 不支持 | 支持 (CEF 嵌入) | 支持 (WebView2) | 支持 | 不支持 |
| GIF 动画 | 支持 (逐帧) | 支持 | 支持 | 支持 | 支持 |
| 交互式壁纸 | 不支持 | 支持 (脚本系统) | 支持 (网页) | 支持 | 不支持 |

### 2. 技术架构

| 特性 | Rpaper | Wallpaper Engine | Lively | Sucrose | AutoWall |
|------|--------|-----------------|--------|---------|----------|
| GPU 渲染 | wgpu (DX12/Vulkan) | 自定义引擎 (DX11) | MPV (视频) / C# 渲染 | C# 渲染 | FFmpeg + MPV |
| Compute Shader | 支持 (粒子系统) | 支持 | 不支持 | 不支持 | 不支持 |
| 桌面层级方案 | WorkerW 子窗口 | WorkerW (DirectX) | WorkerW / MPV | WorkerW | WorkerW |
| 内存安全 | Rust (编译期保证) | C++ (手动管理) | C# (GC) | C# (GC) | C# (GC) |
| 跨平台潜力 | wgpu 抽象层支持 | 仅 Windows | 仅 Windows | 仅 Windows | 仅 Windows |

### 3. 性能与资源占用

| 指标 | Rpaper | Wallpaper Engine | Lively | Sucrose | AutoWall |
|------|--------|-----------------|--------|---------|----------|
| CPU 占用 | ~6% (60fps) | ~2-8% (可配置) | ~5-15% | ~5-12% | ~10-20% |
| 内存占用 | ~141MB | ~150-300MB | ~200-400MB | ~150-300MB | ~100-200MB |
| 可执行文件大小 | 6.7MB | ~50MB+ (含运行时) | ~100MB+ (含 .NET) | ~80MB+ | ~50MB+ |
| 帧率控制 | VSync 60fps (Fifo) | 可配置 30/60/120 | VSync | VSync | 无限制 |
| 低配优化 | GPU compute shader 卸载 CPU | 多档质量设置 | 一般 | 一般 | 较差 |

### 4. 社区与生态

| 特性 | Rpaper | Wallpaper Engine | Lively | Sucrose | AutoWall |
|------|--------|-----------------|--------|---------|----------|
| 壁纸打包格式 | .rwp (ZIP+JSON) | .pkg (专用格式) | 无统一格式 | 无统一格式 | 无 |
| 社区壁纸市场 | 待建设 | Steam 创意工坊 (海量) | 社区分享 | 社区分享 | 无 |
| 背景音乐 | 支持 (MP3/WAV/OGG/FLAC) | 支持 | 不支持 | 支持 | 不支持 |
| 配置持久化 | 待实现 | 完整配置系统 | 完整 | 完整 | 简单 |
| 多显示器支持 | 待实现 | 支持 | 支持 | 支持 | 不支持 |
| 国际化 | 中文界面 | 多语言 | 多语言 | 多语言 | 英文 |

### 5. 用户体验

| 特性 | Rpaper | Wallpaper Engine | Lively | Sucrose | AutoWall |
|------|--------|-----------------|--------|---------|----------|
| 安装方式 | 编译或下载 exe | Steam 安装 | MSIX/便携版 | 安装程序 | 便携版 |
| 控制台窗口 | 无 (windows_subsystem) | 无 | 无 | 无 | 可能有 |
| 系统托盘 | 支持 (中文菜单) | 支持 | 支持 | 支持 | 不支持 |
| 开机自启 | 待实现 | 支持 | 支持 | 支持 | 不支持 |
| 暂停/恢复 | 待实现 | 支持 (全屏暂停) | 支持 | 支持 | 不支持 |

## Rpaper 的优势

1. **Rust 内存安全**：编译期消除空指针、数据竞争，无 GC 停顿，长时间运行稳定性优于 C# 方案
2. **wgpu 跨图形 API 抽象**：当前使用 DX12，未来可扩展 Vulkan/Metal/WebGPU，跨平台潜力大
3. **GPU Compute Shader**：粒子系统在 GPU 上计算，CPU 几乎零负载，这是多数开源竞品不具备的
4. **极致轻量**：6.7MB 可执行文件，无运行时依赖（对比 Lively/Sucrose 需要 .NET Runtime）
5. **.rwp 社区格式**：ZIP+JSON 的开放格式，任何人都能制作和分享，门槛低于 Wallpaper Engine 的 .pkg
6. **MIT 协议**：最宽松的开源协议，允许商用和闭源衍生（对比 Lively/Sucrose 的 GPLv3 传染性）

## Rpaper 的不足与改进方向

| 不足项 | 严重程度 | 竞品参考 | 改进建议 |
|--------|---------|---------|---------|
| 无网页壁纸 | 高 | Lively/Sucrose/WE 均支持 | 集成 WebView2 或 winit + browser 引擎 |
| 无多显示器支持 | 中 | WE/Lively/Sucrose 支持 | 枚举显示器，为每个显示器创建独立 Surface |
| 视频依赖外部 ffmpeg | 中 | WE 内置解码器 | 考虑使用 `gstreamer` crate 或 Media Foundation |
| 无配置持久化 | 中 | 所有竞品均有 | 添加 `serde` 序列化到 JSON 配置文件 |
| 无开机自启 | 低 | WE/Lively/Sucrose 支持 | 写入注册表 `Run` 键或创建快捷方式 |
| 无交互式壁纸 | 低 | WE 脚本系统 | 可考虑嵌入 Lua/JS 运行时 |
| 无壁纸市场 | 低 | WE Steam 创意工坊 | 可建设 GitHub 仓库作为社区壁纸集散地 |
| 无暂停机制 | 低 | WE 全屏自动暂停 | 检测前台全屏窗口，暂停渲染降低占用 |

## 定位总结

Rpaper 走的是**轻量级、高性能、技术前沿**的路线。它不追求功能大而全（如 Wallpaper Engine 的生态），而是在核心技术上做到极致——用 Rust 保证安全，用 wgpu 实现 GPU 加速，用 Compute Shader 释放 CPU。对于追求性能和技术深度的用户，Rpaper 是一个有吸引力的选择。

与同为社会开源项目的 Lively/Sucrose/AutoWall 相比，Rpaper 的技术栈更现代（Rust vs C#），二进制更小（6.7MB vs 80-100MB+），且 MIT 协议对社区贡献更友好。当前的差距主要在功能完整度（多显示器、网页壁纸、配置系统）上，这些都可以通过迭代逐步补齐。
