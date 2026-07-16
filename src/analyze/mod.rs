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

impl From<StemmingLanguage> for rust_stemmers::Algorithm {
    fn from(language: StemmingLanguage) -> Self {
        match language {
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

impl Default for ReusableBuffer {
    fn default() -> Self {
        Self::new()
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
    #[inline(always)]
    pub fn analyze(
        &self,
        input: &str,
        buffer: &mut ReusableBuffer,
        mut callback: impl FnMut(Token<'_>) -> bool,
    ) {
        let mut stream = self.token_stream(input, buffer);
        while let Some(token) = stream.next_token() {
            if !callback(token) {
                break;
            }
        }
    }

    /// Analyzes a sequence of input strings, invoking the callback for each token.
    /// Returning false from the callback will stop analysis early.
    #[inline(always)]
    pub fn analyze_inputs<'input>(
        &self,
        inputs: impl IntoIterator<Item = &'input str>,
        buffer: &mut ReusableBuffer,
        mut callback: impl FnMut(Token<'_>) -> bool,
    ) {
        let mut stream = self.token_stream_inputs(inputs, buffer);
        while let Some(token) = stream.next_token() {
            if !callback(token) {
                break;
            }
        }
    }

    /// Lazily analyzes a single input string.
    #[inline(always)]
    pub fn token_stream<'input, 'buffer>(
        &self,
        input: &'input str,
        buffer: &'buffer mut ReusableBuffer,
    ) -> TokenStream<'input, 'buffer, std::iter::Once<&'input str>> {
        self.token_stream_inputs(std::iter::once(input), buffer)
    }

    /// Lazily analyzes a sequence of input strings.
    #[inline(always)]
    pub fn token_stream_inputs<'input, 'buffer, Inputs>(
        &self,
        inputs: Inputs,
        buffer: &'buffer mut ReusableBuffer,
    ) -> TokenStream<'input, 'buffer, Inputs::IntoIter>
    where
        Inputs: IntoIterator<Item = &'input str>,
    {
        let TokenizerOptions::UAX29Word(tokenizer_options) = self.options.tokenizer;

        let stemmer = self.options.stemming.map(|stemming_language| {
            let algorithm = stemming_language.into();
            rust_stemmers::Stemmer::create(algorithm)
        });

        TokenStream {
            options: self.options,
            tokenizer_options,
            inputs: inputs.into_iter().enumerate(),
            buffer,
            stemmer,
            next_position: 0,
            input: None,
            current_token: None,
        }
    }
}

/// A lazy stream of analyzed tokens.
///
/// This is a lending stream rather than an [`Iterator`] because token text may
/// borrow from the reusable buffer.
#[must_use = "token streams are lazy and do nothing unless consumed"]
pub struct TokenStream<'input, 'buffer, I>
where
    I: Iterator<Item = &'input str>,
{
    options: AnalysisOptions,
    tokenizer_options: uax29::word::Options,
    inputs: std::iter::Enumerate<I>,
    buffer: &'buffer mut ReusableBuffer,
    stemmer: Option<rust_stemmers::Stemmer>,
    next_position: usize,
    input: Option<InputStream<'input>>,
    current_token: Option<CurrentToken<'input>>,
}

impl<'input, I> TokenStream<'input, '_, I>
where
    I: Iterator<Item = &'input str>,
{
    #[inline(always)]
    fn advance(&mut self) -> bool {
        self.current_token = None;

        loop {
            if self.input.is_none() {
                let Some((input_index, input)) = self.inputs.next() else {
                    return false;
                };
                self.input = Some(InputStream::new(input_index, input, self.tokenizer_options));
            }

            let Some((byte_range, properties)) = self.input.as_mut().unwrap().next_span() else {
                self.input = None;
                continue;
            };
            if !properties.is_word_like() {
                continue;
            }

            // Monotonic across all inputs. Every word-like token consumes a
            // position, even if a downstream filter drops it.
            let position = self.next_position;
            self.next_position += 1;

            let input = self.input.as_ref().unwrap();
            let input_index = input.index;
            let input = input.text;
            let input_as_bytes = input.as_bytes();
            let text = {
                let ReusableBuffer {
                    a: buffer_a,
                    b: buffer_b,
                    stemming_cache,
                } = &mut *self.buffer;

                // SAFETY: word breakpoints are always valid UTF-8 boundaries.
                buffer_a.clear();
                let mut token_text = InputRefOrBuffered::InputRef {
                    input: unsafe {
                        std::str::from_utf8_unchecked(&input_as_bytes[byte_range.clone()])
                    },
                    buffer_if_needed: buffer_a,
                };

                if let Some(max_token_length) = self.options.maximum_token_length
                    && !filters::within_token_length_limit(token_text.as_str(), max_token_length)
                {
                    continue;
                }

                if !self.options.case_sensitive {
                    token_text.lowercase_in_place(properties.is_ascii());
                }

                if let Some(StopwordRemoval::ForLanguage(language)) = self.options.stopword_removal
                    && filters::is_stopword_in_language(language, token_text.as_str())
                {
                    continue;
                }

                if let Some(stemmer) = &self.stemmer {
                    token_text.stem_in_place(stemmer, stemming_cache, buffer_b);
                }

                if self.options.ascii_folding && !properties.is_ascii() {
                    token_text.ascii_fold_in_place(buffer_b);
                    if !self.options.case_sensitive {
                        let is_ascii = token_text.as_str().is_ascii();
                        token_text.lowercase_in_place(is_ascii);
                    }
                }

                token_text.into_current_token_text()
            };

            self.current_token = Some(CurrentToken {
                text,
                position,
                byte_range,
                input_index,
            });
            return true;
        }
    }

    #[inline(always)]
    fn token(&self) -> Token<'_> {
        let current = self.current_token.as_ref().expect("no current token");
        let text = match current.text {
            CurrentTokenText::Input(text) => text,
            CurrentTokenText::Buffer => self.buffer.a.as_str(),
        };
        Token {
            text,
            position: current.position,
            byte_range: current.byte_range.clone(),
            input_index: current.input_index,
        }
    }

    /// Returns the next analyzed token.
    ///
    /// The token is valid until the next mutable operation on the stream.
    #[inline(always)]
    pub fn next_token(&mut self) -> Option<Token<'_>> {
        if self.advance() {
            Some(self.token())
        } else {
            None
        }
    }
}

struct CurrentToken<'input> {
    text: CurrentTokenText<'input>,
    position: usize,
    byte_range: Range<usize>,
    input_index: usize,
}

struct InputStream<'input> {
    index: usize,
    text: &'input str,
    breakpoints: uax29::word::Breakpoints<'input>,
    previous_breakpoint: Option<usize>,
}

impl<'input> InputStream<'input> {
    fn new(index: usize, text: &'input str, options: uax29::word::Options) -> Self {
        Self {
            index,
            text,
            breakpoints: uax29::word::breakpoints(text, options),
            previous_breakpoint: None,
        }
    }

    #[inline(always)]
    fn next_span(&mut self) -> Option<(Range<usize>, uax29::word::TokenProperties)> {
        loop {
            let (breakpoint, properties) = self.breakpoints.next()?;
            let Some(previous) = self.previous_breakpoint.replace(breakpoint) else {
                continue;
            };
            return Some((previous..breakpoint, properties));
        }
    }
}

enum CurrentTokenText<'input> {
    Input(&'input str),
    Buffer,
}

#[non_exhaustive]
pub struct Token<'a> {
    /// Normalized text of the token, either sliced from the input string or from the reused
    /// buffer. Valid for the callback invocation or until the token stream is advanced.
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

impl<'input> InputRefOrBuffered<'input, '_> {
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

    fn into_current_token_text(self) -> CurrentTokenText<'input> {
        match self {
            Self::InputRef { input, .. } => CurrentTokenText::Input(input),
            Self::Buffered(_) => CurrentTokenText::Buffer,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Owned copy of a `Token`'s fields, so tests can use named access.
    #[derive(Debug, Eq, PartialEq)]
    struct Tok {
        text: String,
        position: usize,
        byte_range: Range<usize>,
        input_index: usize,
    }

    impl Tok {
        fn from_token(token: Token<'_>) -> Self {
            Self {
                text: token.text.to_string(),
                position: token.position,
                byte_range: token.byte_range,
                input_index: token.input_index,
            }
        }
    }

    fn collect_inputs<'a>(
        opts: AnalysisOptions,
        inputs: impl IntoIterator<Item = &'a str>,
    ) -> Vec<Tok> {
        let mut buffer = ReusableBuffer::new();
        let analyzer = Analyzer::new(opts);
        let mut stream = analyzer.token_stream_inputs(inputs, &mut buffer);
        let mut out = Vec::new();
        while let Some(token) = stream.next_token() {
            out.push(Tok::from_token(token));
        }
        out
    }

    fn collect(opts: AnalysisOptions, input: &str) -> Vec<Tok> {
        collect_inputs(opts, std::iter::once(input))
    }

    fn collect_callback_inputs<'a>(
        opts: AnalysisOptions,
        inputs: impl IntoIterator<Item = &'a str>,
    ) -> Vec<Tok> {
        let mut out = Vec::new();
        Analyzer::new(opts).analyze_inputs(inputs, &mut ReusableBuffer::new(), |token| {
            out.push(Tok::from_token(token));
            true
        });
        out
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
    fn token_stream_matches_callback_api() {
        let inputs = ["The café runners", "ΣΟΦΟΣ and 中 👨\u{200D}👩"];
        let base = opts();
        let options = [
            AnalysisOptions {
                case_sensitive: true,
                ..base
            },
            base,
            AnalysisOptions {
                maximum_token_length: Some(4),
                ..base
            },
            AnalysisOptions {
                stopword_removal: Some(StopwordRemoval::ForLanguage(
                    LanguageWithStopwords::English,
                )),
                ..base
            },
            AnalysisOptions {
                stemming: Some(StemmingLanguage::English),
                ..base
            },
            AnalysisOptions {
                maximum_token_length: Some(40),
                stopword_removal: Some(StopwordRemoval::ForLanguage(
                    LanguageWithStopwords::English,
                )),
                stemming: Some(StemmingLanguage::English),
                ascii_folding: true,
                ..base
            },
        ];

        for options in options {
            assert_eq!(
                collect_inputs(options, inputs),
                collect_callback_inputs(options, inputs),
            );
        }
    }

    #[test]
    fn token_stream_pulls_inputs_lazily() {
        let inputs_pulled = std::cell::Cell::new(0);
        let inputs = ["first", "second"].into_iter().inspect(|_| {
            inputs_pulled.set(inputs_pulled.get() + 1);
        });
        let analyzer = Analyzer::new(opts());
        let mut buffer = ReusableBuffer::new();
        let mut stream = analyzer.token_stream_inputs(inputs, &mut buffer);

        assert_eq!(inputs_pulled.get(), 0);
        assert_eq!(stream.next_token().unwrap().text, "first");
        assert_eq!(inputs_pulled.get(), 1);
        assert_eq!(stream.next_token().unwrap().text, "second");
        assert_eq!(inputs_pulled.get(), 2);
        assert!(stream.next_token().is_none());
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
