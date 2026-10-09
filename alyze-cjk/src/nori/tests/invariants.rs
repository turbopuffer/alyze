//! Structural properties that hold for every input, checked without goldens on the fuzz and
//! Wikipedia inputs (so they also run in the large differential setups).

use super::{
    Filter, TokenizerConfig, collect, read_cases, read_testdata, run_chain, run_tokenizer,
};
use crate::nori::{self, AnalyzerOptions, DecompoundMode, Tokens, pos};

fn inputs() -> Vec<String> {
    let mut inputs = read_cases(&read_testdata("cases/edge.txt"));
    inputs.extend(read_cases(&read_testdata("cases/fuzz.txt")));
    inputs.extend(read_cases(&read_testdata("cases/wiki_ko.txt")));
    inputs
}

/// With no decompounding and punctuation kept, the tokens tile the input exactly and every token's
/// text is its slice of the input.
#[test]
fn undecompounded_tokens_tile_the_input() {
    let config = TokenizerConfig {
        decompound: DecompoundMode::None,
        keep_punctuation: true,
        ..TokenizerConfig::DEFAULT
    };
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

/// Offsets never go backwards, tokens are never empty, and each decomposed morpheme stays inside
/// the input. In `Mixed` mode the output is a graph: increments are 0 or 1 and a whole entry spans
/// exactly the positions of its morphemes.
#[test]
fn offsets_and_positions_are_well_formed() {
    for config in [
        "default",
        "mixed",
        "punct",
        "mixed_punct",
        "unigrams",
        "userdict",
    ] {
        let config = TokenizerConfig::named(config);
        for input in inputs() {
            let toks = run_tokenizer(&input, config);
            let mut last_start = 0;
            for (i, t) in toks.iter().enumerate() {
                assert!(
                    t.byte_range.start >= last_start,
                    "token {i} starts before the previous one"
                );
                assert!(!t.text.is_empty(), "empty text at token {i}");
                last_start = t.byte_range.start;
                if config.decompound == DecompoundMode::Mixed {
                    if t.position_length > 1 {
                        // The whole entry: the next `position_length` tokens are its morphemes,
                        // the first at the same position, the last ending at the same offset.
                        let morphemes = &toks[i + 1..i + 1 + t.position_length as usize];
                        assert_eq!(morphemes[0].position, t.position);
                        assert_eq!(
                            morphemes.last().unwrap().position,
                            t.position + t.position_length as usize - 1
                        );
                        assert_eq!(morphemes.last().unwrap().byte_range.end, t.byte_range.end);
                        assert!(!t.morphemes.is_empty());
                    }
                } else {
                    assert_eq!(t.position, i, "positions are dense without decompounding");
                    assert_eq!(t.position_length, 1);
                    if config.decompound == DecompoundMode::Discard {
                        assert!(
                            t.morphemes.is_empty(),
                            "decomposed tokens carry no morphemes"
                        );
                    }
                }
            }
        }
    }
}

/// With punctuation discarded, no token is punctuation; in `Discard` mode no token is a compound.
#[test]
fn discarded_things_are_gone() {
    for input in inputs() {
        let toks = run_tokenizer(&input, TokenizerConfig::DEFAULT);
        for t in &toks {
            assert_ne!(t.pos_type, pos::Type::Compound);
            assert_ne!(t.pos_type, pos::Type::Inflect);
            assert_ne!(t.pos_type, pos::Type::Preanalysis);
            assert_ne!(t.left_pos, pos::Tag::SP, "space token survived: {t}");
        }
    }
}

/// The part-of-speech stop filter removes exactly the stop-tagged tokens and keeps the positions
/// of the removed ones as gaps.
#[test]
fn part_of_speech_stop_leaves_gaps() {
    let stop = pos::TagSet::DEFAULT_STOP_TAGS;
    let config = TokenizerConfig::named("punct");
    for input in inputs() {
        let before = run_tokenizer(&input, config);
        let after = run_chain(&input, config, &[Filter::PosStop(stop)]);
        let expected: Vec<_> = before
            .iter()
            .filter(|t| !stop.contains(t.left_pos))
            .cloned()
            .collect();
        assert_eq!(
            after,
            expected,
            "stop filter on {:?}",
            super::escape(&input)
        );
    }
}

/// The analyzer never panics and never emits a stop-tagged token.
#[test]
fn analyzer_runs_everywhere() {
    let mut tokens = Tokens::new();
    for input in inputs() {
        nori::analyze(&input, AnalyzerOptions::default(), &mut tokens);
        for t in collect(&input, &tokens) {
            assert!(!pos::TagSet::DEFAULT_STOP_TAGS.contains(t.left_pos), "{t}");
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
        "뿌리가깊은나무는바람에".repeat(20_000),
        "a".repeat(300_000),
    ] {
        nori::tokenize(&input, nori::Options::default(), &mut tokens);
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
        nori::tokenize(
            &input,
            TokenizerConfig::named("mixed_punct").options(),
            &mut shared,
        );
        let mut fresh = Tokens::new();
        nori::tokenize(
            &input,
            TokenizerConfig::named("mixed_punct").options(),
            &mut fresh,
        );
        assert_eq!(collect(&input, &shared), collect(&input, &fresh));
    }
}

/// Tokenizing the same text twice gives the same result (no state leaks between calls).
#[test]
fn deterministic() {
    for input in read_cases(&read_testdata("cases/upstream.txt")) {
        let a = run_tokenizer(&input, TokenizerConfig::named("userdict_mixed_punct"));
        let b = run_tokenizer(&input, TokenizerConfig::named("userdict_mixed_punct"));
        assert_eq!(a, b);
    }
}
