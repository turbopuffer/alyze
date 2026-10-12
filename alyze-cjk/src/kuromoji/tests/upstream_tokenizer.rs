//! The cases from Lucene's kuromoji tokenizer and analyzer tests (`TestJapaneseTokenizer`,
//! `TestSearchMode`, `TestExtendedMode`, `TestJapaneseTokenizerFactory`, `TestJapaneseAnalyzer`)
//! and the Elasticsearch plugin's tokenizer and analyzer cases (`KuromojiAnalysisTests`, the REST
//! tests and the documentation's examples), transcribed with their expectations. Offsets in
//! Lucene's tests are UTF-16; they are converted here. Random, fuzz, performance-only and
//! commented-out tests are skipped; each is named in a comment next to its neighbours.

use super::{
    CharFilter, Filter, Tok, TokenizerConfig, java_lowercase_text, run_analyzer,
    run_analyzer_unlowercased, run_chain, run_tokenizer, user_dictionary, utf16_to_byte_offset,
};
use crate::kuromoji::filter::{StopTags, StopWords};
use crate::kuromoji::{self, AnalyzerOptions, Mode, Options, Tokens, UserDictionary};

fn texts(toks: &[Tok]) -> Vec<&str> {
    toks.iter().map(|t| t.text.as_str()).collect()
}

/// Lucene's `assertAnalyzesTo` / `assertTokenStreamContents`: texts, then optionally UTF-16
/// start/end offsets, position increments and position lengths. `finalOffset` is not modelled.
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
        // Lucene's tests sometimes list more offsets than tokens (`testEnd`); the extras are
        // ignored.
        let expected: Vec<(usize, usize)> = starts
            .iter()
            .zip(ends)
            .take(expected.len())
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
            .take(expected.len())
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

/// One optional attribute of every token, for `assertReadings` and its siblings.
fn attribute<'a>(
    toks: &'a [Tok],
    get: impl Fn(&'a Tok) -> Option<&'a String>,
) -> Vec<Option<&'a str>> {
    toks.iter().map(|t| get(t).map(String::as_str)).collect()
}

/// Tokenizes with raw [`Options`], for tests that build their own dictionary or n-best cost.
fn run_options(input: &str, options: Options<'_>) -> Vec<Tok> {
    let mut tokens = Tokens::new();
    kuromoji::tokenize(input, options, &mut tokens);
    super::collect(input, &tokens)
}

/// Lucene's analyzers end with a `LowerCaseFilter`, which the chain runs leave out: applies
/// Java's lowercase to the texts so Latin expectations can be compared.
fn java_lowercased(toks: &[Tok]) -> Vec<Tok> {
    toks.iter()
        .map(|t| Tok {
            text: java_lowercase_text(&t.text),
            ..t.clone()
        })
        .collect()
}

// --- TestJapaneseTokenizer -----------------------------------------------------------------------
// Its analyzers all use Lucene's test user dictionary (userdict/lucene.txt).
//
// Skipped: testRandomStrings, testRandomHugeStrings, testRandomHugeStringsAtNight,
// testRandomHugeStringsMockGraphAfter(AtNight), testLargeDocReliability and testSurrogates2
// (random inputs); testBocchan and testBocchanBig (performance, no assertions); testBigDocument
// and testDecomposition5 (only check that nothing throws or hangs); testLatticeToDot (asserts on
// Graphviz output); testDecomposition6, testUserDict4 and testWikipedia (commented out upstream).

fn lucene(mode: Mode, keep_punctuation: bool, discard_compound: bool) -> TokenizerConfig {
    TokenizerConfig {
        mode,
        keep_punctuation,
        discard_compound,
        user_dict: Some("lucene"),
        ..TokenizerConfig::DEFAULT
    }
}

/// `analyzer`: SEARCH, punctuation kept, compounds kept.
fn analyzer(input: &str) -> Vec<Tok> {
    run_tokenizer(input, lucene(Mode::Search, true, false))
}
/// `analyzerNormal`.
fn analyzer_normal(input: &str) -> Vec<Tok> {
    run_tokenizer(input, lucene(Mode::Normal, true, false))
}
/// `analyzerNoPunct`.
fn analyzer_no_punct(input: &str) -> Vec<Tok> {
    run_tokenizer(input, lucene(Mode::Search, false, false))
}
/// `extendedModeAnalyzerNoPunct`.
fn extended_analyzer_no_punct(input: &str) -> Vec<Tok> {
    run_tokenizer(input, lucene(Mode::Extended, false, false))
}
/// `analyzerNoCompound`.
fn analyzer_no_compound(input: &str) -> Vec<Tok> {
    run_tokenizer(input, lucene(Mode::Search, true, true))
}
/// `extendedModeAnalyzerNoCompound`.
fn extended_analyzer_no_compound(input: &str) -> Vec<Tok> {
    run_tokenizer(input, lucene(Mode::Extended, true, true))
}
/// `makeTokenizer(true, mode)` with `setNBestCost`: punctuation and compounds discarded.
fn nbest_tokenizer(mode: Mode, nbest_cost: i32) -> TokenizerConfig {
    TokenizerConfig {
        nbest_cost,
        ..lucene(mode, false, true)
    }
}

/// `testNormalMode`.
#[test]
fn normal_mode() {
    assert_eq!(
        texts(&analyzer_normal("シニアソフトウェアエンジニア")),
        ["シニアソフトウェアエンジニア"]
    );
}

/// `testNormalModeNbest`.
#[test]
fn normal_mode_nbest() {
    let run =
        |input, cost| texts(&run_tokenizer(input, nbest_tokenizer(Mode::Normal, cost))).join(" ");
    assert_eq!(
        run("シニアソフトウェアエンジニア", 2000),
        "シニア シニアソフトウェアエンジニア ソフトウェア エンジニア"
    );
    assert_eq!(
        run("シニアソフトウェアエンジニア", 5000),
        "シニア シニアソフトウェアエンジニア ソフト ソフトウェア ウェア エンジニア"
    );
    assert_eq!(run("数学部長谷川", 0), "数学 部長 谷川");
    assert_eq!(run("数学部長谷川", 3000), "数学 部 部長 長谷川 谷川");
    assert_eq!(run("経済学部長", 0), "経済 学 部長");
    assert_eq!(run("経済学部長", 2000), "経済 経済学部 学 部長 長");
    assert_eq!(run("成田空港、米原油流出", 0), "成田空港 米 原油 流出");
    assert_eq!(
        run("成田空港、米原油流出", 4000),
        "成田空港 米 米原 原油 油 流出"
    );
}

/// `testSearchModeNbest`.
#[test]
fn search_mode_nbest() {
    let run =
        |input, cost| texts(&run_tokenizer(input, nbest_tokenizer(Mode::Search, cost))).join(" ");
    assert_eq!(run("成田空港、米原油流出", 0), "成田 空港 米 原油 流出");
    assert_eq!(
        run("成田空港、米原油流出", 4000),
        "成田 成田空港 空港 米 米原 原油 油 流出"
    );
}

/// `testNBestCost`: `calcNBestCost` on `makeTokenizer(true, NORMAL)`.
#[test]
fn nbest_cost() {
    let has_token = |input: &str, options: Options<'_>, token: &str| {
        texts(&run_options(input, options)).contains(&token)
    };
    let options = nbest_tokenizer(Mode::Normal, 0).options();
    assert!(
        !has_token("数学部長谷川", options, "学部"),
        "学部 is not a token of 数学部長谷川"
    );

    let cost = kuromoji::calc_nbest_cost("/数学部長谷川-学部/", options).unwrap();
    assert!(cost >= 0, "cost calculated /数学部長谷川-学部/");
    let options = Options {
        nbest_cost: cost,
        ..options
    };
    assert!(
        has_token("数学部長谷川", options, "学部"),
        "学部 is a token of 数学部長谷川"
    );

    let cost = kuromoji::calc_nbest_cost("/数学部長谷川-数/成田空港-成/", options).unwrap();
    assert!(cost >= 0, "cost calculated /数学部長谷川-数/成田空港-成/");
    let options = Options {
        nbest_cost: cost,
        ..options
    };
    assert!(
        has_token("数学部長谷川", options, "数"),
        "数 is a token of 数学部長谷川"
    );
    assert!(
        has_token("成田空港", options, "成"),
        "成 is a token of 成田空港"
    );
}

/// `testDecomposition1`.
#[test]
fn decomposition1() {
    let input = "本来は、貧困層の女性や子供に医療保護を提供するために創設された制度である、アメリカ低所得者医療援助制度が、今日では、その予算の約３分の１を老人に費やしている。";
    assert_analyzes_to(
        &analyzer_no_punct(input),
        input,
        &[
            "本来",
            "は",
            "貧困",
            "層",
            "の",
            "女性",
            "や",
            "子供",
            "に",
            "医療",
            "保護",
            "を",
            "提供",
            "する",
            "ため",
            "に",
            "創設",
            "さ",
            "れ",
            "た",
            "制度",
            "で",
            "ある",
            "アメリカ",
            "低",
            "所得",
            "者",
            "医療",
            "援助",
            "制度",
            "が",
            "今日",
            "で",
            "は",
            "その",
            "予算",
            "の",
            "約",
            "３",
            "分の",
            "１",
            "を",
            "老人",
            "に",
            "費やし",
            "て",
            "いる",
        ],
        Some(&[
            0, 2, 4, 6, 7, 8, 10, 11, 13, 14, 16, 18, 19, 21, 23, 25, 26, 28, 29, 30, 31, 33, 34,
            37, 41, 42, 44, 45, 47, 49, 51, 53, 55, 56, 58, 60, 62, 63, 64, 65, 67, 68, 69, 71, 72,
            75, 76,
        ]),
        Some(&[
            2, 3, 6, 7, 8, 10, 11, 13, 14, 16, 18, 19, 21, 23, 25, 26, 28, 29, 30, 31, 33, 34, 36,
            41, 42, 44, 45, 47, 49, 51, 52, 55, 56, 57, 60, 62, 63, 64, 65, 67, 68, 69, 71, 72, 75,
            76, 78,
        ]),
        None,
        None,
    );
}

/// `testDecomposition2`.
#[test]
fn decomposition2() {
    let input = "麻薬の密売は根こそぎ絶やさなければならない";
    assert_analyzes_to(
        &analyzer_no_punct(input),
        input,
        &[
            "麻薬",
            "の",
            "密売",
            "は",
            "根こそぎ",
            "絶やさ",
            "なけれ",
            "ば",
            "なら",
            "ない",
        ],
        Some(&[0, 2, 3, 5, 6, 10, 13, 16, 17, 19]),
        Some(&[2, 3, 5, 6, 10, 13, 16, 17, 19, 21]),
        None,
        None,
    );
}

/// `testDecomposition3`.
#[test]
fn decomposition3() {
    let input = "魔女狩大将マシュー・ホプキンス。";
    assert_analyzes_to(
        &analyzer_no_punct(input),
        input,
        &["魔女", "狩", "大将", "マシュー", "ホプキンス"],
        Some(&[0, 2, 3, 5, 10]),
        Some(&[2, 3, 5, 9, 15]),
        None,
        None,
    );
}

/// `testDecomposition4`.
#[test]
fn decomposition4() {
    let input = "これは本ではない";
    assert_analyzes_to(
        &analyzer(input),
        input,
        &["これ", "は", "本", "で", "は", "ない"],
        Some(&[0, 2, 3, 4, 5, 6]),
        Some(&[2, 3, 4, 5, 6, 8]),
        None,
        None,
    );
}

/// `testTwoSentences`: the second sentence's offsets include the first's.
#[test]
fn two_sentences() {
    let input = "魔女狩大将マシュー・ホプキンス。 魔女狩大将マシュー・ホプキンス。";
    assert_analyzes_to(
        &analyzer_no_punct(input),
        input,
        &[
            "魔女",
            "狩",
            "大将",
            "マシュー",
            "ホプキンス",
            "魔女",
            "狩",
            "大将",
            "マシュー",
            "ホプキンス",
        ],
        Some(&[0, 2, 3, 5, 10, 17, 19, 20, 22, 27]),
        Some(&[2, 3, 5, 9, 15, 19, 20, 22, 26, 32]),
        None,
        None,
    );
}

/// `testSurrogates`.
#[test]
fn surrogates() {
    assert_eq!(
        texts(&analyzer("𩬅艱鍟䇹愯瀛")),
        ["𩬅", "艱", "鍟", "䇹", "愯", "瀛"]
    );
}

/// `testOnlyPunctuation`.
#[test]
fn only_punctuation() {
    assert!(analyzer_no_punct("。、。。").is_empty());
}

/// `testOnlyPunctuationExtended`.
#[test]
fn only_punctuation_extended() {
    assert!(extended_analyzer_no_punct("......").is_empty());
}

/// `testEnd`: trailing spaces produce no token (Lucene's expectation lists a seventh offset
/// pair, which its assertion never reaches; the final offsets 8 and 12 are not modelled).
#[test]
fn end() {
    let input = "これは本ではない";
    assert_analyzes_to(
        &analyzer_no_punct(input),
        input,
        &["これ", "は", "本", "で", "は", "ない"],
        Some(&[0, 2, 3, 4, 5, 6]),
        Some(&[2, 3, 4, 5, 6, 8]),
        None,
        None,
    );
    let input = "これは本ではない    ";
    assert_analyzes_to(
        &analyzer_no_punct(input),
        input,
        &["これ", "は", "本", "で", "は", "ない"],
        Some(&[0, 2, 3, 4, 5, 6, 8]),
        Some(&[2, 3, 4, 5, 6, 8, 9]),
        None,
        None,
    );
}

/// `testUserDict`: "not a great test because w/o userdict.txt the segmentation is the same";
/// note that the user entry yields no compound token even though compounds are kept.
#[test]
fn user_dict() {
    let input = "関西国際空港に行った";
    assert_analyzes_to(
        &analyzer(input),
        input,
        &["関西", "国際", "空港", "に", "行っ", "た"],
        Some(&[0, 2, 4, 6, 7, 9]),
        Some(&[2, 4, 6, 7, 9, 10]),
        None,
        None,
    );
}

/// `testUserDict2`: without the user dictionary the segmentation differs.
#[test]
fn user_dict2() {
    let input = "朝青龍";
    assert_analyzes_to(
        &analyzer(input),
        input,
        &["朝青龍"],
        Some(&[0]),
        Some(&[3]),
        None,
        None,
    );
}

/// `testUserDict3`: an entry that breaks into multiple tokens.
#[test]
fn user_dict3() {
    let input = "abcd";
    assert_analyzes_to(
        &analyzer(input),
        input,
        &["a", "b", "cd"],
        Some(&[0, 1, 2]),
        Some(&[1, 2, 4]),
        None,
        None,
    );
}

/// `testSegmentation` (the Michelle Kwan variant is commented out upstream).
#[test]
fn segmentation() {
    assert_eq!(
        texts(&analyzer("スペースステーションに行きます。うたがわしい。")),
        [
            "スペース",
            "ステーション",
            "に",
            "行き",
            "ます",
            "。",
            "うたがわしい",
            "。"
        ]
    );
}

/// `testReadings`.
#[test]
fn readings() {
    let toks = analyzer("寿司が食べたいです。");
    assert_eq!(
        attribute(&toks, |t| t.reading.as_ref()),
        [
            Some("スシ"),
            Some("ガ"),
            Some("タベ"),
            Some("タイ"),
            Some("デス"),
            Some("。")
        ]
    );
}

/// `testReadings2`.
#[test]
fn readings2() {
    let toks = analyzer("多くの学生が試験に落ちた。");
    assert_eq!(
        attribute(&toks, |t| t.reading.as_ref()),
        [
            Some("オオク"),
            Some("ノ"),
            Some("ガクセイ"),
            Some("ガ"),
            Some("シケン"),
            Some("ニ"),
            Some("オチ"),
            Some("タ"),
            Some("。"),
        ]
    );
}

/// `testPronunciations`.
#[test]
fn pronunciations() {
    let toks = analyzer("寿司が食べたいです。");
    assert_eq!(
        attribute(&toks, |t| t.pronunciation.as_ref()),
        [
            Some("スシ"),
            Some("ガ"),
            Some("タベ"),
            Some("タイ"),
            Some("デス"),
            Some("。")
        ]
    );
}

/// `testPronunciations2`: the pronunciation of 多く differs from its reading.
#[test]
fn pronunciations2() {
    let toks = analyzer("多くの学生が試験に落ちた。");
    assert_eq!(
        attribute(&toks, |t| t.pronunciation.as_ref()),
        [
            Some("オーク"),
            Some("ノ"),
            Some("ガクセイ"),
            Some("ガ"),
            Some("シケン"),
            Some("ニ"),
            Some("オチ"),
            Some("タ"),
            Some("。"),
        ]
    );
}

/// `testBasicForms`: null where the base form equals the surface.
#[test]
fn basic_forms() {
    let toks = analyzer("それはまだ実験段階にあります。");
    assert_eq!(
        attribute(&toks, |t| t.base_form.as_ref()),
        [None, None, None, None, None, None, Some("ある"), None, None]
    );
}

/// `testInflectionTypes`.
#[test]
fn inflection_types() {
    let toks = analyzer("それはまだ実験段階にあります。");
    assert_eq!(
        attribute(&toks, |t| t.inflection_type.as_ref()),
        [
            None,
            None,
            None,
            None,
            None,
            None,
            Some("五段・ラ行"),
            Some("特殊・マス"),
            None
        ]
    );
}

/// `testInflectionForms`.
#[test]
fn inflection_forms() {
    let toks = analyzer("それはまだ実験段階にあります。");
    assert_eq!(
        attribute(&toks, |t| t.inflection_form.as_ref()),
        [
            None,
            None,
            None,
            None,
            None,
            None,
            Some("連用形"),
            Some("基本形"),
            None
        ]
    );
}

/// `testPartOfSpeech`.
#[test]
fn part_of_speech() {
    let toks = analyzer("それはまだ実験段階にあります。");
    let pos: Vec<&str> = toks.iter().map(|t| t.part_of_speech.as_str()).collect();
    assert_eq!(
        pos,
        [
            "名詞-代名詞-一般",
            "助詞-係助詞",
            "副詞-助詞類接続",
            "名詞-サ変接続",
            "名詞-一般",
            "助詞-格助詞-一般",
            "動詞-自立",
            "助動詞",
            "記号-句点",
        ]
    );
}

/// `testYabottai`.
#[test]
fn yabottai() {
    assert_eq!(texts(&analyzer("やぼったい")), ["やぼったい"]);
}

/// `testTsukitosha`.
#[test]
fn tsukitosha() {
    assert_eq!(texts(&analyzer("突き通しゃ")), ["突き通しゃ"]);
}

/// `testWithPunctuation`: the `int[]` of this `assertAnalyzesTo` overload is position increments.
#[test]
fn with_punctuation() {
    let input = "羽田。空港";
    assert_analyzes_to(
        &analyzer_no_punct(input),
        input,
        &["羽田", "空港"],
        None,
        None,
        Some(&[1, 1]),
        None,
    );
}

/// `testCompoundOverPunctuation`: ϶ (U+03F6, a math symbol) is discarded as punctuation.
#[test]
fn compound_over_punctuation() {
    let input = "dεε϶ϢϏΎϷΞͺ羽田";
    assert_analyzes_to(
        &analyzer_no_punct(input),
        input,
        &["d", "ε", "ε", "ϢϏΎϷΞͺ", "羽田"],
        None,
        None,
        Some(&[1, 1, 1, 1, 1]),
        Some(&[1, 1, 1, 1, 1]),
    );
}

/// `testEmptyUserDict`: a dictionary of only comments and blank lines is no dictionary at all
/// (`UserDictionary.open` returns null); `JapaneseTokenizer(dict, false, SEARCH)` discards
/// compounds.
#[test]
fn empty_user_dict() {
    let dict = UserDictionary::parse("\n# This is an empty user dictionary\n\n", false).unwrap();
    assert!(dict.is_empty());
    let input = "これは本ではない";
    let toks = run_options(
        input,
        Options {
            mode: Mode::Search,
            discard_punctuation: false,
            discard_compound_token: true,
            user_dictionary: Some(&dict),
            ..Options::default()
        },
    );
    assert_analyzes_to(
        &toks,
        input,
        &["これ", "は", "本", "で", "は", "ない"],
        Some(&[0, 2, 3, 4, 5, 6]),
        Some(&[2, 3, 4, 5, 6, 8]),
        None,
        None,
    );
}

/// `testPatchedSystemDict`: the shipped dictionary is patched with 令和.
#[test]
fn patched_system_dict() {
    let input = "令和元年";
    for toks in [analyzer(input), analyzer_normal(input)] {
        assert_analyzes_to(
            &toks,
            input,
            &["令和", "元年"],
            Some(&[0, 2]),
            Some(&[2, 4]),
            None,
            None,
        );
    }
}

/// `testNoCompoundToken`.
#[test]
fn no_compound_token() {
    let input = "株式会社とアカデミア";
    assert_eq!(
        texts(&analyzer_normal(input)),
        ["株式会社", "と", "アカデミア"]
    );
    assert_eq!(
        texts(&analyzer(input)),
        ["株式", "株式会社", "会社", "と", "アカデミア"]
    );
    assert_eq!(
        texts(&analyzer_no_compound(input)),
        ["株式", "会社", "と", "アカデミア"]
    );
    assert_eq!(
        texts(&extended_analyzer_no_punct(input)),
        [
            "株式",
            "株式会社",
            "会社",
            "と",
            "ア",
            "カ",
            "デ",
            "ミ",
            "ア"
        ]
    );
    assert_eq!(
        texts(&extended_analyzer_no_compound(input)),
        ["株式", "会社", "と", "ア", "カ", "デ", "ミ", "ア"]
    );

    let input = "北海道日本ハムファイターズ";
    assert_eq!(
        texts(&analyzer(input)),
        ["北海道", "日本", "ハムファイターズ"]
    );
    assert_eq!(
        texts(&analyzer_no_compound(input)),
        ["北海道", "日本", "ハムファイターズ"]
    );
}

/// `testEmptyBacktrace`: 1023 × あ then 手紙. The first 1023 characters generate multiple paths
/// so that, with `MAX_BACKTRACE_GAP` 1024, the regular backtrace is not executed, and the last
/// two characters are a valid word so that they end up together.
#[test]
fn empty_backtrace() {
    let input = "あ".repeat(1023) + "手紙";
    let mut expected = vec!["ああ"; 511];
    expected.push("あ");
    expected.push("手紙");
    assert_eq!(texts(&analyzer(&input)), expected);
}

// --- TestSearchMode ------------------------------------------------------------------------------
// No user dictionary, punctuation discarded. The cases are search-segmentation-tests.txt,
// verbatim: a token marked `/0` is the compound, emitted at position increment 0 and spanning the
// other tokens of the line.

const SEARCH_SEGMENTATION_TESTS: &[(&str, &str)] = &[
    // Organizations
    ("関西国際空港", "関西 関西国際空港/0 国際 空港"),
    ("成田空港", "成田 成田空港/0 空港"),
    ("羽田空港", "羽田 羽田空港/0 空港"),
    (
        "奈良先端科学技術大学院大学",
        "奈良 奈良先端科学技術大学院大学/0 先端 科学 技術 大学院 大学",
    ),
    ("東京大学", "東京 東京大学/0 大学"),
    ("京都大学", "京都 京都大学/0 大学"),
    // "NOTE: differs from non-compound mode"
    ("京都大学硬式野球部", "京都大 学 硬式 野球 部"),
    // Katakana titles
    (
        "シニアソフトウェアエンジニア",
        "シニア シニアソフトウェアエンジニア/0 ソフトウェア エンジニア",
    ),
    ("ソフトウェアエンジニア", "ソフトウェア エンジニア"),
    (
        "シニアプロジェクトマネジャー",
        "シニア シニアプロジェクトマネジャー/0 プロジェクト マネジャー",
    ),
    ("プロジェクトマネジャー", "プロジェクト マネジャー"),
    (
        "シニアセールスエンジニア",
        "シニア シニアセールスエンジニア/0 セールス エンジニア",
    ),
    (
        "システムアーキテクト",
        "システム システムアーキテクト/0 アーキテクト",
    ),
    (
        "シニアシステムアーキテクト",
        "シニア シニアシステムアーキテクト/0 システム アーキテクト",
    ),
    ("システムアドミニストレータ", "システム アドミニストレータ"),
    (
        "システムアドミニストレーター",
        "システム システムアドミニストレーター/0 アドミニストレーター",
    ),
    (
        "シニアシステムアドミニストレーター",
        "シニア シニアシステムアドミニストレーター/0 システム アドミニストレーター",
    ),
    // Company names (several are fictitious)
    ("ソフトバンクモバイル", "ソフトバンク モバイル"),
    (
        "アルパインマテリアルズ",
        "アルパイン アルパインマテリアルズ/0 マテリアルズ",
    ),
    ("サッポロホールディングス", "サッポロ ホールディングス"),
    (
        "ヤマダコーポレーション",
        "ヤマダ ヤマダコーポレーション/0 コーポレーション",
    ),
    // "Semiconductor becomes semi + conductor"
    (
        "キヤノンセミコンダクターエクィップメント",
        "キヤノン キヤノンセミコンダクターエクィップメント/0 セミ コンダクター エクィップメント",
    ),
    (
        "オリエンタルチエン",
        "オリエンタル オリエンタルチエン/0 チエン",
    ),
    // "Becomes one token as プロジェクツ is not in IPADIC"
    (
        "アーリープロジェクツジャパン",
        "アーリープロジェクツジャパン",
    ),
    (
        "ピーターパンコーポレーション",
        "ピーター ピーターパンコーポレーション/0 パン コーポレーション",
    ),
    ("エイムクリエイツ", "エイムクリエイツ"),
    (
        "マースエンジニアリング",
        "マース マースエンジニアリング/0 エンジニアリング",
    ),
    (
        "フジプロテインテクノロジー",
        "フジ フジプロテインテクノロジー/0 プロテイン テクノロジー",
    ),
    // Person names
    ("マイケルジャクソン", "マイケル ジャクソン"),
    ("スティーブジョブズ", "スティーブ ジョブズ"),
    // "Becomes one token (short word)"
    ("ハリーポッター", "ハリーポッター"),
    ("ビルゲイツ", "ビルゲイツ"),
    // "Becomes one token (okay)"
    ("ショーンコネリー", "ショーンコネリー"),
    // Other nouns
    ("ホールディングス", "ホールディングス"),
    ("エンジニアリング", "エンジニアリング"),
    (
        "ソフトウェアエンジニアリング",
        "ソフトウェア エンジニアリング",
    ),
    ("ショッピングセンター", "ショッピング センター"),
    // "One token because of short word"
    ("ゲームセンター", "ゲームセンター"),
    ("クリスマスショッピング", "クリスマス ショッピング"),
    ("ダウンロードファイル", "ダウンロード ファイル"),
    ("テクノロジー", "テクノロジー"),
    ("リレハンメルオリンピック", "リレハンメル オリンピック"),
    // Problematic terms: "Becomes J Tien ginia ring (substrings are in IPADIC)"
    (
        "ジェイティエンジニアリング",
        "ジェイ ジェイティエンジニアリング/0 ティエン ジニア リング",
    ),
    // "Become Anch yvipasta"
    ("アンチョビパスタ", "アンチ アンチョビパスタ/0 ョビパスタ"),
    // "Becomes one token (surprise not in IPADIC)"
    ("サプライズギフト", "サプライズギフト"),
];

/// One expectation as `testSearchSegmentation` reads it: texts, position increments and lengths.
fn search_segmentation_expectation(line: &str) -> (Vec<&str>, Vec<usize>, Vec<u32>) {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let count = tokens.len();
    let mut texts = Vec::with_capacity(count);
    let mut incs = Vec::with_capacity(count);
    let mut lens = Vec::with_capacity(count);
    for token in tokens {
        match token.strip_suffix("/0") {
            Some(compound) => {
                texts.push(compound);
                incs.push(0);
                lens.push((count - 1) as u32);
            }
            None => {
                texts.push(token);
                incs.push(1);
                lens.push(1);
            }
        }
    }
    (texts, incs, lens)
}

/// `testSearchSegmentation`.
#[test]
fn search_segmentation() {
    assert_eq!(SEARCH_SEGMENTATION_TESTS.len(), 45);
    for (input, line) in SEARCH_SEGMENTATION_TESTS {
        let (texts, incs, lens) = search_segmentation_expectation(line);
        assert_analyzes_to(
            &run_tokenizer(input, TokenizerConfig::DEFAULT),
            input,
            &texts,
            None,
            None,
            Some(&incs),
            Some(&lens),
        );
    }
}

/// `testSearchSegmentationNoOriginal`: with compounds discarded, the `/0` tokens disappear and
/// every remaining token has increment and length 1.
#[test]
fn search_segmentation_no_original() {
    for (input, line) in SEARCH_SEGMENTATION_TESTS {
        let texts: Vec<&str> = line
            .split_whitespace()
            .filter(|t| !t.ends_with("/0"))
            .collect();
        assert_analyzes_to(
            &run_tokenizer(input, TokenizerConfig::named("nocompound")),
            input,
            &texts,
            None,
            None,
            Some(&vec![1; texts.len()]),
            Some(&vec![1; texts.len()]),
        );
    }
}

// --- TestExtendedMode ----------------------------------------------------------------------------
// `JapaneseTokenizer(attr, null, true, EXTENDED)`: punctuation and compounds discarded.
// Skipped: testSurrogates2, testRandomStrings, testRandomHugeStrings(AtNight) (random inputs).

/// `testSurrogates`.
#[test]
fn extended_surrogates() {
    let config = TokenizerConfig {
        mode: Mode::Extended,
        discard_compound: true,
        ..TokenizerConfig::DEFAULT
    };
    assert_eq!(
        texts(&run_tokenizer("𩬅艱鍟䇹愯瀛", config)),
        ["𩬅", "艱", "鍟", "䇹", "愯", "瀛"]
    );
}

// --- TestJapaneseTokenizerFactory ----------------------------------------------------------------
// Lucene's factory defaults differ from Elasticsearch's: compounds are discarded.
// Skipped: testBogusArguments (factory argument validation).

/// `JapaneseTokenizerFactory` with no arguments: SEARCH, punctuation and compounds discarded.
fn lucene_factory() -> TokenizerConfig {
    TokenizerConfig {
        discard_compound: true,
        ..TokenizerConfig::DEFAULT
    }
}

/// `testSimple`.
#[test]
fn factory_simple() {
    let input = "これは本ではない";
    assert_analyzes_to(
        &run_tokenizer(input, lucene_factory()),
        input,
        &["これ", "は", "本", "で", "は", "ない"],
        Some(&[0, 2, 3, 4, 5, 6]),
        Some(&[2, 3, 4, 5, 6, 8]),
        None,
        None,
    );
}

/// `testDefaults`: search mode by default.
#[test]
fn factory_defaults() {
    assert_eq!(
        texts(&run_tokenizer(
            "シニアソフトウェアエンジニア",
            lucene_factory()
        )),
        ["シニア", "ソフトウェア", "エンジニア"]
    );
}

/// `testMode`: `mode=normal`.
#[test]
fn factory_mode() {
    let config = TokenizerConfig {
        mode: Mode::Normal,
        ..lucene_factory()
    };
    assert_eq!(
        texts(&run_tokenizer("シニアソフトウェアエンジニア", config)),
        ["シニアソフトウェアエンジニア"]
    );
}

/// `testUserDict`: the inline dictionary of the test (the first three entries of userdict.txt).
#[test]
fn factory_user_dict() {
    let rules = "# Custom segmentation for long entries\n\
                 日本経済新聞,日本 経済 新聞,ニホン ケイザイ シンブン,カスタム名詞\n\
                 関西国際空港,関西 国際 空港,カンサイ コクサイ クウコウ,テスト名詞\n\
                 # Custom reading for sumo wrestler\n\
                 朝青龍,朝青龍,アサショウリュウ,カスタム人名\n";
    let dict = UserDictionary::parse(rules, false).unwrap();
    let toks = run_options(
        "関西国際空港に行った",
        Options {
            discard_compound_token: true,
            user_dictionary: Some(&dict),
            ..Options::default()
        },
    );
    assert_eq!(texts(&toks), ["関西", "国際", "空港", "に", "行っ", "た"]);
}

/// `testPreservePunctuation`: `discardPunctuation=false`.
#[test]
fn factory_preserve_punctuation() {
    let config = TokenizerConfig {
        keep_punctuation: true,
        ..lucene_factory()
    };
    assert_eq!(
        texts(&run_tokenizer(
            "今ノルウェーにいますが、来週の頭日本に戻ります。楽しみにしています！お寿司が食べたいな。。。",
            config
        )),
        [
            "今",
            "ノルウェー",
            "に",
            "い",
            "ます",
            "が",
            "、",
            "来週",
            "の",
            "頭",
            "日本",
            "に",
            "戻り",
            "ます",
            "。",
            "楽しみ",
            "に",
            "し",
            "て",
            "い",
            "ます",
            "！",
            "お",
            "寿司",
            "が",
            "食べ",
            "たい",
            "な",
            "。",
            "。",
            "。",
        ]
    );
}

/// `testPreserveCompoundToken`: `discardCompoundToken=false`.
#[test]
fn factory_preserve_compound_token() {
    assert_eq!(
        texts(&run_tokenizer(
            "シニアソフトウェアエンジニア",
            TokenizerConfig::DEFAULT
        )),
        [
            "シニア",
            "シニアソフトウェアエンジニア",
            "ソフトウェア",
            "エンジニア"
        ]
    );
}

/// `testNbestCost`: `nBestCost=2000`.
#[test]
fn factory_nbest_cost() {
    let config = TokenizerConfig {
        nbest_cost: 2000,
        ..lucene_factory()
    };
    assert_eq!(
        texts(&run_tokenizer("鳩山積み", config)),
        ["鳩", "鳩山", "山積み", "積み"]
    );
}

/// `testNbestExample`: `nBestExamples=/鳩山積み-鳩山/鳩山積み-鳩/`.
#[test]
fn factory_nbest_example() {
    let config = TokenizerConfig {
        nbest_examples: Some("/鳩山積み-鳩山/鳩山積み-鳩/"),
        ..lucene_factory()
    };
    assert_eq!(
        texts(&run_tokenizer("鳩山積み", config)),
        ["鳩", "鳩山", "山積み", "積み"]
    );
}

// --- TestJapaneseAnalyzer ------------------------------------------------------------------------
// Skipped: testResourcesAvailable (only constructs the analyzer), testRandom and
// testRandomHugeStrings (random inputs), testCuriousString through test5thCuriousString
// (checkAnalysisConsistency only: no expected tokens, and the 4th and 5th contain lone
// surrogates).

/// `JapaneseAnalyzer(userdict, SEARCH, default stop set, default stop tags)` as a chain, minus
/// the final `LowerCaseFilter` (see [`java_lowercased`]).
fn lucene_analyzer_with_user_dict(input: &str) -> Vec<Tok> {
    let config = TokenizerConfig {
        discard_compound: true,
        user_dict: Some("lucene"),
        ..TokenizerConfig::DEFAULT
    };
    run_chain(
        input,
        Some(CharFilter::CjkWidth),
        config,
        &[
            Filter::BaseForm,
            Filter::PosStop(StopTags::defaults()),
            Filter::Stop(StopWords::japanese()),
            Filter::Stem(4),
        ],
    )
}

/// `testBasics`: particles removed by part of speech, lemmatization with the base form, and the
/// position gaps the removals leave.
#[test]
fn analyzer_basics() {
    let input = "多くの学生が試験に落ちた。";
    assert_analyzes_to(
        &run_analyzer(input, "default"),
        input,
        &["多く", "学生", "試験", "落ちる"],
        Some(&[0, 3, 6, 9]),
        Some(&[2, 5, 8, 11]),
        Some(&[1, 2, 2, 2]),
        None,
    );
}

/// `testDecomposition`: search mode is on by default (`assertAnalyzesToPositions`, every
/// increment and length 1).
#[test]
fn analyzer_decomposition() {
    let cases: [(&str, &[&str]); 6] = [
        (
            "シニアソフトウェアエンジニア",
            &["シニア", "ソフトウェア", "エンジニア"],
        ),
        // Trailing ー removed by stemming.
        (
            "シニアプロジェクトマネージャー",
            &["シニア", "プロジェクト", "マネージャ"],
        ),
        ("関西国際空港", &["関西", "国際", "空港"]),
        // "Not quite the right segmentation (see LUCENE-3726)".
        (
            "コニカミノルタホールディングス",
            &["コニカ", "ミノルタ", "ホールディングス"],
        ),
        ("成田空港", &["成田", "空港"]),
        (
            "京都大学硬式野球部",
            &["京都大", "学", "硬式", "野球", "部"],
        ),
    ];
    for (input, expected) in cases {
        assert_analyzes_to(
            &run_analyzer(input, "default"),
            input,
            expected,
            None,
            None,
            Some(&vec![1; expected.len()]),
            Some(&vec![1; expected.len()]),
        );
    }
}

/// `testUserDict3`: the user dictionary passed to the analyzer works (an entry that breaks into
/// multiple tokens). Built as a chain with Lucene's exact dictionary; the "userdict" analyzer
/// configuration (the edge dictionary) contains Lucene's entries too, but the chain is exact.
#[test]
fn analyzer_user_dict3() {
    let input = "abcd";
    assert_analyzes_to(
        &java_lowercased(&lucene_analyzer_with_user_dict(input)),
        input,
        &["a", "b", "cd"],
        Some(&[0, 1, 2]),
        Some(&[1, 2, 4]),
        None,
        None,
    );
}

/// `testCharWidthNormalization`: `CJKWidthCharFilter` folds the fullwidth digits, with offsets
/// mapped back to the input.
#[test]
fn analyzer_char_width_normalization() {
    let input = "新橋６－２０－１";
    assert_analyzes_to(
        &java_lowercased(&lucene_analyzer_with_user_dict(input)),
        input,
        &["新橋", "6", "20", "1"],
        Some(&[0, 2, 4, 7]),
        Some(&[2, 3, 6, 8]),
        None,
        None,
    );
}

// --- Elasticsearch: KuromojiAnalysisTests --------------------------------------------------------
// Tokenizers from kuromoji_analysis.json have Elasticsearch's defaults (search mode, punctuation
// discarded, compounds kept).

fn es_analyze(input: &str, options: AnalyzerOptions<'_>) -> Vec<Tok> {
    let mut tokens = Tokens::new();
    kuromoji::analyze(input, options, &mut tokens);
    super::collect(input, &tokens)
}

/// `testKuromojiUserDict`: `kuromoji_user_dict` with user_dict.txt (制限スピード).
#[test]
fn es_kuromoji_user_dict() {
    let config = TokenizerConfig {
        user_dict: Some("es"),
        ..TokenizerConfig::DEFAULT
    };
    assert_eq!(
        texts(&run_tokenizer("私は制限スピードを超える。", config)),
        ["私", "は", "制限スピード", "を", "超える"]
    );
}

/// `testNbestCost`: `kuromoji_nbest_cost` (`nbest_cost: 2000`).
#[test]
fn es_nbest_cost() {
    assert_eq!(
        texts(&run_tokenizer("鳩山積み", TokenizerConfig::named("nbest"))),
        ["鳩", "鳩山", "山積み", "積み"]
    );
}

/// `testNbestExample`: `kuromoji_nbest_examples` (`nbest_examples: /鳩山積み-鳩山/鳩山積み-鳩/`).
#[test]
fn es_nbest_example() {
    assert_eq!(
        texts(&run_tokenizer(
            "鳩山積み",
            TokenizerConfig::named("nbest_examples")
        )),
        ["鳩", "鳩山", "山積み", "積み"]
    );
}

/// `testNbestBothOptions`: `kuromoji_nbest_both` (the examples plus `nbest_cost: 1000`; the
/// larger of the two applies).
#[test]
fn es_nbest_both_options() {
    let config = TokenizerConfig {
        nbest_cost: 1000,
        ..TokenizerConfig::named("nbest_examples")
    };
    assert_eq!(
        texts(&run_tokenizer("鳩山積み", config)),
        ["鳩", "鳩山", "山積み", "積み"]
    );
}

/// `testKuromojiAnalyzerUserDict`: the `kuromoji` analyzer with `user_dictionary_rules`
/// `c++,c++,w,w` and `制限スピード,制限スピード,セイゲンスピード,テスト名詞` (userdict/es.txt).
#[test]
fn es_kuromoji_analyzer_user_dict() {
    let options = AnalyzerOptions {
        user_dictionary: Some(user_dictionary("es")),
        ..AnalyzerOptions::default()
    };
    assert_eq!(
        texts(&es_analyze("制限スピード", options)),
        ["制限スピード"]
    );
    assert_eq!(texts(&es_analyze("c++world", options)), ["c++", "world"]);
}

/// `testDiscardCompoundToken`: `kuromoji_discard_compound_token`.
#[test]
fn es_discard_compound_token() {
    assert_eq!(
        texts(&run_tokenizer(
            "株式会社",
            TokenizerConfig::named("nocompound")
        )),
        ["株式", "会社"]
    );
}

// --- Elasticsearch: REST tests (10_basic.yml, 20_search.yml) -------------------------------------

/// 10_basic.yml "Analyzer": the `kuromoji` analyzer lowercases JR.
#[test]
fn es_rest_analyzer() {
    let toks = java_lowercased(&run_analyzer_unlowercased(
        "JR新宿駅の近くにビールを飲みに行こうか",
        "default",
    ));
    assert_eq!(
        texts(&toks),
        ["jr", "新宿", "駅", "近く", "ビール", "飲む", "行く"]
    );
}

/// 10_basic.yml "Tokenizer": `kuromoji_tokenizer` with its defaults.
#[test]
fn es_rest_tokenizer() {
    assert_eq!(
        texts(&run_tokenizer("関西国際空港", TokenizerConfig::DEFAULT)),
        ["関西", "関西国際空港", "国際", "空港"]
    );
}

/// 20_search.yml indexes "JR新宿駅の近くにビールを飲みに行こうか" with the `kuromoji` analyzer and
/// finds it with a match query for "jr": the analyzed document contains the analyzed query.
#[test]
fn es_rest_search() {
    let doc = java_lowercased(&run_analyzer_unlowercased(
        "JR新宿駅の近くにビールを飲みに行こうか",
        "default",
    ));
    let query = java_lowercased(&run_analyzer_unlowercased("jr", "default"));
    assert_eq!(texts(&query), ["jr"]);
    assert!(texts(&doc).contains(&"jr"));
}

// --- Elasticsearch: analysis-kuromoji-tokenizer.md -----------------------------------------------

/// The `mode` examples: 関西国際空港 and アブラカダブラ in normal, search and extended mode.
#[test]
fn docs_modes() {
    let normal = TokenizerConfig::named("normal");
    let search = TokenizerConfig::DEFAULT;
    let extended = TokenizerConfig::named("extended");
    assert_eq!(
        texts(&run_tokenizer("関西国際空港", normal)),
        ["関西国際空港"]
    );
    assert_eq!(
        texts(&run_tokenizer("アブラカダブラ", normal)),
        ["アブラカダブラ"]
    );
    assert_eq!(
        texts(&run_tokenizer("関西国際空港", search)),
        ["関西", "関西国際空港", "国際", "空港"]
    );
    assert_eq!(
        texts(&run_tokenizer("アブラカダブラ", search)),
        ["アブラカダブラ"]
    );
    assert_eq!(
        texts(&run_tokenizer("関西国際空港", extended)),
        ["関西", "関西国際空港", "国際", "空港"]
    );
    assert_eq!(
        texts(&run_tokenizer("アブラカダブラ", extended)),
        ["ア", "ブ", "ラ", "カ", "ダ", "ブ", "ラ"]
    );
}

/// The `discard_compound_token` example: in search or extended mode the compound is dropped.
#[test]
fn docs_discard_compound_token() {
    for mode in [Mode::Search, Mode::Extended] {
        let config = TokenizerConfig {
            mode,
            discard_compound: true,
            ..TokenizerConfig::DEFAULT
        };
        assert_eq!(
            texts(&run_tokenizer("関西国際空港", config)),
            ["関西", "国際", "空港"]
        );
    }
}

/// The user dictionary example: `kuromoji_user_dict` (extended mode, punctuation kept,
/// userdict_ja.txt with 東京スカイツリー) and the inline `user_dictionary_rules` variant (extended
/// mode, punctuation discarded) both analyze 東京スカイツリー to 東京 (0-2, position 0) and
/// スカイツリー (2-8, position 1).
#[test]
fn docs_user_dictionary() {
    let input = "東京スカイツリー";
    let from_file = TokenizerConfig {
        mode: Mode::Extended,
        keep_punctuation: true,
        user_dict: Some("docs"),
        ..TokenizerConfig::DEFAULT
    };
    let from_rules = TokenizerConfig {
        keep_punctuation: false,
        ..from_file
    };
    for config in [from_file, from_rules] {
        assert_analyzes_to(
            &run_tokenizer(input, config),
            input,
            &["東京", "スカイツリー"],
            Some(&[0, 2]),
            Some(&[2, 8]),
            Some(&[1, 1]),
            None,
        );
    }
}
