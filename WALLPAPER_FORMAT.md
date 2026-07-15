# Rpaper 壁纸包格式 (.rwp)

`.rwp` 文件是 Rpaper 的打包壁纸格式，本质是 ZIP 压缩包。任何人都可以制作并分享壁纸包。

## 文件结构

```
my-wallpaper.rwp (ZIP)
├── manifest.json      # 必需，元数据
├── shader.wgsl        # shader 类型时使用
├── image.png          # image 类型时使用
├── video.mp4          # video 类型时使用
└── audio.mp3          # 可选，所有类型均可附带背景音乐
```

## manifest.json 字段

| 字段 | 类型 | 必需 | 说明 |
|------|------|------|------|
| `name` | string | 是 | 壁纸名称 |
| `type` | string | 是 | 壁纸类型: `shader` / `particles` / `image` / `video` |
| `author` | string | 否 | 作者名 |
| `description` | string | 否 | 描述 |
| `audio` | string | 否 | 音频文件名（如 `"audio.mp3"`） |
| `params` | object | 否 | 自定义参数（保留，暂未使用） |

## 壁纸类型

### shader（着色器）
内置极光效果。后续版本将支持自定义 WGSL 着色器。
```json
{
    "name": "北欧极光",
    "type": "shader",
    "author": "your-name",
    "description": "流动的绿色极光"
}
```

### particles（粒子）
内置粒子系统效果。
```json
{
    "name": "星空粒子",
    "type": "particles",
    "author": "your-name"
}
```

### image（图片）
包含一张图片，cover 模式铺满屏幕 + 呼吸效果。
```json
{
    "name": "山川风景",
    "type": "image",
    "author": "your-name",
    "description": "雪山日出"
}
```
包内需包含图片文件（PNG/JPG/BMP/WebP/GIF）。

### video（视频）
包含一个视频文件，循环播放。需要系统安装 ffmpeg。
```json
{
    "name": "雨夜城市",
    "type": "video",
    "author": "your-name",
    "audio": "bgm.mp3"
}
```
包内需包含视频文件（MP4/MKV/AVI/WebM/MOV 等）。

## 背景音乐

所有类型都可以附带背景音乐。在 manifest.json 中设置 `audio` 字段为音频文件名，并将音频文件放入包中。

支持的音频格式：MP3、WAV、OGG、FLAC。

音乐会自动循环播放，默认音量 50%。

## 制作壁纸包

将文件按上述结构组织，然后压缩为 ZIP，后缀改为 `.rwp`：

```bash
# 示例：制作带背景音乐的图片壁纸包
mkdir my-wallpaper
cd my-wallpaper
echo '{"name":"我的壁纸","type":"image","audio":"bgm.mp3"}' > manifest.json
cp /path/to/image.png .
cp /path/to/bgm.mp3 .
zip -r ../my-wallpaper.rwp .
```

## 分享

`.rwp` 文件可以直接分享。其他用户在 Rpaper 托盘菜单中选择「加载壁纸包 (.rwp)...」即可导入。
