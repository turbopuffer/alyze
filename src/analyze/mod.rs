use std::ops::Range;

use crate::{
    analyze::stemming_cache::{CachedToken, StemmingCache, StemmingCacheEntry},
    uax29,
};

mod filters;
pub mod stemming_cache;
mod stopwords;
mod u17_to_lower;

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
        true
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TokenizerOptions {
    UAX29Word(uax29::word::Options),
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

/// A language whose Snowball stemming algorithm `alyze` can apply.
///
/// Each variant maps 1:1 onto an algorithm shipped by <https://snowballstem.org>,
/// via the `frostem` crate.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum StemmingLanguage {
    Arabic,
    Armenian,
    Basque,
    Catalan,
    Czech,
    Danish,
    Dutch,
    /// Snowball's pre-2023 Dutch algorithm, kept under its upstream name so
    /// indexes built before [`Self::Dutch`] switched to the current algorithm
    /// can keep their existing terms without a reindex.
    DutchPorter,
    English,
    Esperanto,
    Estonian,
    Finnish,
    French,
    German,
    Greek,
    Hindi,
    Hungarian,
    Indonesian,
    Irish,
    Italian,
    Lithuanian,
    Nepali,
    Norwegian,
    Persian,
    Polish,
    Portuguese,
    Romanian,
    Russian,
    Serbian,
    Sesotho,
    Spanish,
    Swedish,
    Tamil,
    Turkish,
    Yiddish,
}

impl From<StemmingLanguage> for frostem::Algorithm {
    fn from(language: StemmingLanguage) -> Self {
        match language {
            StemmingLanguage::Arabic => frostem::Algorithm::Arabic,
            StemmingLanguage::Armenian => frostem::Algorithm::Armenian,
            StemmingLanguage::Basque => frostem::Algorithm::Basque,
            StemmingLanguage::Catalan => frostem::Algorithm::Catalan,
            StemmingLanguage::Czech => frostem::Algorithm::Czech,
            StemmingLanguage::Danish => frostem::Algorithm::Danish,
            StemmingLanguage::Dutch => frostem::Algorithm::Dutch,
            StemmingLanguage::DutchPorter => frostem::Algorithm::DutchPorter,
            StemmingLanguage::English => frostem::Algorithm::English,
            StemmingLanguage::Esperanto => frostem::Algorithm::Esperanto,
            StemmingLanguage::Estonian => frostem::Algorithm::Estonian,
            StemmingLanguage::Finnish => frostem::Algorithm::Finnish,
            StemmingLanguage::French => frostem::Algorithm::French,
            StemmingLanguage::German => frostem::Algorithm::German,
            StemmingLanguage::Greek => frostem::Algorithm::Greek,
            StemmingLanguage::Hindi => frostem::Algorithm::Hindi,
            StemmingLanguage::Hungarian => frostem::Algorithm::Hungarian,
            StemmingLanguage::Indonesian => frostem::Algorithm::Indonesian,
            StemmingLanguage::Irish => frostem::Algorithm::Irish,
            StemmingLanguage::Italian => frostem::Algorithm::Italian,
            StemmingLanguage::Lithuanian => frostem::Algorithm::Lithuanian,
            StemmingLanguage::Nepali => frostem::Algorithm::Nepali,
            StemmingLanguage::Norwegian => frostem::Algorithm::Norwegian,
            StemmingLanguage::Persian => frostem::Algorithm::Persian,
            StemmingLanguage::Polish => frostem::Algorithm::Polish,
            StemmingLanguage::Portuguese => frostem::Algorithm::Portuguese,
            StemmingLanguage::Romanian => frostem::Algorithm::Romanian,
            StemmingLanguage::Russian => frostem::Algorithm::Russian,
            StemmingLanguage::Serbian => frostem::Algorithm::Serbian,
            StemmingLanguage::Sesotho => frostem::Algorithm::Sesotho,
            StemmingLanguage::Spanish => frostem::Algorithm::Spanish,
            StemmingLanguage::Swedish => frostem::Algorithm::Swedish,
            StemmingLanguage::Tamil => frostem::Algorithm::Tamil,
            StemmingLanguage::Turkish => frostem::Algorithm::Turkish,
            StemmingLanguage::Yiddish => frostem::Algorithm::Yiddish,
        }
    }
}

/// A buffer that should be reused across multiple analyze() invocations
/// to avoid unnecessary allocations. Contents are opaque and internal to
/// the implementation.
#[derive(Debug, Clone)]
pub struct ReusableBuffer {
    a: String,
    b: String,
    stemming_cache: StemmingCache,
}

impl ReusableBuffer {
    pub fn new() -> Self {
        Self {
            a: String::new(),
            b: String::new(),
            stemming_cache: StemmingCache::new_with_capacity(32_000),
        }
    }

    pub fn stemming_cache(&mut self) -> &mut StemmingCache {
        &mut self.stemming_cache
    }

    pub fn reset_keep_stemming_cache(&mut self) {
        self.a.clear();
        self.b.clear();
    }
}

#[derive(Clone, Copy)]
pub struct Analyzer {
    options: AnalysisOptions,
}

impl Analyzer {
    pub fn new(options: AnalysisOptions) -> Self {
        assert!(options.valid(), "options are invalid");
        Self { options }
    }

    /// Analyzes a single input string, invoking the callback for each token.
    /// Returning false from the callback will stop analysis early.
    pub fn analyze<'a>(
        &self,
        input: &'a str,
        buffer: &mut ReusableBuffer,
        callback: impl FnMut(Token<'_>) -> bool,
    ) {
        self.analyze_inputs(std::iter::once(input), buffer, callback);
    }

    /// Analyzes a sequence of input strings, invoking the callback for each token.
    /// Returning false from the callback will stop analysis early.
    pub fn analyze_inputs<'a>(
        &self,
        inputs: impl Iterator<Item = &'a str>,
        buffer: &mut ReusableBuffer,
        mut callback: impl FnMut(Token<'_>) -> bool,
    ) {
        let ReusableBuffer {
            a: buffer_a,
            b: buffer_b,
            stemming_cache,
        } = buffer;

        let stemmer = self.options.stemming.map(|stemming_language| {
            let algorithm = stemming_language.into();
            frostem::Stemmer::new(algorithm)
        });

        // Monotonic across all inputs. Every word-like token consumes
        // a position, even if a downstream filter (length, stopword) drops it,
        // which is important for phrase-distance accuracy.
        //
        // TODO configurable gap between inputs
        let mut next_position = 0;

        let TokenizerOptions::UAX29Word(tokenizer_opts) = self.options.tokenizer;

        for (input_index, input) in inputs.enumerate() {
            let mut prev = None;
            let input_as_bytes = input.as_bytes();
            uax29::word::tokenize(input, tokenizer_opts, |bp, props| {
                let Some(prev) = std::mem::replace(&mut prev, Some(bp)) else {
                    return true; // don't emit token on first breakpoint
                };
                if !props.is_word_like() {
                    return true; // skip non-word tokens
                }

                // Advance position after each word-like token.
                let position = next_position;
                next_position += 1;

                // SAFETY: tokenize guarentees that breakpoints are on valid UTF-8 boundaries,
                // thus slicing input by the breakpoint will always produce valid UTF-8.
                buffer_a.clear();
                let mut token_text = InputRefOrBuffered::InputRef {
                    input: unsafe { std::str::from_utf8_unchecked(&input_as_bytes[prev..bp]) },
                    buffer_if_needed: buffer_a,
                };

                // Token length
                if let Some(max_token_length) = self.options.maximum_token_length
                    && !filters::within_token_length_limit(token_text.as_str(), max_token_length)
                {
                    return true;
                }

                // Lowercasing
                if !self.options.case_sensitive && (!props.is_ascii() || props.has_ascii_upper()) {
                    token_text.lowercase_in_place(props.is_ascii());
                } else if !self.options.case_sensitive {
                    // Skipped because props said all-lowercase ASCII; verify that holds.
                    debug_assert!(
                        !token_text.as_str().bytes().any(|b| b.is_ascii_uppercase()),
                        "has_ascii_upper was false but token contains uppercase ASCII"
                    );
                }

                // Stopword removal
                if let Some(StopwordRemoval::ForLanguage(language)) = self.options.stopword_removal
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
                if self.options.ascii_folding && !props.is_ascii() {
                    token_text.ascii_fold_in_place(buffer_b);

                    // ASCII folding can produce uppercase ASCII characters,
                    // so we'll lowercase again if case folding is enabled.
                    if !self.options.case_sensitive {
                        let is_ascii = token_text.as_str().is_ascii();
                        token_text.lowercase_in_place(is_ascii);
                    }
                }

                let token = Token {
                    text: token_text.as_str(),
                    position,
                    byte_range: prev..bp,
                    input_index,
                };
                callback(token)
            });
        }
    }
}

#[non_exhaustive]
pub struct Token<'a> {
    /// Normalized text of the token, either sliced from the input string or from the reused
    /// buffer. Only valid for the duration of the callback invocation.
    pub text: &'a str,

    /// Position of the token in the sequence of tokens. If `analyze_inputs` is used,
    /// token positions are threaded monotonically across all input strings. Every word-like
    /// token consumes one position, even if filtered out (e.g. by stopword removal, etc).
    pub position: usize,

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
        stemmer: &frostem::Stemmer,
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

    fn opts() -> AnalysisOptions {
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

    /// Every `StemmingLanguage`, in declaration order. Adding a variant already
    /// breaks the build at `From<StemmingLanguage> for frostem::Algorithm`;
    /// extend this array and `expected` below at the same time.
    const ALL_STEMMING_LANGUAGES: [StemmingLanguage; 35] = [
        StemmingLanguage::Arabic,
        StemmingLanguage::Armenian,
        StemmingLanguage::Basque,
        StemmingLanguage::Catalan,
        StemmingLanguage::Czech,
        StemmingLanguage::Danish,
        StemmingLanguage::Dutch,
        StemmingLanguage::DutchPorter,
        StemmingLanguage::English,
        StemmingLanguage::Esperanto,
        StemmingLanguage::Estonian,
        StemmingLanguage::Finnish,
        StemmingLanguage::French,
        StemmingLanguage::German,
        StemmingLanguage::Greek,
        StemmingLanguage::Hindi,
        StemmingLanguage::Hungarian,
        StemmingLanguage::Indonesian,
        StemmingLanguage::Irish,
        StemmingLanguage::Italian,
        StemmingLanguage::Lithuanian,
        StemmingLanguage::Nepali,
        StemmingLanguage::Norwegian,
        StemmingLanguage::Persian,
        StemmingLanguage::Polish,
        StemmingLanguage::Portuguese,
        StemmingLanguage::Romanian,
        StemmingLanguage::Russian,
        StemmingLanguage::Serbian,
        StemmingLanguage::Sesotho,
        StemmingLanguage::Spanish,
        StemmingLanguage::Swedish,
        StemmingLanguage::Tamil,
        StemmingLanguage::Turkish,
        StemmingLanguage::Yiddish,
    ];

    fn stem_one(language: StemmingLanguage, input: &str) -> String {
        let mut o = opts();
        o.stemming = Some(language);
        let tokens = collect(o, input);
        assert_eq!(tokens.len(), 1, "{language:?} did not produce one token");
        tokens.into_iter().next().unwrap().text
    }

    /// One word per language, with its stem read off that algorithm's
    /// `voc.txt`/`output.txt` pair in
    /// <https://github.com/snowballstem/snowball-data>. Every word is one the
    /// algorithm actually rewrites, so a variant wired to the wrong
    /// `frostem::Algorithm` fails here unless the two algorithms happen to agree
    /// on that word — which several Latin-script pairs do, hence one vector per
    /// language rather than a shared list.
    #[test]
    fn each_language_applies_its_own_snowball_algorithm() {
        let expected: [(StemmingLanguage, &str, &str); 35] = [
            (StemmingLanguage::Arabic, "الكتاب", "كتاب"),
            (StemmingLanguage::Armenian, "աբբայության", "աբբայ"),
            (StemmingLanguage::Basque, "etxeetan", "etxe"),
            (StemmingLanguage::Catalan, "professora", "profes"),
            (StemmingLanguage::Czech, "krásného", "krásn"),
            (StemmingLanguage::Danish, "bueskytten", "bueskyt"),
            (StemmingLanguage::Dutch, "aalmoezen", "aalmoes"),
            (StemmingLanguage::DutchPorter, "aalmoezen", "aalmoez"),
            (StemmingLanguage::English, "fruitlessly", "fruitless"),
            (StemmingLanguage::Esperanto, "libroj", "libr"),
            (StemmingLanguage::Estonian, "raamatutest", "raama"),
            (StemmingLanguage::Finnish, "voimakkaasti", "voimak"),
            (StemmingLanguage::French, "continuellement", "continuel"),
            (StemmingLanguage::German, "abgeleitete", "abgeleit"),
            (StemmingLanguage::Greek, "έκπληκτα", "εκπληκτ"),
            (StemmingLanguage::Hindi, "लड़कियों", "लड़क"),
            (StemmingLanguage::Hungarian, "alkotmányt", "alkotmány"),
            (StemmingLanguage::Indonesian, "membaca", "baca"),
            (StemmingLanguage::Irish, "abairteach", "abairt"),
            (StemmingLanguage::Italian, "abbandonata", "abbandon"),
            (StemmingLanguage::Lithuanian, "knygomis", "knyg"),
            (StemmingLanguage::Nepali, "दिछिन्", "दि"),
            (StemmingLanguage::Norwegian, "havnebyen", "havneby"),
            (StemmingLanguage::Persian, "کتابها", "کتاب"),
            (StemmingLanguage::Polish, "cieszyć", "cieszyc"),
            (StemmingLanguage::Portuguese, "apontando", "apont"),
            (StemmingLanguage::Romanian, "abandonați", "abandon"),
            (StemmingLanguage::Russian, "красивый", "красив"),
            (StemmingLanguage::Serbian, "beležnika", "beležnik"),
            (StemmingLanguage::Sesotho, "dibuka", "dibuk"),
            (StemmingLanguage::Spanish, "abandonada", "abandon"),
            (StemmingLanguage::Swedish, "aftonbladet", "aftonblad"),
            (StemmingLanguage::Tamil, "அகப்பேய்ச்", "அகப்பேய்"),
            (StemmingLanguage::Turkish, "almasının", "alma"),
            (StemmingLanguage::Yiddish, "ביכער", "ביכ"),
        ];
        let mut seen = std::collections::HashSet::new();
        for (i, language) in ALL_STEMMING_LANGUAGES.into_iter().enumerate() {
            assert!(seen.insert(language), "{language:?} listed twice");
            let (covered, input, want) = expected[i];
            assert_eq!(covered, language, "expected[] is out of order");
            assert_eq!(stem_one(language, input), want, "{language:?}");
        }
    }

    /// Snowball replaced its Dutch algorithm; the pre-2023 one lives on
    /// upstream as `dutch_porter`. The two disagree on roughly half of
    /// Snowball's own Dutch vocabulary, so they must stay distinct variants.
    #[test]
    fn dutch_and_dutch_porter_differ() {
        assert_ne!(
            stem_one(StemmingLanguage::Dutch, "aalmoezen"),
            stem_one(StemmingLanguage::DutchPorter, "aalmoezen"),
        );
    }
}
