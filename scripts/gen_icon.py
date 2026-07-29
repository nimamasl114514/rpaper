"""
生成 Rpaper 应用图标 — Win11 现代风格
- 蓝紫渐变圆角方形背景（#0078D4 → #8B5CF6）
- 白色粗体 R 字母居中
- 输出多尺寸 .ico（16/24/32/48/64/128/256）
"""
import sys, os, math
from PIL import Image, ImageDraw, ImageFont

try:
    sys.stdout.reconfigure(encoding="utf-8")
except Exception:
    pass

OUT_DIR = r"d:\nim\rpaper\res"
os.makedirs(OUT_DIR, exist_ok=True)

def draw_rounded_rect(draw, xy, radius, fill):
    """画圆角矩形 — PIL 原生 rounded_rectangle 在旧版可能没有，手动实现"""
    x0, y0, x1, y1 = xy
    r = radius
    # 四个圆角
    draw.pieslice([x0, y0, x0+2*r, y0+2*r], 180, 270, fill=fill)
    draw.pieslice([x1-2*r, y0, x1, y0+2*r], 270, 360, fill=fill)
    draw.pieslice([x0, y1-2*r, x0+2*r, y1], 90, 180, fill=fill)
    draw.pieslice([x1-2*r, y1-2*r, x1, y1], 0, 90, fill=fill)
    # 两个矩形填充中间
    draw.rectangle([x0+r, y0, x1-r, y1], fill=fill)
    draw.rectangle([x0, y0+r, x1, y1-r], fill=fill)

def make_icon(size):
    """生成单个尺寸的图标"""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # 1. 蓝紫渐变背景 — 从左上蓝到右下紫
    # Win11 蓝 #0078D4 → 紫 #7C3AED
    gradient = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    gdraw = ImageDraw.Draw(gradient)
    for y in range(size):
        for x in range(size):
            t = (x + y) / (2 * size)  # 0-1 对角线渐变
            r = int(0x00 * (1-t) + 0x7C * t)
            g = int(0x78 * (1-t) + 0x3A * t)
            b = int(0xD4 * (1-t) + 0xED * t)
            gdraw.point((x, y), fill=(r, g, b, 255))

    # 2. 创建圆角遮罩
    mask = Image.new("L", (size, size), 0)
    mdraw = ImageDraw.Draw(mask)
    corner_r = max(size // 5, 4)  # 圆角半径约为尺寸的 1/5
    draw_rounded_rect(mdraw, [2, 2, size-3, size-3], corner_r, fill=255)

    # 3. 应用遮罩到渐变背景
    bg = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    bg.paste(gradient, (0, 0), mask)

    # 4. 添加微妙的高光（顶部亮边）— Win11 icon 的反光感
    highlight = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    hdraw = ImageDraw.Draw(highlight)
    hl_height = max(size // 8, 2)
    for i in range(hl_height):
        alpha = int(40 * (1 - i / hl_height))
        hdraw.line([(corner_r + 2, 3 + i), (size - corner_r - 3, 3 + i)],
                   fill=(255, 255, 255, alpha))

    # 5. 合成背景+高光
    img = Image.alpha_composite(bg, highlight)

    # 6. 画白色 R 字母
    # 尝试加载系统字体，fallback 到默认
    font_size = int(size * 0.62)
    font = None
    font_paths = [
        "C:/Windows/Fonts/segoeuib.ttf",   # Segoe UI Bold
        "C:/Windows/Fonts/arialbd.ttf",     # Arial Bold
        "C:/Windows/Fonts/msyhbd.ttc",      # 微软雅黑 Bold
    ]
    for fp in font_paths:
        if os.path.exists(fp):
            try:
                font = ImageFont.truetype(fp, font_size)
                break
            except Exception:
                continue
    if font is None:
        font = ImageFont.load_default()

    # 居中绘制 R
    draw = ImageDraw.Draw(img)
    text = "R"
    bbox = draw.textbbox((0, 0), text, font=font)
    tw = bbox[2] - bbox[0]
    th = bbox[3] - bbox[1]
    tx = (size - tw) // 2 - bbox[0]
    ty = (size - th) // 2 - bbox[1] - int(size * 0.02)  # 微调垂直居中
    draw.text((tx, ty), text, fill=(255, 255, 255, 255), font=font)

    # 7. 添加微妙的投影/边框 — 让图标在浅色任务栏上也有轮廓
    # 外圈 1px 半透明深色边
    border = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    bdraw = ImageDraw.Draw(border)
    border_alpha = 30
    draw_rounded_rect(bdraw, [1, 1, size-2, size-2], corner_r, fill=(0, 0, 0, 0))
    # 用 mask 画边框轮廓
    outline_mask = Image.new("L", (size, size), 0)
    omdraw = ImageDraw.Draw(outline_mask)
    draw_rounded_rect(omdraw, [0, 0, size-1, size-1], corner_r+1, fill=border_alpha)
    draw_rounded_rect(omdraw, [2, 2, size-3, size-3], corner_r-1, fill=0)

    final = img.copy()
    # 将轮廓叠加
    border_layer = Image.new("RGBA", (size, size), (0, 0, 0, border_alpha))
    final.paste(border_layer, (0, 0), outline_mask)

    return final

# 生成多尺寸图标
sizes = [16, 24, 32, 48, 64, 128, 256]
icons = {}
for s in sizes:
    icon = make_icon(s)
    icons[s] = icon
    png_path = os.path.join(OUT_DIR, f"icon_{s}.png")
    icon.save(png_path, "PNG")
    print(f"  {png_path} ({s}x{s})")

# 保存为 .ico（256 尺寸存 PNG 压缩，其他存 BMP 以兼容旧版 Windows）
ico_path = os.path.join(OUT_DIR, "rpaper.ico")
# PIL 的 save 支持多尺寸 ico
icons[256].save(
    ico_path,
    format="ICO",
    sizes=[(s, s) for s in sizes],
    append_images=[icons[s] for s in sizes[:-1]],
)
print(f"\nICO 保存: {ico_path}")
print(f"文件大小: {os.path.getsize(ico_path):,} bytes")
