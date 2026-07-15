//! 背景音乐播放 — 基于 rodio

use std::io::Cursor;
use rodio::{Decoder, OutputStream, Sink, Source};

pub struct AudioPlayer {
    _stream: OutputStream,
    sink: Sink,
}

impl AudioPlayer {
    /// 从内存数据加载并循环播放
    pub fn load_loop(data: Vec<u8>, format_hint: &str) -> Result<Self, String> {
        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| format!("音频输出: {e}"))?;
        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| format!("音频 sink: {e}"))?;

        let cursor = Cursor::new(data);
        let source = Decoder::new(cursor)
            .map_err(|e| format!("音频解码 (.{format_hint}): {e}，仅支持 MP3/WAV/OGG/FLAC"))?;
        sink.append(source.repeat_infinite());
        sink.set_volume(0.5);

        Ok(Self { _stream: stream, sink })
    }

    #[allow(dead_code)]
    pub fn set_volume(&self, vol: f32) {
        self.sink.set_volume(vol);
    }

    #[allow(dead_code)]
    pub fn pause(&self) { self.sink.pause(); }
    #[allow(dead_code)]
    pub fn resume(&self) { self.sink.play(); }
    pub fn stop(&self) { self.sink.stop(); }
}
