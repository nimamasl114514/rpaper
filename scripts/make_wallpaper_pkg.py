#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""制造一个 .rwp 壁纸包并用 rpaper 加载安装"""
import os, sys, json, zipfile, time, ctypes, subprocess
from ctypes import wintypes
from PIL import Image
sys.stdout.reconfigure(encoding='utf-8')

EXE = r"d:\nim\rpaper\target\release\rpaper.exe"
OUT_DIR = r"d:\nim\rpaper\scripts\test_results"
os.makedirs(OUT_DIR, exist_ok=True)

# === 1. 生成一张漂亮的壁纸图片 ===
print("=" * 50)
print("1. 生成壁纸图片")
print("=" * 50)
img = Image.new('RGB', (1920, 1080))
pixels = img.load()
# 渐变：左上深蓝 → 右下紫色，中间有亮带
for y in range(1080):
    for x in range(1920):
        # 对角渐变
        t = (x / 1920 + y / 1080) / 2
        r = int(20 + t * 80)
        g = int(15 + t * 30)
        b = int(80 + (1 - t) * 120)
        # 中间亮带
        if 400 < y < 680:
            band = 1.0 - abs(y - 540) / 140
            r = min(255, int(r + band * 60))
            g = min(255, int(g + band * 40))
            b = min(255, int(b + band * 30))
        pixels[x, y] = (r, g, b)

img_path = os.path.join(OUT_DIR, "wallpaper.png")
img.save(img_path)
print(f"  图片已生成: {img_path} ({os.path.getsize(img_path)} bytes)")

# === 2. 打包成 .rwp 壁纸包 ===
print("\n" + "=" * 50)
print("2. 打包 .rwp 壁纸包")
print("=" * 50)
manifest = {
    "name": "紫蓝渐变",
    "type": "image",
    "author": "Rpaper Test",
    "description": "深蓝到紫色对角渐变壁纸",
    "audio": None,
    "params": None,
}

rwp_path = os.path.join(OUT_DIR, "gradient_wallpaper.rwp")
with zipfile.ZipFile(rwp_path, 'w', zipfile.ZIP_DEFLATED) as zf:
    zf.writestr("manifest.json", json.dumps(manifest, ensure_ascii=False, indent=2))
    zf.write(img_path, "wallpaper.png")
print(f"  壁纸包已生成: {rwp_path} ({os.path.getsize(rwp_path)} bytes)")
print(f"  manifest: {manifest}")

# === 3. 启动 rpaper 并加载壁纸包 ===
print("\n" + "=" * 50)
print("3. 启动 rpaper 并加载壁纸包")
print("=" * 50)

# 先清理旧进程
subprocess.run("taskkill /F /IM rpaper.exe", shell=True, capture_output=True)
time.sleep(2)

# 用 .rwp 文件作为参数启动 rpaper
proc = subprocess.Popen([EXE, rwp_path], creationflags=0x00000008)
print(f"  进程已启动 PID={proc.pid}")
time.sleep(5)

# 检查进程是否存活
if proc.poll() is None:
    print("  ✓ rpaper 成功加载壁纸包，进程存活")
else:
    print(f"  ✗ rpaper 退出，exit code={proc.poll()}")

# === 4. 打开设置窗口验证 ===
print("\n" + "=" * 50)
print("4. 打开设置窗口验证壁纸已加载")
print("=" * 50)

user32 = ctypes.windll.user32
user32.FindWindowW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR]
user32.FindWindowW.restype = wintypes.HWND

hidden = user32.FindWindowW("WallpaperMsg", None)
print(f"  隐藏窗口: HWND={hidden:#x}" if hidden else "  ✗ 隐藏窗口未找到")

if hidden:
    WM_COMMAND = 0x0111
    CMD_OPEN_SETTINGS = 2001
    WM_CLOSE = 0x0010
    user32.SendMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
    user32.SendMessageW(hidden, WM_COMMAND, CMD_OPEN_SETTINGS, 0)
    time.sleep(2)

    settings = user32.FindWindowW("RpaperSettings", None)
    if settings:
        print(f"  ✓ 设置窗口已打开 HWND={settings:#x}")

        # 检查壁纸选择单选 — "图片" 应该被选中
        user32.GetDlgItem.argtypes = [wintypes.HWND, ctypes.c_int]
        user32.GetDlgItem.restype = wintypes.HWND
        # IDC_RADIO_IMAGE = 1005
        radio_image = user32.GetDlgItem(settings, 1005)
        if radio_image:
            BM_GETCHECK = 0x00F0
            checked = user32.SendMessageW(radio_image, BM_GETCHECK, 0, 0)
            print(f"  图片单选状态: {'✓ 已选中' if checked == 1 else '✗ 未选中'}")

        # 检查当前文件路径显示
        # IDC_CURRENT_FILE = 1021
        file_ctrl = user32.GetDlgItem(settings, 1021)
        if file_ctrl:
            WM_GETTEXT = 0x000D
            buf = ctypes.create_unicode_buffer(256)
            user32.SendMessageW(file_ctrl, WM_GETTEXT, 256, ctypes.addressof(buf))
            print(f"  当前文件路径: '{buf.value}'")

        # 截图保存
        import ctypes
        PW_RENDERFULLCONTENT = 2
        user32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
        rect = wintypes.RECT()
        user32.GetWindowRect(settings, ctypes.byref(rect))
        w = rect.right - rect.left
        h = rect.bottom - rect.top
        hdc_screen = user32.GetDC(0)
        hdc_mem = ctypes.windll.gdi32.CreateCompatibleDC(hdc_screen)
        hbmp = ctypes.windll.gdi32.CreateCompatibleBitmap(hdc_screen, w, h)
        ctypes.windll.gdi32.SelectObject(hdc_mem, hbmp)
        user32.PrintWindow(settings, hdc_mem, PW_RENDERFULLCONTENT)
        time.sleep(0.3)

        class BITMAPINFOHEADER(ctypes.Structure):
            _fields_ = [("biSize", ctypes.c_uint32), ("biWidth", ctypes.c_int32),
                        ("biHeight", ctypes.c_int32), ("biPlanes", ctypes.c_uint16),
                        ("biBitCount", ctypes.c_uint16), ("biCompression", ctypes.c_uint32),
                        ("biSizeImage", ctypes.c_uint32), ("biXPelsPerMeter", ctypes.c_int32),
                        ("biYPelsPerMeter", ctypes.c_int32), ("biClrUsed", ctypes.c_uint32),
                        ("biClrImportant", ctypes.c_uint32)]
        bi = BITMAPINFOHEADER()
        bi.biSize = ctypes.sizeof(BITMAPINFOHEADER)
        bi.biWidth = w
        bi.biHeight = -h
        bi.biPlanes = 1
        bi.biBitCount = 24
        bi.biCompression = 0
        buf2 = ctypes.create_string_buffer(w * h * 3)
        ctypes.windll.gdi32.GetDIBits(hdc_mem, hbmp, 0, h, buf2, ctypes.byref(bi), 0)
        screenshot = Image.frombytes("RGB", (w, h), buf2.raw, "raw", "BGR")
        ctypes.windll.gdi32.DeleteObject(hbmp)
        ctypes.windll.gdi32.DeleteDC(hdc_mem)
        user32.ReleaseDC(0, hdc_screen)

        shot_path = os.path.join(OUT_DIR, "wallpaper_loaded.png")
        screenshot.save(shot_path)
        print(f"  截图已保存: {shot_path}")

        # 关闭设置窗口
        user32.PostMessageW(settings, WM_CLOSE, 0, 0)
        time.sleep(1)
    else:
        print("  ✗ 设置窗口未打开")

# === 5. 检查配置文件 ===
print("\n" + "=" * 50)
print("5. 检查配置持久化")
print("=" * 50)
config_path = os.path.join(os.environ.get("APPDATA", ""), "rpaper", "config.json")
if os.path.exists(config_path):
    with open(config_path, "r", encoding="utf-8") as f:
        cfg = f.read()
    print(f"  配置文件: {config_path}")
    print(f"  内容: {cfg}")
    if "image" in cfg and "gradient_wallpaper" in cfg:
        print("  ✓ 配置已持久化壁纸包路径")
    else:
        print("  ⚠ 配置中未找到壁纸包路径")
else:
    print("  ✗ 配置文件不存在")

# === 6. 清理 ===
print("\n" + "=" * 50)
print("6. 清理")
print("=" * 50)
subprocess.run("taskkill /F /IM rpaper.exe", shell=True, capture_output=True)
time.sleep(2)
print("  ✓ rpaper 进程已终止")

# 清理临时文件
for tmp in [img_path]:
    try:
        os.remove(tmp)
    except:
        pass

print("\n" + "=" * 50)
print("完成！壁纸包制造+安装验证流程结束")
print("=" * 50)
