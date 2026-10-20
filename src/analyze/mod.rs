use std::ops::Range;

use tpuf_icu_properties_211::{CodePointSetData, props::WhiteSpace};

#[cfg(feature = "kuromoji")]
use crate::cjk::kuromoji;
#[cfg(feature = "nori")]
use crate::cjk::nori;
#[cfg(feature = "smartcn")]
use crate::cjk::smartcn;

use crate::{
    analyze::stemming_cache::{CachedToken, StemmingCache, StemmingCacheEntry},
    uax29,
};

mod filters;
pub mod stemming_cache;
mod stopwords;
pub(crate) mod u17_to_lower;

#[cfg(test)]
mod pipeline_tests;

#[derive(Clone, Copy, Debug)]
pub struct AnalysisOptions {
    pub tokenizer: TokenizerOptions,

    // Note: These are ordered in the sequence they are applied in
    pub maximum_token_length: Option<usize>,
    pub case_sensitive: bool,
    pub stopword_removal: Option<StopwordRemoval>,
    pub stemming: Option<StemmingLanguage>,
    pub ascii_folding: bool,
}

impl AnalysisOptions {
    pub fn valid(&self) -> bool {
        if self.stemming.is_some() && self.case_sensitive {
            return false; // stemming requires case insensitivity
        }
        if self.stopword_removal.is_some() && self.case_sensitive {
            return false; // stopword removal requires case insensitivity
        }
        if matches!(self.tokenizer, TokenizerOptions::Whitespace)
            && (self.stemming.is_some() || self.stopword_removal.is_some())
        {
            return false;
        }
        true
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TokenizerOptions {
    UAX29Word(uax29::word::Options),
    Whitespace,
}

#[derive(Copy, Clone, Debug)]
pub enum StopwordRemoval {
    ForLanguage(LanguageWithStopwords),
}

#[derive(Copy, Clone, Debug)]
pub enum LanguageWithStopwords {
    Danish,
    Dutch,
    English,
    Finnish,
    French,
    German,
    Hungarian,
    Italian,
    Norwegian,
    Portuguese,
    Russian,
    Spanish,
    Swedish,
}

#[derive(Copy, Clone, Debug)]
pub enum StemmingLanguage {
    Arabic,
    Danish,
    Dutch,
    English,
    Finnish,
    French,
    German,
    Greek,
    Hungarian,
    Italian,
    Norwegian,
    Portuguese,
    Romanian,
    Russian,
    Spanish,
    Swedish,
    Tamil,
    Turkish,
}

impl Into<rust_stemmers::Algorithm> for StemmingLanguage {
    fn into(self) -> rust_stemmers::Algorithm {
        match self {
            StemmingLanguage::Arabic => rust_stemmers::Algorithm::Arabic,
            StemmingLanguage::Danish => rust_stemmers::Algorithm::Danish,
            StemmingLanguage::Dutch => rust_stemmers::Algorithm::Dutch,
            StemmingLanguage::English => rust_stemmers::Algorithm::English,
            StemmingLanguage::Finnish => rust_stemmers::Algorithm::Finnish,
            StemmingLanguage::French => rust_stemmers::Algorithm::French,
            StemmingLanguage::German => rust_stemmers::Algorithm::German,
            StemmingLanguage::Greek => rust_stemmers::Algorithm::Greek,
            StemmingLanguage::Hungarian => rust_stemmers::Algorithm::Hungarian,
            StemmingLanguage::Italian => rust_stemmers::Algorithm::Italian,
            StemmingLanguage::Norwegian => rust_stemmers::Algorithm::Norwegian,
            StemmingLanguage::Portuguese => rust_stemmers::Algorithm::Portuguese,
            StemmingLanguage::Romanian => rust_stemmers::Algorithm::Romanian,
            StemmingLanguage::Russian => rust_stemmers::Algorithm::Russian,
            StemmingLanguage::Spanish => rust_stemmers::Algorithm::Spanish,
            StemmingLanguage::Swedish => rust_stemmers::Algorithm::Swedish,
            StemmingLanguage::Tamil => rust_stemmers::Algorithm::Tamil,
            StemmingLanguage::Turkish => rust_stemmers::Algorithm::Turkish,
        }
    }
}

/// A buffer that should be reused across multiple analyze() invocations
/// to avoid unnecessary allocations. Contents are opaque and internal to
/// the implementation.
#[derive(Debug)]
pub struct ReusableBuffer {
    a: String,
    b: String,
    stemming_cache: StemmingCache,
    #[cfg(feature = "nori")]
    nori: Option<Box<nori::Tokens>>,
    #[cfg(feature = "kuromoji")]
    kuromoji: Option<Box<kuromoji::Tokens>>,
}

impl ReusableBuffer {
    pub fn new() -> Self {
        Self {
            a: String::new(),
            b: String::new(),
            stemming_cache: StemmingCache::new_with_capacity(32_000),
            #[cfg(feature = "nori")]
            nori: None,
            #[cfg(feature = "kuromoji")]
            kuromoji: None,
        }
    }

    pub fn stemming_cache(&mut self) -> &mut StemmingCache {
        &mut self.stemming_cache
    }

    pub fn reset_keep_stemming_cache(&mut self) {
        self.a.clear();
        self.b.clear();
        #[cfg(feature = "nori")]
        if let Some(tokens) = &mut self.nori {
            tokens.clear();
        }
        #[cfg(feature = "kuromoji")]
        if let Some(tokens) = &mut self.kuromoji {
            tokens.clear();
        }
    }
}

impl Clone for ReusableBuffer {
    fn clone(&self) -> Self {
        Self {
            a: self.a.clone(),
            b: self.b.clone(),
            stemming_cache: self.stemming_cache.clone(),
            #[cfg(feature = "nori")]
            nori: None,
            #[cfg(feature = "kuromoji")]
            kuromoji: None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Analyzer<'a> {
    pipeline: Pipeline<'a>,
}

#[derive(Clone, Copy)]
enum Pipeline<'a> {
    Standard(AnalysisOptions, std::marker::PhantomData<&'a ()>),
    #[cfg(feature = "smartcn")]
    SmartCn(smartcn::Options),
    #[cfg(feature = "nori")]
    Nori(nori::AnalyzerOptions<'a>),
    #[cfg(feature = "kuromoji")]
    Kuromoji(kuromoji::AnalyzerOptions<'a>),
}

impl<'a> Analyzer<'a> {
    pub fn new(options: AnalysisOptions) -> Self {
        // TODO: Return descriptive errors for invalid options instead of panicking.
        assert!(options.valid(), "options are invalid");
        Self {
            pipeline: Pipeline::Standard(options, std::marker::PhantomData),
        }
    }

    #[cfg(feature = "smartcn")]
    pub fn smartcn(options: smartcn::Options) -> Self {
        Self {
            pipeline: Pipeline::SmartCn(options),
        }
    }

    #[cfg(feature = "nori")]
    pub fn nori(options: nori::AnalyzerOptions<'a>) -> Self {
        Self {
            pipeline: Pipeline::Nori(options),
        }
    }

    #[cfg(feature = "kuromoji")]
    pub fn kuromoji(options: kuromoji::AnalyzerOptions<'a>) -> Self {
        Self {
            pipeline: Pipeline::Kuromoji(options),
        }
    }

    /// Analyzes a single input string, invoking the callback for each token.
    /// Returning false from the callback will stop analysis early.
    pub fn analyze(
        &self,
        input: &str,
        buffer: &mut ReusableBuffer,
        callback: impl FnMut(Token<'_>) -> bool,
    ) {
        self.analyze_inputs(std::iter::once(input), buffer, callback);
    }

    /// Analyzes a sequence of input strings, invoking the callback for each token.
    /// Returning false from the callback will stop analysis early.
    pub fn analyze_inputs<'input>(
        &self,
        inputs: impl Iterator<Item = &'input str>,
        buffer: &mut ReusableBuffer,
        callback: impl FnMut(Token<'_>) -> bool,
    ) {
        match self.pipeline {
            Pipeline::Standard(options, _) => {
                Self::analyze_standard(options, inputs, buffer, callback);
            }
            #[cfg(any(feature = "smartcn", feature = "nori", feature = "kuromoji"))]
            pipeline => {
                let mut callback = callback;
                let mut next_position = 0;
                for (input_index, input) in inputs.enumerate() {
                    let base = next_position;
                    let mut keep_going = true;
                    let mut emit = |mut token: Token<'_>| {
                        token.position += base;
                        token.input_index = input_index;
                        next_position = next_position.max(token.position + token.position_length);
                        keep_going = callback(token);
                        keep_going
                    };
                    match pipeline {
                        #[cfg(feature = "smartcn")]
                        Pipeline::SmartCn(options) => {
                            smartcn::analyze(input, options, &mut buffer.a, |token| {
                                emit(Token {
                                    text: token.text,
                                    position: token.position,
                                    position_length: 1,
                                    byte_range: token.byte_range,
                                    input_index,
                                })
                            });
                        }
                        #[cfg(feature = "nori")]
                        Pipeline::Nori(options) => {
                            let tokens = buffer.nori.get_or_insert_with(Default::default);
                            nori::analyze(input, options, tokens);
                            emit_morph_tokens(
                                tokens.iter().map(|token| {
                                    (
                                        token.text,
                                        token.byte_range,
                                        token.position_increment,
                                        token.position_length,
                                    )
                                }),
                                &mut emit,
                            );
                        }
                        #[cfg(feature = "kuromoji")]
                        Pipeline::Kuromoji(options) => {
                            let tokens = buffer.kuromoji.get_or_insert_with(Default::default);
                            kuromoji::analyze(input, options, tokens);
                            emit_morph_tokens(
                                tokens.iter().map(|token| {
                                    (
                                        token.text,
                                        token.byte_range,
                                        token.position_increment,
                                        token.position_length,
                                    )
                                }),
                                &mut emit,
                            );
                        }
                        Pipeline::Standard(..) => unreachable!(),
                    }
                    if !keep_going {
                        return;
                    }
                }
            }
        }
    }

    fn analyze_standard<'input>(
        options: AnalysisOptions,
        inputs: impl Iterator<Item = &'input str>,
        buffer: &mut ReusableBuffer,
        mut callback: impl FnMut(Token<'_>) -> bool,
    ) {
        let ReusableBuffer {
            a: buffer_a,
            b: buffer_b,
            stemming_cache,
            ..
        } = buffer;

        let stemmer = options.stemming.map(|stemming_language| {
            let algorithm = stemming_language.into();
            rust_stemmers::Stemmer::create(algorithm)
        });

        // Monotonic across all inputs. Every word-like token consumes
        // a position, even if a downstream filter (length, stopword) drops it,
        // which is important for phrase-distance accuracy.
        //
        // TODO configurable gap between inputs
        let mut next_position = 0;

        let mut keep_going = true;
        for (input_index, input) in inputs.enumerate() {
            let input_as_bytes = input.as_bytes();
            let mut emit = |byte_range: Range<usize>, is_ascii: bool, has_ascii_upper: bool| {
                let position = next_position;
                next_position += 1;

                buffer_a.clear();
                let mut token_text = InputRefOrBuffered::InputRef {
                    input: unsafe {
                        std::str::from_utf8_unchecked(&input_as_bytes[byte_range.clone()])
                    },
                    buffer_if_needed: buffer_a,
                };

                // Token length
                if let Some(max_token_length) = options.maximum_token_length
                    && !filters::within_token_length_limit(token_text.as_str(), max_token_length)
                {
                    return true;
                }

                // Lowercasing
                if !options.case_sensitive && (!is_ascii || has_ascii_upper) {
                    token_text.lowercase_in_place(is_ascii);
                } else if !options.case_sensitive {
                    // Skipped because props said all-lowercase ASCII; verify that holds.
                    debug_assert!(
                        !token_text.as_str().bytes().any(|b| b.is_ascii_uppercase()),
                        "has_ascii_upper was false but token contains uppercase ASCII"
                    );
                }

                // Stopword removal
                if let Some(StopwordRemoval::ForLanguage(language)) = options.stopword_removal
                    && filters::is_stopword_in_language(language, token_text.as_str())
                {
                    return true;
                }

                // Stemming
                if let Some(stemmer) = &stemmer {
                    token_text.stem_in_place(stemmer, stemming_cache, buffer_b);
                }

                // ASCII folding
                // Note: Not needed if token is already ASCII
                if options.ascii_folding && !is_ascii {
                    token_text.ascii_fold_in_place(buffer_b);

                    // ASCII folding can produce uppercase ASCII characters,
                    // so we'll lowercase again if case folding is enabled.
                    if !options.case_sensitive {
                        let is_ascii = token_text.as_str().is_ascii();
                        token_text.lowercase_in_place(is_ascii);
                    }
                }

                let token = Token {
                    text: token_text.as_str(),
                    position,
                    position_length: 1,
                    byte_range,
                    input_index,
                };
                keep_going = callback(token);
                keep_going
            };
            match options.tokenizer {
                TokenizerOptions::UAX29Word(tokenizer_opts) => {
                    let mut prev = None;
                    uax29::word::tokenize(input, tokenizer_opts, |bp, props| {
                        let Some(start) = prev.replace(bp) else {
                            return true;
                        };
                        !props.is_word_like()
                            || emit(start..bp, props.is_ascii(), props.has_ascii_upper())
                    });
                }
                TokenizerOptions::Whitespace => {
                    // Use versioned Unicode data so Rust upgrades cannot change token boundaries.
                    let whitespace = CodePointSetData::new::<WhiteSpace>();
                    for text in input
                        .split(|c| whitespace.contains(c))
                        .filter(|text| !text.is_empty())
                    {
                        let start = text.as_ptr() as usize - input.as_ptr() as usize;
                        if !emit(
                            start..start + text.len(),
                            text.is_ascii(),
                            text.bytes().any(|b| b.is_ascii_uppercase()),
                        ) {
                            break;
                        }
                    }
                }
            }
            if !keep_going {
                return;
            }
        }
    }
}

#[cfg(any(feature = "nori", feature = "kuromoji"))]
fn emit_morph_tokens<'a>(
    tokens: impl Iterator<Item = (&'a str, Range<usize>, u32, u32)>,
    mut callback: impl FnMut(Token<'_>) -> bool,
) {
    let mut position = 0;
    for (text, byte_range, increment, length) in tokens {
        position += increment as usize;
        if !callback(Token {
            text,
            position: position - 1,
            position_length: length as usize,
            byte_range,
            input_index: 0,
        }) {
            return;
        }
    }
}

#[non_exhaustive]
pub struct Token<'a> {
    /// Normalized text of the token, either sliced from the input string or from the reused
    /// buffer. Only valid for the duration of the callback invocation.
    pub text: &'a str,

    /// Position of the token in the sequence of tokens. If `analyze_inputs` is used,
    /// token positions are threaded monotonically across all input strings. Overlapping tokens
    /// can share a position; removed tokens can leave gaps.
    pub position: usize,

    /// Number of positions spanned by this token: usually 1, but compounds can span more.
    pub position_length: usize,

    /// Byte range of the token's raw substring within its input (not into `text`, which may be
    /// normalized). Recover with `&inputs[input_index][byte_range]`. Always on UTF-8 boundaries.
    pub byte_range: Range<usize>,

    /// Index of the input (in the `analyze_inputs` iterator) that `byte_range` refers to.
    /// Always 0 for `analyze`, which takes a single input.
    pub input_index: usize,
}

enum InputRefOrBuffered<'input, 'buf> {
    InputRef {
        input: &'input str,
        buffer_if_needed: &'buf mut String,
    },
    Buffered(&'buf mut String),
}

impl InputRefOrBuffered<'_, '_> {
    fn as_str(&self) -> &str {
        match self {
            Self::InputRef { input, .. } => input,
            Self::Buffered(s) => s.as_str(),
        }
    }

    fn lowercase_in_place(&mut self, is_ascii: bool) {
        debug_assert_eq!(
            is_ascii,
            self.as_str().is_ascii(),
            "caller must ensure is_ascii is correct"
        );

        if is_ascii && self.as_str().bytes().all(|b| !b.is_ascii_uppercase()) {
            return;
        }

        if let Self::InputRef {
            input,
            buffer_if_needed,
        } = self
        {
            debug_assert!(
                buffer_if_needed.is_empty(),
                "buffer must be empty when passed in for potential reuse"
            );
            buffer_if_needed.push_str(input);
            self.transition_to_buffered();
        }

        let Self::Buffered(s) = self else {
            unreachable!()
        };
        if is_ascii {
            s.make_ascii_lowercase();
        } else {
            filters::lowercase_chars_in_place(s);
        }
    }

    fn ascii_fold_in_place(&mut self, scratch: &mut String) {
        match self {
            Self::InputRef {
                input,
                buffer_if_needed,
            } => {
                debug_assert!(
                    buffer_if_needed.is_empty(),
                    "buffer must be empty when passed in for potential reuse"
                );
                filters::ascii_fold(input, buffer_if_needed);
                self.transition_to_buffered();
            }
            Self::Buffered(s) => {
                debug_assert!(
                    scratch.is_empty(),
                    "scratch buffer must be empty when passed in for potential reuse"
                );
                filters::ascii_fold(s, scratch);
                std::mem::swap(*s, scratch);
                scratch.clear();
            }
        }
    }

    fn stem_in_place(
        &mut self,
        stemmer: &rust_stemmers::Stemmer,
        cache: &mut StemmingCache,
        scratch: &mut String,
    ) {
        let token_str = self.as_str();

        let cache_key = CachedToken::new_from_str(token_str);
        if let Some(cache_key) = cache_key.as_ref()
            && let Some(entry) = cache.lookup(cache_key)
        {
            match entry {
                StemmingCacheEntry::Stemmed(s) => match self {
                    Self::InputRef {
                        buffer_if_needed, ..
                    } => {
                        debug_assert!(
                            buffer_if_needed.is_empty(),
                            "buffer must be empty when passed in for potential reuse"
                        );
                        buffer_if_needed.push_str(s.as_str());
                        self.transition_to_buffered();
                    }
                    Self::Buffered(buf) => {
                        buf.clear();
                        buf.push_str(s.as_str());
                    }
                },
                StemmingCacheEntry::Unchanged => {}
            }
            return;
        }

        let cached_value_to_insert = match self {
            Self::InputRef {
                input,
                buffer_if_needed,
            } => {
                let stemmed = stemmer.stem(input);
                if stemmed == *input {
                    Some(StemmingCacheEntry::Unchanged)
                } else {
                    debug_assert!(
                        buffer_if_needed.is_empty(),
                        "buffer must be empty when passed in for potential reuse"
                    );
                    buffer_if_needed.push_str(&stemmed);
                    self.transition_to_buffered();
                    CachedToken::new_from_str(&stemmed).map(StemmingCacheEntry::Stemmed)
                }
            }
            Self::Buffered(s) => {
                let stemmed = stemmer.stem(s.as_str());
                if stemmed == s.as_str() {
                    Some(StemmingCacheEntry::Unchanged)
                } else {
                    debug_assert!(
                        scratch.is_empty(),
                        "scratch buffer must be empty when passed in for potential reuse"
                    );
                    scratch.push_str(&stemmed);
                    std::mem::swap(*s, scratch);
                    scratch.clear(); // cleanup for caller's next use
                    CachedToken::new_from_str(s.as_str()).map(StemmingCacheEntry::Stemmed)
                }
            }
        };

        if let Some(cache_key) = cache_key
            && let Some(cache_value) = cached_value_to_insert
            && cache.has_remaining_capacity()
        {
            cache.insert_no_clobber_assume_capacity(cache_key, cache_value);
        }
    }

    // Mutates self to transition from `InputRef` to `Buffered`. Caller is responsible
    // for populating `buffer_if_needed` with the appropriate contents before calling this.
    fn transition_to_buffered(&mut self) {
        // SAFETY: `InputRef` holds only `&mut` references (no owned data), so
        // dropping its bytes via overwrite is a no-op. We `ptr::read` self,
        // consume it to construct the new variant, then `ptr::write` back —
        // `*self` is never observed in an uninitialized state, and no value
        // is dropped twice.
        unsafe {
            let new = match std::ptr::read(self) {
                Self::InputRef {
                    buffer_if_needed, ..
                } => Self::Buffered(buffer_if_needed),
                Self::Buffered(_) => unreachable!(),
            };
            std::ptr::write(self, new);
        }
    }
}

// TODO this has extensive coverage in the turbopuffer repo, but not in the crate itself
// move some of the test suite in here

#[cfg(test)]
mod tests {
    use super::*;

    /// Owned copy of a `Token`'s fields, so tests can use named access.
    struct Tok {
        text: String,
        position: usize,
        byte_range: Range<usize>,
        input_index: usize,
    }

    fn collect_inputs<'a>(
        opts: AnalysisOptions,
        inputs: impl Iterator<Item = &'a str>,
    ) -> Vec<Tok> {
        let mut out = Vec::new();
        Analyzer::new(opts).analyze_inputs(inputs, &mut ReusableBuffer::new(), |t| {
            out.push(Tok {
                text: t.text.to_string(),
                position: t.position,
                byte_range: t.byte_range,
                input_index: t.input_index,
            });
            true
        });
        out
    }

    fn collect(opts: AnalysisOptions, input: &str) -> Vec<Tok> {
        collect_inputs(opts, std::iter::once(input))
    }

    pub(super) fn opts() -> AnalysisOptions {
        AnalysisOptions {
            tokenizer: TokenizerOptions::UAX29Word(Default::default()),
            maximum_token_length: None,
            case_sensitive: false,
            stopword_removal: None,
            stemming: None,
            ascii_folding: false,
        }
    }

    #[test]
    fn byte_range_recovers_raw_substring_when_normalized() {
        let input = "Hello WORLD";
        let tokens = collect(opts(), input);
        assert_eq!(tokens[0].text, "hello"); // normalized text is lowercased
        assert_eq!(&input[tokens[0].byte_range.clone()], "Hello"); // raw slice preserved
        assert_eq!(&input[tokens[1].byte_range.clone()], "WORLD");
    }

    #[test]
    fn byte_range_recovers_raw_when_lowercasing_changes_char() {
        // Greek capital sigma lowercases to a different code point (Σ → σ/ς);
        // byte_range still recovers the original capitals.
        let input = "ΣΟΦΟΣ";
        let tokens = collect(opts(), input);
        assert_eq!(tokens[0].text, "σοφοσ");
        assert_eq!(&input[tokens[0].byte_range.clone()], "ΣΟΦΟΣ");
    }

    #[test]
    fn byte_range_recovers_raw_when_ascii_folding_shrinks_bytes() {
        // "café" (5 bytes) folds to "cafe" (4 bytes); byte_range indexes the
        // source, not the shorter normalized text.
        let mut o = opts();
        o.ascii_folding = true;
        let input = "café";
        let tokens = collect(o, input);
        assert_eq!(tokens[0].text, "cafe");
        assert_eq!(&input[tokens[0].byte_range.clone()], "café");
    }

    #[test]
    fn byte_range_recovers_raw_when_stemming_shrinks_bytes() {
        let mut o = opts();
        o.stemming = Some(StemmingLanguage::English);
        let input = "running";
        let tokens = collect(o, input);
        assert_eq!(tokens[0].text, "run");
        assert_eq!(&input[tokens[0].byte_range.clone()], "running");
    }

    #[test]
    fn input_index_and_byte_range_across_multiple_inputs() {
        let inputs = ["Hello world", "Foo"];
        let tokens = collect_inputs(opts(), inputs.iter().copied());
        // byte_range is relative to each token's own input; input_index identifies it.
        assert_eq!(tokens[0].input_index, 0);
        assert_eq!(
            &inputs[tokens[0].input_index][tokens[0].byte_range.clone()],
            "Hello"
        );
        assert_eq!(tokens[1].input_index, 0);
        assert_eq!(
            &inputs[tokens[1].input_index][tokens[1].byte_range.clone()],
            "world"
        );
        assert_eq!(tokens[2].input_index, 1);
        assert_eq!(
            &inputs[tokens[2].input_index][tokens[2].byte_range.clone()],
            "Foo"
        );
        // Positions stay monotonic across inputs.
        assert_eq!(
            [tokens[0].position, tokens[1].position, tokens[2].position],
            [0, 1, 2]
        );
    }

    #[test]
    fn byte_range_correct_after_filtering() {
        let mut o = opts();
        o.stopword_removal = Some(StopwordRemoval::ForLanguage(LanguageWithStopwords::English));
        let input = "the Quick fox";
        let tokens = collect(o, input);
        // "the" is dropped but still consumes position 0.
        assert_eq!(tokens[0].position, 1);
        assert_eq!(&input[tokens[0].byte_range.clone()], "Quick");
        assert_eq!(&input[tokens[1].byte_range.clone()], "fox");
    }
}
