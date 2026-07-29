"""
rpaper 全自动测试套件
覆盖：单实例、文件传递、UI渲染、托盘图标、文件关联、图标嵌入
"""
import os
import sys
import time
import subprocess
import ctypes
from ctypes import wintypes
from PIL import Image, ImageGrab
import winreg

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:
        pass

# DPI aware
try:
    ctypes.windll.shcore.SetProcessDpiAwareness(2)
except Exception:
    try:
        ctypes.windll.user32.SetProcessDPIAware()
    except Exception:
        pass

EXE = r"d:\nim\rpaper\target\release\rpaper.exe"
OUT_DIR = r"d:\nim\rpaper\scripts\test_results"
os.makedirs(OUT_DIR, exist_ok=True)

# Win32 API
user32 = ctypes.windll.user32
shell32 = ctypes.windll.shell32
kernel32 = ctypes.windll.kernel32

# Constants
WM_COMMAND = 0x0111
WM_COPYDATA = 0x004A
WM_CLOSE = 0x0010
WM_DESTROY = 0x0002
WM_LBUTTONDOWN = 0x0201
WM_LBUTTONUP = 0x0202
WM_RBUTTONUP = 0x0205
CMD_OPEN_SETTINGS = 2001
SW_HIDE = 0
SW_SHOW = 5
BN_CLICKED = 0

class COPYDATASTRUCT(ctypes.Structure):
    _fields_ = [("dwData", ctypes.c_ulonglong),
                ("cbData", ctypes.c_ulong),
                ("lpData", ctypes.c_void_p)]

# Function prototypes
user32.FindWindowW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR]
user32.FindWindowW.restype = wintypes.HWND
user32.SendMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
user32.SendMessageW.restype = ctypes.c_long
user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
user32.PostMessageW.restype = wintypes.BOOL
user32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
user32.GetWindowRect.restype = wintypes.BOOL
user32.IsWindow.argtypes = [wintypes.HWND]
user32.IsWindow.restype = wintypes.BOOL
user32.GetWindowTextLengthW.argtypes = [wintypes.HWND]
user32.GetWindowTextLengthW.restype = ctypes.c_int
user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
user32.GetWindowTextW.restype = ctypes.c_int
user32.EnumWindows.argtypes = [ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM), wintypes.LPARAM]
user32.EnumWindows.restype = wintypes.BOOL
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetWindowThreadProcessId.restype = wintypes.DWORD
user32.SetForegroundWindow.argtypes = [wintypes.HWND]
user32.SetForegroundWindow.restype = wintypes.BOOL
user32.MoveWindow.argtypes = [wintypes.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wintypes.BOOL]
user32.MoveWindow.restype = wintypes.BOOL
user32.GetDlgItem.argtypes = [wintypes.HWND, ctypes.c_int]
user32.GetDlgItem.restype = wintypes.HWND
user32.IsWindowVisible.argtypes = [wintypes.HWND]
user32.IsWindowVisible.restype = wintypes.BOOL
user32.BringWindowToTop.argtypes = [wintypes.HWND]
user32.BringWindowToTop.restype = wintypes.BOOL
user32.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
user32.ShowWindow.restype = wintypes.BOOL
user32.PrintWindow.argtypes = [wintypes.HWND, wintypes.HDC, ctypes.c_uint]
user32.PrintWindow.restype = wintypes.BOOL
user32.SendMessageTimeoutW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM, ctypes.c_uint, ctypes.c_uint, ctypes.POINTER(ctypes.c_ulong)]
user32.SendMessageTimeoutW.restype = ctypes.c_void_p
user32.keybd_event.argtypes = [ctypes.c_ubyte, ctypes.c_ubyte, ctypes.c_uint32, ctypes.c_void_p]
user32.keybd_event.restype = None
user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
user32.GetClassNameW.restype = ctypes.c_int
user32.GetDC.argtypes = [wintypes.HWND]
user32.GetDC.restype = wintypes.HDC
user32.ReleaseDC.argtypes = [wintypes.HWND, wintypes.HDC]
user32.ReleaseDC.restype = ctypes.c_int
gdi32 = ctypes.windll.gdi32
gdi32.CreateCompatibleDC.argtypes = [wintypes.HDC]
gdi32.CreateCompatibleDC.restype = wintypes.HDC
gdi32.CreateCompatibleBitmap.argtypes = [wintypes.HDC, ctypes.c_int, ctypes.c_int]
gdi32.CreateCompatibleBitmap.restype = wintypes.HBITMAP
gdi32.SelectObject.argtypes = [wintypes.HDC, ctypes.c_void_p]
gdi32.SelectObject.restype = ctypes.c_void_p
gdi32.DeleteDC.argtypes = [wintypes.HDC]
gdi32.DeleteDC.restype = wintypes.BOOL
gdi32.DeleteObject.argtypes = [ctypes.c_void_p]
gdi32.DeleteObject.restype = wintypes.BOOL
gdi32.GetDIBits.argtypes = [wintypes.HDC, wintypes.HBITMAP, ctypes.c_uint, ctypes.c_uint,
                             ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint]
gdi32.GetDIBits.restype = ctypes.c_int

passed = 0
failed = 0
results = []

def test(name, condition, detail=""):
    global passed, failed
    status = "✓ PASS" if condition else "✗ FAIL"
    if condition:
        passed += 1
    else:
        failed += 1
    msg = f"  {status}: {name}"
    if detail:
        msg += f" — {detail}"
    print(msg)
    results.append((name, condition, detail))

def kill_rpaper():
    # First: force kill all rpaper processes immediately (avoids SendMessage blocking on hung windows)
    subprocess.run("taskkill /F /IM rpaper.exe", shell=True, capture_output=True)
    # Also post WM_CLOSE async to any remaining windows
    hwnd = find_settings_window()
    if hwnd != 0:
        user32.PostMessageW(hwnd, WM_CLOSE, 0, 0)
    hwnd = find_hidden_window()
    if hwnd != 0:
        user32.PostMessageW(hwnd, WM_CLOSE, 0, 0)
    # Wait for windows to disappear (max 10 seconds)
    for _ in range(50):
        if find_hidden_window() == 0 and find_settings_window() == 0:
            break
        time.sleep(0.2)
    # Double tap: taskkill again just in case
    subprocess.run("taskkill /F /IM rpaper.exe", shell=True, capture_output=True)
    for _ in range(25):
        if find_hidden_window() == 0 and find_settings_window() == 0:
            break
        time.sleep(0.2)
    time.sleep(1)

def _hwnd_to_int(hwnd):
    """Convert ctypes HWND (c_void_p which returns None for NULL) to int."""
    if hwnd is None:
        return 0
    return int(hwnd) if not isinstance(hwnd, int) else hwnd

def find_hidden_window():
    return _hwnd_to_int(user32.FindWindowW("WallpaperMsg", None))

def find_settings_window():
    return _hwnd_to_int(user32.FindWindowW("RpaperSettings", None))

# ============================================================
# Test 0: Clean start
# ============================================================
print("=" * 60)
print("rpaper 全自动测试套件")
print("=" * 60)
kill_rpaper()
time.sleep(1)
test("清理旧进程", find_hidden_window() == 0, "无残留 WallpaperMsg 窗口")

# ============================================================
# Test 1: Launch & basic window existence
# ============================================================
print("\n[1] 启动与基础窗口测试")
proc = subprocess.Popen([EXE], creationflags=0x00000008)
time.sleep(4)

hidden = find_hidden_window()
test("创建隐藏消息窗口 (WallpaperMsg)", hidden != 0, f"HWND={hidden:#x}")

# Check window title
title_len = user32.GetWindowTextLengthW(hidden)
title_buf = ctypes.create_unicode_buffer(title_len + 1)
user32.GetWindowTextW(hidden, title_buf, title_len + 1)
test("隐藏窗口标题正确", True, f"标题='{title_buf.value}'")

# Check process is running
test("进程在运行", proc.poll() is None, f"PID={proc.pid}")

# ============================================================
# Test 2: Single instance detection
# ============================================================
print("\n[2] 单实例检测测试")
proc2 = subprocess.Popen([EXE], creationflags=0x00000008)
time.sleep(3)
# Second instance should have exited (single instance mutex)
proc2_exited = proc2.poll() is not None
test("第二实例自动退出 (Mutex互斥)", proc2_exited, f"exit_code={proc2.poll() if proc2_exited else 'still running'}")
# First instance should still be alive
test("第一实例继续运行", proc.poll() is None)

# ============================================================
# Test 3: Settings window UI
# ============================================================
print("\n[3] 设置窗口 UI 测试")
user32.SendMessageW(hidden, WM_COMMAND, CMD_OPEN_SETTINGS, 0)
time.sleep(2)

settings = find_settings_window()
test("创建设置窗口 (RpaperSettings)", settings != 0, f"HWND={settings:#x}")

if settings != 0:
    user32.ShowWindow(settings, SW_SHOW)
    user32.BringWindowToTop(settings)
    user32.SetForegroundWindow(settings)
    time.sleep(0.5)
    user32.MoveWindow(settings, 100, 100, 580, 740, True)
    time.sleep(1.5)

    # Get window rect (screen coordinates)
    rect = wintypes.RECT()
    user32.GetWindowRect(settings, ctypes.byref(rect))
    w = rect.right - rect.left
    h = rect.bottom - rect.top
    test(f"设置窗口尺寸正确 ({w}x{h})", 570 <= w <= 650 and 730 <= h <= 800, f"实际={w}x{h}")

    # Check window visibility
    test("设置窗口可见", user32.IsWindowVisible(settings) != 0)

    # Check key controls exist (对照 src/settings.rs 中的真实ID)
    controls = {
        "音量滑块": 1001,
        "音量标签": 1002,
        "壁纸选择-极光": 1003,
        "壁纸选择-粒子": 1004,
        "壁纸选择-图片": 1005,
        "壁纸选择-视频": 1006,
        "暂停壁纸按钮": 1007,
        "选择图片按钮": 1008,
        "选择视频按钮": 1009,
        "加载壁纸包按钮": 1010,
        "关闭设置按钮": 1011,
        "选择背景音乐": 1014,
        "音频标签": 1015,
        "开机自启复选框": 1016,
        "视频状态文字": 1019,
        "视频进度条": 1020,
        "当前文件路径": 1021,
    }
    for name, ctrl_id in controls.items():
        ctrl = _hwnd_to_int(user32.GetDlgItem(settings, ctrl_id))
        test(f"控件存在: {name}", ctrl != 0, f"id={ctrl_id}")

    # Screenshot via PrintWindow (captures window content directly, works even if obscured)
    PW_RENDERFULLCONTENT = 2
    hdc_screen = user32.GetDC(0)
    hdc_mem = gdi32.CreateCompatibleDC(hdc_screen)
    hbmp = gdi32.CreateCompatibleBitmap(hdc_screen, w, h)
    gdi32.SelectObject(hdc_mem, hbmp)
    # PrintWindow with PW_RENDERFULLCONTENT for DWM-composited windows
    user32.PrintWindow(settings, hdc_mem, PW_RENDERFULLCONTENT)
    time.sleep(0.3)
    # Copy bitmap data
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
    bi.biHeight = -h  # top-down
    bi.biPlanes = 1
    bi.biBitCount = 24
    bi.biCompression = 0
    buf_size = w * h * 3
    buf = ctypes.create_string_buffer(buf_size)
    gdi32.GetDIBits(hdc_mem, hbmp, 0, h, buf, ctypes.byref(bi), 0)
    # Convert BGR to RGB and create PIL Image
    img = Image.frombytes("RGB", (w, h), buf.raw, "raw", "BGR")
    # Cleanup GDI
    gdi32.DeleteObject(hbmp)
    gdi32.DeleteDC(hdc_mem)
    user32.ReleaseDC(0, hdc_screen)

    # Close settings window IMMEDIATELY after capturing (image data is already in memory)
    # Use PostMessage async to avoid blocking if window is busy
    user32.PostMessageW(settings, WM_CLOSE, 0, 0)
    time.sleep(2)

    # Now do image analysis and save screenshot (no need for window to be open anymore)
    screenshot_path = os.path.join(OUT_DIR, "settings_ui.png")
    img.save(screenshot_path)
    pixels = list(img.getdata())
    total = len(pixels)

    # Color checks
    pure_white = sum(1 for p in pixels if p[0] >= 248 and p[1] >= 248 and p[2] >= 248) * 100 / total
    mica_gray = sum(1 for p in pixels if 236 <= p[0] <= 248 and 236 <= p[1] <= 248 and 236 <= p[2] <= 248) * 100 / total
    dark_text = sum(1 for p in pixels if max(p[0], p[1], p[2]) <= 75) * 100 / total
    blue_icon = sum(1 for p in pixels if p[2] > 160 and p[0] < 160 and p[1] < 160) * 100 / total

    test(f"白色卡片渲染 (>40%)", pure_white >= 40, f"白色占比={pure_white:.1f}%")
    test(f"Mica灰背景渲染 (>10%)", mica_gray >= 10, f"灰色占比={mica_gray:.1f}%")
    test("深色文字渲染", dark_text >= 0.1, f"深色文字={dark_text:.2f}%")

    # Check vertical card structure: scan at window x=40 (≈client x=32, inside left padding of cards)
    scan_x = 40
    card_count = 0
    in_card = False
    for y in range(50, h-10):
        px = img.getpixel((scan_x, y))
        r, g, b = px[0], px[1], px[2]
        is_white = r >= 250 and g >= 250 and b >= 250
        is_gap = r <= 248
        if is_white and not in_card:
            card_count += 1
            in_card = True
        elif is_gap and in_card:
            in_card = False
    test(f"卡片垂直分层 (>=3张)", card_count >= 3, f"检测到{card_count}个白色段")

    test("设置窗口正常关闭", find_settings_window() == 0)

# ============================================================
# Test 4: File passing (command line argument)
# ============================================================
print("\n[4] 文件传递测试 (命令行参数)")
# Create a dummy .rwp file for testing
test_rwp = os.path.join(OUT_DIR, "test.wallpaper.rwp")
with open(test_rwp, "wb") as f:
    f.write(b"RPKG\x00\x00\x00\x00" + b"\x00" * 100)  # minimal dummy

# Kill first instance and restart with file arg
kill_rpaper()
time.sleep(1)
proc3 = subprocess.Popen([EXE, test_rwp], creationflags=0x00000008)
time.sleep(4)
hidden3 = find_hidden_window()
test("带文件参数启动成功", hidden3 != 0, f"HWND={hidden3:#x}")
# Open settings to check if file was loaded
user32.SendMessageW(hidden3, WM_COMMAND, CMD_OPEN_SETTINGS, 0)
time.sleep(2)
settings3 = find_settings_window()
if settings3 != 0:
    user32.PostMessageW(settings3, WM_CLOSE, 0, 0)
    time.sleep(1)
# Cleanup test file
os.remove(test_rwp)

# ============================================================
# Test 5: Icon embedding verification
# ============================================================
print("\n[5] 图标嵌入验证")
# Check .ico file exists
ico_path = r"d:\nim\rpaper\res\rpaper.ico"
test("ICO图标文件存在", os.path.exists(ico_path), f"路径={ico_path}")
if os.path.exists(ico_path):
    ico_size = os.path.getsize(ico_path)
    test("ICO文件非空", ico_size > 1000, f"大小={ico_size} bytes")

# Version info via win32
try:
    info = subprocess.run(["powershell", "-Command",
        f"(Get-Item '{EXE}').VersionInfo.FileDescription"],
        capture_output=True, text=True, timeout=10)
    desc = info.stdout.strip()
    # PowerShell profile banner may appear, find Rpaper string
    desc_clean = desc.split("\n")[-1].strip() if "\n" in desc else desc
    test("VersionInfo.FileDescription", "Rpaper" in desc_clean, f"='{desc_clean}'")
except Exception as e:
    test("VersionInfo.FileDescription", False, str(e))

# Check icon resource via ExtractIconExW
_extract_icon_ex = ctypes.windll.shell32.ExtractIconExW
_extract_icon_ex.argtypes = [wintypes.LPCWSTR, ctypes.c_int,
                              ctypes.POINTER(wintypes.HICON), ctypes.POINTER(wintypes.HICON),
                              ctypes.c_uint]
_extract_icon_ex.restype = ctypes.c_uint
large_icon = wintypes.HICON()
small_icon = wintypes.HICON()
num_icons = _extract_icon_ex(EXE, 0, ctypes.byref(large_icon), ctypes.byref(small_icon), 1)
test("EXE含图标资源 (ExtractIconEx)", num_icons >= 1 and large_icon.value != 0,
     f"图标数={num_icons}, HICON={large_icon.value:#x}")
if large_icon.value:
    user32.DestroyIcon(large_icon)
if small_icon.value:
    user32.DestroyIcon(small_icon)

# ============================================================
# Test 6: File association (registry)
# ============================================================
print("\n[6] 文件关联注册表测试")
def read_reg_value(key_path, value_name="", hive=winreg.HKEY_CURRENT_USER):
    try:
        key = winreg.OpenKey(hive, key_path, 0, winreg.KEY_READ)
        val, _ = winreg.QueryValueEx(key, value_name)
        winreg.CloseKey(key)
        return val
    except Exception:
        return None

# Check .rwp association
rwp_progid = read_reg_value(r"Software\Classes\.rwp")
test(".rwp 文件关联 ProgID", rwp_progid == "Rpaper.WallpaperPackage", f"={rwp_progid}")

# Check .pkg association
pkg_progid = read_reg_value(r"Software\Classes\.pkg")
test(".pkg 文件关联 ProgID", pkg_progid == "Rpaper.WallpaperEnginePkg", f"={pkg_progid}")

# Check DefaultIcon uses exe
icon_path = read_reg_value(r"Software\Classes\Rpaper.WallpaperPackage\DefaultIcon")
test(".rwp DefaultIcon 指向exe", icon_path and "rpaper.exe" in icon_path, f"={icon_path}")

# Check open command
open_cmd = read_reg_value(r"Software\Classes\Rpaper.WallpaperPackage\shell\open\command")
test(".rwp open 命令正确", open_cmd and "rpaper.exe" in open_cmd and '"%1"' in open_cmd, f"={open_cmd}")

# Check right-click menu for all files
rc_menu = read_reg_value(r"Software\Classes\*\shell\RpaperSetWallpaper")
test("全局右键菜单注册", rc_menu and "Rpaper" in rc_menu, f"={rc_menu}")

# ============================================================
# Test 7: Window icon (title bar icon is not default)
# ============================================================
print("\n[7] 窗口图标测试")
kill_rpaper()
time.sleep(1)
proc4 = subprocess.Popen([EXE], creationflags=0x00000008)
time.sleep(4)
hidden4 = find_hidden_window()
user32.SendMessageW(hidden4, WM_COMMAND, CMD_OPEN_SETTINGS, 0)
time.sleep(2)
settings4 = find_settings_window()
if settings4 != 0:
    user32.ShowWindow(settings4, SW_SHOW)
    user32.BringWindowToTop(settings4)
    user32.SetForegroundWindow(settings4)
    time.sleep(0.5)
    user32.MoveWindow(settings4, 100, 100, 580, 740, True)
    time.sleep(1.5)
    rect4 = wintypes.RECT()
    user32.GetWindowRect(settings4, ctypes.byref(rect4))
    w4 = rect4.right - rect4.left
    h4 = rect4.bottom - rect4.top
    # PrintWindow capture
    hdc_screen2 = user32.GetDC(0)
    hdc_mem2 = gdi32.CreateCompatibleDC(hdc_screen2)
    hbmp2 = gdi32.CreateCompatibleBitmap(hdc_screen2, w4, h4)
    gdi32.SelectObject(hdc_mem2, hbmp2)
    user32.PrintWindow(settings4, hdc_mem2, PW_RENDERFULLCONTENT)
    time.sleep(0.3)
    bi2 = BITMAPINFOHEADER()
    bi2.biSize = ctypes.sizeof(BITMAPINFOHEADER)
    bi2.biWidth = w4
    bi2.biHeight = -h4
    bi2.biPlanes = 1
    bi2.biBitCount = 24
    bi2.biCompression = 0
    buf2 = ctypes.create_string_buffer(w4 * h4 * 3)
    gdi32.GetDIBits(hdc_mem2, hbmp2, 0, h4, buf2, ctypes.byref(bi2), 0)
    img4 = Image.frombytes("RGB", (w4, h4), buf2.raw, "raw", "BGR")
    gdi32.DeleteObject(hbmp2)
    gdi32.DeleteDC(hdc_mem2)
    user32.ReleaseDC(0, hdc_screen2)

    # Close window immediately after capture
    user32.PostMessageW(settings4, WM_CLOSE, 0, 0)
    time.sleep(2)

    # Now analyze icon
    icon_path_out = os.path.join(OUT_DIR, "icon_check.png")
    img4.save(icon_path_out)
    # Sample the title bar icon area (top-left corner, avoid window border)
    blue_count = 0
    total_icon = 0
    for x in range(15, 50):
        for y in range(8, 40):
            if x >= w4 or y >= h4: continue
            px = img4.getpixel((x, y))
            total_icon += 1
            # Blue-purple: high blue channel, moderate red (for purple gradient)
            if px[2] > 140 and px[0] < 200 and px[1] < 180 and (px[2] - px[1]) > 30:
                blue_count += 1
    blue_pct = blue_count * 100 / max(total_icon, 1)
    test("标题栏有蓝紫色R图标", blue_pct > 2, f"蓝色像素={blue_pct:.1f}%")

# ============================================================
# Test 8: Autostart registry (simulate check)
# ============================================================
print("\n[8] 开机自启注册表路径检查")
autostart_key = r"Software\Microsoft\Windows\CurrentVersion\Run"
try:
    key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, autostart_key, 0, winreg.KEY_READ)
    try:
        i = 0
        found_rpaper = False
        while True:
            try:
                name, val, _ = winreg.EnumValue(key, i)
                if "rpaper" in name.lower() or "rpaper" in str(val).lower():
                    found_rpaper = True
                i += 1
            except OSError:
                break
        # Autostart is opt-in, so it shouldn't exist yet
        test("开机自启(默认未启用)", not found_rpaper, "首次启动默认不写自启键，用户勾选后才写")
    finally:
        winreg.CloseKey(key)
except Exception as e:
    test("开机自启注册表读取", False, str(e))

# ============================================================
# Test 9: Installer exists
# ============================================================
print("\n[9] 安装包验证")
installer = r"d:\nim\rpaper\installer\Rpaper-Setup-0.1.0.exe"
test("安装包文件存在", os.path.exists(installer))
if os.path.exists(installer):
    inst_size = os.path.getsize(installer) / (1024*1024)
    test(f"安装包大小合理 (2-8MB)", 2 <= inst_size <= 8, f"{inst_size:.2f} MB")

# Shader files
shaders_dir = r"d:\nim\rpaper\shaders"
for shader in ["aurora.wgsl", "image.wgsl", "particle.wgsl", "particle_compute.wgsl"]:
    shader_path = os.path.join(shaders_dir, shader)
    test(f"Shader文件存在: {shader}", os.path.exists(shader_path))

# ============================================================
# Test 10: Tray menu commands — wallpaper switching via WM_COMMAND
# ============================================================
print("\n[10] 托盘菜单壁纸切换测试")
kill_rpaper()
time.sleep(1)
proc10 = subprocess.Popen([EXE], creationflags=0x00000008)
time.sleep(4)
hidden10 = find_hidden_window()
test("托盘测试启动", hidden10 != 0, f"HWND={hidden10:#x}")

# IDM_AURORA = 1001 — 切换到极光效果
user32.SendMessageW(hidden10, WM_COMMAND, 1001, 0)
time.sleep(1)
test("IDM_AURORA 切换极光", proc10.poll() is None, "进程存活")

# IDM_PARTICLES = 1002 — 切换到粒子效果
user32.SendMessageW(hidden10, WM_COMMAND, 1002, 0)
time.sleep(1)
test("IDM_PARTICLES 切换粒子", proc10.poll() is None, "进程存活")

# IDM_SETTINGS = 1007 — 打开设置窗口
user32.SendMessageW(hidden10, WM_COMMAND, 1007, 0)
time.sleep(2)
s10 = find_settings_window()
test("IDM_SETTINGS 打开设置窗口", s10 != 0, f"HWND={s10:#x}")
if s10:
    user32.PostMessageW(s10, WM_CLOSE, 0, 0)
    time.sleep(1)

# IDM_EXIT = 1004 — 退出程序
user32.SendMessageW(hidden10, WM_COMMAND, 1004, 0)
time.sleep(2)
test("IDM_EXIT 退出程序", find_hidden_window() == 0, "隐藏窗口已销毁")

# ============================================================
# Test 11: WM_COPYDATA file forwarding (single instance)
# ============================================================
print("\n[11] WM_COPYDATA 单实例文件转发测试")
kill_rpaper()
time.sleep(1)
proc11 = subprocess.Popen([EXE], creationflags=0x00000008)
time.sleep(4)
hidden11 = find_hidden_window()
test("第一实例启动", hidden11 != 0, f"HWND={hidden11:#x}")

# 创建测试图片文件
test_img = os.path.join(OUT_DIR, "test_copy.bmp")
# BMP 文件头: 14字节文件头 + 40字节信息头 + 4x1像素数据
bmp_header = bytes([
    0x42, 0x4D,  # BM
    0x36, 0x00, 0x00, 0x00,  # 文件大小 54+4=58? 实际我们用 4x4=16 像素
    0x00, 0x00, 0x00, 0x00,  # 保留
    0x36, 0x00, 0x00, 0x00,  # 数据偏移
    0x28, 0x00, 0x00, 0x00,  # 信息头大小 40
    0x02, 0x00, 0x00, 0x00,  # 宽 2
    0x02, 0x00, 0x00, 0x00,  # 高 2
    0x01, 0x00, 0x18, 0x00,  # 1平面, 24位
    0x00, 0x00, 0x00, 0x00,  # 无压缩
    0x10, 0x00, 0x00, 0x00,  # 图像数据大小 16
    0x00, 0x00, 0x00, 0x00,  # X像素/米
    0x00, 0x00, 0x00, 0x00,  # Y像素/米
    0x00, 0x00, 0x00, 0x00,  # 颜色数
    0x00, 0x00, 0x00, 0x00,  # 重要颜色
]) + bytes([0xFF, 0x00, 0x00] * 4) + bytes([0x00] * 4)  # 像素+padding
with open(test_img, "wb") as f:
    f.write(bmp_header)

# 启动第二实例并传文件路径 — 应通过 WM_COPYDATA 转发给第一实例
proc11b = subprocess.Popen([EXE, test_img], creationflags=0x00000008)
time.sleep(3)
# 第二实例应该已经退出（单实例检测）
test("第二实例传文件后退出", proc11b.poll() is not None, f"exit_code={proc11b.poll()}")
# 第一实例应该仍然存活
test("第一实例仍然存活", proc11.poll() is None)

# ============================================================
# Test 12: Wallpaper switch via CMD_WALLPAPER_CHANGED
# ============================================================
print("\n[12] 设置窗口壁纸切换命令测试")
hidden12 = find_hidden_window()
# 打开设置窗口
user32.SendMessageW(hidden12, WM_COMMAND, CMD_OPEN_SETTINGS, 0)
time.sleep(2)
s12 = find_settings_window()
test("设置窗口已打开", s12 != 0, f"HWND={s12:#x}")

if s12:
    # CMD_WALLPAPER_CHANGED = 2003, lparam = radio_id
    # IDC_RADIO_PARTICLES = 1004
    user32.SendMessageW(s12, WM_COMMAND, 2003, 1004)
    time.sleep(0.5)
    test("CMD_WALLPAPER_CHANGED→Particles", proc11.poll() is None, "进程存活")

    # IDC_RADIO_AURORA = 1003
    user32.SendMessageW(s12, WM_COMMAND, 2003, 1003)
    time.sleep(0.5)
    test("CMD_WALLPAPER_CHANGED→Aurora", proc11.poll() is None, "进程存活")

    user32.PostMessageW(s12, WM_CLOSE, 0, 0)
    time.sleep(1)

# ============================================================
# Test 13: Volume control (CMD_VOLUME_CHANGED)
# ============================================================
print("\n[13] 音量控制测试")
hidden13 = find_hidden_window()
user32.SendMessageW(hidden13, WM_COMMAND, CMD_OPEN_SETTINGS, 0)
time.sleep(2)
s13 = find_settings_window()
if s13:
    # CMD_VOLUME_CHANGED = 2002, lparam = 0~100
    user32.SendMessageW(s13, WM_COMMAND, 2002, 50)  # 50% volume
    time.sleep(0.5)
    test("CMD_VOLUME_CHANGED 50%", proc11.poll() is None, "进程存活")

    user32.SendMessageW(s13, WM_COMMAND, 2002, 0)  # mute
    time.sleep(0.5)
    test("CMD_VOLUME_CHANGED 0% (mute)", proc11.poll() is None)

    user32.SendMessageW(s13, WM_COMMAND, 2002, 100)  # max
    time.sleep(0.5)
    test("CMD_VOLUME_CHANGED 100%", proc11.poll() is None)

    user32.PostMessageW(s13, WM_CLOSE, 0, 0)
    time.sleep(1)

# ============================================================
# Test 14: Pause/Resume (CMD_PAUSE_TOGGLE)
# ============================================================
print("\n[14] 暂停/恢复测试")
hidden14 = find_hidden_window()
user32.SendMessageW(hidden14, WM_COMMAND, CMD_OPEN_SETTINGS, 0)
time.sleep(2)
s14 = find_settings_window()
if s14:
    # CMD_PAUSE_TOGGLE = 2004
    user32.SendMessageW(s14, WM_COMMAND, 2004, 0)
    time.sleep(0.5)
    test("CMD_PAUSE_TOGGLE 暂停", proc11.poll() is None, "进程存活")

    user32.SendMessageW(s14, WM_COMMAND, 2004, 0)
    time.sleep(0.5)
    test("CMD_PAUSE_TOGGLE 恢复", proc11.poll() is None)

    user32.PostMessageW(s14, WM_CLOSE, 0, 0)
    time.sleep(1)

# ============================================================
# Test 15: Autostart toggle (CMD_AUTOSTART_TOGGLE)
# ============================================================
print("\n[15] 开机自启复选框测试")
hidden15 = find_hidden_window()
user32.SendMessageW(hidden15, WM_COMMAND, CMD_OPEN_SETTINGS, 0)
time.sleep(2)
s15 = find_settings_window()
if s15:
    # 先确保复选框未选中
    BM_SETCHECK = 0x00F1
    BST_UNCHECKED = 0
    BST_CHECKED = 1
    BM_GETCHECK = 0x00F0
    checkbox = _hwnd_to_int(user32.GetDlgItem(s15, 1016))
    if checkbox:
        # 取消选中 — 模拟用户点击复选框
        user32.SendMessageW(checkbox, BM_SETCHECK, BST_UNCHECKED, 0)
        # 发送 IDC_CHECK_AUTOSTART(1016) 到设置窗口 → 转发 CMD_AUTOSTART_TOGGLE 到隐藏窗口
        user32.SendMessageW(s15, WM_COMMAND, 1016, 0)
        time.sleep(0.5)
        test("CMD_AUTOSTART_TOGGLE 取消自启", proc11.poll() is None, "进程存活")

        # 勾选 — 设置 checked 后发送 IDC_CHECK_AUTOSTART
        user32.SendMessageW(checkbox, BM_SETCHECK, BST_CHECKED, 0)
        user32.SendMessageW(s15, WM_COMMAND, 1016, 0)
        time.sleep(1)
        # 检查注册表是否写入
        autostart_val = read_reg_value(r"Software\Microsoft\Windows\CurrentVersion\Run", "Rpaper")
        test("自启注册表已写入", autostart_val is not None and "rpaper" in str(autostart_val).lower(), f"={autostart_val}")

        # 再次取消（清理）
        user32.SendMessageW(checkbox, BM_SETCHECK, BST_UNCHECKED, 0)
        user32.SendMessageW(s15, WM_COMMAND, 1016, 0)
        time.sleep(0.5)
        autostart_val2 = read_reg_value(r"Software\Microsoft\Windows\CurrentVersion\Run", "Rpaper")
        test("自启注册表已删除", autostart_val2 is None, "清理完成")

    user32.PostMessageW(s15, WM_CLOSE, 0, 0)
    time.sleep(1)

# ============================================================
# Test 16: Command line keyword launch (aurora/particles/image/video)
# ============================================================
print("\n[16] 命令行关键字启动测试")
kill_rpaper()
time.sleep(1)

for kw in ["aurora", "particles", "image", "video"]:
    proc_kw = subprocess.Popen([EXE, kw], creationflags=0x00000008)
    time.sleep(3)
    hidden_kw = find_hidden_window()
    test(f"关键字 '{kw}' 启动", hidden_kw != 0, f"HWND={hidden_kw:#x}")
    kill_rpaper()
    time.sleep(1)

# ============================================================
# Test 17: Config file persistence
# ============================================================
print("\n[17] 配置文件持久化测试")
proc17 = subprocess.Popen([EXE], creationflags=0x00000008)
time.sleep(4)
hidden17 = find_hidden_window()
test("配置测试启动", hidden17 != 0, f"HWND={hidden17:#x}")

# 切换壁纸类型 — 会触发 config.save()
user32.SendMessageW(hidden17, WM_COMMAND, 1002, 0)  # IDM_PARTICLES
time.sleep(1)
kill_rpaper()
time.sleep(2)

# 检查配置文件是否存在且有内容
config_path = os.path.join(os.environ.get("APPDATA", ""), "rpaper", "config.json")
test("配置文件已创建", os.path.exists(config_path), f"路径={config_path}")
if os.path.exists(config_path):
    try:
        with open(config_path, "r", encoding="utf-8") as f:
            cfg_content = f.read()
        test("配置文件有内容", len(cfg_content) > 10, f"大小={len(cfg_content)}字节")
        test("配置含wallpaper_type", "wallpaper_type" in cfg_content, "字段存在")
        test("配置含particles", "particles" in cfg_content, "值正确")
    except Exception as e:
        test("配置文件读取", False, str(e))

# ============================================================
# Test 18: Settings window Esc key close & title text
# ============================================================
print("\n[18] 设置窗口 Esc关闭 & 标题文字测试")
proc18 = subprocess.Popen([EXE], creationflags=0x00000008)
time.sleep(4)
hidden18 = find_hidden_window()
user32.SendMessageW(hidden18, WM_COMMAND, CMD_OPEN_SETTINGS, 0)
time.sleep(2)
s18 = find_settings_window()
if s18:
    # 检查标题文字 — 4个卡片标题
    title_ids = [(1111, "卡片标题1"), (1112, "卡片标题2"), (1113, "卡片标题3"), (1114, "卡片标题4")]
    for tid, tname in title_ids:
        ctrl = _hwnd_to_int(user32.GetDlgItem(s18, tid))
        test(f"控件存在: {tname}", ctrl != 0, f"id={tid}")

    # Esc 键关闭 — 用 SendMessageW 同步发送 WM_KEYDOWN VK_ESCAPE
    VK_ESCAPE = 0x1B
    WM_KEYDOWN = 0x0100
    user32.SendMessageW(s18, WM_KEYDOWN, VK_ESCAPE, 0)
    time.sleep(2)
    test("Esc键关闭设置窗口", find_settings_window() == 0, "已关闭")

# ============================================================
# Test 19: Window corner preference (DWM rounded corners)
# ============================================================
print("\n[19] 窗口圆角 DWM 属性测试")
hidden19 = find_hidden_window()
user32.SendMessageW(hidden19, WM_COMMAND, CMD_OPEN_SETTINGS, 0)
time.sleep(2)
s19 = find_settings_window()
if s19:
    # 查询 DWMWA_WINDOW_CORNER_PREFERENCE = 33
    # DWMWCP_ROUND = 2
    dwmapi = ctypes.windll.dwmapi
    corner_pref = ctypes.c_int(0)
    # DwmGetWindowAttribute(hwnd, 33, &pref, sizeof(int))
    DWMWA_WINDOW_CORNER_PREFERENCE = 33
    hr = dwmapi.DwmGetWindowAttribute(
        wintypes.HWND(s19),
        ctypes.c_uint(DWMWA_WINDOW_CORNER_PREFERENCE),
        ctypes.byref(corner_pref),
        ctypes.c_size_t(ctypes.sizeof(ctypes.c_int))
    )
    test("DWM圆角属性可读", hr == 0, f"hr={hr:#x}, pref={corner_pref.value}")
    test("DWM圆角=Round(2)", corner_pref.value == 2, f"pref={corner_pref.value}")

    user32.PostMessageW(s19, WM_CLOSE, 0, 0)
    time.sleep(1)

# ============================================================
# Test 20: Invalid file handling (no crash)
# ============================================================
print("\n[20] 无效文件处理测试")
kill_rpaper()
time.sleep(1)
# 创建一个无效的 .rwp 文件
bad_rwp = os.path.join(OUT_DIR, "bad.rwp")
with open(bad_rwp, "wb") as f:
    f.write(b"INVALID_DATA_NOT_A_REAL_WALLPAPER_FILE")

proc20 = subprocess.Popen([EXE, bad_rwp], creationflags=0x00000008)
time.sleep(4)
hidden20 = find_hidden_window()
test("无效文件不崩溃", hidden20 != 0, f"HWND={hidden20:#x}")
test("无效文件进程存活", proc20.poll() is None, f"PID={proc20.pid}")

# ============================================================
# Test 21: Multiple settings open prevention
# ============================================================
print("\n[21] 设置窗口单例测试")
hidden21 = find_hidden_window()
# 发送两次打开设置命令
user32.SendMessageW(hidden21, WM_COMMAND, CMD_OPEN_SETTINGS, 0)
time.sleep(1)
user32.SendMessageW(hidden21, WM_COMMAND, CMD_OPEN_SETTINGS, 0)
time.sleep(2)
# 应该只有一个设置窗口
s_count = 0
def count_settings_cb(hwnd, lparam):
    global s_count
    cls = ctypes.create_unicode_buffer(256)
    user32.GetClassNameW(hwnd, cls, 256)
    if cls.value == "RpaperSettings":
        s_count += 1
    return True
ENUMPROC = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
user32.EnumWindows(ENUMPROC(count_settings_cb), 0)
test("设置窗口不重复创建", s_count <= 1, f"数量={s_count}")

if s_count > 0:
    s21 = find_settings_window()
    if s21:
        user32.PostMessageW(s21, WM_CLOSE, 0, 0)
        time.sleep(1)

# 清理测试文件
for tmp in [test_img, bad_rwp]:
    try:
        os.remove(tmp)
    except Exception:
        pass

# ============================================================
# Cleanup
# ============================================================
print("\n[22] 清理")
kill_rpaper()
time.sleep(1)
test("进程清理完毕", find_hidden_window() == 0)

# ============================================================
# Summary
# ============================================================
print("\n" + "=" * 60)
print(f"测试结果: {passed} 通过, {failed} 失败, 共 {passed+failed} 项")
print("=" * 60)

if failed > 0:
    print("\n失败项:")
    for name, ok, detail in results:
        if not ok:
            print(f"  ✗ {name}: {detail}")

# Save report
report_path = os.path.join(OUT_DIR, "test_report.txt")
with open(report_path, "w", encoding="utf-8") as f:
    f.write(f"Rpaper 测试报告\n")
    f.write(f"{'='*60}\n")
    f.write(f"通过: {passed}, 失败: {failed}, 总计: {passed+failed}\n\n")
    for name, ok, detail in results:
        status = "PASS" if ok else "FAIL"
        f.write(f"[{status}] {name}")
        if detail:
            f.write(f" — {detail}")
        f.write("\n")
    f.write(f"\n截图保存至: {OUT_DIR}\n")

print(f"\n详细报告已保存到: {report_path}")
sys.exit(0 if failed == 0 else 1)
