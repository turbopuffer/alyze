//! Port of Lucene's `nori` analyzer: the Korean morphological analyzer behind Elasticsearch's
//! `nori` analyzer, `nori_tokenizer`, `nori_part_of_speech`, `nori_readingform` and `nori_number`.
//! Derived from Apache Lucene's `lucene-analysis-nori` module (Apache License 2.0; see the
//! crate's `NOTICE`), whose dictionaries are built from mecab-ko-dic 2.1.1 (Apache License 2.0).
//!
//! Nori is a MeCab-style analyzer: per input it builds a lattice of candidate morphemes (every
//! dictionary word starting at each position, the longest user-dictionary word, and heuristically
//! grouped unknown words), scores paths with per-word costs plus a connection-cost matrix and a
//! penalty for words preceded by spaces, and emits the cheapest path. Dictionary entries that are
//! compounds, inflected forms or pre-analysed sequences can be decomposed into their morphemes
//! ([`DecompoundMode`]). Every token carries its part of speech ([`pos`]), and Hanja entries carry
//! a Hangul reading.
//!
//! Parity contract: [`tokenize`] and the filters match Lucene token for token (text, offsets,
//! position increments and lengths, part of speech, reading), with two deliberate exceptions:
//!
//! - Lucene caps unknown words at 1024 UTF-16 code units, so a run of supplementary characters
//!   (emoji, say) can be cut inside a surrogate pair, yielding half-code-point junk at the cut;
//!   the port keeps code points whole, attaching the pair to the token that holds its first half.
//! - [`filter::lowercase`] (the last stage of the `nori` analyzer) uses alyze's pinned Unicode
//!   lowercase mapping, where Lucene's `LowerCaseFilter` uses Java's simple one-to-one mapping;
//!   the two differ for a handful of code points (e.g. U+0130).
//!
//! All of this is checked against golden files produced by the Java implementation, see
//! `testdata/nori/README.md`.
//!
//! This module is deliberately self-contained (it doesn't plug into `alyze`'s analyzer and
//! filter chain yet); integrating it there is a later step.
//!
//! TODO(size): the dictionaries (`data/nori/`) are stored uncompressed, about 26 MB. That is fine
//! while the crate is `publish = false`, but crates.io caps a crate at 10 MB, so before publishing
//! they need to be compressed (deflate gets them to roughly 3-4 MB) and inflated on first use.

pub(crate) mod char_def;
pub(crate) mod dict;
pub mod filter;
pub mod pos;
pub(crate) mod unicode;
mod user_dict;
mod viterbi;

#[cfg(test)]
mod tests;

use std::ops::Range;

pub use user_dict::{UserDictionary, UserDictionaryError};

/// How compound, inflected and pre-analysed dictionary entries are emitted (Elasticsearch's
/// `decompound_mode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecompoundMode {
    /// Emit the whole entry as one token.
    None,
    /// Emit only the morphemes it decomposes into (the default).
    #[default]
    Discard,
    /// Emit the whole entry followed by its morphemes, as a token graph: the whole entry spans
    /// the morphemes' positions (`position_length` = morpheme count) and the first morpheme has
    /// `position_increment` 0.
    Mixed,
}

/// Tokenizer options (Elasticsearch's `nori_tokenizer` settings, plus Lucene's
/// `outputUnknownUnigrams`, which Elasticsearch doesn't expose).
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Options<'a> {
    pub decompound_mode: DecompoundMode,
    /// Drop punctuation tokens (default `true`). When `false`, the whitespace between words is
    /// emitted as tokens too, so the tokens tile the input exactly.
    pub discard_punctuation: bool,
    /// Emit unknown words as one token per character instead of one token per run (default
    /// `false`). Not reachable from Elasticsearch.
    pub output_unknown_unigrams: bool,
    pub user_dictionary: Option<&'a UserDictionary>,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Options {
            decompound_mode: DecompoundMode::Discard,
            discard_punctuation: true,
            output_unknown_unigrams: false,
            user_dictionary: None,
        }
    }
}

/// Where a token came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// The system dictionary (mecab-ko-dic).
    Known,
    /// Heuristic grouping of characters not in any dictionary.
    Unknown,
    /// The user dictionary.
    User,
}

/// One morpheme of a decomposable dictionary entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Morpheme {
    pub tag: pos::Tag,
    pub text: String,
}

/// A token, as a view into a [`Tokens`] buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Token<'a> {
    /// Token text. Usually a slice of the input, but not always: the morphemes of an inflected
    /// entry differ from its surface form (감싸여 → 감싸이 + 어), the reading-form filter replaces
    /// Hanja with Hangul, and the number and lowercase filters rewrite text.
    pub text: &'a str,
    /// Byte range of the token in the input. Always on UTF-8 boundaries. Morphemes of an inflected
    /// or pre-analysed entry all share the entry's range.
    pub byte_range: Range<usize>,
    /// Lucene's position increment: 1, except 0 for the first morpheme after a whole entry in
    /// [`DecompoundMode::Mixed`] and >1 after tokens the part-of-speech stop filter removed.
    pub position_increment: u32,
    /// Lucene's position length: 1, except for a whole entry in [`DecompoundMode::Mixed`], which
    /// spans its morphemes.
    pub position_length: u32,
    pub kind: TokenKind,
    pub pos_type: pos::Type,
    /// Part of speech of the token's first morpheme (what the part-of-speech stop filter checks).
    pub left_pos: pos::Tag,
    /// Part of speech of the token's last morpheme; equals `left_pos` unless the token is an
    /// undecomposed inflected or pre-analysed entry.
    pub right_pos: pos::Tag,
    /// Hangul reading of a Hanja entry.
    pub reading: Option<&'a str>,
    /// The morphemes of an undecomposed compound, inflected or pre-analysed entry (empty for
    /// everything else, including the decomposed morphemes themselves).
    pub morphemes: &'a [Morpheme],
}

/// Output buffer for [`tokenize`], [`analyze`] and the [`filter`]s: a list of tokens plus the
/// storage their texts live in. Reuse one across inputs to amortise allocations.
#[derive(Default)]
pub struct Tokens {
    pub(crate) text: String,
    pub(crate) morphemes: Vec<Morpheme>,
    pub(crate) items: Vec<TokenData>,
    /// The tokenizer's working memory, kept here so callers reusing a `Tokens` reuse it too.
    pub(crate) scratch: viterbi::Scratch,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// A token's data, with text as ranges into the [`Tokens`] buffers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TokenData {
    pub text: Range<usize>,
    pub byte_range: Range<usize>,
    pub position_increment: u32,
    pub position_length: u32,
    pub kind: TokenKind,
    pub pos_type: pos::Type,
    pub left_pos: pos::Tag,
    pub right_pos: pos::Tag,
    pub reading: Option<Range<usize>>,
    pub morphemes: Range<usize>,
}

impl Tokens {
    pub fn new() -> Tokens {
        Tokens::default()
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.morphemes.clear();
        self.items.clear();
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<Token<'_>> {
        self.items.get(index).map(|data| self.view(data))
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = Token<'_>> + '_ {
        self.items.iter().map(|data| self.view(data))
    }

    fn view<'a>(&'a self, data: &TokenData) -> Token<'a> {
        Token {
            text: &self.text[data.text.clone()],
            byte_range: data.byte_range.clone(),
            position_increment: data.position_increment,
            position_length: data.position_length,
            kind: data.kind,
            pos_type: data.pos_type,
            left_pos: data.left_pos,
            right_pos: data.right_pos,
            reading: data.reading.clone().map(|r| &self.text[r]),
            morphemes: &self.morphemes[data.morphemes.clone()],
        }
    }

    /// Appends `text` to the text buffer and returns its range.
    pub(crate) fn push_text(&mut self, text: &str) -> Range<usize> {
        let start = self.text.len();
        self.text.push_str(text);
        start..self.text.len()
    }

    pub(crate) fn push(&mut self, data: TokenData) {
        self.items.push(data);
    }
}

/// Tokenizes `text` exactly like Lucene's `KoreanTokenizer` (Elasticsearch's `nori_tokenizer`),
/// appending the tokens to `out` (which is cleared first).
pub fn tokenize(text: &str, options: Options<'_>, out: &mut Tokens) {
    out.clear();
    viterbi::tokenize(text, options, out);
}

/// Options of the `nori` analyzer: the tokenizer with punctuation discarded, then the
/// part-of-speech stop filter, the reading-form filter and lowercasing.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct AnalyzerOptions<'a> {
    pub decompound_mode: DecompoundMode,
    /// Tags of the tokens to drop (Elasticsearch's `stoptags`).
    pub stop_tags: pos::TagSet,
    pub user_dictionary: Option<&'a UserDictionary>,
}

impl Default for AnalyzerOptions<'_> {
    fn default() -> Self {
        AnalyzerOptions {
            decompound_mode: DecompoundMode::Discard,
            stop_tags: pos::TagSet::DEFAULT_STOP_TAGS,
            user_dictionary: None,
        }
    }
}

/// Runs the `nori` analyzer (Lucene's `KoreanAnalyzer`) over `text`, filling `out`.
pub fn analyze(text: &str, options: AnalyzerOptions<'_>, out: &mut Tokens) {
    tokenize(
        text,
        Options {
            decompound_mode: options.decompound_mode,
            discard_punctuation: true,
            output_unknown_unigrams: false,
            user_dictionary: options.user_dictionary,
        },
        out,
    );
    filter::part_of_speech_stop(out, options.stop_tags);
    filter::reading_form(out);
    filter::lowercase(out);
}
