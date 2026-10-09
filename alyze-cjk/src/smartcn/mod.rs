//! Port of Lucene's `smartcn` analyzer: the segmenter behind Elasticsearch's `smartcn` analyzer
//! and `smartcn_tokenizer`. Derived from Apache Lucene's `lucene-analysis-smartcn` (Apache License
//! 2.0; see the crate's `NOTICE`).
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
//! 5. ([`analyze`] only) Porter-stem every token and drop punctuation, which consumes a position.
//!
//! Parity contract: steps 1-4 ([`tokenize`]) match Lucene bit for bit, including offsets, with one
//! exception: when Lucene's 1024-unit cut lands inside a surrogate pair it emits each half as a
//! junk one-unit token; the port emits the whole code point once, at the first half's position.
//! Step 5 ([`analyze`]) uses Porter2 rather than Lucene's Porter, so stems of English words may
//! differ slightly from Elasticsearch; everything else matches.
//!
//! All of this is checked against golden files produced by the Java implementation, see
//! `testdata/smartcn/README.md`.
//!
//! This module is deliberately self-contained (it doesn't plug into `alyze`'s analyzer and
//! filter chain yet); integrating it there is a later step.

pub(crate) mod char_type;
mod chunks;
pub(crate) mod dict;
mod jdk_sentence_tables;
mod segmenter;
pub(crate) mod sentence;

#[cfg(test)]
mod tests;

use std::ops::Range;

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
    mut on_token: impl FnMut(Token<'_>) -> bool,
) {
    let Options {} = options;
    let mut segmenter = segmenter::Segmenter::new();
    let mut chunks = chunks::ChunkReader::new(text);
    let mut chunk = String::new();
    let mut boundaries = Vec::new();
    while let Some(chunk_offset) = chunks.next_chunk(&mut chunk) {
        boundaries.clear();
        sentence::for_each_boundary(&chunk, |boundary| {
            boundaries.push(boundary);
            true
        });
        for window in boundaries.windows(2) {
            let (sentence_start, sentence_end) = (window[0], window[1]);
            let sentence_offset = chunk_offset + sentence_start;
            let keep_going = segmenter.segment(
                &chunk[sentence_start..sentence_end],
                buffer,
                |text, byte_range, kind| {
                    on_token(Token {
                        text,
                        byte_range: sentence_offset + byte_range.start
                            ..sentence_offset + byte_range.end,
                        kind,
                    })
                },
            );
            if !keep_going {
                return;
            }
        }
    }
}

/// A token from [`analyze`].
#[non_exhaustive]
pub struct AnalyzedToken<'a> {
    /// Normalized, stemmed token text. Only valid for the duration of the callback.
    pub text: &'a str,
    /// Position of the token in the sequence of tokens, 0-based. Punctuation tokens consume a
    /// position without being emitted, like Lucene's stop filter.
    pub position: usize,
    /// Byte range of the token in the input. Always on UTF-8 boundaries.
    pub byte_range: Range<usize>,
}

/// Analyzes `text` like Elasticsearch's `smartcn` analyzer: [`tokenize`], drop punctuation, stem
/// every token. Lucene stems with the original Porter algorithm, which is a no-op on anything but
/// ASCII letters; this uses Porter2 (`rust_stemmers`' English stemmer) on ASCII tokens, so stems
/// of English words can differ slightly. `buffer` is scratch space and should be reused.
pub fn analyze(
    text: &str,
    options: Options,
    buffer: &mut String,
    mut on_token: impl FnMut(AnalyzedToken<'_>) -> bool,
) {
    let stemmer = rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English);
    let mut next_position = 0;
    tokenize(text, options, buffer, |token| {
        let position = next_position;
        next_position += 1;
        if token.kind == TokenKind::Punctuation {
            return true;
        }
        let stemmed = if token.text.is_ascii() {
            stemmer.stem(token.text)
        } else {
            std::borrow::Cow::Borrowed(token.text)
        };
        on_token(AnalyzedToken {
            text: &stemmed,
            position,
            byte_range: token.byte_range,
        })
    });
}
