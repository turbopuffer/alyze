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
            rust_stemmers::Stemmer::create(algorithm)
        });

        // Monotonic across all inputs. Every word-like token consumes
        // a position, even if a downstream filter (length, stopword) drops it,
        // which is important for phrase-distance accuracy.
        //
        // TODO configurable gap between inputs
        let mut next_position = 0;

        let TokenizerOptions::UAX29Word(tokenizer_opts) = self.options.tokenizer;

        for input in inputs {
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
                if !self.options.case_sensitive {
                    token_text.lowercase_in_place(props.is_ascii());
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
                };
                callback(token)
            });
        }
    }
}

pub struct Token<'a> {
    /// Text of the token, either sliced from the input string or from the reused
    /// buffer. Only valid for the duration of the callback invocation.
    pub text: &'a str,

    /// Position of the token in the sequence of tokens. If `analyze_inputs` is used,
    /// token positions are threaded monotonically across all input strings. Every word-like
    /// token consumes one position, even if filtered out (e.g. by stopword removal, etc).
    pub position: usize,
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
