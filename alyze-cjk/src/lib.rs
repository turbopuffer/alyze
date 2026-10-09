//! Dictionary data for `alyze`'s CJK (Chinese, Japanese, Korean) analysis.
//!
//! Pulled into `alyze` through its `cjk` feature. The analysis code itself lives in `alyze`
//! (`alyze::smartcn`) so it can share `alyze`'s tokenizer infrastructure and filter chain; this
//! crate exists only so that the multi-megabyte dictionaries don't inflate `alyze` for users who
//! don't need them.

/// Dictionaries for the `smartcn` segmenter, converted from Lucene's `coredict.mem` and
/// `bigramdict.mem` (ICTCLAS data, Apache License 2.0) by `alyze`'s `smartcn_convert_dicts`
/// example, which also documents the blob formats.
pub mod smartcn {
    /// The core word dictionary: every known word with its frequency.
    pub const CORE_DICT: &[u8] = include_bytes!("../data/coredict.bin");

    /// The bigram dictionary: a hash of every known word pair with its frequency.
    pub const BIGRAM_DICT: &[u8] = include_bytes!("../data/bigramdict.bin");
}
