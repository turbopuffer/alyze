//! The cases from Lucene's nori tests (`TestKoreanTokenizer`, `TestKoreanAnalyzer`, the filter
//! and factory tests) and the Elasticsearch plugin's unit and REST tests, transcribed with their
//! expectations. Offsets in Lucene's tests are UTF-16; they are converted here.

use super::{
    Filter, Tok, TokenizerConfig, run_analyzer, run_chain, run_tokenizer, user_dictionary,
    utf16_to_byte_offset,
};
use crate::nori::{self, AnalyzerOptions, DecompoundMode, Tokens, UserDictionary, pos};
use pos::Tag::*;
use pos::Type::*;

fn texts(toks: &[Tok]) -> Vec<&str> {
    toks.iter().map(|t| t.text.as_str()).collect()
}

/// Lucene's `assertAnalyzesTo`: texts, then optionally UTF-16 start/end offsets, position
/// increments and position lengths.
fn assert_analyzes_to(
    toks: &[Tok],
    input: &str,
    expected: &[&str],
    starts: Option<&[usize]>,
    ends: Option<&[usize]>,
    incs: Option<&[usize]>,
    lens: Option<&[u32]>,
) {
    assert_eq!(texts(toks), expected, "texts for {input:?}");
    if let (Some(starts), Some(ends)) = (starts, ends) {
        let actual: Vec<(usize, usize)> = toks
            .iter()
            .map(|t| (t.byte_range.start, t.byte_range.end))
            .collect();
        // Lucene's tests sometimes list more offsets than tokens; the extras are ignored.
        let expected: Vec<(usize, usize)> = starts
            .iter()
            .zip(ends)
            .take(toks.len())
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
            .take(toks.len())
            .enumerate()
            .map(|(i, &inc)| {
                position = if i == 0 { inc - 1 } else { position + inc };
                position
            })
            .collect();
        let actual: Vec<usize> = toks.iter().map(|t| t.position).collect();
        assert_eq!(actual, expected, "positions for {input:?}");
    }
    if let Some(lens) = lens {
        let actual: Vec<u32> = toks.iter().map(|t| t.position_length).collect();
        assert_eq!(actual, lens, "position lengths for {input:?}");
    }
}

/// Lucene's `assertPartsOfSpeech`.
fn assert_pos(
    toks: &[Tok],
    input: &str,
    types: &[pos::Type],
    left: &[pos::Tag],
    right: &[pos::Tag],
) {
    let actual: Vec<(pos::Type, pos::Tag, pos::Tag)> = toks
        .iter()
        .map(|t| (t.pos_type, t.left_pos, t.right_pos))
        .collect();
    let expected: Vec<(pos::Type, pos::Tag, pos::Tag)> = types
        .iter()
        .zip(left)
        .zip(right)
        .map(|((&t, &l), &r)| (t, l, r))
        .collect();
    assert_eq!(actual, expected, "parts of speech for {input:?}");
}

fn readings(toks: &[Tok]) -> Vec<Option<&str>> {
    toks.iter().map(|t| t.reading.as_deref()).collect()
}

// --- TestKoreanTokenizer -------------------------------------------------------------------------
// Its analyzers all use Lucene's test user dictionary (userdict/lucene.txt).

fn lucene(decompound: DecompoundMode, keep_punctuation: bool, unigrams: bool) -> TokenizerConfig {
    TokenizerConfig {
        decompound,
        keep_punctuation,
        unigrams,
        user_dict: Some("lucene"),
    }
}

/// `analyzer`: no decompounding, punctuation discarded.
fn analyzer(input: &str) -> Vec<Tok> {
    run_tokenizer(input, lucene(DecompoundMode::None, false, false))
}
/// `analyzerWithPunctuation`.
fn analyzer_with_punctuation(input: &str) -> Vec<Tok> {
    run_tokenizer(input, lucene(DecompoundMode::None, true, false))
}
/// `analyzerUnigram`.
fn analyzer_unigram(input: &str) -> Vec<Tok> {
    run_tokenizer(input, lucene(DecompoundMode::None, false, true))
}
/// `analyzerDecompound`.
fn analyzer_decompound(input: &str) -> Vec<Tok> {
    run_tokenizer(input, lucene(DecompoundMode::Discard, false, false))
}
/// `analyzerDecompoundKeep`.
fn analyzer_decompound_keep(input: &str) -> Vec<Tok> {
    run_tokenizer(input, lucene(DecompoundMode::Mixed, false, false))
}
/// `analyzerReading`: no decompounding + the reading-form filter.
fn analyzer_reading(input: &str) -> Vec<Tok> {
    run_chain(
        input,
        lucene(DecompoundMode::None, false, false),
        &[Filter::ReadingForm],
    )
}

#[test]
fn separate_number() {
    let input = "44사이즈";
    assert_analyzes_to(
        &analyzer(input),
        input,
        &["44", "사이즈"],
        Some(&[0, 2]),
        Some(&[2, 5]),
        Some(&[1, 1]),
        None,
    );
    let input = "９.９사이즈";
    assert_analyzes_to(
        &analyzer(input),
        input,
        &["９", "９", "사이즈"],
        Some(&[0, 2, 3]),
        Some(&[1, 3, 6]),
        Some(&[1, 1, 1]),
        None,
    );
}

#[test]
fn spaces() {
    let input = "화학        이외의         것";
    assert_analyzes_to(
        &analyzer(input),
        input,
        &["화학", "이외", "의", "것"],
        Some(&[0, 10, 12, 22]),
        Some(&[2, 12, 13, 23]),
        Some(&[1, 1, 1, 1]),
        None,
    );
    let input = "화학 이외의         것";
    assert_pos(
        &analyzer(input),
        input,
        &[Morpheme; 4],
        &[NNG, NNG, JKG, NNB],
        &[NNG, NNG, JKG, NNB],
    );
}

#[test]
fn part_of_speech() {
    let input = "화학 이외의 것";
    assert_analyzes_to(
        &analyzer(input),
        input,
        &["화학", "이외", "의", "것"],
        Some(&[0, 3, 5, 7]),
        Some(&[2, 5, 6, 8]),
        Some(&[1, 1, 1, 1]),
        None,
    );
    assert_pos(
        &analyzer(input),
        input,
        &[Morpheme; 4],
        &[NNG, NNG, JKG, NNB],
        &[NNG, NNG, JKG, NNB],
    );
}

#[test]
fn part_of_speech_with_punc() {
    let input = "화학 이외의 것!";
    let toks = analyzer_with_punctuation(input);
    assert_analyzes_to(
        &toks,
        input,
        &["화학", " ", "이외", "의", " ", "것", "!"],
        Some(&[0, 2, 3, 5, 6, 7, 8]),
        Some(&[2, 3, 5, 6, 7, 8, 9]),
        Some(&[1; 7]),
        None,
    );
    assert_pos(
        &toks,
        input,
        &[Morpheme; 7],
        &[NNG, SP, NNG, JKG, SP, NNB, SF],
        &[NNG, SP, NNG, JKG, SP, NNB, SF],
    );
}

#[test]
fn floating_point_number() {
    let input = "10.1 인치 모니터";
    assert_analyzes_to(
        &analyzer_with_punctuation(input),
        input,
        &["10", ".", "1", " ", "인치", " ", "모니터"],
        Some(&[0, 2, 3, 4, 5, 7, 8]),
        Some(&[2, 3, 4, 5, 7, 8, 11]),
        Some(&[1; 7]),
        None,
    );
    assert_analyzes_to(
        &analyzer(input),
        input,
        &["10", "1", "인치", "모니터"],
        Some(&[0, 3, 5, 8]),
        Some(&[2, 4, 7, 11]),
        Some(&[1; 4]),
        None,
    );
}

#[test]
fn part_of_speech_with_compound() {
    let input = "가락지나물은 한국, 중국, 일본";
    let toks = analyzer(input);
    assert_analyzes_to(
        &toks,
        input,
        &["가락지나물", "은", "한국", "중국", "일본"],
        Some(&[0, 5, 7, 11, 15]),
        Some(&[5, 6, 9, 13, 17]),
        Some(&[1; 5]),
        None,
    );
    assert_pos(
        &toks,
        input,
        &[Compound, Morpheme, Morpheme, Morpheme, Morpheme],
        &[NNG, JX, NNP, NNP, NNP],
        &[NNG, JX, NNP, NNP, NNP],
    );
    assert_eq!(
        toks[0].morphemes,
        [(NNG, "가락지".to_owned()), (NNG, "나물".to_owned())]
    );

    let toks = analyzer_decompound(input);
    assert_analyzes_to(
        &toks,
        input,
        &["가락지", "나물", "은", "한국", "중국", "일본"],
        Some(&[0, 3, 5, 7, 11, 15]),
        Some(&[3, 5, 6, 9, 13, 17]),
        Some(&[1; 6]),
        None,
    );
    assert_pos(
        &toks,
        input,
        &[Morpheme; 6],
        &[NNG, NNG, JX, NNP, NNP, NNP],
        &[NNG, NNG, JX, NNP, NNP, NNP],
    );

    let toks = analyzer_decompound_keep(input);
    assert_analyzes_to(
        &toks,
        input,
        &["가락지나물", "가락지", "나물", "은", "한국", "중국", "일본"],
        Some(&[0, 0, 3, 5, 7, 11, 15]),
        Some(&[5, 3, 5, 6, 9, 13, 17]),
        Some(&[1, 0, 1, 1, 1, 1, 1]),
        Some(&[2, 1, 1, 1, 1, 1, 1]),
    );
    assert_pos(
        &toks,
        input,
        &[
            Compound, Morpheme, Morpheme, Morpheme, Morpheme, Morpheme, Morpheme,
        ],
        &[NNG, NNG, NNG, JX, NNP, NNP, NNP],
        &[NNG, NNG, NNG, JX, NNP, NNP, NNP],
    );
}

#[test]
fn part_of_speech_with_inflects() {
    let input = "감싸여";
    let toks = analyzer(input);
    assert_analyzes_to(
        &toks,
        input,
        &["감싸여"],
        Some(&[0]),
        Some(&[3]),
        Some(&[1]),
        None,
    );
    assert_pos(&toks, input, &[Inflect], &[VV], &[EC]);
    assert_eq!(
        toks[0].morphemes,
        [(VV, "감싸이".to_owned()), (EC, "어".to_owned())]
    );

    let toks = analyzer_decompound(input);
    assert_analyzes_to(
        &toks,
        input,
        &["감싸이", "어"],
        Some(&[0, 0]),
        Some(&[3, 3]),
        Some(&[1, 1]),
        None,
    );
    assert_pos(&toks, input, &[Morpheme, Morpheme], &[VV, EC], &[VV, EC]);

    let toks = analyzer_decompound_keep(input);
    assert_analyzes_to(
        &toks,
        input,
        &["감싸여", "감싸이", "어"],
        Some(&[0, 0, 0]),
        Some(&[3, 3, 3]),
        Some(&[1, 0, 1]),
        Some(&[2, 1, 1]),
    );
    assert_pos(
        &toks,
        input,
        &[Inflect, Morpheme, Morpheme],
        &[VV, VV, EC],
        &[EC, VV, EC],
    );
}

#[test]
fn unknown_word() {
    let input = "2018 평창 동계올림픽대회";
    let toks = analyzer(input);
    assert_analyzes_to(
        &toks,
        input,
        &["2018", "평창", "동계", "올림픽", "대회"],
        Some(&[0, 5, 8, 10, 13]),
        Some(&[4, 7, 10, 13, 15]),
        Some(&[1; 5]),
        None,
    );
    assert_pos(
        &toks,
        input,
        &[Morpheme; 5],
        &[SN, NNP, NNP, NNP, NNG],
        &[SN, NNP, NNP, NNP, NNG],
    );
    assert_eq!(toks[0].kind, Some(nori::TokenKind::Unknown));
    assert_eq!(toks[1].kind, Some(nori::TokenKind::Known));

    let toks = analyzer_unigram(input);
    assert_analyzes_to(
        &toks,
        input,
        &["2", "0", "1", "8", "평창", "동계", "올림픽", "대회"],
        Some(&[0, 1, 2, 3, 5, 8, 10, 13]),
        Some(&[1, 2, 3, 4, 7, 10, 13, 15]),
        Some(&[1; 8]),
        None,
    );
    assert_pos(
        &toks,
        input,
        &[Morpheme; 8],
        &[SY, SY, SY, SY, NNP, NNP, NNP, NNG],
        &[SY, SY, SY, SY, NNP, NNP, NNP, NNG],
    );
}

#[test]
fn reading() {
    assert_eq!(readings(&analyzer("喜悲哀歡")), [Some("희비애환")]);
    assert_eq!(readings(&analyzer("五朔居廬")), [Some("오삭거려")]);
    assert_eq!(readings(&analyzer("가늘라")), [None]);
    let input = "喜悲哀歡";
    assert_analyzes_to(
        &analyzer_reading(input),
        input,
        &["희비애환"],
        Some(&[0]),
        Some(&[4]),
        Some(&[1]),
        None,
    );
    let input = "五朔居廬";
    assert_analyzes_to(
        &analyzer_reading(input),
        input,
        &["오삭거려"],
        Some(&[0]),
        Some(&[4]),
        Some(&[1]),
        None,
    );
    let input = "가늘라";
    assert_analyzes_to(
        &analyzer_reading(input),
        input,
        &["가늘라"],
        Some(&[0]),
        Some(&[3]),
        Some(&[1]),
        None,
    );
}

#[test]
fn user_dict() {
    let input = "c++ 프로그래밍 언어";
    let toks = analyzer(input);
    assert_analyzes_to(
        &toks,
        input,
        &["c++", "프로그래밍", "언어"],
        Some(&[0, 4, 10]),
        Some(&[3, 9, 12]),
        Some(&[1; 3]),
        None,
    );
    assert_pos(&toks, input, &[Morpheme; 3], &[NNG; 3], &[NNG; 3]);
    assert_eq!(toks[0].kind, Some(nori::TokenKind::User));

    let input = "정부세종청사";
    let toks = analyzer_decompound(input);
    assert_analyzes_to(
        &toks,
        input,
        &["정부", "세종", "청사"],
        Some(&[0, 2, 4]),
        Some(&[2, 4, 6]),
        Some(&[1; 3]),
        None,
    );
    assert_pos(&toks, input, &[Morpheme; 3], &[NNG; 3], &[NNG; 3]);

    let input = "대한민국날씨";
    assert_analyzes_to(
        &analyzer(input),
        input,
        &["대한민국날씨"],
        Some(&[0]),
        Some(&[6]),
        Some(&[1]),
        None,
    );
    let input = "21세기대한민국";
    assert_analyzes_to(
        &analyzer(input),
        input,
        &["21세기대한민국"],
        Some(&[0]),
        Some(&[8]),
        Some(&[1]),
        None,
    );
}

#[test]
fn interpunct() {
    let input = "도로ㆍ지반ㆍ수자원ㆍ건설환경ㆍ건축ㆍ화재설비연구";
    assert_analyzes_to(
        &analyzer(input),
        input,
        &[
            "도로",
            "지반",
            "수자원",
            "건설",
            "환경",
            "건축",
            "화재",
            "설비",
            "연구",
        ],
        Some(&[0, 3, 6, 10, 12, 15, 18, 20, 22]),
        Some(&[2, 5, 9, 12, 14, 17, 20, 22, 24]),
        Some(&[1; 9]),
        None,
    );
}

#[test]
fn combining() {
    let input = "Ба̀лтичко мо̑ре";
    let toks = analyzer(input);
    assert_analyzes_to(
        &toks,
        input,
        &["Ба̀лтичко", "мо̑ре"],
        Some(&[0, 10]),
        Some(&[9, 15]),
        Some(&[1, 1]),
        None,
    );
    assert_pos(&toks, input, &[Morpheme, Morpheme], &[SL, SL], &[SL, SL]);
    let input = "ka̠k̚t͡ɕ͈a̠k̚";
    let toks = analyzer(input);
    assert_analyzes_to(
        &toks,
        input,
        &["ka̠k̚t͡ɕ͈a̠k̚"],
        Some(&[0]),
        Some(&[13]),
        Some(&[1]),
        None,
    );
    assert_pos(&toks, input, &[Morpheme], &[SL], &[SL]);
    let input = "εἰμί";
    let toks = analyzer(input);
    assert_analyzes_to(
        &toks,
        input,
        &["εἰμί"],
        Some(&[0]),
        Some(&[4]),
        Some(&[1]),
        None,
    );
    assert_pos(&toks, input, &[Morpheme], &[SL], &[SL]);
}

// --- TestKoreanAnalyzer --------------------------------------------------------------------------

#[test]
fn analyzer_sentence() {
    let input = "한국은 대단한 나라입니다.";
    assert_analyzes_to(
        &run_analyzer(input, "default"),
        input,
        &["한국", "대단", "나라", "이"],
        Some(&[0, 4, 8, 10]),
        Some(&[2, 6, 10, 13]),
        Some(&[1, 2, 3, 1]),
        None,
    );
}

#[test]
fn analyzer_stop_tags() {
    let input = "한국은 대단한 나라입니다.";
    let mut tokens = Tokens::new();
    nori::analyze(
        input,
        AnalyzerOptions {
            stop_tags: pos::TagSet::from_tags(&[NNP, NNG]),
            ..AnalyzerOptions::default()
        },
        &mut tokens,
    );
    let toks = super::collect(input, &tokens);
    assert_analyzes_to(
        &toks,
        input,
        &["은", "대단", "하", "ᆫ", "이", "ᄇ니다"],
        Some(&[2, 4, 6, 6, 10, 10]),
        Some(&[3, 6, 7, 7, 13, 13]),
        Some(&[2, 1, 1, 1, 2, 1]),
        None,
    );
}

/// `KoreanAnalyzer(null, DISCARD, DEFAULT_STOP_TAGS, outputUnknownUnigrams)`, which Elasticsearch
/// can't configure; built as the equivalent chain.
#[test]
fn analyzer_unknown_word() {
    let input = "2018 평창 동계올림픽대회";
    let analyzer_filters = [
        Filter::PosStop(pos::TagSet::DEFAULT_STOP_TAGS),
        Filter::ReadingForm,
        Filter::Lowercase,
    ];
    let toks = run_chain(
        input,
        TokenizerConfig {
            unigrams: true,
            ..TokenizerConfig::DEFAULT
        },
        &analyzer_filters,
    );
    assert_analyzes_to(
        &toks,
        input,
        &["2", "0", "1", "8", "평창", "동계", "올림픽", "대회"],
        Some(&[0, 1, 2, 3, 5, 8, 10, 13]),
        Some(&[1, 2, 3, 4, 7, 10, 13, 15]),
        Some(&[1; 8]),
        None,
    );
    let toks = run_chain(input, TokenizerConfig::DEFAULT, &analyzer_filters);
    assert_analyzes_to(
        &toks,
        input,
        &["2018", "평창", "동계", "올림픽", "대회"],
        Some(&[0, 5, 8, 10, 13]),
        Some(&[4, 7, 10, 13, 15]),
        Some(&[1; 5]),
        None,
    );
}

#[test]
fn analyzer_user_dict() {
    let input = "c++ 프로그래밍 언어";
    let mut tokens = Tokens::new();
    nori::analyze(
        input,
        AnalyzerOptions {
            user_dictionary: Some(user_dictionary("lucene")),
            ..AnalyzerOptions::default()
        },
        &mut tokens,
    );
    assert_analyzes_to(
        &super::collect(input, &tokens),
        input,
        &["c++", "프로그래밍", "언어"],
        Some(&[0, 4, 10]),
        Some(&[3, 9, 12]),
        Some(&[1; 3]),
        None,
    );
}

// --- TestKoreanNumberFilter ----------------------------------------------------------------------
// Its analyzer: Lucene's test user dictionary, decompounding, punctuation kept, spaces stopped,
// then the number filter.

fn number_analyzer(input: &str) -> Vec<Tok> {
    run_chain(
        input,
        lucene(DecompoundMode::Discard, true, false),
        &[
            Filter::PosStop(pos::TagSet::from_tags(&[SP])),
            Filter::Number,
        ],
    )
}

fn assert_numbers(input: &str, expected: &[&str]) {
    assert_eq!(
        texts(&number_analyzer(input)),
        expected,
        "number filter on {input:?}"
    );
}

#[test]
fn number_basics() {
    let input = "오늘 십만이천오백원의 와인 구입";
    assert_analyzes_to(
        &number_analyzer(input),
        input,
        &["오늘", "102500", "원", "의", "와인", "구입"],
        Some(&[0, 3, 9, 10, 12, 15]),
        Some(&[2, 9, 10, 11, 14, 17]),
        None,
        None,
    );
    // "Wrong analysis" in Lucene's words: 초밥 comes out as 초 + 밥.
    let input = "어제 초밥 가격은 10만 원";
    assert_analyzes_to(
        &number_analyzer(input),
        input,
        &["어제", "초", "밥", "가격", "은", "100000", "원"],
        Some(&[0, 3, 4, 6, 8, 10, 14]),
        Some(&[2, 4, 5, 8, 9, 13, 15]),
        None,
        None,
    );
    let input = "자본금 600만 원";
    assert_analyzes_to(
        &number_analyzer(input),
        input,
        &["자본", "금", "6000000", "원"],
        Some(&[0, 2, 4, 9]),
        Some(&[2, 3, 8, 10]),
        None,
        None,
    );
}

#[test]
fn number_variants() {
    for input in [
        "3",
        "３",
        "삼",
        "03",
        "０３",
        "영삼",
        "003",
        "００３",
        "영영삼",
    ] {
        assert_numbers(input, &["3"]);
    }
    for input in ["천", "1천", "１천", "일천", "일영영영", "１０백"] {
        assert_numbers(input, &["1000"]);
    }
}

#[test]
fn number_large_variants() {
    assert_numbers("삼오칠팔구", &["35789"]);
    assert_numbers("육백이만오천일", &["6025001"]);
    assert_numbers("조육백만오천일", &["1000006005001"]);
    assert_numbers("십조육백만오천일", &["10000006005001"]);
    assert_numbers("일경일", &["10000000000000001"]);
    assert_numbers("십경십", &["100000000000000010"]);
    assert_numbers("해경조억만천백십일", &["100010001000100011111"]);
}

#[test]
fn number_negative() {
    assert_numbers("-백만", &["-", "1000000"]);
}

#[test]
fn number_mixed() {
    assert_numbers("삼천2백２십삼", &["3223"]);
    assert_numbers("３２이삼", &["3223"]);
}

#[test]
fn number_funny() {
    assert_numbers("십십", &["20"]);
    assert_numbers("백백백", &["300"]);
    assert_numbers("천천천천", &["4000"]);
}

#[test]
fn number_hangul_arabic() {
    assert_numbers(
        "영일이삼사오육칠팔구구팔칠육오사삼이일영",
        &["1234567899876543210"],
    );
    assert_numbers("영영칠", &["7"]);
}

#[test]
fn number_double_zero() {
    let input = "영영";
    assert_analyzes_to(
        &number_analyzer(input),
        input,
        &["0"],
        Some(&[0]),
        Some(&[2]),
        Some(&[1]),
        None,
    );
}

/// 경일 is a name that normalizes to a number (Lucene's keyword-marker variant has no port
/// counterpart).
#[test]
fn number_name() {
    let input = "전중경일";
    assert_analyzes_to(
        &number_analyzer(input),
        input,
        &["전중", "10000000000000001"],
        Some(&[0, 2]),
        Some(&[2, 4]),
        Some(&[1, 1]),
        None,
    );
}

#[test]
fn number_decimal_and_separators() {
    assert_numbers("１．２만３４５．６７", &["12345.67"]);
    assert_numbers("３．２천 원", &["3200", "원"]);
    assert_numbers("4,647", &["4647"]);
    assert_numbers("4,647.0010", &["4647.001"]);
    assert_numbers("15,7", &["157"]);
    assert_numbers("1000.1000", &["1000.1"]);
    assert_numbers("1000.0000", &["1000"]);
    assert_numbers("", &[]);
}

// --- TestKoreanReadingFormFilter, TestKoreanReadingFormFilterFactory ---------------------------

#[test]
fn reading_form_filter() {
    let toks = run_chain("車丞相", TokenizerConfig::DEFAULT, &[Filter::ReadingForm]);
    assert_eq!(texts(&toks), ["차", "승상"]);
    let toks = run_chain("丞相", TokenizerConfig::DEFAULT, &[Filter::ReadingForm]);
    assert_eq!(texts(&toks), ["승상"]);
}

// --- TestKoreanTokenizerFactory ------------------------------------------------------------------

#[test]
fn factory_simple() {
    let input = "안녕하세요";
    assert_analyzes_to(
        &run_tokenizer(input, TokenizerConfig::DEFAULT),
        input,
        &["안녕", "하", "시", "어요"],
        Some(&[0, 2, 3, 3]),
        Some(&[2, 3, 5, 5]),
        None,
        None,
    );
}

#[test]
fn factory_decompound_modes() {
    let d = TokenizerConfig::DEFAULT;
    assert_eq!(texts(&run_tokenizer("갠지스강", d)), ["갠지스", "강"]);
    assert_eq!(
        texts(&run_tokenizer(
            "갠지스강",
            TokenizerConfig {
                decompound: DecompoundMode::None,
                ..d
            }
        )),
        ["갠지스강"]
    );
    assert_eq!(
        texts(&run_tokenizer(
            "갠지스강",
            TokenizerConfig {
                decompound: DecompoundMode::Mixed,
                ..d
            }
        )),
        ["갠지스강", "갠지스", "강"]
    );
}

#[test]
fn factory_user_dict() {
    let dict =
        UserDictionary::parse("# Additional nouns\n세종시 세종 시\n# \nc++\n", false).unwrap();
    let mut tokens = Tokens::new();
    nori::tokenize(
        "세종시",
        nori::Options {
            user_dictionary: Some(&dict),
            ..nori::Options::default()
        },
        &mut tokens,
    );
    assert_eq!(texts(&super::collect("세종시", &tokens)), ["세종", "시"]);
}

#[test]
fn factory_discard_punctuation() {
    let d = TokenizerConfig::DEFAULT;
    assert_eq!(
        texts(&run_tokenizer("10.1 인치 모니터", d)),
        ["10", "1", "인치", "모니터"]
    );
    assert_eq!(
        texts(&run_tokenizer(
            "10.1 인치 모니터",
            TokenizerConfig {
                keep_punctuation: true,
                ..d
            }
        )),
        ["10", ".", "1", " ", "인치", " ", "모니터"]
    );
}

// --- TestKoreanPartOfSpeechStopFilterFactory, TestKoreanNumberFilterFactory ---------------------

#[test]
fn pos_stop_factory() {
    let tags =
        pos::TagSet::parse("EP, EF, EC, ETN, ETM, JKS, JKC, JKG, JKO, JKB, JKV, JKQ, JX, JC")
            .unwrap();
    let toks = run_chain(
        " 한국은 대단한 나라입니다.",
        TokenizerConfig::DEFAULT,
        &[Filter::PosStop(tags)],
    );
    assert_eq!(texts(&toks), ["한국", "대단", "하", "나라", "이"]);
}

#[test]
fn number_factory() {
    let toks = run_chain(
        "어제 초밥 가격은 10만 원",
        TokenizerConfig {
            keep_punctuation: true,
            ..TokenizerConfig::DEFAULT
        },
        &[Filter::Number],
    );
    assert_eq!(
        texts(&toks),
        [
            "어제", " ", "초", "밥", " ", "가격", "은", " ", "100000", " ", "원"
        ]
    );
}

// --- Elasticsearch: NoriAnalysisTests ------------------------------------------------------------

fn es_analyze(input: &str, options: AnalyzerOptions<'_>) -> Vec<Tok> {
    let mut tokens = Tokens::new();
    nori::analyze(input, options, &mut tokens);
    super::collect(input, &tokens)
}

#[test]
fn es_nori_analyzer() {
    let options = AnalyzerOptions {
        stop_tags: pos::TagSet::parse("NR, SP").unwrap(),
        decompound_mode: DecompoundMode::Mixed,
        ..AnalyzerOptions::default()
    };
    assert_eq!(texts(&es_analyze("여섯 용이", options)), ["용", "이"]);
    assert_eq!(
        texts(&es_analyze("가늠표", options)),
        ["가늠표", "가늠", "표"]
    );
}

#[test]
fn es_nori_analyzer_user_dict() {
    // `user_dictionary_rules: ["c++", "C쁠쁠", "세종", "세종시 세종 시"]`, also the content of
    // the plugin test's user_dict.txt.
    let options = AnalyzerOptions {
        user_dictionary: Some(user_dictionary("es")),
        ..AnalyzerOptions::default()
    };
    assert_eq!(texts(&es_analyze("세종시", options)), ["세종", "시"]);
    assert_eq!(texts(&es_analyze("c++world", options)), ["c++", "world"]);
    // Lenient duplicates behave like Lucene (`testNoriAnalyzerDuplicateUserDictRuleDeduplication`).
    let dict = UserDictionary::parse("c++\nC쁠쁠\n세종\n세종\n세종시 세종 시", true).unwrap();
    let options = AnalyzerOptions {
        user_dictionary: Some(&dict),
        ..AnalyzerOptions::default()
    };
    assert_eq!(texts(&es_analyze("세종시", options)), ["세종", "시"]);
    assert_eq!(texts(&es_analyze("세종", options)), ["세종"]);
}

#[test]
fn es_nori_tokenizer() {
    let mixed = TokenizerConfig {
        decompound: DecompoundMode::Mixed,
        ..TokenizerConfig::DEFAULT
    };
    assert_eq!(
        texts(&run_tokenizer("뿌리가 깊은 나무", mixed)),
        ["뿌리", "가", "깊", "은", "나무"]
    );
    assert_eq!(
        texts(&run_tokenizer("가늠표", mixed)),
        ["가늠표", "가늠", "표"]
    );
    assert_eq!(texts(&run_tokenizer("3.2개", mixed)), ["3", "2", "개"]);
    assert_eq!(
        texts(&run_tokenizer("3.2개", TokenizerConfig::DEFAULT)),
        ["3", "2", "개"]
    );
    assert_eq!(
        texts(&run_tokenizer(
            "3.2개",
            TokenizerConfig {
                keep_punctuation: true,
                ..TokenizerConfig::DEFAULT
            }
        )),
        ["3", ".", "2", "개"]
    );
}

#[test]
fn es_nori_part_of_speech() {
    let toks = run_chain(
        "여섯 용이",
        TokenizerConfig::DEFAULT,
        &[Filter::PosStop(pos::TagSet::parse("NR, SP").unwrap())],
    );
    assert_eq!(texts(&toks), ["용", "이"]);
}

#[test]
fn es_nori_reading_form() {
    assert_eq!(
        texts(&run_chain(
            "鄕歌",
            TokenizerConfig::DEFAULT,
            &[Filter::ReadingForm]
        )),
        ["향가"]
    );
}

#[test]
fn es_nori_number() {
    let toks = run_chain(
        "오늘 십만이천오백원짜리 와인 구입",
        TokenizerConfig::DEFAULT,
        &[Filter::Number],
    );
    assert_eq!(
        texts(&toks),
        ["오늘", "102500", "원", "짜리", "와인", "구입"]
    );
}

// --- Elasticsearch: REST tests (10_basic.yml, 20_search.yml, 30_graph_phrase.yml) --------------

#[test]
fn es_rest_basic() {
    assert_eq!(
        texts(&run_analyzer("뿌리가 깊은 나무", "default")),
        ["뿌리", "깊", "나무"]
    );
    assert_eq!(
        texts(&run_tokenizer("뿌리가 깊은 나무", TokenizerConfig::DEFAULT)),
        ["뿌리", "가", "깊", "은", "나무"]
    );
    let toks = run_chain(
        "뿌리가 깊은 나무",
        TokenizerConfig::DEFAULT,
        &[Filter::PosStop(pos::TagSet::DEFAULT_STOP_TAGS)],
    );
    assert_eq!(texts(&toks), ["뿌리", "깊", "나무"]);
    assert_eq!(
        texts(&run_chain(
            "鄕歌",
            TokenizerConfig::DEFAULT,
            &[Filter::ReadingForm]
        )),
        ["향가"]
    );
    let toks = run_chain(
        "십만이천오백과 ３.２천",
        TokenizerConfig {
            keep_punctuation: true,
            ..TokenizerConfig::DEFAULT
        },
        &[
            Filter::PosStop(pos::TagSet::from_tags(&[SP])),
            Filter::Number,
        ],
    );
    assert_eq!(texts(&toks), ["102500", "과", "3200"]);
}

/// 20_search.yml indexes "뿌리가 깊은 나무는" with the `nori` analyzer and finds it with a match
/// query for "나무": the analyzed document contains the analyzed query.
#[test]
fn es_rest_search() {
    let doc = run_analyzer("뿌리가 깊은 나무는", "default");
    let query = run_analyzer("나무", "default");
    assert_eq!(texts(&query), ["나무"]);
    assert!(texts(&doc).contains(&"나무"));
}

/// 30_graph_phrase.yml: with `decompound_mode: mixed` and `nori_part_of_speech`, the token graph
/// for "보험계약대출이율" is 보험계약@0 (length 2), 보험@0, 계약@1, 대출@2, 율@4; the particle 이 at
/// position 3 is removed and leaves a hole, so a phrase query for the exact text still matches.
#[test]
fn es_rest_graph_phrase() {
    let toks = run_chain(
        "보험계약대출이율",
        TokenizerConfig {
            decompound: DecompoundMode::Mixed,
            ..TokenizerConfig::DEFAULT
        },
        &[Filter::PosStop(pos::TagSet::DEFAULT_STOP_TAGS)],
    );
    let graph: Vec<(&str, usize, u32)> = toks
        .iter()
        .map(|t| (t.text.as_str(), t.position, t.position_length))
        .collect();
    assert_eq!(
        graph,
        [
            ("보험계약", 0, 2),
            ("보험", 0, 1),
            ("계약", 1, 1),
            ("대출", 2, 1),
            ("율", 4, 1)
        ]
    );
}

// --- Part-of-speech tag plumbing (POS, KoreanPartOfSpeechStopFilterFactory, ES resolvePOSList) -

#[test]
fn tags() {
    assert_eq!(pos::Tag::ALL.len(), 48);
    assert_eq!(pos::TagSet::DEFAULT_STOP_TAGS.iter().count(), 30);
    assert_eq!(pos::Tag::from_name("nng"), Some(NNG));
    assert_eq!(pos::Tag::from_name("Vsv"), Some(VSV));
    assert_eq!(pos::Tag::from_name("XYZ"), None);
    assert_eq!(pos::Tag::NNG.code(), 150);
    assert_eq!(pos::Tag::UNKNOWN.code(), 999);
    assert_eq!(pos::Tag::NA.code(), -1);
    assert_eq!(pos::Tag::SF.description(), "Terminal punctuation");
    assert_eq!(
        pos::TagSet::parse("NR, SP").unwrap(),
        pos::TagSet::from_tags(&[NR, SP])
    );
    assert_eq!(pos::TagSet::parse("").unwrap(), pos::TagSet::EMPTY);
    assert_eq!(pos::TagSet::parse("NR, bogus").unwrap_err().0, "bogus");
    assert_eq!(pos::Type::from_name("*"), Some(Morpheme));
    assert_eq!(pos::Type::from_name("Compound"), Some(Compound));
    for (i, tag) in pos::Tag::ALL.iter().enumerate() {
        assert_eq!(pos::Tag::from_index(i as u8), Some(*tag));
        assert_eq!(pos::Tag::from_name(tag.name()), Some(*tag));
    }
}
