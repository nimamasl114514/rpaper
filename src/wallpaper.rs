//! Wallpaper trait — 所有壁纸效果的统一接口

use wgpu::*;

/// 壁纸渲染接口
pub trait Wallpaper: Send + 'static {
    /// 初始化 GPU 资源
    fn init(device: &Device, config: &SurfaceConfiguration, format: TextureFormat) -> Self
    where
        Self: Sized;

    /// 渲染一帧
    fn render(&self, view: &TextureView, encoder: &mut CommandEncoder);
}
