//! Port of Lucene's `kuromoji` analyzer: the Japanese morphological analyzer behind
//! Elasticsearch's `kuromoji` and `kuromoji_completion` analyzers, `kuromoji_tokenizer`,
//! `kuromoji_iteration_mark`, and the `kuromoji_baseform`, `kuromoji_part_of_speech`,
//! `kuromoji_readingform`, `kuromoji_stemmer`, `ja_stop`, `kuromoji_number`,
//! `kuromoji_completion`, `hiragana_uppercase` and `katakana_uppercase` filters. Derived from
//! Apache Lucene's `lucene-analysis-kuromoji` module (Apache License 2.0; see the crate's
//! `NOTICE`), whose dictionaries are built from mecab-ipadic 2.7.0-20070801 (BSD-style licence,
//! see `NOTICE`).
//!
//! Kuromoji is a MeCab-style analyzer, a sibling of [`crate::nori`]: per input it builds a
//! lattice of candidate words (every dictionary word starting at each position, the longest
//! user-dictionary word, and heuristically grouped unknown words), scores paths with per-word
//! costs plus a connection-cost matrix, and emits the cheapest path. In [`Mode::Search`] (the
//! default) long words are penalized and a compound's best sub-segmentation is emitted alongside
//! it; [`Mode::Extended`] additionally emits unknown words as unigrams; an n-best cost
//! ([`Options::nbest_cost`]) emits every word of every path within that cost of the best one.
//! Every token carries its part of speech, base form, reading, pronunciation and inflection.
//!
//! Parity contract: [`tokenize`], the analyzers, the [`filter`]s and the [`char_filter`]s match
//! Lucene token for token (text, offsets, position increments and lengths, and every attribute),
//! with these deliberate exceptions, shared with the `nori` port:
//!
//! - Lucene caps unknown words at 1024 UTF-16 code units, so a run of supplementary characters
//!   can be cut inside a surrogate pair, yielding half-code-point junk at the cut; the port keeps
//!   code points whole, attaching the pair to the token that holds its first half.
//! - [`filter::lowercase`] (the last stage of both analyzers) uses alyze's pinned Unicode
//!   lowercase mapping, where Lucene's `LowerCaseFilter` uses Java's simple one-to-one mapping;
//!   the two differ for a handful of code points (e.g. U+0130).
//! - Filters are applied one at a time to a token buffer, so Lucene's attribute leaks between
//!   filters (the number filter's merged token taking a discarded token's attributes) are not
//!   reproduced.
//!
//! All of this is checked against golden files produced by the Java implementation, see
//! `testdata/kuromoji/README.md`.
//!
//! This module is deliberately self-contained (it doesn't plug into `alyze`'s analyzer and
//! filter chain yet); integrating it there is a later step.

pub(crate) mod char_def;
pub mod char_filter;
pub(crate) mod dict;
pub mod filter;
pub mod romaji;
pub(crate) mod unicode;
mod user_dict;
mod viterbi;

#[cfg(test)]
mod tests;

use std::ops::Range;

pub use user_dict::{UserDictionary, UserDictionaryError};

/// Tokenization mode (Elasticsearch's `mode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Ordinary segmentation: no decomposition of compounds.
    Normal,
    /// Segmentation geared towards search (the default): long words are penalized so that
    /// compounds decompose, and the compound itself is emitted too unless
    /// [`Options::discard_compound_token`] is set.
    #[default]
    Search,
    /// Like `Search`, and unknown words are emitted as unigrams (one token per character).
    Extended,
}

/// Tokenizer options (Elasticsearch's `kuromoji_tokenizer` settings).
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Options<'a> {
    pub mode: Mode,
    /// Drop punctuation tokens (default `true`). When `false`, whitespace and punctuation are
    /// emitted as tokens too, so the tokens tile the input exactly.
    pub discard_punctuation: bool,
    /// In `Search` and `Extended` mode, drop the compound token and keep only its decomposition
    /// (default `false`; Lucene's `JapaneseAnalyzer` sets it).
    pub discard_compound_token: bool,
    /// Elasticsearch's `nbest_cost`: when positive, every word of every lattice path whose cost
    /// is within this much of the best path is emitted too, as a token graph. Zero or negative
    /// disables it (Elasticsearch's default is -1). For `nbest_examples`, see
    /// [`calc_nbest_cost`].
    pub nbest_cost: i32,
    pub user_dictionary: Option<&'a UserDictionary>,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Options {
            mode: Mode::Search,
            discard_punctuation: true,
            discard_compound_token: false,
            nbest_cost: -1,
            user_dictionary: None,
        }
    }
}

/// Where a token came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// The system dictionary (mecab-ipadic).
    Known,
    /// Heuristic grouping of characters not in any dictionary.
    Unknown,
    /// The user dictionary.
    User,
}

/// A token, as a view into a [`Tokens`] buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Token<'a> {
    /// Token text. A slice of the (char-filtered) input until a filter rewrites it: the base-form,
    /// reading-form, stemmer, number, uppercase, width, completion and lowercase filters all do.
    pub text: &'a str,
    /// Byte range of the token in the input. Always on UTF-8 boundaries. After a
    /// [`char_filter`], ranges refer to the original input (see
    /// [`char_filter::Filtered::correct_tokens`]).
    pub byte_range: Range<usize>,
    /// Lucene's position increment: 1, except 0 for a token starting where the previous one did
    /// (a compound's first part in `Search` mode, n-best alternatives, completion romanizations)
    /// and >1 after tokens a stop filter removed.
    pub position_increment: u32,
    /// Lucene's position length: the number of positions the token spans (a compound in
    /// `Search` mode spans its parts; with an n-best cost every token's length is computed from
    /// the graph).
    pub position_length: u32,
    pub kind: TokenKind,
    /// Part of speech, as mecab-ipadic's hyphen-joined hierarchy (`名詞-固有名詞-地域-一般`).
    /// Empty for tokens the completion filter produced.
    pub part_of_speech: &'a str,
    /// Base form of an inflected verb or adjective; `None` when it equals the surface form, and
    /// for unknown and user words.
    pub base_form: Option<&'a str>,
    /// Katakana reading. Known words always have one (the surface form with hiragana shifted to
    /// katakana when the dictionary has none), user words have the rule's, unknown words none.
    pub reading: Option<&'a str>,
    /// Pronunciation (差 → チャー ...); the reading when the dictionary has none. `None` for
    /// unknown and user words.
    pub pronunciation: Option<&'a str>,
    /// Inflection type (`五段・ラ行`) of a conjugating word, else `None`.
    pub inflection_type: Option<&'a str>,
    /// Inflection form (`連用形`) of a conjugating word, else `None`.
    pub inflection_form: Option<&'a str>,
}

/// Output buffer for [`tokenize`], the analyzers and the [`filter`]s: a list of tokens plus the
/// storage their texts live in. Reuse one across inputs to amortise allocations.
#[derive(Default)]
pub struct Tokens {
    pub(crate) text: String,
    pub(crate) items: Vec<TokenData>,
    /// The tokenizer's working memory, kept here so callers reusing a `Tokens` reuse it too.
    pub(crate) scratch: viterbi::Scratch,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// A token's data, with strings as ranges into the [`Tokens`] text buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TokenData {
    pub text: Range<usize>,
    pub byte_range: Range<usize>,
    pub position_increment: u32,
    pub position_length: u32,
    pub kind: TokenKind,
    pub part_of_speech: Range<usize>,
    pub base_form: Option<Range<usize>>,
    pub reading: Option<Range<usize>>,
    pub pronunciation: Option<Range<usize>>,
    pub inflection_type: Option<Range<usize>>,
    pub inflection_form: Option<Range<usize>>,
}

impl Tokens {
    pub fn new() -> Tokens {
        Tokens::default()
    }

    pub fn clear(&mut self) {
        self.text.clear();
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
        let text = |r: &Range<usize>| &self.text[r.clone()];
        Token {
            text: text(&data.text),
            byte_range: data.byte_range.clone(),
            position_increment: data.position_increment,
            position_length: data.position_length,
            kind: data.kind,
            part_of_speech: text(&data.part_of_speech),
            base_form: data.base_form.as_ref().map(text),
            reading: data.reading.as_ref().map(text),
            pronunciation: data.pronunciation.as_ref().map(text),
            inflection_type: data.inflection_type.as_ref().map(text),
            inflection_form: data.inflection_form.as_ref().map(text),
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

    /// (position slots, code-unit buffer capacity) of the tokenizer's retained working memory.
    #[cfg(test)]
    pub(crate) fn scratch_footprint(&self) -> (usize, usize) {
        self.scratch.footprint()
    }
}

/// Tokenizes `text` exactly like Lucene's `JapaneseTokenizer` (Elasticsearch's
/// `kuromoji_tokenizer`), appending the tokens to `out` (which is cleared first).
pub fn tokenize(text: &str, options: Options<'_>, out: &mut Tokens) {
    out.clear();
    viterbi::tokenize(text, options, out);
}

/// Derives an n-best cost from examples, like Lucene's `JapaneseTokenizer.calcNBestCost`
/// (Elasticsearch's `nbest_examples`): `examples` is `/text-word/text-word/...`, and the result
/// is the smallest cost at which every `word` appears among the tokens of its `text`, given the
/// rest of `options` (0 when every word already appears, or no example applies). Elasticsearch
/// uses the larger of this and `nbest_cost`.
pub fn calc_nbest_cost(examples: &str, options: Options<'_>) -> Result<i32, NBestExamplesError> {
    let mut max_delta = 0;
    for example in java_split(examples, '/') {
        if example.is_empty() {
            continue;
        }
        let pair = java_split(example, '-');
        if pair.len() != 2 {
            return Err(NBestExamplesError(example.to_owned()));
        }
        max_delta = max_delta.max(viterbi::probe_delta(pair[0], pair[1], options));
    }
    Ok(max_delta)
}

/// `String.split` with a one-character separator: trailing empty strings are dropped.
fn java_split(s: &str, separator: char) -> Vec<&str> {
    let mut parts: Vec<&str> = s.split(separator).collect();
    while parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    if parts == [""] && !s.is_empty() {
        parts.clear();
    }
    parts
}

/// An `nbest_examples` entry that isn't `text-word`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NBestExamplesError(pub String);

impl std::fmt::Display for NBestExamplesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unexpected example form: {} (expected two '-')", self.0)
    }
}

impl std::error::Error for NBestExamplesError {}

/// Options of the `kuromoji` analyzer (Lucene's `JapaneseAnalyzer`): width normalization
/// ([`char_filter::cjk_width`]), the tokenizer with punctuation and compound tokens discarded,
/// then the base-form filter, the part-of-speech stop filter, the stop-word filter, the katakana
/// stemmer and lowercasing.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct AnalyzerOptions<'a> {
    pub mode: Mode,
    pub user_dictionary: Option<&'a UserDictionary>,
    /// Stop words (Elasticsearch's `stopwords`); `None` is the default Japanese list.
    pub stop_words: Option<&'a filter::StopWords>,
    /// Stop tags (Lucene's `stoptags`, always the default in Elasticsearch); `None` is the
    /// default list.
    pub stop_tags: Option<&'a filter::StopTags>,
}

/// Runs the `kuromoji` analyzer over `text`, filling `out`.
pub fn analyze(text: &str, options: AnalyzerOptions<'_>, out: &mut Tokens) {
    let filtered = char_filter::cjk_width(text);
    tokenize(
        filtered.text(),
        Options {
            mode: options.mode,
            discard_punctuation: true,
            discard_compound_token: true,
            nbest_cost: -1,
            user_dictionary: options.user_dictionary,
        },
        out,
    );
    filtered.correct_tokens(out);
    filter::base_form(out);
    match options.stop_tags {
        Some(tags) => filter::part_of_speech_stop(out, tags),
        None => filter::part_of_speech_stop(out, &filter::StopTags::defaults()),
    }
    match options.stop_words {
        Some(words) => filter::stop(out, words),
        None => filter::stop(out, &filter::StopWords::japanese()),
    }
    filter::katakana_stem(out, 4);
    filter::lowercase(out);
}

/// Options of the `kuromoji_completion` analyzer (Lucene's `JapaneseCompletionAnalyzer`): width
/// normalization, the tokenizer in [`Mode::Normal`] with punctuation and compounds discarded,
/// then the completion filter and lowercasing.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct CompletionAnalyzerOptions<'a> {
    pub mode: filter::CompletionMode,
    pub user_dictionary: Option<&'a UserDictionary>,
}

/// Runs the `kuromoji_completion` analyzer over `text`, filling `out`.
pub fn analyze_completion(text: &str, options: CompletionAnalyzerOptions<'_>, out: &mut Tokens) {
    let filtered = char_filter::cjk_width(text);
    tokenize(
        filtered.text(),
        Options {
            mode: Mode::Normal,
            discard_punctuation: true,
            discard_compound_token: true,
            nbest_cost: -1,
            user_dictionary: options.user_dictionary,
        },
        out,
    );
    filtered.correct_tokens(out);
    filter::completion(out, options.mode);
    filter::lowercase(out);
}
