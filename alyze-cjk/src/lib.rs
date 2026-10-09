//! CJK (Chinese, Japanese, Korean) text analysis for `alyze`.
//!
//! Enabled through the `cjk` feature of the `alyze` crate, which re-exports this crate as
//! `alyze::cjk`. Kept as a separate crate so the multi-megabyte dictionaries don't inflate
//! `alyze` for users who don't need them, and because it is Apache-2.0 licensed: `smartcn`,
//! `nori` and `kuromoji` are derived from Apache Lucene (Copyright The Apache Software Foundation;
//! SmartChineseAnalyzer provided by Xiaoping Gao, copyright 2009 www.imdict.net; nori's
//! dictionaries from mecab-ko-dic, kuromoji's from mecab-ipadic), used under the Apache License
//! 2.0. See `LICENSE` and `NOTICE`.
//!
//! SPDX-License-Identifier: Apache-2.0

pub mod kuromoji;
pub(crate) mod morph;
pub mod nori;
pub mod smartcn;

/// alyze's pinned Unicode 17 lowercase table, included by path rather than copied so there is one
/// table (`alyze` can't be a dependency: it depends on this crate).
/// TODO(publish): the file lives outside this crate's directory, so a published `alyze-cjk` needs
/// it moved into a shared crate (or copied in) first; see the data-size TODO in Cargo.toml.
#[path = "../../src/analyze/u17_to_lower.rs"]
#[allow(dead_code, clippy::all)]
mod unicode_lower;

#[cfg(test)]
pub(crate) mod testutil;
