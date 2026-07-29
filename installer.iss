; Rpaper 动态壁纸引擎 — Inno Setup 安装脚本
; 用 Inno Setup 6 编译生成安装包

#define MyAppName "Rpaper"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "Rpaper"
#define MyAppExeName "rpaper.exe"

[Setup]
AppId={{Rpaper-Dynamic-Wallpaper-Engine}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
; 安装包图标和卸载图标都用 exe 自带的图标
SetupIconFile=res\rpaper.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
; 输出目录
OutputDir=installer
OutputBaseFilename=Rpaper-Setup-{#MyAppVersion}
; 压缩
Compression=lzma2/ultra
SolidCompression=yes
; 权限 — HKCU 安装不需要管理员
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
; Win10/11 风格安装界面
WizardStyle=modern
; 勾选"创建桌面快捷方式"
DisableProgramGroupPage=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "chinesesimplified"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "附加图标:"; Flags: unchecked
Name: "autostart"; Description: "开机自动启动 Rpaper"; GroupDescription: "其他:"; Flags: unchecked

[Files]
; 主程序
Source: "target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; shader 文件（运行时需要）
Source: "shaders\*"; DestDir: "{app}\shaders"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppExeName}"
Name: "{group}\卸载 {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppExeName}"; Tasks: desktopicon
Name: "{userstartup}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: autostart

[Registry]
; 文件关联 — 安装时注册（卸载时自动清理）
Root: HKCU; Subkey: "Software\Classes\.rwp"; ValueType: string; ValueName: ""; ValueData: "Rpaper.WallpaperPackage"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\Rpaper.WallpaperPackage"; ValueType: string; ValueName: ""; ValueData: "Rpaper 壁纸包"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Rpaper.WallpaperPackage\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#MyAppExeName},0"
Root: HKCU; Subkey: "Software\Classes\Rpaper.WallpaperPackage\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""
; .pkg (Wallpaper Engine)
Root: HKCU; Subkey: "Software\Classes\.pkg"; ValueType: string; ValueName: ""; ValueData: "Rpaper.WallpaperEnginePkg"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\Rpaper.WallpaperEnginePkg"; ValueType: string; ValueName: ""; ValueData: "Wallpaper Engine 壁纸包"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Rpaper.WallpaperEnginePkg\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#MyAppExeName},0"
Root: HKCU; Subkey: "Software\Classes\Rpaper.WallpaperEnginePkg\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""

[Run]
; 安装完成后启动 Rpaper（可选）
Filename: "{app}\{#MyAppExeName}"; Description: "立即启动 Rpaper"; Flags: nowait postinstall skipifsilent
