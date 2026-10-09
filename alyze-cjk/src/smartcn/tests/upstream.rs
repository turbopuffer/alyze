//! The cases from Lucene's `TestSmartChineseAnalyzer` / `TestHMMChineseTokenizerFactory` and the
//! Elasticsearch plugin's REST tests, transcribed with their expectations. Offsets in Lucene's
//! tests are UTF-16; they are converted here.

use super::{run_analyzer, run_tokenizer, utf16_to_byte_offset};

const SENTENCE: &str = "我购买了道具和服装。";
const SENTENCE_TOKENS: &[&str] = &["我", "购买", "了", "道具", "和", "服装"];

fn texts(toks: &[super::Tok]) -> Vec<&str> {
    toks.iter().map(|t| t.text.as_str()).collect()
}

fn assert_analyzes_to(input: &str, expected: &[&str]) {
    assert_eq!(
        texts(&run_analyzer(input)),
        expected,
        "analyzer output for {input:?}"
    );
}

fn assert_tokenizes_to(input: &str, expected: &[&str]) {
    assert_eq!(
        texts(&run_tokenizer(input)),
        expected,
        "tokenizer output for {input:?}"
    );
}

fn assert_offsets(toks: &[super::Tok], input: &str, starts: &[usize], ends: &[usize]) {
    let actual: Vec<(usize, usize)> = toks
        .iter()
        .map(|t| (t.byte_range.start, t.byte_range.end))
        .collect();
    let expected: Vec<(usize, usize)> = starts
        .iter()
        .zip(ends)
        .map(|(&s, &e)| {
            (
                utf16_to_byte_offset(input, s),
                utf16_to_byte_offset(input, e),
            )
        })
        .collect();
    assert_eq!(actual, expected, "offsets for {input:?}");
}

// --- TestSmartChineseAnalyzer -------------------------------------------------------------------

#[test]
fn chinese_stop_words_default() {
    assert_analyzes_to(SENTENCE, SENTENCE_TOKENS);
}

/// Two sentences: checks the sentence splitting + per-sentence segmentation chain.
#[test]
fn chinese_stop_words_default_two_phrases() {
    let expected = [SENTENCE_TOKENS, SENTENCE_TOKENS].concat();
    assert_analyzes_to("我购买了道具和服装。 我购买了道具和服装。", &expected);
}

/// Surrogate pairs must not be split (LUCENE-8325).
#[test]
fn surrogate_pair_character() {
    let chars = [
        "\u{2cb3b}",
        "\u{2cb4a}",
        "\u{2cb73}",
        "\u{2cb5b}",
        "\u{9fcf}",
        "\u{2b7fc}",
        "\u{2cb2d}",
        "\u{9fd4}",
    ];
    let input: String = chars.concat();
    assert_analyzes_to(&input, &chars);
    assert_tokenizes_to(&input, &chars);
}

/// Ideographic space as separator.
#[test]
fn chinese_stop_words_default_two_phrases_ideo_space() {
    let expected = [SENTENCE_TOKENS, SENTENCE_TOKENS].concat();
    assert_analyzes_to("我购买了道具和服装　我购买了道具和服装。", &expected);
}

/// Without the stop filter (i.e. the bare tokenizer) the IDEOGRAPHIC FULL STOP comes out as ",".
#[test]
fn chinese_stop_words_off() {
    let expected = [SENTENCE_TOKENS, &[","]].concat();
    assert_tokenizes_to(SENTENCE, &expected);
    assert_tokenizes_to(SENTENCE, &expected); // reuse
}

/// Position increments after a stopword (":" is punctuation, hence a stopword).
#[test]
fn chinese_stop_words_2() {
    let input = "Title:San";
    let toks = run_analyzer(input);
    assert_eq!(texts(&toks), ["titl", "san"]);
    assert_offsets(&toks, input, &[0, 6], &[5, 9]);
    let positions: Vec<usize> = toks.iter().map(|t| t.position).collect();
    assert_eq!(positions, [0, 2], "position increments {{1, 2}}");
}

#[test]
fn chinese_analyzer() {
    assert_analyzes_to(SENTENCE, SENTENCE_TOKENS);
}

/// English words are lowercased and stemmed.
#[test]
fn mixed_latin_chinese() {
    assert_analyzes_to(
        "我购买 Tests 了道具和服装",
        &["我", "购买", "test", "了", "道具", "和", "服装"],
    );
}

/// Numerics are their own tokens.
#[test]
fn numerics() {
    assert_analyzes_to(
        "我购买 Tests 了道具和服装1234",
        &["我", "购买", "test", "了", "道具", "和", "服装", "1234"],
    );
}

/// Fullwidth letters and digits are folded to halfwidth.
#[test]
fn full_width() {
    assert_analyzes_to(
        "我购买 Ｔｅｓｔｓ 了道具和服装１２３４",
        &["我", "购买", "test", "了", "道具", "和", "服装", "1234"],
    );
}

/// Presentation-form delimiters are removed.
#[test]
fn delimiters() {
    assert_analyzes_to(
        "我购买︱ Tests 了道具和服装",
        &["我", "购买", "test", "了", "道具", "和", "服装"],
    );
}

/// Other writing systems come out character by character.
#[test]
fn non_chinese() {
    assert_analyzes_to(
        "我购买 روبرتTests 了道具和服装",
        &[
            "我", "购买", "ر", "و", "ب", "ر", "ت", "test", "了", "道具", "和", "服装",
        ],
    );
}

/// Out-of-vocabulary names come out character by character.
#[test]
fn oov() {
    assert_analyzes_to(
        "优素福·拉扎·吉拉尼",
        &["优", "素", "福", "拉", "扎", "吉", "拉", "尼"],
    );
    assert_analyzes_to(
        "优素福拉扎吉拉尼",
        &["优", "素", "福", "拉", "扎", "吉", "拉", "尼"],
    );
}

#[test]
fn offsets() {
    let input = "我购买了道具和服装";
    let toks = run_analyzer(input);
    assert_eq!(texts(&toks), SENTENCE_TOKENS);
    assert_offsets(&toks, input, &[0, 1, 3, 4, 6, 7], &[1, 3, 4, 6, 7, 9]);
}

#[test]
fn reusable_token_stream() {
    let input = "我购买 Tests 了道具和服装";
    let toks = run_analyzer(input);
    assert_eq!(
        texts(&toks),
        ["我", "购买", "test", "了", "道具", "和", "服装"]
    );
    assert_offsets(
        &toks,
        input,
        &[0, 1, 4, 10, 11, 13, 14],
        &[1, 3, 9, 11, 13, 14, 16],
    );

    let toks = run_analyzer(SENTENCE);
    assert_eq!(texts(&toks), SENTENCE_TOKENS);
    assert_offsets(&toks, SENTENCE, &[0, 1, 3, 4, 6, 7], &[1, 3, 4, 6, 7, 9]);
}

/// LUCENE-3026: documents larger than the read buffer. Exact output is covered by the golden
/// tests (`cases/upstream.txt` has 1000-sentence versions); here just make sure nothing breaks.
/// The 1024-unit read buffer cuts a few sentences in two, which yields a few extra tokens.
#[test]
fn large_document() {
    let input = SENTENCE.repeat(5000);
    let toks = run_analyzer(&input);
    assert!(
        (6 * 5000..6 * 5000 + 100).contains(&toks.len()),
        "{} tokens",
        toks.len()
    );
}

/// LUCENE-3026: a sentence larger than the read buffer (no boundaries at all).
#[test]
fn large_sentence() {
    let input = "我购买了道具和服装".repeat(5000);
    let toks = run_analyzer(&input);
    assert!(toks.len() >= 6 * 5000 - 100, "{} tokens", toks.len());
}

// --- TestHMMChineseTokenizerFactory / Elasticsearch 10_basic.yml ----------------------------------

#[test]
fn tokenizer_simple() {
    assert_tokenizes_to(SENTENCE, &["我", "购买", "了", "道具", "和", "服装", ","]);
}

#[test]
fn es_tokenizer_and_analyzer() {
    let toks = run_tokenizer(SENTENCE);
    assert_eq!(toks.len(), 7);
    assert_eq!(
        texts(&toks),
        ["我", "购买", "了", "道具", "和", "服装", ","]
    );
    let toks = run_analyzer(SENTENCE);
    assert_eq!(toks.len(), 6);
    assert_eq!(texts(&toks), SENTENCE_TOKENS);
}

/// Elasticsearch 20_search.yml: indexing "我购买了道具和服装" and searching for 购买 must match,
/// i.e. the analyzer must produce 购买 as a token.
#[test]
fn es_search() {
    assert!(texts(&run_analyzer("我购买了道具和服装")).contains(&"购买"));
    assert_eq!(texts(&run_analyzer("购买")), ["购买"]);
}
