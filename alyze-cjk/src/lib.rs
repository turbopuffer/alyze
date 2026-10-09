//! CJK (Chinese, Japanese, Korean) text analysis for `alyze`.
//!
//! Enabled through the `cjk` feature of the `alyze` crate, which re-exports this crate as
//! `alyze::cjk`. Kept as a separate crate so the multi-megabyte dictionaries don't inflate
//! `alyze` for users who don't need them, and because it is Apache-2.0 licensed: `smartcn` and
//! `nori` are derived from Apache Lucene (Copyright The Apache Software Foundation;
//! SmartChineseAnalyzer provided by Xiaoping Gao, copyright 2009 www.imdict.net; nori's
//! dictionaries from mecab-ko-dic), used under the Apache License 2.0. See `LICENSE` and `NOTICE`.
//!
//! SPDX-License-Identifier: Apache-2.0

pub mod nori;
pub mod smartcn;

#[cfg(test)]
pub(crate) mod testutil;
