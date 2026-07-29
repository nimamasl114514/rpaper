//! 编译时配置：
//! 1. 嵌入 manifest — 启用 comctl32 v6 visual styles + PerMonitorV2 DPI 感知
//! 2. 编译 .rc 资源文件 — 嵌入应用图标 + 版本信息

fn main() {
    // 嵌入 manifest（现代控件风格 + DPI 感知）
    embed_manifest::embed_manifest_file("app.manifest")
        .expect("嵌入 manifest 失败");
    println!("cargo:rerun-if-changed=app.manifest");

    // 编译 .rc 资源文件（图标 + VersionInfo）
    // embed-resource 会自动查找 Windows SDK 的 rc.exe 并编译链接
    embed_resource::compile("rpaper.rc", embed_resource::NONE);
    println!("cargo:rerun-if-changed=rpaper.rc");
    println!("cargo:rerun-if-changed=res/rpaper.ico");
}
