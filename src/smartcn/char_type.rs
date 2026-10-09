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
    // Stub: the port has not been written yet.
    let _ = cp;
    CharType::Other
}
