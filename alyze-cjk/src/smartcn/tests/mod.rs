//! Tests for the smartcn port. Almost everything here compares against golden files produced by
//! the reference Java implementation; see `testdata/smartcn/README.md` for how they are made and
//! their formats.

mod chartypes;
mod dict;
mod golden;
mod sentence;
mod upstream;

use std::fmt::Write as _;
use std::ops::Range;

use crate::smartcn::{self, TokenKind};

// ------------------------------------------------------------------------------------------------
// Running the port

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Tok {
    pub text: String,
    pub byte_range: Range<usize>,
    /// Token position (0-based, gaps where filtered tokens consumed positions). Always equal to
    /// the token index for the tokenizer stage.
    pub position: usize,
}

impl std::fmt::Display for Tok {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}..{} {:?} @{}",
            self.byte_range.start, self.byte_range.end, self.text, self.position
        )
    }
}

/// Runs the tokenizer stage (`smartcn_tokenizer`). Also checks the invariant that `Punctuation`
/// tokens are exactly the tokens whose text is `","`.
pub(super) fn run_tokenizer(input: &str) -> Vec<Tok> {
    let mut tokens = Vec::new();
    let mut buffer = String::new();
    smartcn::tokenize(input, smartcn::Options::default(), &mut buffer, |token| {
        assert_eq!(
            token.kind == TokenKind::Punctuation,
            token.text == ",",
            "punctuation kind must coincide with text \",\": {:?} {:?}",
            token.kind,
            token.text
        );
        assert!(input.is_char_boundary(token.byte_range.start));
        assert!(input.is_char_boundary(token.byte_range.end));
        tokens.push(Tok {
            text: token.text.to_owned(),
            byte_range: token.byte_range,
            position: tokens.len(),
        });
        true
    });
    tokens
}

/// Runs the full analyzer (`smartcn` analyzer).
pub(super) fn run_analyzer(input: &str) -> Vec<Tok> {
    let mut tokens = Vec::new();
    let mut buffer = String::new();
    smartcn::analyze(input, smartcn::Options::default(), &mut buffer, |token| {
        tokens.push(Tok {
            text: token.text.to_owned(),
            byte_range: token.byte_range,
            position: token.position,
        });
        true
    });
    tokens
}

// ------------------------------------------------------------------------------------------------
// Reading case and golden files

pub(super) fn testdata_path(relative: &str) -> String {
    format!("{}/testdata/smartcn/{relative}", env!("CARGO_MANIFEST_DIR"))
}

pub(super) fn read_testdata(relative: &str) -> String {
    let path = testdata_path(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"))
}

use crate::testutil::unescape_allowing_lone_surrogate;
pub(super) use crate::testutil::{escape, read_cases, unescape, utf16_to_byte_offset};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GoldenToken {
    pub start: usize,
    pub end: usize,
    pub pos_inc: usize,
    pub text: String,
    /// Set when the Java token is half of a surrogate pair (see [`golden_to_toks`]).
    pub lone_surrogate: Option<u16>,
}

/// Reads a token golden (`.tokens` or `.analyze`): one list of tokens per case.
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
        let mut fields = line.splitn(4, '\t');
        let start = fields.next().unwrap().parse().unwrap();
        let end = fields.next().unwrap().parse().unwrap();
        let pos_inc = fields.next().unwrap().parse().unwrap();
        let (text, lone_surrogate) = unescape_allowing_lone_surrogate(
            fields
                .next()
                .unwrap_or_else(|| panic!("bad token line {line:?}")),
        );
        let token = GoldenToken {
            start,
            end,
            pos_inc,
            text,
            lone_surrogate,
        };
        cases
            .last_mut()
            .expect("token before first case header")
            .push(token);
    }
    cases
}

/// Converts a golden token list to the port's representation: positions are the running sum of
/// position increments, 0-based.
///
/// One documented divergence is normalized here. Java reads the input in 1024-UTF-16-unit chunks
/// and, when the cut lands inside a surrogate pair, emits each half as its own one-unit token
/// (junk that can't be represented in UTF-8). The port emits the whole code point once instead,
/// at the first half's position and byte range, so the golden's high+low pair is merged into
/// that one token and the positions after it shift down by one.
pub(super) fn golden_to_toks(golden: &[GoldenToken]) -> Vec<Tok> {
    let mut toks = Vec::with_capacity(golden.len());
    let mut position = 0usize;
    let mut first = true;
    let mut i = 0;
    while i < golden.len() {
        let t = &golden[i];
        // Lucene's first token has increment 1 from the (virtual) position -1.
        position = if first {
            t.pos_inc - 1
        } else {
            position + t.pos_inc
        };
        first = false;
        let mut text = t.text.clone();
        if let Some(high) = t.lone_surrogate {
            let low = golden.get(i + 1).and_then(|n| n.lone_surrogate);
            let (Some(low), true) = (low, (0xD800..=0xDBFF).contains(&high)) else {
                panic!("lone surrogate token {t:?} not followed by its other half");
            };
            assert!(
                (0xDC00..=0xDFFF).contains(&low),
                "{high:#x} followed by {low:#x}"
            );
            let second = &golden[i + 1];
            assert_eq!(
                second.start, second.end,
                "second half must have an empty range"
            );
            assert_eq!(second.start, t.end);
            assert_eq!(second.pos_inc, 1);
            let cp = 0x10000 + ((u32::from(high) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
            text = char::from_u32(cp).unwrap().to_string();
            i += 1; // skip the second half; its position is simply never used
        }
        toks.push(Tok {
            text,
            byte_range: t.start..t.end,
            position,
        });
        i += 1;
    }
    toks
}

/// Reads a sentence golden: one list of boundary offsets per case.
pub(super) fn read_sentence_golden(contents: &str) -> Vec<Vec<usize>> {
    let mut cases: Vec<Vec<usize>> = Vec::new();
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
        let boundaries = line.split(' ').map(|n| n.parse().unwrap()).collect();
        *cases
            .last_mut()
            .expect("boundaries before first case header") = boundaries;
    }
    cases
}

// ------------------------------------------------------------------------------------------------
// Comparing

/// How token texts are compared.
#[derive(Clone, Copy)]
pub(super) enum TextMatch {
    Exact,
    /// Lucene stems with Porter, the port with Porter2, so for a token of ASCII letters the
    /// expected text is instead Porter2 applied to Lucene's normalization of the raw token
    /// (fullwidth folded, lowercased); offsets and positions must still match the golden.
    StemsWithPorter2,
}

fn toks_match(input: &str, expected: &Tok, actual: &Tok, text_match: TextMatch) -> bool {
    if expected.byte_range != actual.byte_range || expected.position != actual.position {
        return false;
    }
    if expected.text == actual.text {
        return true;
    }
    let is_letters = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphabetic());
    if !matches!(text_match, TextMatch::StemsWithPorter2) || !is_letters(&expected.text) {
        return false;
    }
    let raw = &input[expected.byte_range.clone()];
    let normalized: String = raw
        .chars()
        .map(|c| match c as u32 {
            0xFF21..=0xFF3A | 0xFF41..=0xFF5A => char::from_u32(c as u32 - 0xFEE0).unwrap(),
            _ => c,
        })
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let porter2 = rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English);
    actual.text == porter2.stem(&normalized)
}

fn fmt_toks(toks: &[Tok]) -> String {
    const LIMIT: usize = 60;
    let mut s = String::new();
    for (i, t) in toks.iter().enumerate() {
        if i == LIMIT {
            write!(s, " … ({} more)", toks.len() - LIMIT).unwrap();
            break;
        }
        if i > 0 {
            s.push_str("  ");
        }
        write!(s, "[{t}]").unwrap();
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
    text_match: TextMatch,
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
        let expected = golden_to_toks(golden);
        let actual = run(input);
        let matches = expected.len() == actual.len()
            && expected
                .iter()
                .zip(&actual)
                .all(|(e, a)| toks_match(input, e, a, text_match));
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
            .position(|(e, a)| !toks_match(input, e, a, text_match))
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
        writeln!(
            report,
            "  expected: {} tokens: {}",
            expected.len(),
            fmt_toks(&expected)
        )
        .unwrap();
        writeln!(
            report,
            "  actual:   {} tokens: {}",
            actual.len(),
            fmt_toks(&actual)
        )
        .unwrap();
    }
    assert!(
        failures == 0,
        "{name}: {failures} of {} cases differ from the Java reference\n{report}",
        inputs.len()
    );
}
