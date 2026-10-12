//! Differential tests against the golden files: every case file through every tokenizer
//! configuration, the `kuromoji` and `kuromoji_completion` analyzers and the custom filter chains
//! that `gen.sh` runs.

use super::{
    CharFilter, Compare, Filter, TokenizerConfig, analyzer_chain, analyzer_options,
    assert_cases_match, chain, completion_chain, completion_options, read_cases, read_testdata,
    read_token_golden, run_analyzer, run_analyzer_unlowercased, run_chain, run_completion,
    run_completion_unlowercased, run_tokenizer,
};
use crate::kuromoji::filter;

fn cases(case: &str) -> Vec<String> {
    read_cases(&read_testdata(&format!("cases/{case}.txt")))
}

fn tokens(case: &str, config: &str) {
    let golden = read_token_golden(&read_testdata(&format!("golden/{case}.{config}.tokens")));
    let config = TokenizerConfig::named(config);
    assert_cases_match(
        &format!("cases/{case}.txt"),
        &cases(case),
        &golden,
        |input| run_tokenizer(input, config),
        Compare::TOKENIZER,
    );
}

fn analyze(case: &str, config: &str) {
    let golden = read_token_golden(&read_testdata(&format!("golden/{case}.{config}.analyze")));
    assert_cases_match(
        &format!("cases/{case}.txt"),
        &cases(case),
        &golden,
        |input| run_analyzer_unlowercased(input, config),
        Compare {
            attributes: true,
            lowercases: true,
        },
    );
}

fn completion(case: &str, config: &str) {
    let golden = read_token_golden(&read_testdata(&format!(
        "golden/{case}.{config}.completion"
    )));
    assert_cases_match(
        &format!("cases/{case}.txt"),
        &cases(case),
        &golden,
        |input| run_completion_unlowercased(input, config),
        Compare {
            attributes: false,
            lowercases: true,
        },
    );
}

fn chained(case: &str, name: &str) {
    let golden = read_token_golden(&read_testdata(&format!("golden/{case}.{name}.chain")));
    let (char_filter, config, mut filters) = chain(name);
    // Lucene's lowercasing is applied to the port's unlowercased text instead (see `Compare`).
    let lowercases = filters.last() == Some(&Filter::Lowercase);
    if lowercases {
        filters.pop();
    }
    assert!(
        !filters.contains(&Filter::Lowercase),
        "lowercase only as the last filter"
    );
    let compare = Compare {
        attributes: !filters.iter().any(|f| matches!(f, Filter::Completion(_))),
        lowercases,
    };
    assert_cases_match(
        &format!("cases/{case}.txt"),
        &cases(case),
        &golden,
        |input| run_chain(input, char_filter, config, &filters),
        compare,
    );
}

macro_rules! golden_tests {
    ($kind:ident: $( $name:ident = ($case:literal, $config:literal), )*) => {
        $(
            #[test]
            fn $name() {
                $kind($case, $config);
            }
        )*
    };
}

golden_tests! { tokens:
    tokens_upstream_default = ("upstream", "default"),
    tokens_upstream_normal = ("upstream", "normal"),
    tokens_upstream_extended = ("upstream", "extended"),
    tokens_upstream_nocompound = ("upstream", "nocompound"),
    tokens_upstream_punct = ("upstream", "punct"),
    tokens_upstream_normal_punct = ("upstream", "normal_punct"),
    tokens_upstream_extended_punct = ("upstream", "extended_punct"),
    tokens_upstream_nbest = ("upstream", "nbest"),
    tokens_upstream_normal_nbest = ("upstream", "normal_nbest"),
    tokens_upstream_nbest_examples = ("upstream", "nbest_examples"),
    tokens_upstream_userdict = ("upstream", "userdict"),
    tokens_upstream_userdict_normal = ("upstream", "userdict_normal"),
    tokens_upstream_userdict_extended_punct = ("upstream", "userdict_extended_punct"),
    tokens_edge_default = ("edge", "default"),
    tokens_edge_normal = ("edge", "normal"),
    tokens_edge_extended = ("edge", "extended"),
    tokens_edge_nocompound = ("edge", "nocompound"),
    tokens_edge_punct = ("edge", "punct"),
    tokens_edge_normal_punct = ("edge", "normal_punct"),
    tokens_edge_extended_punct = ("edge", "extended_punct"),
    tokens_edge_nbest = ("edge", "nbest"),
    tokens_edge_normal_nbest = ("edge", "normal_nbest"),
    tokens_edge_nbest_examples = ("edge", "nbest_examples"),
    tokens_edge_userdict = ("edge", "userdict"),
    tokens_edge_userdict_normal = ("edge", "userdict_normal"),
    tokens_edge_userdict_extended_punct = ("edge", "userdict_extended_punct"),
    tokens_long_default = ("long", "default"),
    tokens_long_extended_punct = ("long", "extended_punct"),
    tokens_long_nbest = ("long", "nbest"),
    tokens_long_userdict = ("long", "userdict"),
    tokens_fuzz_default = ("fuzz", "default"),
    tokens_fuzz_extended_punct = ("fuzz", "extended_punct"),
    tokens_fuzz_nbest = ("fuzz", "nbest"),
    tokens_fuzz_userdict = ("fuzz", "userdict"),
    tokens_wiki_ja_default = ("wiki_ja", "default"),
}

golden_tests! { analyze:
    analyze_upstream_default = ("upstream", "default"),
    analyze_upstream_normal = ("upstream", "normal"),
    analyze_upstream_extended = ("upstream", "extended"),
    analyze_upstream_userdict = ("upstream", "userdict"),
    analyze_upstream_stopwords = ("upstream", "stopwords"),
    analyze_edge_default = ("edge", "default"),
    analyze_edge_normal = ("edge", "normal"),
    analyze_edge_extended = ("edge", "extended"),
    analyze_edge_userdict = ("edge", "userdict"),
    analyze_edge_stopwords = ("edge", "stopwords"),
    analyze_long_default = ("long", "default"),
    analyze_fuzz_default = ("fuzz", "default"),
}

golden_tests! { completion:
    completion_index = ("completion", "index"),
    completion_query = ("completion", "query"),
    completion_userdict = ("completion", "userdict"),
}

golden_tests! { chained:
    chain_upstream_baseform = ("upstream", "baseform"),
    chain_upstream_pos = ("upstream", "pos"),
    chain_upstream_pos_docs = ("upstream", "pos_docs"),
    chain_upstream_pos_verb = ("upstream", "pos_verb"),
    chain_upstream_reading = ("upstream", "reading"),
    chain_upstream_romaji = ("upstream", "romaji"),
    chain_upstream_width_reading = ("upstream", "width_reading"),
    chain_upstream_width_romaji = ("upstream", "width_romaji"),
    chain_upstream_stem = ("upstream", "stem"),
    chain_upstream_stem6 = ("upstream", "stem6"),
    chain_upstream_stop = ("upstream", "stop"),
    chain_upstream_stop_custom = ("upstream", "stop_custom"),
    chain_upstream_stop_custom_nocase = ("upstream", "stop_custom_nocase"),
    chain_upstream_number = ("upstream", "number"),
    chain_upstream_number_nbest = ("upstream", "number_nbest"),
    chain_upstream_hiragana_upper = ("upstream", "hiragana_upper"),
    chain_upstream_katakana_upper = ("upstream", "katakana_upper"),
    chain_upstream_itermark_kanji = ("upstream", "itermark_kanji"),
    chain_upstream_itermark_kana = ("upstream", "itermark_kana"),
    chain_upstream_itermark_punct = ("upstream", "itermark_punct"),
    chain_upstream_width_char_punct = ("upstream", "width_char_punct"),
    chain_upstream_lowercase = ("upstream", "lowercase"),
    chain_upstream_recommended = ("upstream", "recommended"),
    chain_edge_baseform = ("edge", "baseform"),
    chain_edge_pos = ("edge", "pos"),
    chain_edge_pos_docs = ("edge", "pos_docs"),
    chain_edge_pos_verb = ("edge", "pos_verb"),
    chain_edge_reading = ("edge", "reading"),
    chain_edge_romaji = ("edge", "romaji"),
    chain_edge_width_reading = ("edge", "width_reading"),
    chain_edge_width_romaji = ("edge", "width_romaji"),
    chain_edge_stem = ("edge", "stem"),
    chain_edge_stem6 = ("edge", "stem6"),
    chain_edge_stop = ("edge", "stop"),
    chain_edge_stop_custom = ("edge", "stop_custom"),
    chain_edge_stop_custom_nocase = ("edge", "stop_custom_nocase"),
    chain_edge_number = ("edge", "number"),
    chain_edge_number_nbest = ("edge", "number_nbest"),
    chain_edge_hiragana_upper = ("edge", "hiragana_upper"),
    chain_edge_katakana_upper = ("edge", "katakana_upper"),
    chain_edge_itermark_kanji = ("edge", "itermark_kanji"),
    chain_edge_itermark_kana = ("edge", "itermark_kana"),
    chain_edge_itermark_punct = ("edge", "itermark_punct"),
    chain_edge_width_char_punct = ("edge", "width_char_punct"),
    chain_edge_lowercase = ("edge", "lowercase"),
    chain_edge_recommended = ("edge", "recommended"),
    chain_long_number_nbest = ("long", "number_nbest"),
    chain_long_recommended = ("long", "recommended"),
    chain_fuzz_number_nbest = ("fuzz", "number_nbest"),
    chain_fuzz_itermark_punct = ("fuzz", "itermark_punct"),
    chain_fuzz_recommended = ("fuzz", "recommended"),
}

/// The goldens of lowercasing stages are compared against the port's unlowercased text through
/// Java's simple lowercase mapping; this pins down the other half: the analyzers' real output is
/// exactly the unlowercased pipeline through the port's own lowercase filter, and the two
/// lowercase mappings agree wherever the input has no code point from the documented exception
/// list (checked exhaustively in `unicode::lowercase_matches_java_except_expansions`).
#[test]
fn lowercase_is_the_only_difference() {
    for case in ["upstream", "edge"] {
        for input in cases(case) {
            for name in ["default", "userdict", "stopwords"] {
                let mut expected = run_analyzer_unlowercased(&input, name);
                for t in &mut expected {
                    t.text = filter::lowercase_text(&t.text);
                }
                assert_eq!(run_analyzer(&input, name), expected, "{name}: {input:?}");
            }
            let (cf, config, filters) = chain("recommended");
            let mut expected = run_chain(&input, cf, config, &filters[..filters.len() - 1]);
            for t in &mut expected {
                t.text = filter::lowercase_text(&t.text);
            }
            assert_eq!(run_chain(&input, cf, config, &filters), expected);
        }
    }
    for input in cases("completion") {
        for name in ["index", "query"] {
            let mut expected = run_completion_unlowercased(&input, name);
            for t in &mut expected {
                t.text = filter::lowercase_text(&t.text);
            }
            assert_eq!(run_completion(&input, name), expected, "{name}: {input:?}");
        }
    }
}

/// The analyzers' chains, as the goldens see them, are what `analyze` and `analyze_completion`
/// are documented to run (same char filter, same tokenizer options, same filters, same order).
#[test]
fn analyzer_chains_are_as_documented() {
    let (cf, config, filters) = analyzer_chain(analyzer_options("default"));
    assert_eq!(cf, Some(CharFilter::CjkWidth));
    assert_eq!(config, TokenizerConfig::named("nocompound"));
    assert!(matches!(
        filters.as_slice(),
        [
            Filter::BaseForm,
            Filter::PosStop(_),
            Filter::Stop(_),
            Filter::Stem(4)
        ]
    ));
    let (cf, config, filters) = completion_chain(completion_options("query"));
    assert_eq!(cf, Some(CharFilter::CjkWidth));
    assert_eq!(
        config,
        TokenizerConfig {
            discard_compound: true,
            ..TokenizerConfig::named("normal")
        }
    );
    assert!(matches!(
        filters.as_slice(),
        [Filter::Completion(filter::CompletionMode::Query)]
    ));
}

/// Ad-hoc large differential run over any case file + default-configuration tokenizer golden,
/// given by `KUROMOJI_CASES` and `KUROMOJI_TOKENS` (see `testdata/kuromoji/README.md`). Ignored
/// by default.
#[test]
#[ignore]
fn tokens_large() {
    let cases_path = std::env::var("KUROMOJI_CASES").expect("set KUROMOJI_CASES to a case file");
    let golden_path = std::env::var("KUROMOJI_TOKENS")
        .expect("set KUROMOJI_TOKENS to its default .tokens golden");
    let inputs = read_cases(&std::fs::read_to_string(&cases_path).unwrap());
    let golden = read_token_golden(&std::fs::read_to_string(&golden_path).unwrap());
    assert_cases_match(
        &cases_path,
        &inputs,
        &golden,
        |input| run_tokenizer(input, TokenizerConfig::DEFAULT),
        Compare::TOKENIZER,
    );
}
