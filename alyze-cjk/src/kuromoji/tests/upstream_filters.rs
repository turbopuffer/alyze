//! The cases from Lucene's kuromoji filter and char-filter tests (`TestJapaneseBaseFormFilter`,
//! `TestJapaneseReadingFormFilter`, `TestJapaneseNumberFilter`,
//! `TestJapaneseIterationMarkCharFilter`, `TestJapaneseKatakanaStemFilter`, the uppercase
//! filters, the completion filter and analyzer, `TestKatakanaRomanizer`, `TestToStringUtil`, the
//! factory tests, and `TestCJKWidthCharFilter` / `TestCJKWidthFilter` from analysis/common),
//! plus the Elasticsearch plugin's unit tests, REST tests and docs examples, transcribed with
//! their expectations. Offsets in Lucene's tests are UTF-16; they are converted here.
//!
//! Left out, as for nori: the random-data tests (`checkRandomData`), `testEmptyTerm` (a
//! `KeywordTokenizer` on "" yields one empty term, which the port's tokenizer can't produce),
//! `testBogusArguments` (factory argument validation), and the `SetKeywordMarkerFilter` variants
//! (`testKeyword` of the base-form and stem filters, the keyword-marking analyzer of
//! `TestJapaneseNumberFilter.testName`), since the port has no keyword attribute.

use super::{
    CharFilter, Filter, Tok, TokenizerConfig, collect, java_lowercase_text, run_chain,
    run_completion, run_completion_unlowercased, utf16_to_byte_offset,
};
use crate::kuromoji::filter::{self, CompletionMode, DEFAULT_STOP_WORDS, StopTags, StopWords};
use crate::kuromoji::{Mode, TokenData, TokenKind, Tokens, char_filter, romaji};

fn texts(toks: &[Tok]) -> Vec<&str> {
    toks.iter().map(|t| t.text.as_str()).collect()
}

/// Lucene's `assertAnalyzesTo`: texts, then optionally UTF-16 start/end offsets and position
/// increments.
fn assert_analyzes_to(
    toks: &[Tok],
    input: &str,
    expected: &[&str],
    starts: Option<&[usize]>,
    ends: Option<&[usize]>,
    incs: Option<&[usize]>,
) {
    assert_eq!(texts(toks), expected, "texts for {input:?}");
    if let (Some(starts), Some(ends)) = (starts, ends) {
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
    if let Some(incs) = incs {
        let mut position = 0usize;
        let expected: Vec<usize> = incs
            .iter()
            .enumerate()
            .map(|(i, &inc)| {
                position = if i == 0 { inc - 1 } else { position + inc };
                position
            })
            .collect();
        let actual: Vec<usize> = toks.iter().map(|t| t.position).collect();
        assert_eq!(actual, expected, "positions for {input:?}");
    }
}

/// Lucene's `MockTokenizer(WHITESPACE, false)` (`whitespace`) or `MockTokenizer(KEYWORD, false)`:
/// the tokens the filter tests that don't involve the Japanese tokenizer start from, built
/// directly in a `Tokens` buffer with the crate-internal constructor. Every token is an unknown
/// word with no attributes, position increment and length 1, like the mock's.
fn mock_tokenize(input: &str, whitespace: bool) -> Tokens {
    let mut pieces: Vec<(usize, usize)> = Vec::new();
    if whitespace {
        let mut start = None;
        for (i, c) in input.char_indices() {
            if c.is_whitespace() {
                if let Some(s) = start.take() {
                    pieces.push((s, i));
                }
            } else if start.is_none() {
                start = Some(i);
            }
        }
        if let Some(s) = start {
            pieces.push((s, input.len()));
        }
    } else if !input.is_empty() {
        pieces.push((0, input.len()));
    }
    let mut tokens = Tokens::new();
    for (start, end) in pieces {
        let text = tokens.push_text(&input[start..end]);
        tokens.push(TokenData {
            text,
            byte_range: start..end,
            position_increment: 1,
            position_length: 1,
            kind: TokenKind::Unknown,
            part_of_speech: 0..0,
            base_form: None,
            reading: None,
            pronunciation: None,
            inflection_type: None,
            inflection_form: None,
        });
    }
    tokens
}

/// A mock tokenizer followed by `filters`.
fn run_mock(input: &str, whitespace: bool, filters: &[Filter]) -> Vec<Tok> {
    let mut tokens = mock_tokenize(input, whitespace);
    for filter in filters {
        filter.apply(&mut tokens);
    }
    collect(input, &tokens)
}

/// `JapaneseTokenizer(null, true, SEARCH)` and `JapaneseTokenizer(attr, null, true, SEARCH)`: the
/// four-and-fewer-argument constructors discard compound tokens, as does
/// `JapaneseTokenizerFactory` by default.
const LUCENE: TokenizerConfig = TokenizerConfig {
    discard_compound: true,
    ..TokenizerConfig::DEFAULT
};

/// `JapaneseTokenizer(attr, null, false, SEARCH)`: punctuation kept, compounds discarded.
const LUCENE_PUNCT: TokenizerConfig = TokenizerConfig {
    keep_punctuation: true,
    ..LUCENE
};

/// `JapaneseTokenizer(attr, null, false, false, SEARCH)`: punctuation and compounds kept.
const NUMBER_TOKENIZER: TokenizerConfig = TokenizerConfig {
    keep_punctuation: true,
    discard_compound: false,
    ..TokenizerConfig::DEFAULT
};

/// The completion filter tests' tokenizer: `JapaneseTokenizer(null, true, NORMAL)`.
const COMPLETION_TOKENIZER: TokenizerConfig = TokenizerConfig {
    mode: Mode::Normal,
    ..LUCENE
};

// --- TestJapaneseBaseFormFilter, TestJapaneseBaseFormFilterFactory -----------------------------

fn base_form_analyzer(input: &str) -> Vec<Tok> {
    run_chain(input, None, LUCENE, &[Filter::BaseForm])
}

/// `testBasics` of both the filter and the factory test.
#[test]
fn base_form_basics() {
    let input = "それはまだ実験段階にあります";
    assert_analyzes_to(
        &base_form_analyzer(input),
        input,
        &["それ", "は", "まだ", "実験", "段階", "に", "ある", "ます"],
        None,
        None,
        None,
    );
}

/// `testEnglish`.
#[test]
fn base_form_english() {
    let input = "this atest";
    assert_analyzes_to(
        &base_form_analyzer(input),
        input,
        &["this", "atest"],
        None,
        None,
        None,
    );
}

// --- TestJapaneseReadingFormFilter, TestJapaneseReadingFormFilterFactory -----------------------

/// `katakanaAnalyzer`.
fn katakana_analyzer(input: &str) -> Vec<Tok> {
    run_chain(
        input,
        None,
        LUCENE,
        &[Filter::ReadingForm { romaji: false }],
    )
}
/// `romajiAnalyzer`.
fn romaji_analyzer(input: &str) -> Vec<Tok> {
    run_chain(input, None, LUCENE, &[Filter::ReadingForm { romaji: true }])
}
/// The half-width variants: `CJKWidthFilter` (the token filter) between the tokenizer and the
/// reading-form filter.
fn half_width_analyzer(input: &str, romaji: bool) -> Vec<Tok> {
    run_chain(
        input,
        None,
        LUCENE,
        &[Filter::Width, Filter::ReadingForm { romaji }],
    )
}

/// `testKatakanaReadings`.
#[test]
fn reading_form_katakana_readings() {
    let input = "今夜はロバート先生と話した";
    assert_analyzes_to(
        &katakana_analyzer(input),
        input,
        &["コンヤ", "ハ", "ロバート", "センセイ", "ト", "ハナシ", "タ"],
        None,
        None,
        None,
    );
}

/// `testKatakanaReadingsHalfWidth` (ﾛﾊﾞｰﾄ is U+FF9B U+FF8A U+FF9E U+FF70 U+FF84).
#[test]
fn reading_form_katakana_readings_half_width() {
    let input = "今夜はﾛﾊﾞｰﾄ先生と話した";
    assert_analyzes_to(
        &half_width_analyzer(input, false),
        input,
        &["コンヤ", "ハ", "ロバート", "センセイ", "ト", "ハナシ", "タ"],
        None,
        None,
        None,
    );
}

/// `testKatakanaReadingsHiragana`: tokens without a reading fall back to the surface form
/// shifted to katakana.
#[test]
fn reading_form_katakana_readings_hiragana() {
    let input = "が ぎ ぐ げ ご ぁ ゔ";
    assert_analyzes_to(
        &katakana_analyzer(input),
        input,
        &["ガ", "ギ", "グ", "ゲ", "ゴ", "ァ", "ヴ"],
        None,
        None,
        None,
    );
}

/// `testRomajiReadings`.
#[test]
fn reading_form_romaji_readings() {
    let input = "今夜はロバート先生と話した";
    assert_analyzes_to(
        &romaji_analyzer(input),
        input,
        &["kon'ya", "ha", "robato", "sensei", "to", "hanashi", "ta"],
        None,
        None,
        None,
    );
}

/// `testRomajiReadingsHalfWidth`.
#[test]
fn reading_form_romaji_readings_half_width() {
    let input = "今夜はﾛﾊﾞｰﾄ先生と話した";
    assert_analyzes_to(
        &half_width_analyzer(input, true),
        input,
        &["kon'ya", "ha", "robato", "sensei", "to", "hanashi", "ta"],
        None,
        None,
        None,
    );
}

/// `testRomajiReadingsHiragana`.
#[test]
fn reading_form_romaji_readings_hiragana() {
    let input = "が ぎ ぐ げ ご ぁ ゔ";
    assert_analyzes_to(
        &romaji_analyzer(input),
        input,
        &["ga", "gi", "gu", "ge", "go", "a", "v"],
        None,
        None,
        None,
    );
}

/// `TestJapaneseReadingFormFilterFactory.testReadings`: the default factories (katakana).
#[test]
fn reading_form_factory_readings() {
    let input = "先ほどベルリンから来ました。";
    assert_analyzes_to(
        &katakana_analyzer(input),
        input,
        &["サキ", "ホド", "ベルリン", "カラ", "キ", "マシ", "タ"],
        None,
        None,
        None,
    );
}

// --- TestJapaneseNumberFilter, TestJapaneseNumberFilterFactory ---------------------------------
// Its analyzer keeps punctuation and compound tokens, then applies the number filter.

fn number_analyzer(input: &str) -> Vec<Tok> {
    run_chain(input, None, NUMBER_TOKENIZER, &[Filter::Number])
}

fn assert_numbers(input: &str, expected: &[&str]) {
    assert_eq!(
        texts(&number_analyzer(input)),
        expected,
        "number filter on {input:?}"
    );
}

/// A case whose whole input is one numeral run that normalizes to a single token: the chain
/// yields it, and so does `normalizeNumber` on the input text directly.
fn assert_normalizes(input: &str, expected: &str) {
    assert_numbers(input, &[expected]);
    assert_eq!(
        filter::normalize_number(input),
        expected,
        "normalize_number({input:?})"
    );
}

/// `testBasics`.
#[test]
fn number_basics() {
    let input = "本日十万二千五百円のワインを買った";
    assert_analyzes_to(
        &number_analyzer(input),
        input,
        &["本日", "102500", "円", "の", "ワイン", "を", "買っ", "た"],
        Some(&[0, 2, 8, 9, 10, 13, 14, 16]),
        Some(&[2, 8, 9, 10, 13, 14, 16, 17]),
        None,
    );
    let input = "昨日のお寿司は１０万円でした。";
    assert_analyzes_to(
        &number_analyzer(input),
        input,
        &[
            "昨日", "の", "お", "寿司", "は", "100000", "円", "でし", "た", "。",
        ],
        Some(&[0, 2, 3, 4, 6, 7, 10, 11, 13, 14]),
        Some(&[2, 3, 4, 6, 7, 10, 11, 13, 14, 15]),
        None,
    );
    let input = "アティリカの資本金は６００万円です";
    assert_analyzes_to(
        &number_analyzer(input),
        input,
        &[
            "アティリカ",
            "の",
            "資本",
            "金",
            "は",
            "6000000",
            "円",
            "です",
        ],
        Some(&[0, 5, 6, 8, 9, 10, 14, 15]),
        Some(&[5, 6, 8, 9, 10, 14, 15, 17]),
        None,
    );
}

/// `testVariants`.
#[test]
fn number_variants() {
    for input in [
        "3",
        "３",
        "三",
        "03",
        "０３",
        "〇三",
        "003",
        "００３",
        "〇〇三",
    ] {
        assert_normalizes(input, "3");
    }
    // "１０百": strange, but supported.
    for input in ["千", "1千", "１千", "一千", "一〇〇〇", "１０百"] {
        assert_normalizes(input, "1000");
    }
}

/// `testLargeVariants`.
#[test]
fn number_large_variants() {
    assert_normalizes("三五七八九", "35789");
    assert_normalizes("六百二万五千一", "6025001");
    assert_normalizes("兆六百万五千一", "1000006005001");
    assert_normalizes("十兆六百万五千一", "10000006005001");
    assert_normalizes("一京一", "10000000000000001");
    assert_normalizes("十京十", "100000000000000010");
    assert_normalizes("垓京兆億万千百十一", "100010001000100011111");
}

/// `testNegative`.
#[test]
fn number_negative() {
    assert_numbers("-100万", &["-", "1000000"]);
}

/// `testMixed`.
#[test]
fn number_mixed() {
    assert_normalizes("三千2百２十三", "3223");
    assert_normalizes("３２二三", "3223");
}

/// `testNininsankyaku`: unstacked tokens are normalized; a compound (stacked tokens) makes the
/// filter emit the tokens as they are.
#[test]
fn number_nininsankyaku() {
    assert_numbers("二", &["2"]);
    assert_numbers("二人", &["2", "人"]);
    assert_numbers("二人三", &["2", "人", "3"]);
    assert_numbers("二人三脚", &["二", "二人三脚", "人", "三", "脚"]);
}

/// `testFujiyaichinisanu`: stacked tokens with a numeral partial.
#[test]
fn number_fujiyaichinisanu() {
    assert_numbers("不二家一二三", &["不", "不二家", "二", "家", "123"]);
}

/// `testFunny`: oddities for inconsistent input.
#[test]
fn number_funny() {
    assert_normalizes("十十", "20");
    assert_normalizes("百百百", "300");
    assert_normalizes("千千千千", "4000");
}

/// `testKanjiArabic`: kanji numerals used as Arabic digits (with a head zero).
#[test]
fn number_kanji_arabic() {
    assert_normalizes(
        "〇一二三四五六七八九九八七六五四三二一〇",
        "1234567899876543210",
    );
    assert_normalizes("〇〇七", "7");
}

/// `testDoubleZero`.
#[test]
fn number_double_zero() {
    let input = "〇〇";
    assert_analyzes_to(
        &number_analyzer(input),
        input,
        &["0"],
        Some(&[0]),
        Some(&[2]),
        Some(&[1]),
    );
}

/// `testName`: 京一 is a name that normalizes to a number (the keyword-marking analyzer that
/// keeps it has no port counterpart).
#[test]
fn number_name() {
    let input = "田中京一";
    assert_analyzes_to(
        &number_analyzer(input),
        input,
        &["田中", "10000000000000001"],
        Some(&[0, 2]),
        Some(&[2, 4]),
        Some(&[1, 1]),
    );
}

/// `testDecimal` (full-width digits and U+FF0E).
#[test]
fn number_decimal() {
    assert_normalizes("１．２万３４５．６７", "12345.67");
}

/// `testDecimalPunctuation`.
#[test]
fn number_decimal_punctuation() {
    assert_numbers("３．２千円", &["3200", "円"]);
}

/// `testThousandSeparator`.
#[test]
fn number_thousand_separator() {
    assert_normalizes("4,647", "4647");
}

/// `testDecimalThousandSeparator`.
#[test]
fn number_decimal_thousand_separator() {
    assert_normalizes("4,647.0010", "4647.001");
}

/// `testCommaDecimalSeparator`: the comma is a thousands separator.
#[test]
fn number_comma_decimal_separator() {
    assert_normalizes("15,7", "157");
}

/// `testTrailingZeroStripping`.
#[test]
fn number_trailing_zero_stripping() {
    assert_normalizes("1000.1000", "1000.1");
    assert_normalizes("1000.0000", "1000");
}

/// `testEmpty`.
#[test]
fn number_empty() {
    assert_numbers("", &[]);
}

/// `testFunnyIssue` only checks analysis consistency (`checkAnalysisConsistency`) on this
/// input; here the chain runs and `collect`'s structural checks pass.
#[test]
fn number_funny_issue() {
    let input = "〇〇\u{302f}\u{3029}\u{3039}\u{3023}\u{3033}\u{302b}B";
    let toks = number_analyzer(input);
    assert!(!toks.is_empty());
}

/// `TestJapaneseNumberFilterFactory.testBasics`: `discardPunctuation=false`, otherwise the
/// factory defaults (compounds discarded). Note the half-width 1 followed by a full-width ０.
#[test]
fn number_factory_basics() {
    let toks = run_chain(
        "昨日のお寿司は1０万円でした。",
        None,
        LUCENE_PUNCT,
        &[Filter::Number],
    );
    assert_eq!(
        texts(&toks),
        [
            "昨日", "の", "お", "寿司", "は", "100000", "円", "でし", "た", "。"
        ]
    );
}

// --- TestJapaneseIterationMarkCharFilter, TestJapaneseIterationMarkCharFilterFactory -----------

/// `keywordAnalyzer`: the char filter's output as one term (`MockTokenizer(KEYWORD)`), so the
/// filtered text itself. Lucene's filter never changes the length, so the character count is
/// checked too.
fn iteration_mark_text(input: &str, kanji: bool, kana: bool) -> String {
    let filtered = char_filter::iteration_mark(input, kanji, kana);
    assert_eq!(
        filtered.text().chars().count(),
        input.chars().count(),
        "the iteration-mark filter changed the length of {input:?}"
    );
    filtered.text().to_owned()
}

fn assert_keyword(input: &str, expected: &str) {
    assert_eq!(
        iteration_mark_text(input, true, true),
        expected,
        "iteration marks in {input:?}"
    );
}

/// `japaneseAnalyzer`: the char filter, then `JapaneseTokenizer(attr, null, false, SEARCH)`.
fn iteration_mark_japanese(input: &str) -> Vec<Tok> {
    run_chain(
        input,
        Some(CharFilter::IterationMark {
            kanji: true,
            kana: true,
        }),
        LUCENE_PUNCT,
        &[],
    )
}

fn assert_japanese(input: &str, expected: &[&str]) {
    assert_eq!(
        texts(&iteration_mark_japanese(input)),
        expected,
        "iteration marks + tokenizer on {input:?}"
    );
}

/// `testKanji`.
#[test]
fn iteration_mark_kanji() {
    assert_keyword("時々", "時時");
    assert_japanese("時々", &["時時"]);
    assert_keyword("馬鹿々々しい", "馬鹿馬鹿しい");
    assert_japanese("馬鹿々々しい", &["馬鹿馬鹿しい"]);
}

/// `testKatakana`.
#[test]
fn iteration_mark_katakana() {
    assert_keyword("ミスヾ", "ミスズ");
    // "Side effect" in Lucene's words.
    assert_japanese("ミスヾ", &["ミ", "スズ"]);
}

/// `testHiragana`.
#[test]
fn iteration_mark_hiragana() {
    assert_keyword("おゝの", "おおの");
    assert_japanese("おゝの", &["お", "おの"]); // Side effect
    assert_keyword("みすゞ", "みすず");
    assert_japanese("みすゞ", &["みすず"]);
    assert_keyword("じゞ", "じじ");
    assert_japanese("じゞ", &["じじ"]);
    assert_keyword("じゝ", "じし");
    assert_japanese("じゝ", &["じし"]);
    assert_keyword("ところゞゝゝ", "ところどころ");
    assert_japanese("ところゞゝゝ", &["ところどころ"]);
}

/// `testMalformed`.
#[test]
fn iteration_mark_malformed() {
    // c can't be iterated, so it is emitted as it is.
    assert_keyword("abcところゝゝゝゝ", "abcところcところ");
    // Nor with a dakuten change.
    assert_keyword("abcところゞゝゝゝ", "abcところcところ");
    // Nothing to iterate before the beginning of the stream.
    assert_keyword("ところゞゝゝゞゝゞ", "ところどころゞゝゞ");
    // An iteration mark alone can't be iterated.
    assert_keyword("々", "々");
    assert_keyword("ゞ", "ゞ");
    assert_keyword("ゞゝ", "ゞゝ");
    // A full stop is the flush marker, so it can't be iterated.
    assert_keyword("。ゝ", "。ゝ");
    assert_keyword("。。ゝゝ", "。。ゝゝ");
    // Other punctuation can.
    assert_keyword("？ゝ", "？？");
    // No dakuten variant of ぽ (also a corner case for Lucene's `inside()`).
    assert_keyword("ねやぽゞつむぴ", "ねやぽぽつむぴ");
    assert_keyword("ねやぽゝつむぴ", "ねやぽぽつむぴ");
}

/// `testEmpty`.
#[test]
fn iteration_mark_empty() {
    assert_keyword("", "");
    assert_japanese("", &[]);
}

/// `testFullStop`.
#[test]
fn iteration_mark_full_stop() {
    assert_keyword("。", "。");
    assert_keyword("。。", "。。");
    assert_keyword("。。。", "。。。");
}

const ITERATION_MARK_MIXED: &str =
    "時々、おゝのさんと一緒にお寿司が食べたいです。abcところゞゝゝ。";

/// `testKanjiOnly`.
#[test]
fn iteration_mark_kanji_only() {
    assert_eq!(
        iteration_mark_text(ITERATION_MARK_MIXED, true, false),
        "時時、おゝのさんと一緒にお寿司が食べたいです。abcところゞゝゝ。"
    );
}

/// `testKanaOnly`.
#[test]
fn iteration_mark_kana_only() {
    assert_eq!(
        iteration_mark_text(ITERATION_MARK_MIXED, false, true),
        "時々、おおのさんと一緒にお寿司が食べたいです。abcところどころ。"
    );
}

/// `testNone`.
#[test]
fn iteration_mark_none() {
    assert_eq!(
        iteration_mark_text(ITERATION_MARK_MIXED, false, false),
        ITERATION_MARK_MIXED
    );
}

/// `testCombinations`.
#[test]
fn iteration_mark_combinations() {
    assert_keyword(
        "時々、おゝのさんと一緒にお寿司を食べに行きます。",
        "時時、おおのさんと一緒にお寿司を食べに行きます。",
    );
}

/// `testHiraganaCoverage`: every hiragana iteration variant, plain and with dakuten.
#[test]
fn iteration_mark_hiragana_coverage() {
    assert_keyword(
        "かゝがゝきゝぎゝくゝぐゝけゝげゝこゝごゝさゝざゝしゝじゝすゝずゝせゝぜゝそゝぞゝたゝだゝちゝぢゝつゝづゝてゝでゝとゝどゝはゝばゝひゝびゝふゝぶゝへゝべゝほゝぼゝ",
        "かかがかききぎきくくぐくけけげけここごこささざさししじしすすずすせせぜせそそぞそたただたちちぢちつつづつててでてととどとははばはひひびひふふぶふへへべへほほぼほ",
    );
    assert_keyword(
        "かゞがゞきゞぎゞくゞぐゞけゞげゞこゞごゞさゞざゞしゞじゞすゞずゞせゞぜゞそゞぞゞたゞだゞちゞぢゞつゞづゞてゞでゞとゞどゞはゞばゞひゞびゞふゞぶゞへゞべゞほゞぼゞ",
        "かがががきぎぎぎくぐぐぐけげげげこごごごさざざざしじじじすずずずせぜぜぜそぞぞぞただだだちぢぢぢつづづづてでででとどどどはばばばひびびびふぶぶぶへべべべほぼぼぼ",
    );
}

/// `testKatakanaCoverage`.
#[test]
fn iteration_mark_katakana_coverage() {
    assert_keyword(
        "カヽガヽキヽギヽクヽグヽケヽゲヽコヽゴヽサヽザヽシヽジヽスヽズヽセヽゼヽソヽゾヽタヽダヽチヽヂヽツヽヅヽテヽデヽトヽドヽハヽバヽヒヽビヽフヽブヽヘヽベヽホヽボヽ",
        "カカガカキキギキククグクケケゲケココゴコササザサシシジシススズスセセゼセソソゾソタタダタチチヂチツツヅツテテデテトトドトハハバハヒヒビヒフフブフヘヘベヘホホボホ",
    );
    assert_keyword(
        "カヾガヾキヾギヾクヾグヾケヾゲヾコヾゴヾサヾザヾシヾジヾスヾズヾセヾゼヾソヾゾヾタヾダヾチヾヂヾツヾヅヾテヾデヾトヾドヾハヾバヾヒヾビヾフヾブヾヘヾベヾホヾボヾ",
        "カガガガキギギギクグググケゲゲゲコゴゴゴサザザザシジジジスズズズセゼゼゼソゾゾゾタダダダチヂヂヂツヅヅヅテデデデトドドドハバババヒビビビフブブブヘベベベホボボボ",
    );
}

const ITERATION_MARK_FACTORY_INPUT: &str = "時々馬鹿々々しいところゞゝゝミスヾ";

/// `TestJapaneseIterationMarkCharFilterFactory.testIterationMarksWithKeywordTokenizer`.
#[test]
fn iteration_mark_factory_keyword_tokenizer() {
    assert_keyword(
        ITERATION_MARK_FACTORY_INPUT,
        "時時馬鹿馬鹿しいところどころミスズ",
    );
}

/// `testIterationMarksWithJapaneseTokenizer`: the default factories.
#[test]
fn iteration_mark_factory_japanese_tokenizer() {
    let toks = run_chain(
        ITERATION_MARK_FACTORY_INPUT,
        Some(CharFilter::IterationMark {
            kanji: true,
            kana: true,
        }),
        LUCENE,
        &[],
    );
    assert_eq!(
        texts(&toks),
        ["時時", "馬鹿馬鹿しい", "ところどころ", "ミ", "スズ"]
    );
}

/// `testKanjiOnlyIterationMarksWithJapaneseTokenizer`.
#[test]
fn iteration_mark_factory_kanji_only_japanese_tokenizer() {
    let toks = run_chain(
        ITERATION_MARK_FACTORY_INPUT,
        Some(CharFilter::IterationMark {
            kanji: true,
            kana: false,
        }),
        LUCENE,
        &[],
    );
    assert_eq!(
        texts(&toks),
        [
            "時時",
            "馬鹿馬鹿しい",
            "ところ",
            "ゞ",
            "ゝ",
            "ゝ",
            "ミス",
            "ヾ"
        ]
    );
}

/// `testKanaOnlyIterationMarksWithJapaneseTokenizer`.
#[test]
fn iteration_mark_factory_kana_only_japanese_tokenizer() {
    let toks = run_chain(
        ITERATION_MARK_FACTORY_INPUT,
        Some(CharFilter::IterationMark {
            kanji: false,
            kana: true,
        }),
        LUCENE,
        &[],
    );
    assert_eq!(
        texts(&toks),
        [
            "時々",
            "馬鹿",
            "々",
            "々",
            "しい",
            "ところどころ",
            "ミ",
            "スズ"
        ]
    );
}

// --- TestJapaneseKatakanaStemFilter, TestJapaneseKatakanaStemFilterFactory ---------------------
// The filter test uses MockTokenizer(WHITESPACE) with the default minimum length (4).

/// `testStemVariants`: copy, coffee, taxi, party, party (without long sound), center. コピー is
/// below the minimum length; the long sound of コーヒー is removed although it is required.
#[test]
fn katakana_stem_variants() {
    let input = "コピー コーヒー タクシー パーティー パーティ センター";
    assert_analyzes_to(
        &run_mock(input, true, &[Filter::Stem(4)]),
        input,
        &[
            "コピー",
            "コーヒ",
            "タクシ",
            "パーティ",
            "パーティ",
            "センタ",
        ],
        Some(&[0, 4, 9, 14, 20, 25]),
        Some(&[3, 8, 13, 19, 24, 29]),
        None,
    );
}

/// `testUnsupportedHalfWidthVariants`: only full-width katakana is stemmed.
#[test]
fn katakana_stem_unsupported_half_width_variants() {
    let input = "ﾀｸｼｰ";
    assert_analyzes_to(
        &run_mock(input, true, &[Filter::Stem(4)]),
        input,
        &["ﾀｸｼｰ"],
        None,
        None,
        None,
    );
}

/// `TestJapaneseKatakanaStemFilterFactory.testKatakanaStemming`: the default factories.
/// パーティー is stemmed, コピー is not.
#[test]
fn katakana_stem_factory() {
    let toks = run_chain(
        "明後日パーティーに行く予定がある。図書館で資料をコピーしました。",
        None,
        LUCENE,
        &[Filter::Stem(4)],
    );
    assert_eq!(
        texts(&toks),
        [
            "明後日",
            "パーティ",
            "に",
            "行く",
            "予定",
            "が",
            "ある",
            "図書館",
            "で",
            "資料",
            "を",
            "コピー",
            "し",
            "まし",
            "た"
        ]
    );
}

// --- TestJapaneseKatakanaUppercaseFilter, TestJapaneseKatakanaUppercaseFilterFactory -----------
// `keywordAnalyzer` is MockTokenizer(WHITESPACE) + the filter; `japaneseAnalyzer` is
// JapaneseTokenizer(attr, null, false, SEARCH) + the filter.

fn katakana_uppercase_keyword(input: &str) -> Vec<Tok> {
    run_mock(input, true, &[Filter::KatakanaUppercase])
}

/// `testKanaUppercase` (ㇷ゚ is U+31F7 U+309A and becomes the single character プ).
#[test]
fn katakana_uppercase_kana() {
    let input = "ァィゥェォヵㇰヶㇱㇲッㇳㇴㇵㇶㇷㇷ゚ㇸㇹㇺャュョㇻㇼㇽㇾㇿヮ";
    assert_analyzes_to(
        &katakana_uppercase_keyword(input),
        input,
        &["アイウエオカクケシスツトヌハヒフプヘホムヤユヨラリルレロワ"],
        None,
        None,
        None,
    );
    let input = "ストップウォッチ";
    assert_analyzes_to(
        &katakana_uppercase_keyword(input),
        input,
        &["ストツプウオツチ"],
        None,
        None,
        None,
    );
    let input = "サラニㇷ゚ カムイチェㇷ゚ ㇷ゚ㇷ゚";
    assert_analyzes_to(
        &katakana_uppercase_keyword(input),
        input,
        &["サラニプ", "カムイチエプ", "ププ"],
        None,
        None,
        None,
    );
    let input = "カムイチェㇷ゚カムイチェ";
    assert_analyzes_to(
        &katakana_uppercase_keyword(input),
        input,
        &["カムイチエプカムイチエ"],
        None,
        None,
        None,
    );
}

/// `testKanaUppercaseWithSurrogatePair` (𠀋 is U+2000B, a surrogate pair in Java).
#[test]
fn katakana_uppercase_with_surrogate_pair() {
    let input = "𠀋ストップウォッチ ストップ𠀋ウォッチ ストップウォッチ𠀋";
    assert_analyzes_to(
        &katakana_uppercase_keyword(input),
        input,
        &[
            "𠀋ストツプウオツチ",
            "ストツプ𠀋ウオツチ",
            "ストツプウオツチ𠀋",
        ],
        None,
        None,
        None,
    );
}

/// `testKanaUppercaseWithJapaneseTokenizer`.
#[test]
fn katakana_uppercase_with_japanese_tokenizer() {
    let input = "時間をストップウォッチで測る";
    assert_analyzes_to(
        &run_chain(input, None, LUCENE_PUNCT, &[Filter::KatakanaUppercase]),
        input,
        &["時間", "を", "ストツプウオツチ", "で", "測る"],
        None,
        None,
        None,
    );
}

/// `testUnsupportedHalfWidthVariants`: only full-width katakana is supported.
#[test]
fn katakana_uppercase_unsupported_half_width_variants() {
    let input = "ｽﾄｯﾌﾟｳｫｯﾁ";
    assert_analyzes_to(
        &katakana_uppercase_keyword(input),
        input,
        &["ｽﾄｯﾌﾟｳｫｯﾁ"],
        None,
        None,
        None,
    );
}

/// `testEmptyTerm` of the uppercase filters is really an empty-input test on the whitespace
/// tokenizer: no tokens.
#[test]
fn katakana_uppercase_empty_term() {
    assert_analyzes_to(&katakana_uppercase_keyword(""), "", &[], None, None, None);
}

/// `TestJapaneseKatakanaUppercaseFilterFactory.testBasics`: `discardPunctuation=false`.
#[test]
fn katakana_uppercase_factory() {
    let toks = run_chain(
        "ストップウォッチ",
        None,
        LUCENE_PUNCT,
        &[Filter::KatakanaUppercase],
    );
    assert_eq!(texts(&toks), ["ストツプウオツチ"]);
}

// --- TestJapaneseHiraganaUppercaseFilter, TestJapaneseHiraganaUppercaseFilterFactory -----------

fn hiragana_uppercase_keyword(input: &str) -> Vec<Tok> {
    run_mock(input, true, &[Filter::HiraganaUppercase])
}

/// `testKanaUppercase`.
#[test]
fn hiragana_uppercase_kana() {
    let input = "ぁぃぅぇぉっゃゅょゎゕゖ";
    assert_analyzes_to(
        &hiragana_uppercase_keyword(input),
        input,
        &["あいうえおつやゆよわかけ"],
        None,
        None,
        None,
    );
    let input = "ちょっとまって";
    assert_analyzes_to(
        &hiragana_uppercase_keyword(input),
        input,
        &["ちよつとまつて"],
        None,
        None,
        None,
    );
}

/// `testKanaUppercaseWithSurrogatePair`.
#[test]
fn hiragana_uppercase_with_surrogate_pair() {
    let input = "𠀋ちょっとまって ちょっと𠀋まって ちょっとまって𠀋";
    assert_analyzes_to(
        &hiragana_uppercase_keyword(input),
        input,
        &["𠀋ちよつとまつて", "ちよつと𠀋まつて", "ちよつとまつて𠀋"],
        None,
        None,
        None,
    );
}

/// `testKanaUppercaseWithJapaneseTokenizer`.
#[test]
fn hiragana_uppercase_with_japanese_tokenizer() {
    let input = "ちょっとまって";
    assert_analyzes_to(
        &run_chain(input, None, LUCENE_PUNCT, &[Filter::HiraganaUppercase]),
        input,
        &["ちよつと", "まつ", "て"],
        None,
        None,
        None,
    );
}

/// `testEmptyTerm`: no tokens from an empty input.
#[test]
fn hiragana_uppercase_empty_term() {
    assert_analyzes_to(&hiragana_uppercase_keyword(""), "", &[], None, None, None);
}

/// `TestJapaneseHiraganaUppercaseFilterFactory.testBasics`: `discardPunctuation=false`.
#[test]
fn hiragana_uppercase_factory() {
    let toks = run_chain(
        "ちょっとまって",
        None,
        LUCENE_PUNCT,
        &[Filter::HiraganaUppercase],
    );
    assert_eq!(texts(&toks), ["ちよつと", "まつ", "て"]);
}

// --- TestJapanesePartOfSpeechStopFilterFactory -------------------------------------------------

/// `testBasics`: the tags file is "#  verb-main:\n動詞-自立\n" (the comment line is stripped by
/// Lucene's word-list loader); 超える is removed, 。 was already discarded by the tokenizer.
#[test]
fn pos_stop_factory_basics() {
    let toks = run_chain(
        "私は制限スピードを超える。",
        None,
        LUCENE,
        &[Filter::PosStop(StopTags::new(["動詞-自立"]))],
    );
    assert_eq!(texts(&toks), ["私", "は", "制限", "スピード", "を"]);
}

/// `testNoTagsSpecified`: the default `stoptags.txt`.
#[test]
fn pos_stop_factory_no_tags_specified() {
    let toks = run_chain(
        "私は制限スピードを超える。",
        None,
        LUCENE,
        &[Filter::PosStop(StopTags::defaults())],
    );
    assert_eq!(texts(&toks), ["私", "制限", "スピード", "超える"]);
}

// --- TestJapaneseCompletionFilter ----------------------------------------------------------------
// `CJKWidthCharFilter`, then `JapaneseTokenizer(null, true, NORMAL)`, then the completion filter.
// No lowercasing, unlike the analyzer.

fn completion_filter(input: &str, mode: CompletionMode) -> Vec<Tok> {
    run_chain(
        input,
        Some(CharFilter::CjkWidth),
        COMPLETION_TOKENIZER,
        &[Filter::Completion(mode)],
    )
}

/// `testCompletionIndex`.
#[test]
fn completion_index() {
    let index = CompletionMode::Index;
    let input = "東京";
    assert_analyzes_to(
        &completion_filter(input, index),
        input,
        &["東京", "toukyou"],
        Some(&[0, 0]),
        Some(&[2, 2]),
        Some(&[1, 0]),
    );
    let input = "東京都";
    assert_analyzes_to(
        &completion_filter(input, index),
        input,
        &["東京", "toukyou", "都", "to"],
        Some(&[0, 0, 2, 2]),
        Some(&[2, 2, 3, 3]),
        Some(&[1, 0, 1, 0]),
    );
    let input = "ドラえもん";
    assert_analyzes_to(
        &completion_filter(input, index),
        input,
        &["ドラえもん", "doraemon", "doraemonn"],
        Some(&[0, 0, 0]),
        Some(&[5, 5, 5]),
        Some(&[1, 0, 0]),
    );
    // The ー (U+30FC) is kept inside the romaji.
    let input = "ソースコード";
    assert_analyzes_to(
        &completion_filter(input, index),
        input,
        &["ソース", "soーsu", "コード", "koーdo"],
        Some(&[0, 0, 3, 3]),
        Some(&[3, 3, 6, 6]),
        Some(&[1, 0, 1, 0]),
    );
    let input = "反社会的勢力";
    assert_analyzes_to(
        &completion_filter(input, index),
        input,
        &[
            "反", "han", "hann", "社会", "syakai", "shakai", "的", "teki", "勢力", "seiryoku",
        ],
        Some(&[0, 0, 0, 1, 1, 1, 3, 3, 4, 4]),
        Some(&[1, 1, 1, 3, 3, 3, 4, 4, 6, 6]),
        Some(&[1, 0, 0, 1, 0, 0, 1, 0, 1, 0]),
    );
    let input = "々";
    assert_analyzes_to(
        &completion_filter(input, index),
        input,
        &["々"],
        Some(&[0]),
        Some(&[1]),
        Some(&[1]),
    );
    let input = "是々";
    assert_analyzes_to(
        &completion_filter(input, index),
        input,
        &["是", "ze", "々"],
        Some(&[0, 0, 1]),
        Some(&[1, 1, 2]),
        Some(&[1, 0, 1]),
    );
    let input = "是々の";
    assert_analyzes_to(
        &completion_filter(input, index),
        input,
        &["是", "ze", "々", "の", "no"],
        Some(&[0, 0, 1, 2, 2]),
        Some(&[1, 1, 2, 3, 3]),
        Some(&[1, 0, 1, 1, 0]),
    );
}

/// `testCompletionQuery`. (Its "是々の" assertion uses the index analyzer, a copy-paste quirk;
/// it is in [`completion_index`].)
#[test]
fn completion_query() {
    let query = CompletionMode::Query;
    let input = "東京";
    assert_analyzes_to(
        &completion_filter(input, query),
        input,
        &["東京", "toukyou"],
        Some(&[0, 0]),
        Some(&[2, 2]),
        Some(&[1, 0]),
    );
    let input = "東京都";
    assert_analyzes_to(
        &completion_filter(input, query),
        input,
        &["東京", "toukyou", "都", "to"],
        Some(&[0, 0, 2, 2]),
        Some(&[2, 2, 3, 3]),
        Some(&[1, 0, 1, 0]),
    );
    let input = "ドラえもん";
    assert_analyzes_to(
        &completion_filter(input, query),
        input,
        &["ドラえもん", "doraemon", "doraemonn"],
        Some(&[0, 0, 0]),
        Some(&[5, 5, 5]),
        Some(&[1, 0, 0]),
    );
    // Query mode concatenates the consecutive katakana tokens.
    let input = "ソースコード";
    assert_analyzes_to(
        &completion_filter(input, query),
        input,
        &["ソースコード", "soーsukoーdo"],
        Some(&[0, 0]),
        Some(&[6, 6]),
        Some(&[1, 0]),
    );
    let input = "反社会的勢力";
    assert_analyzes_to(
        &completion_filter(input, query),
        input,
        &[
            "反", "han", "hann", "社会", "syakai", "shakai", "的", "teki", "勢力", "seiryoku",
        ],
        Some(&[0, 0, 0, 1, 1, 1, 3, 3, 4, 4]),
        Some(&[1, 1, 1, 3, 3, 3, 4, 4, 6, 6]),
        Some(&[1, 0, 0, 1, 0, 0, 1, 0, 1, 0]),
    );
    let input = "々";
    assert_analyzes_to(
        &completion_filter(input, query),
        input,
        &["々"],
        Some(&[0]),
        Some(&[1]),
        Some(&[1]),
    );
    let input = "是々";
    assert_analyzes_to(
        &completion_filter(input, query),
        input,
        &["是", "ze", "々"],
        Some(&[0, 0, 1]),
        Some(&[1, 1, 2]),
        Some(&[1, 0, 1]),
    );
    // A trailing half-typed romaji syllable (full-width in the input, folded by the char filter)
    // is appended to the token before it.
    let input = "東京ｔ";
    assert_analyzes_to(
        &completion_filter(input, query),
        input,
        &["東京t", "toukyout"],
        Some(&[0, 0]),
        Some(&[3, 3]),
        Some(&[1, 0]),
    );
    let input = "サッｋ";
    assert_analyzes_to(
        &completion_filter(input, query),
        input,
        &["サッk", "sakk"],
        Some(&[0, 0]),
        Some(&[3, 3]),
        Some(&[1, 0]),
    );
    let input = "反ｓｙ";
    assert_analyzes_to(
        &completion_filter(input, query),
        input,
        &["反sy", "hansy", "hannsy"],
        Some(&[0, 0, 0]),
        Some(&[3, 3, 3]),
        Some(&[1, 0, 0]),
    );
    let input = "さーきゅｒ";
    assert_analyzes_to(
        &completion_filter(input, query),
        input,
        &["さーきゅr", "saーkyur"],
        Some(&[0, 0]),
        Some(&[5, 5]),
        Some(&[1, 0]),
    );
    let input = "是々ｈ";
    assert_analyzes_to(
        &completion_filter(input, query),
        input,
        &["是", "ze", "々h"],
        Some(&[0, 0, 1]),
        Some(&[1, 1, 3]),
        Some(&[1, 0, 1]),
    );
}

/// `testEnglish`, in both modes.
#[test]
fn completion_english() {
    let input = "this atest";
    for mode in [CompletionMode::Index, CompletionMode::Query] {
        assert_analyzes_to(
            &completion_filter(input, mode),
            input,
            &["this", "atest"],
            None,
            None,
            None,
        );
    }
}

// --- TestJapaneseCompletionAnalyzer, TestJapaneseCompletionFilterFactory -----------------------

/// `testCompletionDefault`: `new JapaneseCompletionAnalyzer()` (index mode).
#[test]
fn completion_analyzer_default() {
    let input = "東京";
    assert_analyzes_to(
        &run_completion(input, "index"),
        input,
        &["東京", "toukyou"],
        Some(&[0, 0]),
        Some(&[2, 2]),
        Some(&[1, 0]),
    );
}

/// `testCompletionQuery`: `JapaneseCompletionAnalyzer(null, QUERY)`. The analyzer ends with
/// `LowerCaseFilter`, so the port's unlowercased chain is compared through Java's lowercase.
#[test]
fn completion_analyzer_query() {
    let input = "東京ｔ";
    let toks: Vec<Tok> = run_completion_unlowercased(input, "query")
        .into_iter()
        .map(|t| Tok {
            text: java_lowercase_text(&t.text),
            ..t
        })
        .collect();
    assert_analyzes_to(
        &toks,
        input,
        &["東京t", "toukyout"],
        Some(&[0, 0]),
        Some(&[3, 3]),
        Some(&[1, 0]),
    );
}

/// `TestJapaneseCompletionFilterFactory.testCompletion`: the default tokenizer factory (search
/// mode, compounds discarded), `CJKWidthFilter` (the token filter), then the completion filter in
/// query mode.
#[test]
fn completion_factory() {
    let toks = run_chain(
        "東京ｔ",
        None,
        LUCENE,
        &[Filter::Width, Filter::Completion(CompletionMode::Query)],
    );
    assert_eq!(texts(&toks), ["東京t", "toukyout"]);
}

// --- completion/TestKatakanaRomanizer ----------------------------------------------------------
// Results are compared as unordered sets of equal size.

fn assert_keystrokes(katakana: &str, expected: &[&str]) {
    let mut actual = romaji::keystrokes(katakana);
    actual.sort();
    let mut expected: Vec<&str> = expected.to_vec();
    expected.sort();
    assert_eq!(actual, expected, "keystrokes of {katakana:?}");
}

/// `testRomanize`.
#[test]
fn katakana_romanizer_romanize() {
    assert_keystrokes("ハシ", &["hasi", "hashi"]);
    assert_keystrokes("ユウキュウ", &["yuukyuu"]);
    assert_keystrokes("ヤキュウ", &["yakyuu"]);
    assert_keystrokes("トウキョウ", &["toukyou"]);
    assert_keystrokes("トーキョー", &["toーkyoー"]);
    assert_keystrokes("サッカ", &["sakka"]);
    assert_keystrokes("ヒャッカテン", &["hyakkaten", "hyakkatenn"]);
    assert_keystrokes("ヴォルテール", &["voruteーru", "vuxoruteーru"]);
}

/// `testRomanizeWithAlphabets`: a trailing ASCII suffix is passed through.
#[test]
fn katakana_romanizer_with_alphabets() {
    assert_keystrokes("トウキョウt", &["toukyout"]);
    assert_keystrokes("コダッk", &["kodakk"]);
    assert_keystrokes("ショウsy", &["syousy", "shousy"]);
}

// --- dict/TestToStringUtil ---------------------------------------------------------------------
// `testPOS` (`getPOSTranslation`, the English names of parts of speech) has no port counterpart.

/// `testHepburn`: the long vowel mark ー is dropped.
#[test]
fn hepburn() {
    assert_eq!(romaji::hepburn("マージャン"), "majan");
    assert_eq!(romaji::hepburn("ウーロンチャ"), "uroncha");
    assert_eq!(romaji::hepburn("チャーハン"), "chahan");
    assert_eq!(romaji::hepburn("チャーシュー"), "chashu");
    assert_eq!(romaji::hepburn("シューマイ"), "shumai");
}

/// `testHepburnTable`. Lucene's own comment: "this isnt even thorough or really probably what we
/// want!" (ヴ maps to "v", with a TODO saying it should be "vu"). In ラ゜ .. ロ゜ the mark is the
/// spacing U+309C.
#[test]
fn hepburn_table() {
    const TABLE: &[(&str, &str)] = &[
        ("ア", "a"),
        ("イ", "i"),
        ("ウ", "u"),
        ("エ", "e"),
        ("オ", "o"),
        ("カ", "ka"),
        ("キ", "ki"),
        ("ク", "ku"),
        ("ケ", "ke"),
        ("コ", "ko"),
        ("サ", "sa"),
        ("シ", "shi"),
        ("ス", "su"),
        ("セ", "se"),
        ("ソ", "so"),
        ("タ", "ta"),
        ("チ", "chi"),
        ("ツ", "tsu"),
        ("テ", "te"),
        ("ト", "to"),
        ("ナ", "na"),
        ("ニ", "ni"),
        ("ヌ", "nu"),
        ("ネ", "ne"),
        ("ノ", "no"),
        ("ハ", "ha"),
        ("ヒ", "hi"),
        ("フ", "fu"),
        ("ヘ", "he"),
        ("ホ", "ho"),
        ("マ", "ma"),
        ("ミ", "mi"),
        ("ム", "mu"),
        ("メ", "me"),
        ("モ", "mo"),
        ("ヤ", "ya"),
        ("ユ", "yu"),
        ("ヨ", "yo"),
        ("ラ", "ra"),
        ("リ", "ri"),
        ("ル", "ru"),
        ("レ", "re"),
        ("ロ", "ro"),
        ("ワ", "wa"),
        ("ヰ", "i"),
        ("ヱ", "e"),
        ("ヲ", "o"),
        ("ン", "n"),
        ("ガ", "ga"),
        ("ギ", "gi"),
        ("グ", "gu"),
        ("ゲ", "ge"),
        ("ゴ", "go"),
        ("ザ", "za"),
        ("ジ", "ji"),
        ("ズ", "zu"),
        ("ゼ", "ze"),
        ("ゾ", "zo"),
        ("ダ", "da"),
        ("ヂ", "ji"),
        ("ヅ", "zu"),
        ("デ", "de"),
        ("ド", "do"),
        ("バ", "ba"),
        ("ビ", "bi"),
        ("ブ", "bu"),
        ("ベ", "be"),
        ("ボ", "bo"),
        ("パ", "pa"),
        ("ピ", "pi"),
        ("プ", "pu"),
        ("ペ", "pe"),
        ("ポ", "po"),
        ("キャ", "kya"),
        ("キュ", "kyu"),
        ("キョ", "kyo"),
        ("シャ", "sha"),
        ("シュ", "shu"),
        ("ショ", "sho"),
        ("チャ", "cha"),
        ("チュ", "chu"),
        ("チョ", "cho"),
        ("ニャ", "nya"),
        ("ニュ", "nyu"),
        ("ニョ", "nyo"),
        ("ヒャ", "hya"),
        ("ヒュ", "hyu"),
        ("ヒョ", "hyo"),
        ("ミャ", "mya"),
        ("ミュ", "myu"),
        ("ミョ", "myo"),
        ("リャ", "rya"),
        ("リュ", "ryu"),
        ("リョ", "ryo"),
        ("ギャ", "gya"),
        ("ギュ", "gyu"),
        ("ギョ", "gyo"),
        ("ジャ", "ja"),
        ("ジュ", "ju"),
        ("ジョ", "jo"),
        ("ヂャ", "ja"),
        ("ヂュ", "ju"),
        ("ヂョ", "jo"),
        ("ビャ", "bya"),
        ("ビュ", "byu"),
        ("ビョ", "byo"),
        ("ピャ", "pya"),
        ("ピュ", "pyu"),
        ("ピョ", "pyo"),
        ("イィ", "yi"),
        ("イェ", "ye"),
        ("ウァ", "wa"),
        ("ウィ", "wi"),
        ("ウゥ", "wu"),
        ("ウェ", "we"),
        ("ウォ", "wo"),
        ("ウュ", "wyu"),
        ("ヴァ", "va"),
        ("ヴィ", "vi"),
        ("ヴ", "v"),
        ("ヴェ", "ve"),
        ("ヴォ", "vo"),
        ("ヴャ", "vya"),
        ("ヴュ", "vyu"),
        ("ヴィェ", "vye"),
        ("ヴョ", "vyo"),
        ("キェ", "kye"),
        ("ギェ", "gye"),
        ("クァ", "kwa"),
        ("クィ", "kwi"),
        ("クェ", "kwe"),
        ("クォ", "kwo"),
        ("クヮ", "kwa"),
        ("グァ", "gwa"),
        ("グィ", "gwi"),
        ("グェ", "gwe"),
        ("グォ", "gwo"),
        ("グヮ", "gwa"),
        ("シェ", "she"),
        ("ジェ", "je"),
        ("スィ", "si"),
        ("ズィ", "zi"),
        ("チェ", "che"),
        ("ツァ", "tsa"),
        ("ツィ", "tsi"),
        ("ツェ", "tse"),
        ("ツォ", "tso"),
        ("ツュ", "tsyu"),
        ("ティ", "ti"),
        ("トゥ", "tu"),
        ("テュ", "tyu"),
        ("ディ", "di"),
        ("ドゥ", "du"),
        ("デュ", "dyu"),
        ("ニェ", "nye"),
        ("ヒェ", "hye"),
        ("ビェ", "bye"),
        ("ピェ", "pye"),
        ("ファ", "fa"),
        ("フィ", "fi"),
        ("フェ", "fe"),
        ("フォ", "fo"),
        ("フャ", "fya"),
        ("フュ", "fyu"),
        ("フィェ", "fye"),
        ("フョ", "fyo"),
        ("ホゥ", "hu"),
        ("ミェ", "mye"),
        ("リェ", "rye"),
        ("ラ゜", "la"),
        ("リ゜", "li"),
        ("ル゜", "lu"),
        ("レ゜", "le"),
        ("ロ゜", "lo"),
        ("ヷ", "va"),
        ("ヸ", "vi"),
        ("ヹ", "ve"),
        ("ヺ", "vo"),
    ];
    for &(katakana, expected) in TABLE {
        assert_eq!(
            romaji::hepburn(katakana),
            expected,
            "romanization of {katakana}"
        );
    }
}

// --- TestCJKWidthCharFilter (analysis/common) --------------------------------------------------
// `whitespaceMockTokenizer` on the filtered text; texts, UTF-16 offsets into the original input,
// and the final offset are checked.

fn assert_cjk_width_char_filter(
    input: &str,
    expected: &[&str],
    starts: &[usize],
    ends: &[usize],
    final_offset: usize,
) {
    let filtered = char_filter::cjk_width(input);
    let text = filtered.text();
    // The whitespace tokenizer's tokens of the filtered text, as byte ranges of it.
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                ranges.push((s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        ranges.push((s, text.len()));
    }
    let actual: Vec<&str> = ranges.iter().map(|&(s, e)| &text[s..e]).collect();
    assert_eq!(actual, expected, "cjk_width({input:?}) = {text:?}");
    let actual: Vec<(usize, usize)> = ranges
        .iter()
        .map(|&(s, e)| (filtered.correct_offset(s), filtered.correct_offset(e)))
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
    assert_eq!(actual, expected, "corrected offsets for {input:?}");
    assert_eq!(
        filtered.correct_offset(text.len()),
        utf16_to_byte_offset(input, final_offset),
        "final offset for {input:?}"
    );
}

/// `testFullWidthASCII`: full-width ASCII forms are normalized to basic Latin.
#[test]
fn cjk_width_char_filter_full_width_ascii() {
    assert_cjk_width_char_filter("Ｔｅｓｔ １２３４", &["Test", "1234"], &[0, 5], &[4, 9], 9);
}

/// `testHalfWidthKana`: half-width katakana are normalized, recombining voice marks with the
/// preceding base form.
#[test]
fn cjk_width_char_filter_half_width_kana() {
    assert_cjk_width_char_filter("ｶﾀｶﾅ", &["カタカナ"], &[0], &[4], 4);
    assert_cjk_width_char_filter("ｳﾞｨｯﾂ", &["ヴィッツ"], &[0], &[5], 5);
    assert_cjk_width_char_filter("ﾊﾟﾅｿﾆｯｸ", &["パナソニック"], &[0], &[7], 7);
    assert_cjk_width_char_filter(
        "ｳﾞｨｯﾂ ﾊﾟﾅｿﾆｯｸ",
        &["ヴィッツ", "パナソニック"],
        &[0, 6],
        &[5, 13],
        13,
    );
}

/// `testOrphanVoiceMark`: a voice mark that can't combine with the previous character becomes
/// the combining mark (U+3099 / U+309A).
#[test]
fn cjk_width_char_filter_orphan_voice_mark() {
    assert_cjk_width_char_filter("ｱﾞｨｯﾂ", &["ア\u{3099}ィッツ"], &[0], &[5], 5);
    assert_cjk_width_char_filter("ﾞｨｯﾂ", &["\u{3099}ィッツ"], &[0], &[4], 4);
    assert_cjk_width_char_filter("ｱﾟﾅｿﾆｯｸ", &["ア\u{309a}ナソニック"], &[0], &[7], 7);
    assert_cjk_width_char_filter("ﾟﾅｿﾆｯｸ", &["\u{309a}ナソニック"], &[0], &[6], 6);
}

/// `testComplexInput`: mixed widths.
#[test]
fn cjk_width_char_filter_complex_input() {
    assert_cjk_width_char_filter("Ｔｅst １２34", &["Test", "1234"], &[0, 5], &[4, 9], 9);
    assert_cjk_width_char_filter(
        "ｶﾀカナ ｳﾞｨッツ ﾊﾟﾅｿニック",
        &["カタカナ", "ヴィッツ", "パナソニック"],
        &[0, 5, 11],
        &[4, 10, 18],
        18,
    );
}

/// `testEmptyInput`.
#[test]
fn cjk_width_char_filter_empty_input() {
    assert_cjk_width_char_filter("", &[], &[], &[], 0);
}

// --- TestCJKWidthFilter (analysis/common): the token filter on MockTokenizer(WHITESPACE) -------

/// `testFullWidthASCII`: offsets are those of the original (unfolded) tokens.
#[test]
fn cjk_width_filter_full_width_ascii() {
    let input = "Ｔｅｓｔ １２３４";
    assert_analyzes_to(
        &run_mock(input, true, &[Filter::Width]),
        input,
        &["Test", "1234"],
        Some(&[0, 5]),
        Some(&[4, 9]),
        None,
    );
}

/// `testHalfWidthKana`.
#[test]
fn cjk_width_filter_half_width_kana() {
    for (input, expected) in [
        ("ｶﾀｶﾅ", "カタカナ"),
        ("ｳﾞｨｯﾂ", "ヴィッツ"),
        ("ﾊﾟﾅｿﾆｯｸ", "パナソニック"),
    ] {
        assert_analyzes_to(
            &run_mock(input, true, &[Filter::Width]),
            input,
            &[expected],
            None,
            None,
            None,
        );
    }
}

// --- Elasticsearch: KuromojiAnalysisTests ------------------------------------------------------
// The plugin's tests run `JapaneseTokenizer(null, true, SEARCH)` (compounds discarded) into the
// filter under test, configured by kuromoji_analysis.json.

/// `testBaseFormFilterFactory`, which really tests the `kuromoji_pos` filter: `stoptags:
/// ["#  verb-main:", "動詞-自立"]` (Elasticsearch passes the list through as is, comment line
/// included; it matches no part of speech).
#[test]
fn es_base_form_filter_factory() {
    let toks = run_chain(
        "私は制限スピードを超える。",
        None,
        LUCENE,
        &[Filter::PosStop(StopTags::new([
            "#  verb-main:",
            "動詞-自立",
        ]))],
    );
    assert_eq!(texts(&toks), ["私", "は", "制限", "スピード", "を"]);
}

/// `testPartOfSpeechFilter`: the pre-built `kuromoji_part_of_speech` (default stop tags).
#[test]
fn es_part_of_speech_filter() {
    let toks = run_chain(
        "寿司がおいしいね",
        None,
        LUCENE,
        &[Filter::PosStop(StopTags::defaults())],
    );
    assert_eq!(texts(&toks), ["寿司", "おいしい"]);
}

/// `testReadingFormFilterFactory`: `kuromoji_rf` (`use_romaji: true`) and the pre-built
/// `kuromoji_readingform` (katakana).
#[test]
fn es_reading_form_filter_factory() {
    let input = "今夜はロバート先生と話した";
    let toks = run_chain(input, None, LUCENE, &[Filter::ReadingForm { romaji: true }]);
    assert_eq!(
        texts(&toks),
        ["kon'ya", "ha", "robato", "sensei", "to", "hanashi", "ta"]
    );
    let toks = run_chain(
        input,
        None,
        LUCENE,
        &[Filter::ReadingForm { romaji: false }],
    );
    assert_eq!(
        texts(&toks),
        ["コンヤ", "ハ", "ロバート", "センセイ", "ト", "ハナシ", "タ"]
    );
}

/// `testKatakanaStemFilter`: the pre-built `kuromoji_stemmer` (minimum length 4) stems
/// パーティー but not コピー; `kuromoji_ks` (minimum length 6) stems neither.
#[test]
fn es_katakana_stem_filter() {
    let input = "明後日パーティーに行く予定がある。図書館で資料をコピーしました。";
    let toks = run_chain(input, None, LUCENE, &[Filter::Stem(4)]);
    assert_eq!(
        texts(&toks),
        [
            "明後日",
            "パーティ",
            "に",
            "行く",
            "予定",
            "が",
            "ある",
            "図書館",
            "で",
            "資料",
            "を",
            "コピー",
            "し",
            "まし",
            "た"
        ]
    );
    let toks = run_chain(input, None, LUCENE, &[Filter::Stem(6)]);
    assert_eq!(
        texts(&toks),
        [
            "明後日",
            "パーティー",
            "に",
            "行く",
            "予定",
            "が",
            "ある",
            "図書館",
            "で",
            "資料",
            "を",
            "コピー",
            "し",
            "まし",
            "た"
        ]
    );
}

/// `testIterationMarkCharFilter`: `kuromoji_im_only_kanji`, `kuromoji_im_only_kana` and
/// `kuromoji_im_default`, compared as filtered text.
#[test]
fn es_iteration_mark_char_filter() {
    let source = "ところゞゝゝ、ジヾが、時々、馬鹿々々しい";
    assert_eq!(
        iteration_mark_text(source, true, false),
        "ところゞゝゝ、ジヾが、時時、馬鹿馬鹿しい"
    );
    assert_eq!(
        iteration_mark_text(source, false, true),
        "ところどころ、ジジが、時々、馬鹿々々しい"
    );
    assert_eq!(
        iteration_mark_text(source, true, true),
        "ところどころ、ジジが、時時、馬鹿馬鹿しい"
    );
}

/// `testJapaneseStopFilterFactory`: `ja_stop` with `stopwords: ["_japanese_", "スピード"]`
/// (`ignore_case` defaults to false in the plugin).
#[test]
fn es_japanese_stop_filter_factory() {
    let words = DEFAULT_STOP_WORDS.iter().copied().chain(["スピード"]);
    let toks = run_chain(
        "私は制限スピードを超える。",
        None,
        LUCENE,
        &[Filter::Stop(StopWords::new(words, false))],
    );
    assert_eq!(texts(&toks), ["私", "制限", "超える"]);
}

/// `testCompletionFilterFactory`: `kuromoji_completion_index` and `kuromoji_completion_query`
/// on the search-mode tokenizer, without width folding.
#[test]
fn es_completion_filter_factory() {
    let toks = run_chain(
        "東京都",
        None,
        LUCENE,
        &[Filter::Completion(CompletionMode::Index)],
    );
    assert_eq!(texts(&toks), ["東京", "toukyou", "都", "to"]);
    let toks = run_chain(
        "サッk",
        None,
        LUCENE,
        &[Filter::Completion(CompletionMode::Query)],
    );
    assert_eq!(texts(&toks), ["サッk", "sakk"]);
}

/// `testCompletionAnalyzer`, also the "Completion analyzer" case of 10_basic.yml: the
/// `kuromoji_completion` analyzer in both modes on half-width input (ｿｰｽｺｰﾄﾞ is U+FF7F U+FF70
/// U+FF7D U+FF7A U+FF70 U+FF84 U+FF9E).
#[test]
fn es_completion_analyzer() {
    assert_eq!(
        texts(&run_completion("ｿｰｽｺｰﾄﾞ", "index")),
        ["ソース", "soーsu", "コード", "koーdo"]
    );
    assert_eq!(
        texts(&run_completion("ｿｰｽｺｰﾄﾞ", "query")),
        ["ソースコード", "soーsukoーdo"]
    );
}

/// `testNumberFilterFactory`: the pre-built `kuromoji_number` on the search-mode tokenizer
/// (punctuation discarded, unlike Lucene's own number tests).
#[test]
fn es_number_filter_factory() {
    let toks = run_chain(
        "本日十万二千五百円のワインを買った",
        None,
        LUCENE,
        &[Filter::Number],
    );
    assert_eq!(
        texts(&toks),
        ["本日", "102500", "円", "の", "ワイン", "を", "買っ", "た"]
    );
}

/// `testHiraganaUppercaseFilterFactory` (the tokenizer yields the small kana as one token).
#[test]
fn es_hiragana_uppercase_filter_factory() {
    let toks = run_chain(
        "ぁぃぅぇぉっゃゅょゎゕゖ",
        None,
        LUCENE,
        &[Filter::HiraganaUppercase],
    );
    assert_eq!(texts(&toks), ["あいうえおつやゆよわかけ"]);
}

/// `testKatakanaUppercaseFilterFactory`.
#[test]
fn es_katakana_uppercase_filter_factory() {
    let toks = run_chain(
        "ァィゥェォヵㇰヶㇱㇲッㇳㇴㇵㇶㇷ",
        None,
        LUCENE,
        &[Filter::KatakanaUppercase],
    );
    assert_eq!(texts(&toks), ["アイウエオカクケシスツトヌハヒフ"]);
}

// --- Elasticsearch: REST tests (10_basic.yml) --------------------------------------------------
// The pre-built `kuromoji_tokenizer` (compounds kept) with one pre-built filter each; the
// "Completion analyzer" case is in `es_completion_analyzer`.

#[test]
fn es_rest_filters() {
    let d = TokenizerConfig::DEFAULT;
    // "Baseform filter"
    assert_eq!(
        texts(&run_chain("飲み", None, d, &[Filter::BaseForm])),
        ["飲む"]
    );
    // "Reading filter": the pre-built `kuromoji_readingform` is katakana.
    assert_eq!(
        texts(&run_chain(
            "寿司",
            None,
            d,
            &[Filter::ReadingForm { romaji: false }]
        )),
        ["スシ"]
    );
    // "Stemming filter"
    assert_eq!(
        texts(&run_chain("サーバー", None, d, &[Filter::Stem(4)])),
        ["サーバ"]
    );
}

// --- Elasticsearch: docs examples (analysis-kuromoji-*.md) -------------------------------------
// Custom analyzers on `kuromoji_tokenizer` (compounds kept); offsets and positions where the
// docs show a response.

/// `analysis-kuromoji-baseform.md`.
#[test]
fn es_docs_baseform() {
    let input = "飲み";
    assert_analyzes_to(
        &run_chain(input, None, TokenizerConfig::DEFAULT, &[Filter::BaseForm]),
        input,
        &["飲む"],
        Some(&[0]),
        Some(&[2]),
        Some(&[1]),
    );
}

/// `analysis-kuromoji-speech.md`: `stoptags: ["助詞-格助詞-一般", "助詞-終助詞"]`; the removed が
/// leaves a position gap.
#[test]
fn es_docs_speech() {
    let input = "寿司がおいしいね";
    assert_analyzes_to(
        &run_chain(
            input,
            None,
            TokenizerConfig::DEFAULT,
            &[Filter::PosStop(StopTags::new([
                "助詞-格助詞-一般",
                "助詞-終助詞",
            ]))],
        ),
        input,
        &["寿司", "おいしい"],
        Some(&[0, 3]),
        Some(&[2, 7]),
        Some(&[1, 2]),
    );
}

/// `analysis-kuromoji-readingform.md`: katakana and romaji readings of 寿司.
#[test]
fn es_docs_readingform() {
    let d = TokenizerConfig::DEFAULT;
    assert_eq!(
        texts(&run_chain(
            "寿司",
            None,
            d,
            &[Filter::ReadingForm { romaji: false }]
        )),
        ["スシ"]
    );
    assert_eq!(
        texts(&run_chain(
            "寿司",
            None,
            d,
            &[Filter::ReadingForm { romaji: true }]
        )),
        ["sushi"]
    );
}

/// `analysis-kuromoji-stemmer.md`: `minimum_length: 4`.
#[test]
fn es_docs_stemmer() {
    let d = TokenizerConfig::DEFAULT;
    assert_eq!(
        texts(&run_chain("コピー", None, d, &[Filter::Stem(4)])),
        ["コピー"]
    );
    assert_eq!(
        texts(&run_chain("サーバー", None, d, &[Filter::Stem(4)])),
        ["サーバ"]
    );
}

/// `analysis-kuromoji-stop.md`: `stopwords: ["_japanese_", "ストップ"]`; ストップ and は are
/// removed, leaving 消える at position 2.
#[test]
fn es_docs_stop() {
    let input = "ストップは消える";
    let words = DEFAULT_STOP_WORDS.iter().copied().chain(["ストップ"]);
    assert_analyzes_to(
        &run_chain(
            input,
            None,
            TokenizerConfig::DEFAULT,
            &[Filter::Stop(StopWords::new(words, false))],
        ),
        input,
        &["消える"],
        Some(&[5]),
        Some(&[8]),
        Some(&[3]),
    );
}

/// `analysis-kuromoji-number.md`.
#[test]
fn es_docs_number() {
    let input = "一〇〇〇";
    assert_analyzes_to(
        &run_chain(input, None, TokenizerConfig::DEFAULT, &[Filter::Number]),
        input,
        &["1000"],
        Some(&[0]),
        Some(&[4]),
        Some(&[1]),
    );
}

/// `analysis-kuromoji-hiragana-uppercase.md`.
#[test]
fn es_docs_hiragana_uppercase() {
    let input = "ちょっとまって";
    assert_analyzes_to(
        &run_chain(
            input,
            None,
            TokenizerConfig::DEFAULT,
            &[Filter::HiraganaUppercase],
        ),
        input,
        &["ちよつと", "まつ", "て"],
        Some(&[0, 4, 6]),
        Some(&[4, 6, 7]),
        Some(&[1, 1, 1]),
    );
}

/// `analysis-kuromoji-katakana-uppercase.md`.
#[test]
fn es_docs_katakana_uppercase() {
    let input = "ストップウォッチ";
    assert_analyzes_to(
        &run_chain(
            input,
            None,
            TokenizerConfig::DEFAULT,
            &[Filter::KatakanaUppercase],
        ),
        input,
        &["ストツプウオツチ"],
        Some(&[0]),
        Some(&[8]),
        Some(&[1]),
    );
}

/// `analysis-kuromoji-completion.md`: the `kuromoji_completion` analyzer returns 寿司, then its
/// Kunrei-shiki (susi) and Hepburn-shiki (sushi) keystrokes; the docs don't fix the order of
/// the two romanizations (the Lucene cases above do).
#[test]
fn es_docs_completion() {
    let toks = run_completion("寿司", "index");
    let texts = texts(&toks);
    assert_eq!(texts.first(), Some(&"寿司"));
    let mut romanizations: Vec<&str> = texts[1..].to_vec();
    romanizations.sort();
    assert_eq!(romanizations, ["sushi", "susi"]);
}
