//! Dictionary data for `alyze`'s CJK (Chinese, Japanese, Korean) analysis.
//!
//! Pulled into `alyze` through its `cjk` feature. The analysis code itself lives in `alyze`
//! (`alyze::smartcn`) so it can share `alyze`'s tokenizer infrastructure and filter chain; this
//! crate exists only so that the multi-megabyte dictionaries don't inflate `alyze` for users who
//! don't need them.

/// Dictionaries for the `smartcn` segmenter, converted from Lucene's `coredict.mem` and
/// `bigramdict.mem` (ICTCLAS data, Apache License 2.0).
pub mod smartcn {
    /// The core word dictionary: every known word with its frequency. Format is defined by
    /// `alyze::smartcn::dict`.
    ///
    /// Placeholder: the conversion tool has not been written yet, so this is empty.
    pub const CORE_DICT: &[u8] = &[];

    /// The bigram dictionary: a hash of every known word pair with its frequency. Format is
    /// defined by `alyze::smartcn::dict`.
    ///
    /// Placeholder: the conversion tool has not been written yet, so this is empty.
    pub const BIGRAM_DICT: &[u8] = &[];
}
