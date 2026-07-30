//! 编译时配置：
//! 1. 嵌入 manifest — 启用 comctl32 v6 visual styles + PerMonitorV2 DPI 感知
//! 2. 编译 .rc 资源文件 — 嵌入应用图标 + 版本信息
//! 3. 编译 .slint UI 文件

fn main() {
    // 嵌入 manifest（现代控件风格 + DPI 感知）
    embed_manifest::embed_manifest_file("app.manifest")
        .expect("嵌入 manifest 失败");
    println!("cargo:rerun-if-changed=app.manifest");

    // 编译 .rc 资源文件（图标 + VersionInfo）
    embed_resource::compile("rpaper.rc", embed_resource::NONE);
    println!("cargo:rerun-if-changed=rpaper.rc");
    println!("cargo:rerun-if-changed=res/rpaper.ico");

    // 编译 Slint UI 文件
    let mut config = slint_build::CompilerConfiguration::new();
    config = config.with_style("fluent".into()); // 使用 Win11 Fluent 风格
    slint_build::compile_with_config("ui/library.slint", config)
        .expect("Slint UI 编译失败");
    println!("cargo:rerun-if-changed=ui/library.slint");
}
