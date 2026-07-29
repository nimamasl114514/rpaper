//! H.264 位流读取器。

/// H.264 位流读取器，支持按位读取和 Exp-Golomb 编码解码。
pub struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8, // 0-7，当前字节中已读取的位数
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        BitReader {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    /// 读取 1 位，返回 0 或 1；越界返回 0。
    pub fn read_bit(&mut self) -> u32 {
        if self.byte_pos >= self.data.len() {
            return 0;
        }
        let bit = (self.data[self.byte_pos] >> (7 - self.bit_pos)) & 1;
        self.bit_pos += 1;
        if self.bit_pos == 8 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
        bit as u32
    }

    /// 读取 n 位（n <= 24），越界部分补 0。
    pub fn read_bits(&mut self, n: u8) -> u32 {
        if n == 0 {
            return 0;
        }
        let mut result: u32 = 0;
        let mut remaining = n;
        while remaining > 0 {
            if self.byte_pos >= self.data.len() {
                break;
            }
            let available = 8 - self.bit_pos;
            let take = remaining.min(available);
            let byte = self.data[self.byte_pos];
            let mask = ((1u32 << take) - 1) as u8;
            let bits = (byte >> (available - take)) & mask;
            result = (result << take as u32) | bits as u32;
            self.bit_pos += take;
            remaining -= take;
            if self.bit_pos == 8 {
                self.bit_pos = 0;
                self.byte_pos += 1;
            }
        }
        // 越界未读的位补 0，置于低位
        result << remaining as u32
    }

    /// Exp-Golomb 无符号整数。
    pub fn read_ue(&mut self) -> u32 {
        let mut leading_zeros: u32 = 0;
        loop {
            if self.read_bit() == 1 {
                break;
            }
            leading_zeros += 1;
            if leading_zeros >= 32 {
                return 0; // 无效位流，防止移位溢出
            }
        }
        if leading_zeros == 0 {
            return 0;
        }
        let rest = self.read_bits(leading_zeros as u8);
        (1u32 << leading_zeros) - 1 + rest
    }

    /// Exp-Golomb 有符号整数。
    /// codeNum 0→0, 1→1, 2→-1, 3→2, 4→-2, ...
    pub fn read_se(&mut self) -> i32 {
        let code_num = self.read_ue();
        if code_num == 0 {
            return 0;
        }
        if code_num & 1 == 1 {
            code_num.div_ceil(2) as i32
        } else {
            -((code_num / 2) as i32)
        }
    }

    /// 读取 1 位作为布尔值。
    pub fn read_bool(&mut self) -> bool {
        self.read_bit() == 1
    }

    /// 跳过 n 位。
    #[allow(dead_code)]
    pub fn skip_bits(&mut self, n: u32) {
        let total = self.byte_pos * 8 + self.bit_pos as usize + n as usize;
        self.byte_pos = total / 8;
        self.bit_pos = (total % 8) as u8;
    }

    /// 是否字节对齐。
    #[allow(dead_code)]
    pub fn byte_aligned(&self) -> bool {
        self.bit_pos == 0
    }

    /// 跳到下一字节边界。
    pub fn skip_to_byte_boundary(&mut self) {
        if self.bit_pos != 0 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
    }

    /// 剩余位数。
    pub fn remaining_bits(&self) -> usize {
        self.data
            .len()
            .saturating_mul(8)
            .saturating_sub(self.byte_pos * 8 + self.bit_pos as usize)
    }

    /// 当前已读取的位数 (调试用)。
    #[allow(dead_code)]
    pub fn current_bit_pos(&self) -> usize {
        self.byte_pos * 8 + self.bit_pos as usize
    }

    /// 读取 n 字节（必须字节对齐）。
    #[allow(dead_code)]
    pub fn read_bytes(&mut self, n: usize) -> Vec<u8> {
        let available = self.data.len().saturating_sub(self.byte_pos);
        let take = n.min(available);
        let result = self.data[self.byte_pos..self.byte_pos + take].to_vec();
        self.byte_pos += take;
        result
    }
}
