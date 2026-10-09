//! Character classification, a port of Lucene's `Utility.getCharType`.
//!
//! Lucene classifies UTF-16 code units by raw ranges, not Unicode properties: `Hanzi` is only
//! U+4E00..=U+9FA5, letters and digits are ASCII plus their fullwidth forms, three fixed ranges
//! are punctuation, surrogates are their own class, and everything else is `Other`. The exact
//! table is in `testdata/smartcn/golden/chartypes.txt`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CharType {
    Delimiter,
    Letter,
    Digit,
    Hanzi,
    SpaceLike,
    FullwidthLetter,
    FullwidthDigit,
    Other,
    /// Lucene classifies each half of a surrogate pair as this; the segmenter then emits the pair
    /// as one single-character word. The port classifies code points, so any code point above the
    /// BMP is `Surrogate`.
    Surrogate,
}

/// Classifies a code point (or a lone UTF-16 code unit in the surrogate range).
pub(crate) fn char_type(cp: u32) -> CharType {
    match cp {
        0xD800..=0xDFFF | 0x10000.. => CharType::Surrogate,
        0x4E00..=0x9FA5 => CharType::Hanzi,
        0x41..=0x5A | 0x61..=0x7A => CharType::Letter,
        0x30..=0x39 => CharType::Digit,
        0x20 | 0x09 | 0x0D | 0x0A | 0x3000 => CharType::SpaceLike,
        0x21..=0xBB | 0x2010..=0x2642 | 0x3001..=0x301E => CharType::Delimiter,
        0xFF21..=0xFF3A | 0xFF41..=0xFF5A => CharType::FullwidthLetter,
        0xFF10..=0xFF19 => CharType::FullwidthDigit,
        0xFE30..=0xFF63 => CharType::Delimiter,
        _ => CharType::Other,
    }
}
