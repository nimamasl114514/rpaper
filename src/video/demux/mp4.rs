use std::path::Path;

use super::DemuxedSample;

pub struct Mp4Demuxer {
    data: Vec<u8>,
    #[allow(dead_code)]
    timescale: u32,
    #[allow(dead_code)]
    duration: u64,
    width: u32,
    height: u32,
    sample_sizes: Vec<u32>,
    sample_offsets: Vec<u64>,
    sample_deltas: Vec<(u32, u32)>, // stts: (count, delta)
    sps: Vec<u8>,
    pps: Vec<u8>,
    current_sample: usize,
    #[allow(dead_code)]
    chunks: Vec<u64>,
    #[allow(dead_code)]
    sample_to_chunk: Vec<(u32, u32, u32)>, // (first_chunk, samples_per_chunk, sd_index)
    first_sample_returned: bool,
}

// ── 字节读取辅助 ──────────────────────────────────────────────

fn read_u32_be(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn read_u64_be(data: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes(data[offset..offset + 8].try_into().unwrap())
}

// ── Box 遍历 ──────────────────────────────────────────────────

/// 在 [offset, end) 范围内遍历所有 box，对每个 box 调用回调。
/// 回调参数: (box_type, data_start, data_end)
fn for_each_box(data: &[u8], offset: usize, end: usize, mut f: impl FnMut(&str, usize, usize)) {
    let mut pos = offset;
    while pos + 8 <= end {
        let size = read_u32_be(data, pos) as usize;
        let box_type = match std::str::from_utf8(&data[pos + 4..pos + 8]) {
            Ok(s) => s,
            Err(_) => break,
        };

        let (header_size, box_size) = match size {
            0 => (8, end - pos),
            1 => {
                if pos + 16 > end {
                    break;
                }
                (16, read_u64_be(data, pos + 8) as usize)
            }
            _ => (8, size),
        };

        let data_start = pos + header_size;
        let data_end = pos + box_size;
        if data_end > end {
            break;
        }

        f(box_type, data_start, data_end);
        pos = data_end;
    }
}

/// 在 [offset, end) 内查找指定类型的首个 box，返回 (data_start, data_end)
fn find_box(data: &[u8], offset: usize, end: usize, target: &str) -> Option<(usize, usize)> {
    let mut result = None;
    for_each_box(data, offset, end, |box_type, ds, de| {
        if result.is_none() && box_type == target {
            result = Some((ds, de));
        }
    });
    result
}

// ── 视频轨道查找 ──────────────────────────────────────────────

/// 在 moov 范围内查找 video track（hdlr handler_type == "vide"）
fn find_video_trak(
    data: &[u8],
    moov_start: usize,
    moov_end: usize,
) -> Result<(usize, usize), String> {
    let mut result = None;
    for_each_box(data, moov_start, moov_end, |box_type, ds, de| {
        if result.is_some() || box_type != "trak" {
            return;
        }
        if let Some(mdia) = find_box(data, ds, de, "mdia") {
            if let Some(hdlr) = find_box(data, mdia.0, mdia.1, "hdlr") {
                // FullBox: 4 bytes version+flags, then 4 bytes pre_defined, then 4 bytes handler_type
                if hdlr.1 - hdlr.0 >= 12 {
                    let ht = &data[hdlr.0 + 8..hdlr.0 + 12];
                    if ht == b"vide" {
                        result = Some((ds, de));
                    }
                }
            }
        }
    });
    result.ok_or_else(|| "未找到视频轨道 (vide)".to_string())
}

// ── avcC 解析 ─────────────────────────────────────────────────

fn parse_avcc(data: &[u8], offset: usize, _end: usize) -> Result<(Vec<u8>, Vec<u8>), String> {
    // avcC 结构:
    //   byte 0: configurationVersion
    //   byte 1: AVCProfileIndication
    //   byte 2: profile_compatibility
    //   byte 3: AVCLevelIndication
    //   byte 4: (reserved 6 bits) | (lengthSizeMinusOne 2 bits)
    //   byte 5: (reserved 3 bits) | (numOfSequenceParameterSets 5 bits)
    //   for each SPS: u16 length + data
    //   byte: numOfPictureParameterSets
    //   for each PPS: u16 length + data

    let mut pos = offset;
    if pos + 6 > data.len() {
        return Err("avcC 数据过短".to_string());
    }

    pos += 5; // 跳过 configurationVersion .. lengthSizeMinusOne

    let num_sps = (data[pos] & 0x1F) as usize;
    pos += 1;

    let mut sps = Vec::new();
    for _ in 0..num_sps {
        if pos + 2 > data.len() {
            return Err("avcC SPS 长度字段越界".to_string());
        }
        let sps_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2;
        if pos + sps_len > data.len() {
            return Err("avcC SPS 数据越界".to_string());
        }
        sps.extend_from_slice(&data[pos..pos + sps_len]);
        pos += sps_len;
    }

    if pos >= data.len() {
        return Err("avcC 缺少 numPPS".to_string());
    }
    let num_pps = data[pos] as usize;
    pos += 1;

    let mut pps = Vec::new();
    for _ in 0..num_pps {
        if pos + 2 > data.len() {
            return Err("avcC PPS 长度字段越界".to_string());
        }
        let pps_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2;
        if pos + pps_len > data.len() {
            return Err("avcC PPS 数据越界".to_string());
        }
        pps.extend_from_slice(&data[pos..pos + pps_len]);
        pos += pps_len;
    }

    Ok((sps, pps))
}

// ── 采样表构建 ────────────────────────────────────────────────

/// 根据 stsc / stco / stsz 构建每个采样的文件偏移和大小
#[allow(clippy::type_complexity)]
fn build_sample_table(
    data: &[u8],
    stbl_start: usize,
    stbl_end: usize,
) -> Result<(Vec<u64>, Vec<u32>, Vec<(u32, u32)>, Vec<u64>, Vec<(u32, u32, u32)>), String> {
    // ── stco / co64 ──────────────────────────────────────────
    let co_box = find_box(data, stbl_start, stbl_end, "stco")
        .or_else(|| find_box(data, stbl_start, stbl_end, "co64"));

    let (co_start, _co_end) = co_box.ok_or("未找到 stco 或 co64")?;
    let is_co64 = {
        let tag = &data[co_start - 8..co_start - 4];
        tag == b"co64"
    };

    // 跳过 version+flags (4 bytes)
    let mut pos = co_start + 4;
    let entry_count = read_u32_be(data, pos) as usize;
    pos += 4;

    let mut chunks: Vec<u64> = Vec::with_capacity(entry_count);
    if is_co64 {
        for _ in 0..entry_count {
            chunks.push(read_u64_be(data, pos));
            pos += 8;
        }
    } else {
        for _ in 0..entry_count {
            chunks.push(read_u32_be(data, pos) as u64);
            pos += 4;
        }
    }

    // ── stsz ─────────────────────────────────────────────────
    let (stsz_start, _stsz_end) =
        find_box(data, stbl_start, stbl_end, "stsz").ok_or("未找到 stsz")?;
    pos = stsz_start + 4; // 跳过 version+flags
    let sample_size_uniform = read_u32_be(data, pos);
    pos += 4;
    let sample_count = read_u32_be(data, pos) as usize;
    pos += 4;

    let mut sample_sizes: Vec<u32> = Vec::with_capacity(sample_count);
    if sample_size_uniform == 0 {
        // 每个采样有独立的大小
        for _ in 0..sample_count {
            sample_sizes.push(read_u32_be(data, pos));
            pos += 4;
        }
    } else {
        // 所有采样大小相同
        sample_sizes.resize(sample_count, sample_size_uniform);
    }

    // ── stts ─────────────────────────────────────────────────
    let (stts_start, _stts_end) =
        find_box(data, stbl_start, stbl_end, "stts").ok_or("未找到 stts")?;
    pos = stts_start + 4;
    let stts_entry_count = read_u32_be(data, pos) as usize;
    pos += 4;
    let mut sample_deltas: Vec<(u32, u32)> = Vec::with_capacity(stts_entry_count);
    for _ in 0..stts_entry_count {
        let count = read_u32_be(data, pos);
        let delta = read_u32_be(data, pos + 4);
        sample_deltas.push((count, delta));
        pos += 8;
    }

    // ── stsc ─────────────────────────────────────────────────
    let (stsc_start, _stsc_end) =
        find_box(data, stbl_start, stbl_end, "stsc").ok_or("未找到 stsc")?;
    pos = stsc_start + 4;
    let stsc_entry_count = read_u32_be(data, pos) as usize;
    pos += 4;
    let mut sample_to_chunk: Vec<(u32, u32, u32)> = Vec::with_capacity(stsc_entry_count);
    for _ in 0..stsc_entry_count {
        let first_chunk = read_u32_be(data, pos);
        let samples_per_chunk = read_u32_be(data, pos + 4);
        let sd_index = read_u32_be(data, pos + 8);
        sample_to_chunk.push((first_chunk, samples_per_chunk, sd_index));
        pos += 12;
    }

    // ── 构建 sample → chunk 映射 ─────────────────────────────
    // 根据 stsc 表，确定每个 chunk 包含多少个采样
    let num_chunks = chunks.len();
    let mut chunk_sample_counts: Vec<u32> = vec![0; num_chunks];

    if !sample_to_chunk.is_empty() {
        let mut sc_idx = 0;
        for (chunk_idx, count) in chunk_sample_counts.iter_mut().enumerate().take(num_chunks) {
            let chunk_num = (chunk_idx + 1) as u32;
            // 找到适用的 stsc 条目
            while sc_idx + 1 < sample_to_chunk.len()
                && sample_to_chunk[sc_idx + 1].0 <= chunk_num
            {
                sc_idx += 1;
            }
            *count = sample_to_chunk[sc_idx].1;
        }
    }

    // ── 计算每个采样在文件中的偏移 ───────────────────────────
    let mut sample_offsets: Vec<u64> = Vec::with_capacity(sample_count);
    let mut sample_idx = 0;
    for chunk_idx in 0..num_chunks {
        let chunk_offset = chunks[chunk_idx];
        let mut offset_in_chunk: u64 = 0;
        let n_samples = chunk_sample_counts[chunk_idx] as usize;
        for _ in 0..n_samples {
            if sample_idx >= sample_count {
                break;
            }
            sample_offsets.push(chunk_offset + offset_in_chunk);
            offset_in_chunk += sample_sizes[sample_idx] as u64;
            sample_idx += 1;
        }
    }

    // 如果还有未分配的采样（罕见情况），追加到最后一个 chunk
    while sample_idx < sample_count {
        let last_chunk = chunks.last().copied().unwrap_or(0);
        let last_offset = sample_offsets.last().copied().unwrap_or(last_chunk);
        let prev_size = if sample_idx > 0 {
            sample_sizes[sample_idx - 1] as u64
        } else {
            0
        };
        sample_offsets.push(last_offset + prev_size);
        sample_idx += 1;
    }

    Ok((sample_offsets, sample_sizes, sample_deltas, chunks, sample_to_chunk))
}

// ── Mp4Demuxer 实现 ──────────────────────────────────────────

impl Mp4Demuxer {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("读取失败: {e}"))?;
        Self::from_bytes(data)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Self, String> {
        let len = data.len();

        // ── 1. 顶层遍历：找 moov 和 mdat ─────────────────────
        let mut moov_range: Option<(usize, usize)> = None;
        let mut _mdat_offset: Option<usize> = None;

        for_each_box(&data, 0, len, |box_type, ds, _de| match box_type {
            "moov" => moov_range = Some((ds, _de)),
            "mdat" => _mdat_offset = Some(ds - 8), // box 起始偏移 = data_start - header_size(8)
            _ => {}
        });

        let moov = moov_range.ok_or("未找到 moov box")?;
        let _mdat = _mdat_offset.ok_or("未找到 mdat box")?;

        // ── 2. 解析 moov/mvhd ────────────────────────────────
        let mvhd = find_box(&data, moov.0, moov.1, "mvhd").ok_or("未找到 mvhd")?;
        let version = data[mvhd.0];
        let (_cale, duration) = if version == 0 {
            (
                read_u32_be(&data, mvhd.0 + 12),
                read_u32_be(&data, mvhd.0 + 16) as u64,
            )
        } else {
            (
                read_u32_be(&data, mvhd.0 + 20),
                read_u64_be(&data, mvhd.0 + 24),
            )
        };

        // ── 3. 找视频轨道 ────────────────────────────────────
        let trak = find_video_trak(&data, moov.0, moov.1)?;

        // ── 4. 解析 tkhd → width/height ──────────────────────
        let tkhd = find_box(&data, trak.0, trak.1, "tkhd").ok_or("未找到 tkhd")?;
        let tkhd_version = data[tkhd.0];
        let (width, height) = if tkhd_version == 0 {
            let w = read_u32_be(&data, tkhd.0 + 76); // 16.16 定点数
            let h = read_u32_be(&data, tkhd.0 + 80);
            (w >> 16, h >> 16)
        } else {
            let w = read_u32_be(&data, tkhd.0 + 88);
            let h = read_u32_be(&data, tkhd.0 + 92);
            (w >> 16, h >> 16)
        };

        // ── 5. 解析 mdia/mdhd → track timescale ──────────────
        let mdia = find_box(&data, trak.0, trak.1, "mdia").ok_or("未找到 mdia")?;
        let mdhd = find_box(&data, mdia.0, mdia.1, "mdhd").ok_or("未找到 mdhd")?;
        let mdhd_version = data[mdhd.0];
        let track_timescale = if mdhd_version == 0 {
            read_u32_be(&data, mdhd.0 + 12)
        } else {
            read_u32_be(&data, mdhd.0 + 20)
        };

        // ── 6. 解析 minf/stbl/stsd → avcC (SPS/PPS) ──────────
        let minf = find_box(&data, mdia.0, mdia.1, "minf").ok_or("未找到 minf")?;
        let stbl = find_box(&data, minf.0, minf.1, "stbl").ok_or("未找到 stbl")?;

        let stsd = find_box(&data, stbl.0, stbl.1, "stsd").ok_or("未找到 stsd")?;
        let mut pos = stsd.0 + 4; // 跳过 version+flags
        let entry_count = read_u32_be(&data, pos) as usize;
        pos += 4;
        if entry_count == 0 {
            return Err("stsd 条目数为 0".to_string());
        }

        // 取第一个条目
        let entry_size = read_u32_be(&data, pos) as usize;
        let entry_type = std::str::from_utf8(&data[pos + 4..pos + 8])
            .map_err(|_| "stsd 条目类型无效".to_string())?;
        if entry_type != "avc1" && entry_type != "avc3" {
            return Err(format!("不支持的编码格式: {entry_type}"));
        }

        // avc1/avc3 条目结构：8 字节 box header + 78 字节固定字段（reserved/data_ref_index/VisualSampleEntry）
        // 子 box（avcC）从 header + 78 字节后开始
        let entry_data_start = pos + 8 + 78; // 跳过 size+type(8) + VisualSampleEntry 固定字段(78)
        let entry_data_end = pos + entry_size;

        // 在条目内查找 avcC
        let avcc = find_box(&data, entry_data_start, entry_data_end, "avcC")
            .ok_or("未找到 avcC box")?;

        let (sps, pps) = parse_avcc(&data, avcc.0, avcc.1)?;

        // ── 7. 构建采样表 ────────────────────────────────────
        let (sample_offsets, sample_sizes, sample_deltas, chunks, sample_to_chunk) =
            build_sample_table(&data, stbl.0, stbl.1)?;

        // ── 8. 使用 track 的 timescale ───────────────────────
        let timescale = track_timescale;

        Ok(Mp4Demuxer {
            data,
            timescale,
            duration,
            width,
            height,
            sample_sizes,
            sample_offsets,
            sample_deltas,
            sps,
            pps,
            current_sample: 0,
            chunks,
            sample_to_chunk,
            first_sample_returned: false,
        })
    }

    #[allow(dead_code)]
    pub fn sps(&self) -> &[u8] {
        &self.sps
    }

    #[allow(dead_code)]
    pub fn pps(&self) -> &[u8] {
        &self.pps
    }

    /// 计算第 sample_idx 个视频采样的时间戳（以 timescale 为单位）
    fn compute_timestamp(&self, sample_idx: usize) -> u64 {
        let mut ts: u64 = 0;
        let mut remaining = sample_idx as u32;
        for (count, delta) in &self.sample_deltas {
            if remaining < *count {
                ts += remaining as u64 * *delta as u64;
                break;
            }
            ts += *count as u64 * *delta as u64;
            remaining -= *count;
        }
        ts
    }

    /// 将 length-prefixed NAL 数据转换为 Annex B 格式
    fn to_annex_b(raw: &[u8]) -> Vec<u8> {
        let mut annex_b = Vec::with_capacity(raw.len() + 32);
        let mut pos = 0;
        while pos + 4 <= raw.len() {
            let nal_len =
                u32::from_be_bytes([raw[pos], raw[pos + 1], raw[pos + 2], raw[pos + 3]]) as usize;
            pos += 4;
            if pos + nal_len > raw.len() {
                break;
            }
            annex_b.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
            annex_b.extend_from_slice(&raw[pos..pos + nal_len]);
            pos += nal_len;
        }
        annex_b
    }
}

impl super::Demuxer for Mp4Demuxer {
    fn next_sample(&mut self) -> Option<DemuxedSample> {
        // 第一次调用返回 SPS
        if !self.first_sample_returned {
            self.first_sample_returned = true;
            let mut data = vec![0x00, 0x00, 0x00, 0x01];
            data.extend_from_slice(&self.sps);
            return Some(DemuxedSample {
                data,
                timestamp: 0,
            });
        }

        // 第二次调用返回 PPS，并推进 current_sample 越过 PPS 阶段
        if self.current_sample == 0 {
            self.current_sample = 1;
            let mut data = vec![0x00, 0x00, 0x00, 0x01];
            data.extend_from_slice(&self.pps);
            return Some(DemuxedSample {
                data,
                timestamp: 0,
            });
        }

        // 视频采样：current_sample 从 1 开始，对应 sample_offsets[0]
        let idx = self.current_sample - 1;
        if idx >= self.sample_offsets.len() {
            // 循环播放：重置状态
            self.current_sample = 0;
            self.first_sample_returned = false;
            return self.next_sample();
        }

        let offset = self.sample_offsets[idx] as usize;
        let size = self.sample_sizes[idx] as usize;
        let raw = &self.data[offset..offset + size];

        let annex_b = Self::to_annex_b(raw);
        let timestamp = self.compute_timestamp(idx);

        self.current_sample += 1;
        Some(DemuxedSample {
            data: annex_b,
            timestamp,
        })
    }

    fn duration_ms(&self) -> u64 {
        self.duration * 1000 / self.timescale as u64
    }

    fn width(&self) -> u32 {
        self.width
    }

    /// 视频采样数 = sample_offsets.len()，不含 SPS/PPS（前两次 next_sample 返回的）
    fn sample_count(&self) -> usize {
        self.sample_offsets.len()
    }

    /// current_sample 从 0 起步，0/1 表示尚未真正读取视频帧（仍在返回 SPS/PPS）
    /// 真正读取视频帧从 current_sample=2 开始，对应 sample_offsets[1]
    fn current_sample_index(&self) -> usize {
        if self.current_sample < 2 { 0 } else { self.current_sample - 1 }
    }

    fn height(&self) -> u32 {
        self.height
    }
}