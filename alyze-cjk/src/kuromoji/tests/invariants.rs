//! Structural properties that hold for every input, checked without goldens on the edge, fuzz
//! and Wikipedia inputs (so they also run in the large differential setups).

use super::{
    Filter, TokenizerConfig, collect, read_cases, read_testdata, run_chain, run_tokenizer,
};
use crate::kuromoji::filter::{StopTags, StopWords};
use crate::kuromoji::{self, AnalyzerOptions, Mode, TokenKind, Tokens};

fn inputs() -> Vec<String> {
    let mut inputs = read_cases(&read_testdata("cases/edge.txt"));
    inputs.extend(read_cases(&read_testdata("cases/long.txt")));
    inputs.extend(read_cases(&read_testdata("cases/fuzz.txt")));
    inputs.extend(read_cases(&read_testdata("cases/wiki_ja.txt")));
    inputs
}

/// In normal mode with punctuation kept, the tokens tile the input exactly and every token's text
/// is its slice of the input.
#[test]
fn normal_mode_tokens_tile_the_input() {
    let config = TokenizerConfig::named("normal_punct");
    for input in inputs() {
        let toks = run_tokenizer(&input, config);
        let mut end = 0;
        for (i, t) in toks.iter().enumerate() {
            assert_eq!(
                t.byte_range.start,
                end,
                "gap before token {i} in {:?}",
                super::escape(&input)
            );
            assert!(t.byte_range.end > t.byte_range.start, "empty token {i}");
            assert_eq!(t.text, &input[t.byte_range.clone()], "token {i} text");
            assert_eq!(t.position, i);
            assert_eq!(t.position_length, 1);
            end = t.byte_range.end;
        }
        assert_eq!(
            end,
            input.len(),
            "tokens don't reach the end of {:?}",
            super::escape(&input)
        );
    }
}

/// Offsets never go backwards, tokens are never empty and their text is always the input slice
/// (no filter rewrites it). In search mode without n-best the output is a graph: a compound
/// (position length > 1) follows its first part at the same position, and its last part ends
/// where it ends; otherwise positions are dense. With an n-best cost, position lengths follow
/// from the unique token edges of each backtrace window, so only monotonic positions can be
/// checked.
#[test]
fn offsets_and_positions_are_well_formed() {
    for name in [
        "default",
        "normal",
        "extended",
        "nocompound",
        "punct",
        "extended_punct",
        "nbest",
        "userdict",
    ] {
        let config = TokenizerConfig::named(name);
        for input in inputs() {
            let toks = run_tokenizer(&input, config);
            let mut last_start = 0;
            for (i, t) in toks.iter().enumerate() {
                // In extended mode a compound whose first part is an unknown word is served
                // after that part's unigrams, so its start offset goes backwards (and, as its
                // start differs from the last token's, Lucene gives it increment 1 and length 1).
                assert!(
                    t.byte_range.start >= last_start
                        || (config.mode == Mode::Extended
                            && i > 0
                            && toks[i - 1].kind == Some(TokenKind::Unknown)),
                    "{name}: token {i} starts before the previous one: {} then {t} in {:?}",
                    toks[i - 1],
                    super::escape(&input)
                );
                assert!(!t.text.is_empty(), "{name}: empty text at token {i}");
                assert_eq!(
                    t.text,
                    &input[t.byte_range.clone()],
                    "{name}: token {i} text"
                );
                last_start = t.byte_range.start;
                let end = t.position + t.position_length as usize;
                if config.nbest_cost > 0 {
                    // Position lengths come from the distinct token edges of a backtrace window,
                    // and an edge nobody starts at still counts (Lucene), so a token can end past
                    // the next token's position; only monotonicity holds.
                    assert!(
                        toks.get(i + 1).is_none_or(|u| u.position >= t.position),
                        "{name}: positions go backwards at token {i}"
                    );
                } else if t.position_length > 1 {
                    assert!(
                        config.mode != Mode::Normal && !config.discard_compound,
                        "{name}: compound token emitted"
                    );
                    assert!(
                        i > 0 && toks[i - 1].position == t.position,
                        "{name}: compound {i}"
                    );
                    assert_eq!(toks[i - 1].byte_range.start, t.byte_range.start);
                    let last_part = toks[i + 1..]
                        .iter()
                        .find(|u| u.position == end - 1)
                        .unwrap_or_else(|| panic!("{name}: compound {i} has no last part"));
                    assert_eq!(last_part.byte_range.end, t.byte_range.end);
                } else if let Some(next) = toks.get(i + 1) {
                    assert!(
                        next.position == t.position + 1
                            || (next.position == t.position && next.position_length > 1),
                        "{name}: positions are dense ({i}: {t} then {next})"
                    );
                }
            }
        }
    }
}

/// Every token carries a part of speech; unknown words have no reading, pronunciation, base form
/// or inflection; known words always have a reading and a pronunciation.
#[test]
fn attributes_by_kind() {
    for input in inputs() {
        for t in run_tokenizer(&input, TokenizerConfig::named("punct")) {
            assert!(!t.part_of_speech.is_empty(), "{t}");
            match t.kind.unwrap() {
                TokenKind::Unknown => {
                    assert!(t.reading.is_none() && t.pronunciation.is_none(), "{t}");
                    assert!(t.base_form.is_none() && t.inflection_type.is_none(), "{t}");
                }
                TokenKind::Known => {
                    assert!(t.reading.is_some() && t.pronunciation.is_some(), "{t}");
                    assert_eq!(t.inflection_type.is_some(), t.inflection_form.is_some());
                }
                TokenKind::User => {
                    assert!(t.reading.is_some() && t.pronunciation.is_none(), "{t}");
                }
            }
        }
    }
}

/// With punctuation discarded, no token starts with a punctuation character.
#[test]
fn punctuation_is_gone() {
    use crate::kuromoji::unicode;
    for input in inputs() {
        for t in run_tokenizer(&input, TokenizerConfig::DEFAULT) {
            let first = t.text.encode_utf16().next().unwrap();
            assert!(
                !unicode::is_punctuation(unicode::category(first)),
                "punctuation token survived: {t}"
            );
        }
    }
}

/// Extended mode emits every unknown word as single characters, never cutting a surrogate pair
/// (an unknown compound can still appear whole, stacked over its unigrams).
#[test]
fn extended_mode_unigrams() {
    for input in inputs() {
        for t in run_tokenizer(&input, TokenizerConfig::named("extended")) {
            if t.kind == Some(TokenKind::Unknown) && t.position_length == 1 {
                assert_eq!(t.text.chars().count(), 1, "{t}");
            }
        }
    }
}

/// The stop filters remove exactly the stop-tagged / stop-word tokens and keep the positions of
/// the removed ones as gaps.
#[test]
fn stop_filters_leave_gaps() {
    let tags = StopTags::defaults();
    let words = StopWords::japanese();
    let config = TokenizerConfig::named("punct");
    for input in inputs() {
        let before = run_tokenizer(&input, config);
        let after = run_chain(&input, None, config, &[Filter::PosStop(tags.clone())]);
        let expected: Vec<_> = before
            .iter()
            .filter(|t| !tags.contains(&t.part_of_speech))
            .cloned()
            .collect();
        assert_eq!(after, expected, "pos stop on {:?}", super::escape(&input));
        let after = run_chain(&input, None, config, &[Filter::Stop(words.clone())]);
        let expected: Vec<_> = before
            .iter()
            .filter(|t| !words.contains(&t.text))
            .cloned()
            .collect();
        assert_eq!(after, expected, "stop words on {:?}", super::escape(&input));
    }
}

/// The analyzer never panics and never emits a stop-tagged token, a stop word, a compound, or a
/// token with uppercase ASCII.
#[test]
fn analyzer_runs_everywhere() {
    let tags = StopTags::defaults();
    let words = StopWords::japanese();
    let mut tokens = Tokens::new();
    for input in inputs() {
        kuromoji::analyze(&input, AnalyzerOptions::default(), &mut tokens);
        for t in collect(&input, &tokens) {
            assert!(!tags.contains(&t.part_of_speech), "{t}");
            assert!(!words.contains(&t.text), "{t}");
            assert_eq!(t.position_length, 1, "{t}");
            assert!(!t.text.chars().any(|c| c.is_ascii_uppercase()), "{t}");
        }
    }
}

/// The tokenizer's working memory is bounded by the lattice's live window, not the input: a
/// long input (with and without spaces) leaves a reused buffer with a small position ring and
/// trimmed per-input buffers.
#[test]
fn working_memory_is_bounded() {
    let mut tokens = Tokens::new();
    for input in [
        ". ".repeat(500_000),
        "吾輩は猫である名前はまだ無い".repeat(20_000),
        "a".repeat(300_000),
        "ア".repeat(300_000),
    ] {
        kuromoji::tokenize(&input, kuromoji::Options::default(), &mut tokens);
        let (slots, units) = tokens.scratch_footprint();
        assert!(
            slots <= 8192,
            "{slots} position slots after {} bytes",
            input.len()
        );
        assert!(
            units <= 64 * 1024,
            "{units} code units retained after {} bytes",
            input.len()
        );
    }
}

/// Reusing one output buffer across inputs gives the same result as a fresh one.
#[test]
fn output_buffer_reuse() {
    let mut shared = Tokens::new();
    for input in read_cases(&read_testdata("cases/edge.txt")) {
        let options = TokenizerConfig::named("nbest").options();
        kuromoji::tokenize(&input, options, &mut shared);
        let mut fresh = Tokens::new();
        kuromoji::tokenize(&input, options, &mut fresh);
        assert_eq!(collect(&input, &shared), collect(&input, &fresh));
    }
}

/// Tokenizing the same text twice gives the same result (no state leaks between calls).
#[test]
fn deterministic() {
    for input in read_cases(&read_testdata("cases/upstream.txt")) {
        let a = run_tokenizer(&input, TokenizerConfig::named("userdict_extended_punct"));
        let b = run_tokenizer(&input, TokenizerConfig::named("userdict_extended_punct"));
        assert_eq!(a, b);
    }
}
