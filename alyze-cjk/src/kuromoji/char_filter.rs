//! Char filters: text rewrites applied before tokenizing, with the offset bookkeeping that maps
//! token offsets back to the original input (Lucene's `CharFilter.correctOffset`).

use super::Tokens;

/// The output of a char filter: the rewritten text and the map back to the original offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filtered {
    text: String,
    /// `(filtered byte offset, original byte offset)` at every char boundary of `text`, including
    /// its end, ascending.
    map: Vec<(usize, usize)>,
}

impl Filtered {
    /// The rewritten text, to tokenize.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The original byte offset of a byte offset into [`Filtered::text`] (which must be on a
    /// char boundary of it).
    pub fn correct_offset(&self, byte_offset: usize) -> usize {
        match self.map.binary_search_by_key(&byte_offset, |m| m.0) {
            Ok(i) => self.map[i].1,
            Err(_) => panic!("{byte_offset} is not a char boundary of the filtered text"),
        }
    }

    /// Rewrites the byte ranges of tokens produced from [`Filtered::text`] to ranges of the
    /// original input.
    pub fn correct_tokens(&self, tokens: &mut Tokens) {
        for token in &mut tokens.items {
            token.byte_range = self.correct_offset(token.byte_range.start)
                ..self.correct_offset(token.byte_range.end);
        }
    }

    /// Builds the byte map from a UTF-16 correction function: `correct(o)` is the original
    /// UTF-16 offset of filtered UTF-16 offset `o`.
    fn from_units(
        original: &str,
        filtered: Vec<u16>,
        correct: impl Fn(usize) -> usize,
    ) -> Filtered {
        // Original UTF-16 offset → original byte offset.
        let mut original_bytes: Vec<usize> = Vec::with_capacity(original.len() + 1);
        for (byte_offset, c) in original.char_indices() {
            original_bytes.push(byte_offset);
            if c.len_utf16() == 2 {
                original_bytes.push(byte_offset + c.len_utf8());
            }
        }
        original_bytes.push(original.len());
        let text = String::from_utf16(&filtered).expect("char filter produced a lone surrogate");
        let mut map = Vec::with_capacity(text.len() + 1);
        let mut units = 0usize;
        for (byte_offset, c) in text.char_indices() {
            map.push((byte_offset, original_bytes[correct(units)]));
            units += c.len_utf16();
        }
        map.push((text.len(), original_bytes[correct(units)]));
        Filtered { text, map }
    }
}

// ------------------------------------------------------------------------------------------------
// Iteration marks

const KANJI_ITERATION_MARK: u16 = 0x3005; // 々
const HIRAGANA_ITERATION_MARK: u16 = 0x309D; // ゝ
const HIRAGANA_VOICED_ITERATION_MARK: u16 = 0x309E; // ゞ
const KATAKANA_ITERATION_MARK: u16 = 0x30FD; // ヽ
const KATAKANA_VOICED_ITERATION_MARK: u16 = 0x30FE; // ヾ
const FULL_STOP_PUNCTUATION: u16 = 0x3002; // 。

/// Hiragana か..ぼ (U+304B..U+307C) to their dakuten variants (a kana without one maps to itself).
static H2D: [u16; 50] = [
    0x304C, 0x304C, 0x304E, 0x304E, 0x3050, 0x3050, 0x3052, 0x3052, 0x3054, 0x3054, 0x3056, 0x3056,
    0x3058, 0x3058, 0x305A, 0x305A, 0x305C, 0x305C, 0x305E, 0x305E, 0x3060, 0x3060, 0x3062, 0x3062,
    0x3063, 0x3065, 0x3065, 0x3067, 0x3067, 0x3069, 0x3069, 0x306A, 0x306B, 0x306C, 0x306D, 0x306E,
    0x3070, 0x3070, 0x3071, 0x3073, 0x3073, 0x3074, 0x3076, 0x3076, 0x3077, 0x3079, 0x3079, 0x307A,
    0x307C, 0x307C,
];
const HIRAGANA_KA: u16 = 0x304B;
const KATAKANA_KA: u16 = 0x30AB;

/// Lucene's `JapaneseIterationMarkCharFilter` (Elasticsearch's `kuromoji_iteration_mark`):
/// expands the horizontal iteration marks 々 (kanji), ゝゞ (hiragana) and ヽヾ (katakana) to the
/// character they repeat, voicing or unvoicing kana as the mark says. Runs of marks repeat the
/// run before them; a mark with nothing to repeat (at the start, after another run, after 。
/// which flushes the filter's buffer, or after a supplementary character) is left alone. The
/// text length never changes.
pub fn iteration_mark(text: &str, normalize_kanji: bool, normalize_kana: bool) -> Filtered {
    let input: Vec<u16> = text.encode_utf16().collect();
    let mut filter = IterationMarkFilter {
        input: &input,
        normalize_kanji,
        normalize_kana,
        span_size: 0,
        span_end: 0,
    };
    let mut out = Vec::with_capacity(input.len());
    for pos in 0..input.len() {
        out.push(filter.read(pos));
    }
    Filtered::from_units(text, out, |o| o)
}

struct IterationMarkFilter<'a> {
    input: &'a [u16],
    normalize_kanji: bool,
    normalize_kana: bool,
    /// Size of the current span of iteration marks.
    span_size: usize,
    /// Position after the current span (marks before it, or at it, can't start a new span).
    span_end: usize,
}

impl IterationMarkFilter<'_> {
    /// Lucene's `read()` for the character at `pos`.
    fn read(&mut self, pos: usize) -> u16 {
        let mut c = self.input[pos];
        // A surrogate pair, like a full stop, ends any span before it.
        if (0xD800..0xE000).contains(&c) {
            self.span_end = pos + 1;
        }
        if c == FULL_STOP_PUNCTUATION {
            self.span_end = pos + 1;
        }
        if self.is_iteration_mark(c) {
            c = self.normalize_iteration_mark(pos, c);
        }
        c
    }

    fn normalize_iteration_mark(&mut self, pos: usize, c: u16) -> u16 {
        // Inside a span.
        if pos < self.span_end {
            return self.normalize(self.source(pos), c);
        }
        // A new span starting where the previous one ended is illegal: emit the mark and move
        // the end so that the next position can't start one either.
        if pos == self.span_end {
            self.span_end += 1;
            return c;
        }
        // A new span.
        self.span_size = self.next_span_size(pos);
        self.span_end = pos + self.span_size;
        self.normalize(self.source(pos), c)
    }

    /// The number of consecutive iteration marks from `pos`, limited so that the characters they
    /// repeat don't reach back past the previous span's end.
    fn next_span_size(&self, pos: usize) -> usize {
        let mut span = 0usize;
        while pos + span < self.input.len() && self.is_iteration_mark(self.input[pos + span]) {
            span += 1;
        }
        if pos < self.span_end + span {
            span = pos - self.span_end;
        }
        span
    }

    /// The input character a mark at `pos` repeats.
    fn source(&self, pos: usize) -> u16 {
        self.input[pos - self.span_size]
    }

    fn normalize(&self, c: u16, mark: u16) -> u16 {
        if self.is_hiragana_iteration_mark(mark) {
            return match mark {
                HIRAGANA_ITERATION_MARK => {
                    if is_dakuten(c, HIRAGANA_KA) {
                        c - 1
                    } else {
                        c
                    }
                }
                _ => lookup_dakuten(c, HIRAGANA_KA),
            };
        }
        if self.is_katakana_iteration_mark(mark) {
            return match mark {
                KATAKANA_ITERATION_MARK => {
                    if is_dakuten(c, KATAKANA_KA) {
                        c - 1
                    } else {
                        c
                    }
                }
                _ => lookup_dakuten(c, KATAKANA_KA),
            };
        }
        // A kanji mark: the character repeats as-is, whatever it is.
        c
    }

    fn is_iteration_mark(&self, c: u16) -> bool {
        self.is_kanji_iteration_mark(c)
            || self.is_hiragana_iteration_mark(c)
            || self.is_katakana_iteration_mark(c)
    }

    fn is_hiragana_iteration_mark(&self, c: u16) -> bool {
        self.normalize_kana && (c == HIRAGANA_ITERATION_MARK || c == HIRAGANA_VOICED_ITERATION_MARK)
    }

    fn is_katakana_iteration_mark(&self, c: u16) -> bool {
        self.normalize_kana && (c == KATAKANA_ITERATION_MARK || c == KATAKANA_VOICED_ITERATION_MARK)
    }

    fn is_kanji_iteration_mark(&self, c: u16) -> bool {
        self.normalize_kanji && c == KANJI_ITERATION_MARK
    }
}

/// The dakuten variant of a kana in the か..ぼ range starting at `base` (the katakana table is
/// the hiragana one shifted), or the kana itself.
fn lookup_dakuten(c: u16, base: u16) -> u16 {
    if (base..base + H2D.len() as u16).contains(&c) {
        H2D[(c - base) as usize] + (base - HIRAGANA_KA)
    } else {
        c
    }
}

/// Whether `c` is itself a dakuten kana (maps to itself in the table).
fn is_dakuten(c: u16, base: u16) -> bool {
    (base..base + H2D.len() as u16).contains(&c) && lookup_dakuten(c, base) == c
}

// ------------------------------------------------------------------------------------------------
// CJK width

/// Halfwidth katakana U+FF65..U+FF9F to their fullwidth forms (the voiced marks map to the
/// combining marks U+3099 / U+309A).
pub(crate) static KANA_NORM: [u16; 59] = [
    0x30fb, 0x30f2, 0x30a1, 0x30a3, 0x30a5, 0x30a7, 0x30a9, 0x30e3, 0x30e5, 0x30e7, 0x30c3, 0x30fc,
    0x30a2, 0x30a4, 0x30a6, 0x30a8, 0x30aa, 0x30ab, 0x30ad, 0x30af, 0x30b1, 0x30b3, 0x30b5, 0x30b7,
    0x30b9, 0x30bb, 0x30bd, 0x30bf, 0x30c1, 0x30c4, 0x30c6, 0x30c8, 0x30ca, 0x30cb, 0x30cc, 0x30cd,
    0x30ce, 0x30cf, 0x30d2, 0x30d5, 0x30d8, 0x30db, 0x30de, 0x30df, 0x30e0, 0x30e1, 0x30e2, 0x30e4,
    0x30e6, 0x30e8, 0x30e9, 0x30ea, 0x30eb, 0x30ec, 0x30ed, 0x30ef, 0x30f3, 0x3099, 0x309A,
];

/// How much a kana in U+30A6..U+30FD moves when a voiced mark follows it.
pub(crate) static KANA_COMBINE_VOICED: [u16; 88] = [
    78, 0, 0, 0, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 1,
    0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 8, 8, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
];

/// The same for a semi-voiced mark.
pub(crate) static KANA_COMBINE_SEMI_VOICED: [u16; 88] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 2, 0, 0, 2, 0, 0, 2, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

pub(crate) const HW_KATAKANA_VOICED_MARK: u16 = 0xFF9E;
pub(crate) const HW_KATAKANA_SEMI_VOICED_MARK: u16 = 0xFF9F;

/// The kana `ch` combined with a halfwidth voiced / semi-voiced mark, or `ch` itself when there is
/// no precomposed character.
pub(crate) fn combine_voice_mark(ch: u16, voice_mark: u16) -> u16 {
    if (0x30A6..=0x30FD).contains(&ch) {
        let table = if voice_mark == HW_KATAKANA_SEMI_VOICED_MARK {
            &KANA_COMBINE_SEMI_VOICED
        } else {
            &KANA_COMBINE_VOICED
        };
        ch + table[(ch - 0x30A6) as usize]
    } else {
        ch
    }
}

/// The width normalization of one code unit (no combining).
#[inline]
pub(crate) fn normalize_width(ch: u16) -> u16 {
    if (0xFF01..=0xFF5E).contains(&ch) {
        ch - 0xFEE0
    } else if (0xFF65..=0xFF9F).contains(&ch) {
        KANA_NORM[(ch - 0xFF65) as usize]
    } else {
        ch
    }
}

/// Lucene's `CJKWidthCharFilter` (what the `kuromoji` and `kuromoji_completion` analyzers apply
/// first): folds fullwidth ASCII variants to ASCII and halfwidth katakana to katakana, combining
/// a halfwidth voiced or semi-voiced sound mark into the preceding kana where a precomposed
/// character exists (ﾊﾟ → パ), and otherwise mapping it to the combining mark (U+3099/U+309A).
pub fn cjk_width(text: &str) -> Filtered {
    let input: Vec<u16> = text.encode_utf16().collect();
    let mut out: Vec<u16> = Vec::with_capacity(input.len());
    // Lucene's BaseCharFilter offset map: (output offset, cumulative diff) pairs.
    let mut offsets: Vec<(usize, usize)> = Vec::new();
    let mut prev: Option<u16> = None;
    let mut input_off = 0usize;
    for &ch in &input {
        input_off += 1;
        if ch == HW_KATAKANA_SEMI_VOICED_MARK || ch == HW_KATAKANA_VOICED_MARK {
            if let Some(p) = prev {
                let combined = combine_voice_mark(p, ch);
                if combined != p {
                    prev = None;
                    let prev_diff = offsets.last().map_or(0, |o| o.1);
                    add_off_correct_map(&mut offsets, input_off - 1 - prev_diff, prev_diff + 1);
                    out.push(combined);
                    continue;
                }
            }
        }
        if let Some(p) = prev {
            out.push(p);
        }
        prev = Some(normalize_width(ch));
    }
    if let Some(p) = prev {
        out.push(p);
    }
    Filtered::from_units(text, out, |o| correct(&offsets, o))
}

/// `BaseCharFilter.addOffCorrectMap`.
fn add_off_correct_map(offsets: &mut Vec<(usize, usize)>, off: usize, cumulative_diff: usize) {
    match offsets.last_mut() {
        Some(last) if last.0 == off => last.1 = cumulative_diff,
        _ => offsets.push((off, cumulative_diff)),
    }
}

/// `BaseCharFilter.correct`: the diff of the last map entry at or before the offset.
fn correct(offsets: &[(usize, usize)], current: usize) -> usize {
    let index = offsets.partition_point(|o| o.0 <= current);
    if index == 0 {
        current
    } else {
        current + offsets[index - 1].1
    }
}
