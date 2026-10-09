//! Tests for the nori port. Almost everything here compares against golden files produced by the
//! reference Java implementation; see `testdata/nori/README.md` for how they are made and their
//! formats.

mod chardef;
mod dict;
mod golden;
mod invariants;
mod unicode;
mod upstream;
mod user_dict;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::ops::Range;
use std::sync::OnceLock;

use crate::nori::{
    self, AnalyzerOptions, DecompoundMode, Options, TokenKind, Tokens, UserDictionary, filter, pos,
};
use crate::testutil::unescape_utf16;
pub(super) use crate::testutil::{escape, read_cases, unescape, utf16_to_byte_offset};

pub(super) fn testdata_path(relative: &str) -> String {
    format!("{}/testdata/nori/{relative}", env!("CARGO_MANIFEST_DIR"))
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
    /// `None` only in goldens of stages that don't carry it (never for the port).
    pub kind: Option<TokenKind>,
    pub pos_type: pos::Type,
    pub left_pos: pos::Tag,
    pub right_pos: pos::Tag,
    pub reading: Option<String>,
    pub morphemes: Vec<(pos::Tag, String)>,
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
        write!(
            f,
            " {:?}/{}/{}",
            self.pos_type, self.left_pos, self.right_pos
        )?;
        if let Some(kind) = self.kind {
            write!(f, " {kind:?}")?;
        }
        if let Some(reading) = &self.reading {
            write!(f, " reading={reading}")?;
        }
        if !self.morphemes.is_empty() {
            let parts: Vec<String> = self
                .morphemes
                .iter()
                .map(|(tag, text)| format!("{text}/{tag}"))
                .collect();
            write!(f, " morphemes={}", parts.join("+"))?;
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
            pos_type: token.pos_type,
            left_pos: token.left_pos,
            right_pos: token.right_pos,
            reading: token.reading.map(str::to_owned),
            morphemes: token
                .morphemes
                .iter()
                .map(|m| (m.tag, m.text.clone()))
                .collect(),
        });
    }
    out
}

/// A tokenizer configuration, by the name the golden files use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TokenizerConfig {
    pub decompound: DecompoundMode,
    pub keep_punctuation: bool,
    pub unigrams: bool,
    /// A file under `testdata/nori/userdict/`.
    pub user_dict: Option<&'static str>,
}

impl TokenizerConfig {
    pub const DEFAULT: TokenizerConfig = TokenizerConfig {
        decompound: DecompoundMode::Discard,
        keep_punctuation: false,
        unigrams: false,
        user_dict: None,
    };

    /// The configurations `gen.sh` runs, by golden-file name.
    pub fn named(name: &str) -> TokenizerConfig {
        let d = TokenizerConfig::DEFAULT;
        match name {
            "default" => d,
            "mixed" => TokenizerConfig {
                decompound: DecompoundMode::Mixed,
                ..d
            },
            "none" => TokenizerConfig {
                decompound: DecompoundMode::None,
                ..d
            },
            "punct" => TokenizerConfig {
                keep_punctuation: true,
                ..d
            },
            "mixed_punct" => TokenizerConfig {
                decompound: DecompoundMode::Mixed,
                keep_punctuation: true,
                ..d
            },
            "unigrams" => TokenizerConfig {
                unigrams: true,
                ..d
            },
            "userdict" => TokenizerConfig {
                user_dict: Some("edge"),
                ..d
            },
            "userdict_mixed_punct" => TokenizerConfig {
                decompound: DecompoundMode::Mixed,
                keep_punctuation: true,
                user_dict: Some("edge"),
                ..d
            },
            other => panic!("unknown tokenizer config {other}"),
        }
    }

    pub fn options(&self) -> Options<'static> {
        Options {
            decompound_mode: self.decompound,
            discard_punctuation: !self.keep_punctuation,
            output_unknown_unigrams: self.unigrams,
            user_dictionary: self.user_dict.map(user_dictionary),
        }
    }
}

/// The user dictionary built from `testdata/nori/userdict/<name>.txt` (strict parsing; none of
/// the golden-run dictionaries have duplicates).
pub(super) fn user_dictionary(name: &'static str) -> &'static UserDictionary {
    static DICTS: OnceLock<std::sync::Mutex<HashMap<&'static str, &'static UserDictionary>>> =
        OnceLock::new();
    let dicts = DICTS.get_or_init(Default::default);
    let mut dicts = dicts.lock().unwrap();
    dicts.entry(name).or_insert_with(|| {
        let rules = read_testdata(&format!("userdict/{name}.txt"));
        let dict = UserDictionary::parse(&rules, false)
            .unwrap_or_else(|e| panic!("userdict/{name}.txt: {e}"));
        Box::leak(Box::new(dict))
    })
}

pub(super) fn run_tokenizer(input: &str, config: TokenizerConfig) -> Vec<Tok> {
    let mut tokens = Tokens::new();
    nori::tokenize(input, config.options(), &mut tokens);
    collect(input, &tokens)
}

/// The `nori` analyzer configurations `gen.sh` runs, by golden-file name.
pub(super) fn analyzer_options(name: &str) -> AnalyzerOptions<'static> {
    let d = AnalyzerOptions::default();
    match name {
        "default" => d,
        "mixed" => AnalyzerOptions {
            decompound_mode: DecompoundMode::Mixed,
            ..d
        },
        "stoptags" => AnalyzerOptions {
            stop_tags: pos::TagSet::parse("NNP,NNG,NR,SP").unwrap(),
            ..d
        },
        "userdict" => AnalyzerOptions {
            user_dictionary: Some(user_dictionary("edge")),
            ..d
        },
        other => panic!("unknown analyzer config {other}"),
    }
}

pub(super) fn run_analyzer(input: &str, name: &str) -> Vec<Tok> {
    let mut tokens = Tokens::new();
    nori::analyze(input, analyzer_options(name), &mut tokens);
    collect(input, &tokens)
}

/// A filter in a custom chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Filter {
    PosStop(pos::TagSet),
    ReadingForm,
    Number,
    Lowercase,
}

impl Filter {
    pub fn apply(self, tokens: &mut Tokens) {
        match self {
            Filter::PosStop(tags) => filter::part_of_speech_stop(tokens, tags),
            Filter::ReadingForm => filter::reading_form(tokens),
            Filter::Number => filter::number(tokens),
            Filter::Lowercase => filter::lowercase(tokens),
        }
    }
}

/// The custom chains `gen.sh` runs, by golden-file name.
pub(super) fn chain(name: &str) -> (TokenizerConfig, Vec<Filter>) {
    let d = TokenizerConfig::DEFAULT;
    let sp = pos::TagSet::from_tags(&[pos::Tag::SP]);
    match name {
        "reading" => (d, vec![Filter::ReadingForm]),
        "number" => (d, vec![Filter::Number]),
        "number_punct" => (
            TokenizerConfig {
                keep_punctuation: true,
                ..d
            },
            vec![Filter::PosStop(sp), Filter::Number],
        ),
        "number_mixed_punct" => (
            TokenizerConfig {
                decompound: DecompoundMode::Mixed,
                keep_punctuation: true,
                ..d
            },
            vec![Filter::PosStop(sp), Filter::Number],
        ),
        "pos_custom" => (
            d,
            vec![Filter::PosStop(
                pos::TagSet::parse(
                    "EP, EF, EC, ETN, ETM, JKS, JKC, JKG, JKO, JKB, JKV, JKQ, JX, JC",
                )
                .unwrap(),
            )],
        ),
        "graph" => (
            TokenizerConfig {
                decompound: DecompoundMode::Mixed,
                ..d
            },
            vec![Filter::PosStop(pos::TagSet::DEFAULT_STOP_TAGS)],
        ),
        "lowercase" => (d, vec![Filter::Lowercase]),
        other => panic!("unknown chain {other}"),
    }
}

pub(super) fn run_chain(input: &str, config: TokenizerConfig, filters: &[Filter]) -> Vec<Tok> {
    let mut tokens = Tokens::new();
    nori::tokenize(input, config.options(), &mut tokens);
    for filter in filters {
        filter.apply(&mut tokens);
    }
    collect(input, &tokens)
}

// ------------------------------------------------------------------------------------------------
// Reading golden files

/// One line of a token golden, as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GoldenToken {
    pub start: usize,
    pub end: usize,
    pub pos_inc: usize,
    pub pos_len: u32,
    pub kind: Option<TokenKind>,
    pub pos_type: pos::Type,
    pub left_pos: pos::Tag,
    pub right_pos: pos::Tag,
    pub reading: Option<String>,
    pub morphemes: Vec<(pos::Tag, String)>,
    /// The text as UTF-16 code units: it can contain lone surrogates (see [`golden_to_toks`]).
    pub text: Vec<u16>,
}

fn parse_tag(s: &str) -> pos::Tag {
    pos::Tag::from_name(s).unwrap_or_else(|| panic!("unknown tag {s:?}"))
}

fn parse_morphemes(s: &str) -> Vec<(pos::Tag, String)> {
    if s == "-" {
        return Vec::new();
    }
    s.split('+')
        .map(|part| {
            let (text, tag) = part
                .rsplit_once('/')
                .unwrap_or_else(|| panic!("bad morpheme {part:?}"));
            (parse_tag(tag), unescape(text))
        })
        .collect()
}

/// Reads a token golden (`.tokens`, `.analyze` or `.chain`): one list of tokens per case.
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
        let fields: Vec<&str> = line.splitn(11, '\t').collect();
        assert_eq!(fields.len(), 11, "bad token line {line:?}");
        let token = GoldenToken {
            start: fields[0].parse().unwrap(),
            end: fields[1].parse().unwrap(),
            pos_inc: fields[2].parse().unwrap(),
            pos_len: fields[3].parse().unwrap(),
            kind: match fields[4] {
                "KNOWN" => Some(TokenKind::Known),
                "UNKNOWN" => Some(TokenKind::Unknown),
                "USER" => Some(TokenKind::User),
                "-" => None,
                other => panic!("unknown token type {other:?}"),
            },
            pos_type: pos::Type::from_name(fields[5])
                .unwrap_or_else(|| panic!("unknown POS type {:?}", fields[5])),
            left_pos: parse_tag(fields[6]),
            right_pos: parse_tag(fields[7]),
            reading: (fields[8] != "-").then(|| unescape(fields[8])),
            morphemes: parse_morphemes(fields[9]),
            text: unescape_utf16(fields[10]),
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
/// One documented divergence is normalized here. Lucene caps unknown words at 1024 UTF-16 code
/// units (and emits unknown unigrams per code unit), so a run of supplementary characters can be
/// cut inside a surrogate pair. The golden then has a token whose text ends in a lone high
/// surrogate (its byte range already covers the whole code point, since byte offsets are counted
/// on the high surrogate) followed by a token whose text starts with the lone low surrogate (its
/// byte range already starts after the pair), or, in unigram mode, a lone-low-surrogate token with
/// an empty byte range. The port keeps code points whole, so here a token with a lone surrogate
/// takes the input slice of its byte range as its text, and an empty-range one is dropped
/// altogether (the port emits nothing there, so the positions after it are one lower). Such tokens
/// are always unknown words, whose text is otherwise exactly the input slice.
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
            pos_type: t.pos_type,
            left_pos: t.left_pos,
            right_pos: t.right_pos,
            reading: t.reading.clone(),
            morphemes: t.morphemes.clone(),
        });
    }
    toks
}

// ------------------------------------------------------------------------------------------------
// Comparing

/// Which fields are compared, per stage.
#[derive(Clone, Copy, Debug)]
pub(super) struct Compare {
    /// Compare token kind, part of speech, reading and morphemes. Off for chains with the number
    /// filter: Lucene leaves the attributes of whichever token it read last on a merged number
    /// token, which is an artifact not worth reproducing. The same artifact reaches the position
    /// of a merged token that ends the stream (Lucene then reads the increment of a token a
    /// preceding stop filter discarded), so with this off the last token's position isn't compared
    /// either.
    pub attributes: bool,
    /// The stage lowercases: a text mismatch is accepted when Lucene's text is Java's simple
    /// lowercase of the raw token and the port's is alyze's lowercase of it (see the module docs).
    pub lowercases: bool,
}

impl Compare {
    pub const TOKENIZER: Compare = Compare {
        attributes: true,
        lowercases: false,
    };
    pub const ANALYZER: Compare = Compare {
        attributes: true,
        lowercases: true,
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

fn toks_match(input: &str, expected: &Tok, actual: &Tok, compare: Compare) -> bool {
    if expected.byte_range != actual.byte_range
        || expected.position != actual.position
        || expected.position_length != actual.position_length
    {
        return false;
    }
    if compare.attributes
        && (expected.kind != actual.kind
            || expected.pos_type != actual.pos_type
            || expected.left_pos != actual.left_pos
            || expected.right_pos != actual.right_pos
            || expected.reading != actual.reading
            || expected.morphemes != actual.morphemes)
    {
        return false;
    }
    if expected.text == actual.text {
        return true;
    }
    if !compare.lowercases {
        return false;
    }
    let raw = &input[expected.byte_range.clone()];
    let java: String = raw.chars().map(java_simple_lowercase).collect();
    java == expected.text && actual.text == filter::lowercase_text(raw)
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
        let last = expected.len().saturating_sub(1);
        let matches = expected.len() == actual.len()
            && expected.iter().zip(&actual).enumerate().all(|(i, (e, a))| {
                if !compare.attributes && i == last && e.position != a.position {
                    let mut e = e.clone();
                    e.position = a.position;
                    return toks_match(input, &e, a, compare);
                }
                toks_match(input, e, a, compare)
            });
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
            .position(|(e, a)| !toks_match(input, e, a, compare))
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
