//! Lucene's `SegmentingTokenizerBase` doesn't split sentences over the whole input: it reads
//! 1024 UTF-16 code units at a time and, if more input follows, only uses the buffer up to its
//! last line break (or all of it when there is none). Since that changes where sentences are cut
//! on long inputs, the port reproduces the same chunking.

const BUFFER_SIZE: usize = 1024;

pub(crate) struct ChunkReader {
    units: Vec<u16>,
    /// Start of the buffer, as a code-unit offset into `units`.
    offset: usize,
    /// Code units in the buffer.
    length: usize,
    /// Code units of the buffer that the current chunk covers.
    usable_length: usize,
    /// Byte offset of the current chunk's start. A supplementary character counts its four
    /// bytes on its high surrogate, so a chunk boundary inside a pair still maps to a position.
    byte_offset: usize,
}

impl ChunkReader {
    pub(crate) fn new(text: &str) -> Self {
        ChunkReader {
            units: text.encode_utf16().collect(),
            offset: 0,
            length: 0,
            usable_length: 0,
            byte_offset: 0,
        }
    }

    /// Advances to the next chunk, writes its text into `chunk` and returns the byte offset of
    /// its start, or `None` once the input is exhausted.
    ///
    /// Lucene cuts the buffer at a code-unit boundary even inside a surrogate pair, leaving a lone
    /// surrogate at each side. Those can't exist in a `str`, so such a pair is kept whole in the
    /// earlier chunk and omitted from the later one.
    pub(crate) fn next_chunk(&mut self, chunk: &mut String) -> Option<usize> {
        let read_so_far = self.offset + self.length;
        let used = &self.units[self.offset..self.offset + self.usable_length];
        self.byte_offset += used
            .iter()
            .map(|&unit| utf8_len_of_unit(unit))
            .sum::<usize>();
        self.offset += self.usable_length;
        let leftover = read_so_far - self.offset;
        let requested = BUFFER_SIZE - leftover;
        let returned = requested.min(self.units.len() - read_so_far);
        self.length = leftover + returned;
        if self.length == 0 {
            return None;
        }
        let buffer = &self.units[self.offset..self.offset + self.length];
        let input_exhausted = returned < requested;
        self.usable_length = if input_exhausted {
            self.length
        } else {
            last_safe_end(buffer).unwrap_or(self.length)
        };

        let mut start = self.offset;
        let mut end = self.offset + self.usable_length;
        if is_low_surrogate(self.units[start]) {
            start += 1;
        }
        if end < self.units.len() && is_high_surrogate(self.units[end - 1]) {
            end += 1;
        }
        chunk.clear();
        chunk
            .extend(char::decode_utf16(self.units[start..end].iter().copied()).map(|c| c.unwrap()));
        Some(self.byte_offset)
    }
}

/// One past the last line break in `buffer` (CR, LF, NEL, LINE SEPARATOR, PARAGRAPH SEPARATOR).
fn last_safe_end(buffer: &[u16]) -> Option<usize> {
    buffer
        .iter()
        .rposition(|&unit| matches!(unit, 0x000D | 0x000A | 0x0085 | 0x2028 | 0x2029))
        .map(|index| index + 1)
}

fn utf8_len_of_unit(unit: u16) -> usize {
    match unit {
        0..=0x7F => 1,
        0x80..=0x7FF => 2,
        _ if is_high_surrogate(unit) => 4,
        _ if is_low_surrogate(unit) => 0,
        _ => 3,
    }
}

fn is_high_surrogate(unit: u16) -> bool {
    (0xD800..=0xDBFF).contains(&unit)
}

fn is_low_surrogate(unit: u16) -> bool {
    (0xDC00..=0xDFFF).contains(&unit)
}
