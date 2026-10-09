//! CJK (Chinese, Japanese, Korean) text analysis for `alyze`.
//!
//! Enabled through the `cjk` feature of the `alyze` crate, which re-exports this crate as
//! `alyze::cjk`. Kept as a separate crate so the multi-megabyte dictionaries don't inflate
//! `alyze` for users who don't need them.

pub mod smartcn;
