//! Port of Lucene's `smartcn` analyzer: the segmenter behind Elasticsearch's `smartcn` analyzer
//! and `smartcn_tokenizer`.
//!
//! `smartcn` segments Simplified Chinese with a bigram Hidden Markov Model over the ICTCLAS 1.0
//! dictionaries. Per sentence it builds a lattice of candidate tokens (every dictionary word
//! starting at each character, runs of Latin letters, runs of digits, single punctuation and
//! single characters of any other script) and takes the Viterbi path under smoothed bigram
//! probabilities. The reference pipeline is:
//!
//! 1. Read the input in 1024-char chunks, cut at the last line break (or anywhere if none).
//! 2. Split each chunk into sentences with the JDK's sentence `BreakIterator` ([`sentence`]).
//! 3. Classify every UTF-16 code unit ([`char_type`]), build the lattice over the dictionaries
//!    ([`dict`]) and find the shortest path.
//! 4. Normalize tokens: fullwidth to halfwidth, ASCII A-Z lowercased, every punctuation token
//!    rewritten to `","`.
//! 5. (Analyzer only) Porter-stem every token and drop punctuation, which consumes a position.
//!
//! Parity contract: steps 1-4 ([`tokenize`]) match Lucene bit for bit, including offsets, with one
//! exception: when Lucene's 1024-unit cut lands inside a surrogate pair it emits each half as a
//! junk one-unit token; the port emits the whole code point once, at the first half's position.
//! Step 5 is done by `alyze`'s own filter chain ([`crate::analyze::Analyzer`] with
//! [`crate::analyze::TokenizerOptions::SmartCn`]) and uses Porter2 rather than Lucene's Porter, so
//! stems of English words may differ slightly from Elasticsearch; everything else matches.
//!
//! All of this is checked against golden files produced by the Java implementation, see
//! `testdata/smartcn/README.md`.

pub(crate) mod char_type;
pub(crate) mod dict;
mod jdk_sentence_tables;
pub(crate) mod sentence;

#[cfg(test)]
mod tests;

use std::ops::Range;

use crate::analyze::{AnalysisOptions, ReusableBuffer};

/// Tokenizer options. Nothing is configurable yet; the struct exists so that options can be added
/// without breaking callers.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct Options {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// A dictionary word or single Chinese character, a run of Latin letters, a run of digits, or
    /// a single character of any other script.
    Word,
    /// Punctuation. `text` is always `","`: Lucene rewrites every punctuation character to a comma
    /// (its default stop list then removes them, which is what Elasticsearch's `smartcn` analyzer
    /// does; `smartcn_tokenizer` on its own emits them).
    Punctuation,
}

/// A token from [`tokenize`].
#[non_exhaustive]
pub struct Token<'a> {
    /// Normalized token text (see the module docs). Not necessarily a slice of the input: Lucene
    /// skips whitespace inside a dictionary word, folds fullwidth forms and lowercases ASCII. Only
    /// valid for the duration of the callback.
    pub text: &'a str,
    /// Byte range of the token in the input. Always on UTF-8 boundaries.
    pub byte_range: Range<usize>,
    pub kind: TokenKind,
}

/// Tokenizes `text` exactly like Lucene's `HMMChineseTokenizer` (Elasticsearch's
/// `smartcn_tokenizer`), invoking `on_token` for each token in order. Returning `false` from the
/// callback stops tokenization. `buffer` is scratch space for normalized token text and should be
/// reused across calls.
pub fn tokenize(
    text: &str,
    options: Options,
    buffer: &mut String,
    on_token: impl FnMut(Token<'_>) -> bool,
) {
    // Stub: the port has not been written yet, so no tokens are emitted.
    let _ = (text, options, buffer, on_token);
}

/// [`crate::analyze::Analyzer::analyze_inputs`] for [`crate::analyze::TokenizerOptions::SmartCn`]:
/// runs [`tokenize`] and then `alyze`'s filter chain (lowercasing, stopwords, stemming, ASCII
/// folding, per `options`) on each token. Punctuation tokens are dropped but consume a position,
/// like Lucene's stop filter.
pub(crate) fn analyze_inputs<'a>(
    options: &AnalysisOptions,
    smartcn_options: Options,
    inputs: impl Iterator<Item = &'a str>,
    buffer: &mut ReusableBuffer,
    callback: impl FnMut(crate::analyze::Token<'_>) -> bool,
) {
    // Stub: the port has not been written yet, so no tokens are emitted.
    let _ = (options, smartcn_options, inputs, buffer, callback);
}
