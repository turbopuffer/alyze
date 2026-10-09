//! Tests for the kuromoji port. Almost everything here compares against golden files produced by
//! the reference Java implementation; see `testdata/kuromoji/README.md` for how they are made
//! and their formats.

mod chardef;
mod charfilter;
mod dict;
mod golden;
mod invariants;
mod number;
mod romaji;
mod stoplists;
mod unicode;
mod upstream_filters;
mod upstream_tokenizer;
mod user_dict;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::ops::Range;
use std::sync::OnceLock;

use crate::kuromoji::filter::{CompletionMode, StopTags, StopWords};
use crate::kuromoji::{
    self, AnalyzerOptions, CompletionAnalyzerOptions, Mode, Options, TokenKind, Tokens,
    UserDictionary, char_filter, filter,
};
use crate::testutil::unescape_utf16;
pub(super) use crate::testutil::{escape, read_cases, unescape, utf16_to_byte_offset};

pub(super) fn testdata_path(relative: &str) -> String {
    format!(
        "{}/testdata/kuromoji/{relative}",
        env!("CARGO_MANIFEST_DIR")
    )
}

pub(super) fn read_testdata(relative: &str) -> String {
    let path = testdata_path(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"))
}

// ------------------------------------------------------------------------------------------------
// Running the port

/// An owned token for comparisons. `position` is absolute (0-based, the running sum of position
/// increments), which is easier to compare than increments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Tok {
    pub text: String,
    pub byte_range: Range<usize>,
    pub position: usize,
    pub position_length: u32,
    /// `None` only in goldens of stages that don't carry it (completion tokens).
    pub kind: Option<TokenKind>,
    pub part_of_speech: String,
    pub base_form: Option<String>,
    pub reading: Option<String>,
    pub pronunciation: Option<String>,
    pub inflection_type: Option<String>,
    pub inflection_form: Option<String>,
}

impl std::fmt::Display for Tok {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}..{} {:?} @{}",
            self.byte_range.start,
            self.byte_range.end,
            escape(&self.text),
            self.position
        )?;
        if self.position_length != 1 {
            write!(f, " len={}", self.position_length)?;
        }
        if let Some(kind) = self.kind {
            write!(f, " {kind:?}")?;
        }
        if !self.part_of_speech.is_empty() {
            write!(f, " {}", self.part_of_speech)?;
        }
        if let Some(s) = &self.base_form {
            write!(f, " base={s}")?;
        }
        if let Some(s) = &self.reading {
            write!(f, " reading={s}")?;
        }
        if let Some(s) = &self.pronunciation {
            write!(f, " pron={s}")?;
        }
        if let Some(s) = &self.inflection_type {
            write!(f, " infl={s}")?;
        }
        if let Some(s) = &self.inflection_form {
            write!(f, "/{s}")?;
        }
        Ok(())
    }
}

/// Converts the port's output buffer to owned tokens, checking basic structural invariants.
pub(super) fn collect(input: &str, tokens: &Tokens) -> Vec<Tok> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut position: Option<usize> = None;
    for token in tokens.iter() {
        assert!(
            input.is_char_boundary(token.byte_range.start)
                && input.is_char_boundary(token.byte_range.end),
            "token {token:?} not on char boundaries of {:?}",
            escape(input)
        );
        assert!(token.byte_range.start <= token.byte_range.end);
        assert!(token.byte_range.end <= input.len());
        assert!(token.position_length >= 1, "position length 0: {token:?}");
        // Lucene's first token has its increment from the (virtual) position -1; a filter that
        // removed tokens before it leaves a larger increment.
        assert!(
            token.position_increment >= 1 || position.is_some(),
            "first token: {token:?}"
        );
        position = Some(match position {
            None => token.position_increment as usize - 1,
            Some(p) => p + token.position_increment as usize,
        });
        out.push(Tok {
            text: token.text.to_owned(),
            byte_range: token.byte_range.clone(),
            position: position.unwrap(),
            position_length: token.position_length,
            kind: Some(token.kind),
            part_of_speech: token.part_of_speech.to_owned(),
            base_form: token.base_form.map(str::to_owned),
            reading: token.reading.map(str::to_owned),
            pronunciation: token.pronunciation.map(str::to_owned),
            inflection_type: token.inflection_type.map(str::to_owned),
            inflection_form: token.inflection_form.map(str::to_owned),
        });
    }
    out
}

/// A tokenizer configuration, by the name the golden files use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TokenizerConfig {
    pub mode: Mode,
    pub keep_punctuation: bool,
    pub discard_compound: bool,
    /// Elasticsearch's `nbest_cost` (negative: off).
    pub nbest_cost: i32,
    /// Elasticsearch's `nbest_examples`; the larger of the derived cost and `nbest_cost` applies.
    pub nbest_examples: Option<&'static str>,
    /// A file under `testdata/kuromoji/userdict/`.
    pub user_dict: Option<&'static str>,
}

impl TokenizerConfig {
    /// Elasticsearch's `kuromoji_tokenizer` defaults.
    pub const DEFAULT: TokenizerConfig = TokenizerConfig {
        mode: Mode::Search,
        keep_punctuation: false,
        discard_compound: false,
        nbest_cost: -1,
        nbest_examples: None,
        user_dict: None,
    };

    /// The configurations `gen.sh` runs, by golden-file name.
    pub fn named(name: &str) -> TokenizerConfig {
        let d = TokenizerConfig::DEFAULT;
        match name {
            "default" => d,
            "normal" => TokenizerConfig {
                mode: Mode::Normal,
                ..d
            },
            "extended" => TokenizerConfig {
                mode: Mode::Extended,
                ..d
            },
            "nocompound" => TokenizerConfig {
                discard_compound: true,
                ..d
            },
            "punct" => TokenizerConfig {
                keep_punctuation: true,
                ..d
            },
            "normal_punct" => TokenizerConfig {
                mode: Mode::Normal,
                keep_punctuation: true,
                ..d
            },
            "extended_punct" => TokenizerConfig {
                mode: Mode::Extended,
                keep_punctuation: true,
                ..d
            },
            "nbest" => TokenizerConfig {
                nbest_cost: 2000,
                ..d
            },
            "normal_nbest" => TokenizerConfig {
                mode: Mode::Normal,
                keep_punctuation: true,
                nbest_cost: 4000,
                ..d
            },
            "nbest_examples" => TokenizerConfig {
                nbest_examples: Some("/鳩山積み-鳩山/鳩山積み-鳩/"),
                ..d
            },
            "userdict" => TokenizerConfig {
                user_dict: Some("edge"),
                ..d
            },
            "userdict_normal" => TokenizerConfig {
                mode: Mode::Normal,
                user_dict: Some("edge"),
                ..d
            },
            "userdict_extended_punct" => TokenizerConfig {
                mode: Mode::Extended,
                keep_punctuation: true,
                user_dict: Some("edge"),
                ..d
            },
            other => panic!("unknown tokenizer config {other}"),
        }
    }

    pub fn options(&self) -> Options<'static> {
        let mut options = Options {
            mode: self.mode,
            discard_punctuation: !self.keep_punctuation,
            discard_compound_token: self.discard_compound,
            nbest_cost: self.nbest_cost,
            user_dictionary: self.user_dict.map(user_dictionary),
        };
        if let Some(examples) = self.nbest_examples {
            // KuromojiTokenizerFactory: max(nbest_cost, calcNBestCost(nbest_examples)).
            let derived = kuromoji::calc_nbest_cost(examples, options)
                .unwrap_or_else(|e| panic!("nbest_examples {examples:?}: {e}"));
            options.nbest_cost = options.nbest_cost.max(derived);
        }
        options
    }
}

/// The user dictionary built from `testdata/kuromoji/userdict/<name>.txt` (strict parsing; none
/// of the golden-run dictionaries have duplicates).
pub(super) fn user_dictionary(name: &'static str) -> &'static UserDictionary {
    static DICTS: OnceLock<std::sync::Mutex<HashMap<&'static str, &'static UserDictionary>>> =
        OnceLock::new();
    let dicts = DICTS.get_or_init(Default::default);
    // A test that panicked while building a dictionary must not poison the others.
    let mut dicts = dicts.lock().unwrap_or_else(|e| e.into_inner());
    dicts.entry(name).or_insert_with(|| {
        let rules = read_testdata(&format!("userdict/{name}.txt"));
        let dict = UserDictionary::parse(&rules, false)
            .unwrap_or_else(|e| panic!("userdict/{name}.txt: {e}"));
        Box::leak(Box::new(dict))
    })
}

/// A char filter in front of the tokenizer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CharFilter {
    IterationMark { kanji: bool, kana: bool },
    CjkWidth,
}

impl CharFilter {
    pub fn apply(self, input: &str) -> char_filter::Filtered {
        match self {
            CharFilter::IterationMark { kanji, kana } => {
                char_filter::iteration_mark(input, kanji, kana)
            }
            CharFilter::CjkWidth => char_filter::cjk_width(input),
        }
    }

    /// By the name the golden files use (`charfilter.<case>.<name>.txt`).
    pub fn named(name: &str) -> CharFilter {
        match name {
            "itermark" => CharFilter::IterationMark {
                kanji: true,
                kana: true,
            },
            "itermark_kanji" => CharFilter::IterationMark {
                kanji: true,
                kana: false,
            },
            "itermark_kana" => CharFilter::IterationMark {
                kanji: false,
                kana: true,
            },
            "itermark_none" => CharFilter::IterationMark {
                kanji: false,
                kana: false,
            },
            "width" => CharFilter::CjkWidth,
            other => panic!("unknown char filter {other}"),
        }
    }
}

/// Tokenizes, through an optional char filter, mapping offsets back to `input`.
pub(super) fn tokenize(
    input: &str,
    char_filter: Option<CharFilter>,
    config: TokenizerConfig,
    tokens: &mut Tokens,
) {
    match char_filter {
        None => kuromoji::tokenize(input, config.options(), tokens),
        Some(cf) => {
            let filtered = cf.apply(input);
            kuromoji::tokenize(filtered.text(), config.options(), tokens);
            filtered.correct_tokens(tokens);
        }
    }
}

pub(super) fn run_tokenizer(input: &str, config: TokenizerConfig) -> Vec<Tok> {
    let mut tokens = Tokens::new();
    tokenize(input, None, config, &mut tokens);
    collect(input, &tokens)
}

/// A filter in a custom chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Filter {
    BaseForm,
    PosStop(StopTags),
    Stop(StopWords),
    Stem(usize),
    ReadingForm { romaji: bool },
    Number,
    HiraganaUppercase,
    KatakanaUppercase,
    Width,
    Completion(CompletionMode),
    Lowercase,
}

impl Filter {
    pub fn apply(&self, tokens: &mut Tokens) {
        match self {
            Filter::BaseForm => filter::base_form(tokens),
            Filter::PosStop(tags) => filter::part_of_speech_stop(tokens, tags),
            Filter::Stop(words) => filter::stop(tokens, words),
            Filter::Stem(min) => filter::katakana_stem(tokens, *min),
            Filter::ReadingForm { romaji } => filter::reading_form(tokens, *romaji),
            Filter::Number => filter::number(tokens),
            Filter::HiraganaUppercase => filter::hiragana_uppercase(tokens),
            Filter::KatakanaUppercase => filter::katakana_uppercase(tokens),
            Filter::Width => filter::cjk_width(tokens),
            Filter::Completion(mode) => filter::completion(tokens, *mode),
            Filter::Lowercase => filter::lowercase(tokens),
        }
    }
}

/// Stop words from `testdata/kuromoji/stopwords/<name>.txt` (one per line, `#` comments).
pub(super) fn stop_words(name: &str, ignore_case: bool) -> StopWords {
    let words: Vec<String> = read_testdata(&format!("stopwords/{name}.txt"))
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect();
    StopWords::new(words, ignore_case)
}

/// The custom chains `gen.sh` runs, by golden-file name: an optional char filter, the tokenizer
/// configuration and the filters, in order.
pub(super) fn chain(name: &str) -> (Option<CharFilter>, TokenizerConfig, Vec<Filter>) {
    let d = TokenizerConfig::DEFAULT;
    let punct = TokenizerConfig::named("punct");
    let completion_tokenizer = TokenizerConfig {
        mode: Mode::Normal,
        discard_compound: true,
        ..d
    };
    let tags = |list: &[&str]| StopTags::new(list.iter().copied());
    match name {
        "baseform" => (None, d, vec![Filter::BaseForm]),
        "pos" => (None, d, vec![Filter::PosStop(StopTags::defaults())]),
        "pos_docs" => (
            None,
            d,
            vec![Filter::PosStop(tags(&["助詞-格助詞-一般", "助詞-終助詞"]))],
        ),
        "pos_verb" => (None, d, vec![Filter::PosStop(tags(&["動詞-自立"]))]),
        "reading" => (None, d, vec![Filter::ReadingForm { romaji: false }]),
        "romaji" => (None, d, vec![Filter::ReadingForm { romaji: true }]),
        "width_reading" => (
            None,
            d,
            vec![Filter::Width, Filter::ReadingForm { romaji: false }],
        ),
        "width_romaji" => (
            None,
            d,
            vec![Filter::Width, Filter::ReadingForm { romaji: true }],
        ),
        "stem" => (None, d, vec![Filter::Stem(4)]),
        "stem6" => (None, d, vec![Filter::Stem(6)]),
        "stop" => (None, d, vec![Filter::Stop(StopWords::japanese())]),
        "stop_custom" => (None, d, vec![Filter::Stop(stop_words("custom", false))]),
        "stop_custom_nocase" => (None, d, vec![Filter::Stop(stop_words("custom", true))]),
        "number" => (None, punct, vec![Filter::Number]),
        "number_nbest" => (
            None,
            TokenizerConfig {
                nbest_cost: 2000,
                ..punct
            },
            vec![Filter::Number],
        ),
        "hiragana_upper" => (None, d, vec![Filter::HiraganaUppercase]),
        "katakana_upper" => (None, d, vec![Filter::KatakanaUppercase]),
        "completion_index" => (
            Some(CharFilter::CjkWidth),
            completion_tokenizer,
            vec![Filter::Completion(CompletionMode::Index)],
        ),
        "completion_query" => (
            Some(CharFilter::CjkWidth),
            completion_tokenizer,
            vec![Filter::Completion(CompletionMode::Query)],
        ),
        "itermark" => (Some(CharFilter::named("itermark")), d, vec![]),
        "itermark_kanji" => (Some(CharFilter::named("itermark_kanji")), d, vec![]),
        "itermark_kana" => (Some(CharFilter::named("itermark_kana")), d, vec![]),
        "itermark_none" => (Some(CharFilter::named("itermark_none")), d, vec![]),
        "itermark_punct" => (Some(CharFilter::named("itermark")), punct, vec![]),
        "width_char" => (Some(CharFilter::CjkWidth), d, vec![]),
        "width_char_punct" => (Some(CharFilter::CjkWidth), punct, vec![]),
        "lowercase" => (None, d, vec![Filter::Lowercase]),
        // The chain the Elasticsearch docs recommend for a custom `kuromoji`-like analyzer.
        "recommended" => (
            None,
            d,
            vec![
                Filter::BaseForm,
                Filter::PosStop(StopTags::defaults()),
                Filter::Width,
                Filter::Stop(StopWords::japanese()),
                Filter::Stem(4),
                Filter::Lowercase,
            ],
        ),
        other => panic!("unknown chain {other}"),
    }
}

pub(super) fn run_chain(
    input: &str,
    char_filter: Option<CharFilter>,
    config: TokenizerConfig,
    filters: &[Filter],
) -> Vec<Tok> {
    let mut tokens = Tokens::new();
    tokenize(input, char_filter, config, &mut tokens);
    for filter in filters {
        filter.apply(&mut tokens);
    }
    collect(input, &tokens)
}

/// The `kuromoji` analyzer configurations `gen.sh` runs, by golden-file name.
pub(super) fn analyzer_options(name: &str) -> AnalyzerOptions<'static> {
    static CUSTOM_STOP_WORDS: OnceLock<StopWords> = OnceLock::new();
    let d = AnalyzerOptions::default();
    match name {
        "default" => d,
        "normal" => AnalyzerOptions {
            mode: Mode::Normal,
            ..d
        },
        "extended" => AnalyzerOptions {
            mode: Mode::Extended,
            ..d
        },
        "userdict" => AnalyzerOptions {
            user_dictionary: Some(user_dictionary("edge")),
            ..d
        },
        // Elasticsearch's `stopwords` + `stopwords_case: true`: the list replaces the default.
        "stopwords" => AnalyzerOptions {
            stop_words: Some(CUSTOM_STOP_WORDS.get_or_init(|| stop_words("custom", true))),
            ..d
        },
        other => panic!("unknown analyzer config {other}"),
    }
}

/// The `kuromoji` analyzer as a chain, minus its final lowercasing: what the golden comparison
/// runs, so that Lucene's lowercase can be applied to the port's text (see [`Compare`]).
pub(super) fn analyzer_chain(
    options: AnalyzerOptions<'static>,
) -> (Option<CharFilter>, TokenizerConfig, Vec<Filter>) {
    let config = TokenizerConfig {
        mode: options.mode,
        keep_punctuation: false,
        discard_compound: true,
        nbest_cost: -1,
        nbest_examples: None,
        user_dict: match options.user_dictionary {
            None => None,
            Some(_) => Some("edge"),
        },
    };
    (
        Some(CharFilter::CjkWidth),
        config,
        vec![
            Filter::BaseForm,
            Filter::PosStop(options.stop_tags.cloned().unwrap_or_default()),
            Filter::Stop(options.stop_words.cloned().unwrap_or_default()),
            Filter::Stem(4),
        ],
    )
}

/// Runs the analyzer's stages up to (not including) lowercasing.
pub(super) fn run_analyzer_unlowercased(input: &str, name: &str) -> Vec<Tok> {
    let (cf, config, filters) = analyzer_chain(analyzer_options(name));
    run_chain(input, cf, config, &filters)
}

pub(super) fn run_analyzer(input: &str, name: &str) -> Vec<Tok> {
    let mut tokens = Tokens::new();
    kuromoji::analyze(input, analyzer_options(name), &mut tokens);
    collect(input, &tokens)
}

/// The `kuromoji_completion` analyzer configurations `gen.sh` runs.
pub(super) fn completion_options(name: &str) -> CompletionAnalyzerOptions<'static> {
    match name {
        "index" => CompletionAnalyzerOptions {
            mode: CompletionMode::Index,
            user_dictionary: None,
        },
        "query" => CompletionAnalyzerOptions {
            mode: CompletionMode::Query,
            user_dictionary: None,
        },
        "userdict" => CompletionAnalyzerOptions {
            mode: CompletionMode::Index,
            user_dictionary: Some(user_dictionary("edge")),
        },
        other => panic!("unknown completion analyzer config {other}"),
    }
}

/// The completion analyzer as a chain, minus its final lowercasing.
pub(super) fn completion_chain(
    options: CompletionAnalyzerOptions<'static>,
) -> (Option<CharFilter>, TokenizerConfig, Vec<Filter>) {
    (
        Some(CharFilter::CjkWidth),
        TokenizerConfig {
            mode: Mode::Normal,
            discard_compound: true,
            user_dict: options.user_dictionary.map(|_| "edge"),
            ..TokenizerConfig::DEFAULT
        },
        vec![Filter::Completion(options.mode)],
    )
}

pub(super) fn run_completion_unlowercased(input: &str, name: &str) -> Vec<Tok> {
    let (cf, config, filters) = completion_chain(completion_options(name));
    run_chain(input, cf, config, &filters)
}

pub(super) fn run_completion(input: &str, name: &str) -> Vec<Tok> {
    let mut tokens = Tokens::new();
    kuromoji::analyze_completion(input, completion_options(name), &mut tokens);
    collect(input, &tokens)
}

// ------------------------------------------------------------------------------------------------
// Reading golden files

/// The marker the generator writes for a null attribute (never produced by the escaper).
pub(super) const NULL: &str = "\\N";

/// One line of a token golden, as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GoldenToken {
    pub start: usize,
    pub end: usize,
    pub pos_inc: usize,
    pub pos_len: u32,
    pub kind: Option<TokenKind>,
    pub part_of_speech: String,
    pub base_form: Option<String>,
    pub reading: Option<String>,
    pub pronunciation: Option<String>,
    pub inflection_type: Option<String>,
    pub inflection_form: Option<String>,
    /// The text as UTF-16 code units: it can contain lone surrogates (see [`golden_to_toks`]).
    pub text: Vec<u16>,
}

fn optional(field: &str) -> Option<String> {
    (field != NULL).then(|| unescape(field))
}

/// Reads a token golden (`.tokens`, `.analyze`, `.completion` or `.chain`): one list of tokens
/// per case.
pub(super) fn read_token_golden(contents: &str) -> Vec<Vec<GoldenToken>> {
    let mut cases: Vec<Vec<GoldenToken>> = Vec::new();
    for line in contents.lines() {
        if let Some(index) = line.strip_prefix("# ") {
            assert_eq!(
                index.parse::<usize>().unwrap(),
                cases.len(),
                "cases out of order"
            );
            cases.push(Vec::new());
            continue;
        }
        let fields: Vec<&str> = line.splitn(12, '\t').collect();
        assert_eq!(fields.len(), 12, "bad token line {line:?}");
        let token = GoldenToken {
            start: fields[0].parse().unwrap(),
            end: fields[1].parse().unwrap(),
            pos_inc: fields[2].parse().unwrap(),
            pos_len: fields[3].parse().unwrap(),
            kind: match fields[4] {
                "KNOWN" => Some(TokenKind::Known),
                "UNKNOWN" => Some(TokenKind::Unknown),
                "USER" => Some(TokenKind::User),
                NULL => None,
                other => panic!("unknown token type {other:?}"),
            },
            part_of_speech: optional(fields[5]).unwrap_or_default(),
            base_form: optional(fields[6]),
            reading: optional(fields[7]),
            pronunciation: optional(fields[8]),
            inflection_type: optional(fields[9]),
            inflection_form: optional(fields[10]),
            text: unescape_utf16(fields[11]),
        };
        cases
            .last_mut()
            .expect("token before first case header")
            .push(token);
    }
    cases
}

fn has_lone_surrogate(units: &[u16]) -> bool {
    char::decode_utf16(units.iter().copied()).any(|r| r.is_err())
}

/// Converts a golden token list to the port's representation: positions are the running sum of
/// position increments, 0-based.
///
/// One documented divergence is normalized here, exactly as for nori. Lucene caps unknown words
/// at 1024 UTF-16 code units (and extended mode emits unknown unigrams per code unit), so a run
/// of supplementary characters can be cut inside a surrogate pair. The golden then has a token
/// whose text ends in a lone high surrogate (its byte range already covers the whole code point,
/// since byte offsets are counted on the high surrogate) followed by a token whose text starts
/// with the lone low surrogate (its byte range already starts after the pair), or, in extended
/// mode, a lone-low-surrogate token with an empty byte range. The port keeps code points whole,
/// so here a token with a lone surrogate takes the input slice of its byte range as its text, and
/// an empty-range one is dropped altogether (the port emits nothing there, so the positions after
/// it are one lower). Such tokens are always unknown words, whose text is otherwise exactly the
/// input slice.
pub(super) fn golden_to_toks(input: &str, golden: &[GoldenToken]) -> Vec<Tok> {
    let mut toks = Vec::with_capacity(golden.len());
    let mut position = 0usize;
    let mut first = true;
    for t in golden {
        let text = if has_lone_surrogate(&t.text) {
            assert_eq!(
                t.kind,
                Some(TokenKind::Unknown),
                "lone surrogate in a non-unknown token: {t:?}"
            );
            if t.start == t.end {
                assert_eq!(t.pos_inc, 1, "junk half-token with a position gap: {t:?}");
                continue;
            }
            input[t.start..t.end].to_owned()
        } else {
            String::from_utf16(&t.text).unwrap()
        };
        let inc = t.pos_inc;
        // Lucene's first token has increment 1 from the (virtual) position -1.
        position = if first { inc - 1 } else { position + inc };
        first = false;
        toks.push(Tok {
            text,
            byte_range: t.start..t.end,
            position,
            position_length: t.pos_len,
            kind: t.kind,
            part_of_speech: t.part_of_speech.clone(),
            base_form: t.base_form.clone(),
            reading: t.reading.clone(),
            pronunciation: t.pronunciation.clone(),
            inflection_type: t.inflection_type.clone(),
            inflection_form: t.inflection_form.clone(),
        });
    }
    toks
}

// ------------------------------------------------------------------------------------------------
// Comparing

/// Which fields are compared, per stage.
#[derive(Clone, Copy, Debug)]
pub(super) struct Compare {
    /// Compare token kind, part of speech, base form, reading, pronunciation and inflection. Off
    /// for stages ending in the completion filter, which clears every attribute in Lucene (the
    /// port leaves them empty too, but a golden can't tell "cleared" from "absent").
    pub attributes: bool,
    /// The stage ends with Lucene's `LowerCaseFilter`, which the port's run leaves out: the
    /// golden text must equal Java's simple lowercase of the port's (unlowercased) text. The
    /// port's own lowercasing is checked separately (`golden::lowercase_is_the_only_difference`).
    pub lowercases: bool,
}

impl Compare {
    pub const TOKENIZER: Compare = Compare {
        attributes: true,
        lowercases: false,
    };
}

/// Java's `Character.toLowerCase(int)`, from `golden/lowercase.txt`.
pub(super) fn java_simple_lowercase(c: char) -> char {
    static MAP: OnceLock<HashMap<char, char>> = OnceLock::new();
    let map = MAP.get_or_init(|| {
        read_testdata("golden/lowercase.txt")
            .lines()
            .map(|line| {
                let (from, to) = line.split_once('\t').unwrap();
                let parse = |s| char::from_u32(u32::from_str_radix(s, 16).unwrap()).unwrap();
                (parse(from), parse(to))
            })
            .collect()
    });
    map.get(&c).copied().unwrap_or(c)
}

pub(super) fn java_lowercase_text(text: &str) -> String {
    text.chars().map(java_simple_lowercase).collect()
}

fn toks_match(expected: &Tok, actual: &Tok, compare: Compare) -> bool {
    if expected.byte_range != actual.byte_range
        || expected.position != actual.position
        || expected.position_length != actual.position_length
    {
        return false;
    }
    if compare.attributes
        && (expected.kind != actual.kind
            || expected.part_of_speech != actual.part_of_speech
            || expected.base_form != actual.base_form
            || expected.reading != actual.reading
            || expected.pronunciation != actual.pronunciation
            || expected.inflection_type != actual.inflection_type
            || expected.inflection_form != actual.inflection_form)
    {
        return false;
    }
    if compare.lowercases {
        expected.text == java_lowercase_text(&actual.text)
    } else {
        expected.text == actual.text
    }
}

/// Formats a window of tokens around `around`, since the first difference is what matters.
fn fmt_toks(toks: &[Tok], around: usize) -> String {
    const BEFORE: usize = 3;
    const AFTER: usize = 12;
    let start = around.saturating_sub(BEFORE);
    let end = (around + AFTER).min(toks.len());
    let mut s = String::new();
    if start > 0 {
        write!(s, "({start} tokens) … ").unwrap();
    }
    for (i, t) in toks[start..end].iter().enumerate() {
        if i > 0 {
            s.push_str("  ");
        }
        write!(s, "[{t}]").unwrap();
    }
    if end < toks.len() {
        write!(s, " … ({} more)", toks.len() - end).unwrap();
    }
    s
}

/// Compares the port's output against a golden across many cases, reporting every differing case
/// (up to a limit) rather than stopping at the first, since a count is what matters while the port
/// is being brought up.
pub(super) fn assert_cases_match(
    name: &str,
    inputs: &[String],
    golden: &[Vec<GoldenToken>],
    run: impl Fn(&str) -> Vec<Tok>,
    compare: Compare,
) {
    assert_eq!(
        inputs.len(),
        golden.len(),
        "{name}: case file and golden file disagree on the number of cases"
    );
    const REPORT_LIMIT: usize = 5;
    let mut failures = 0usize;
    let mut report = String::new();
    for (i, (input, golden)) in inputs.iter().zip(golden).enumerate() {
        let expected = golden_to_toks(input, golden);
        let actual = run(input);
        let matches = expected.len() == actual.len()
            && expected
                .iter()
                .zip(&actual)
                .all(|(e, a)| toks_match(e, a, compare));
        if matches {
            continue;
        }
        failures += 1;
        if failures > REPORT_LIMIT {
            continue;
        }
        let first_diff = expected
            .iter()
            .zip(&actual)
            .position(|(e, a)| !toks_match(e, a, compare))
            .unwrap_or(expected.len().min(actual.len()));
        let mut input_display = escape(input);
        if input_display.chars().count() > 300 {
            input_display = input_display.chars().take(300).collect::<String>() + "…";
        }
        writeln!(
            report,
            "case {i} (line {} of {name}): first difference at token {first_diff}",
            i + 1
        )
        .unwrap();
        writeln!(report, "  input:    {input_display}").unwrap();
        writeln!(report, "  expected: {}", fmt_toks(&expected, first_diff)).unwrap();
        writeln!(report, "  actual:   {}", fmt_toks(&actual, first_diff)).unwrap();
    }
    assert!(
        failures == 0,
        "{failures} of {} cases differ for {name}:\n{report}",
        inputs.len()
    );
}
