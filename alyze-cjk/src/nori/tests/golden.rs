//! Differential tests against the golden files: every case file through every tokenizer
//! configuration, the `nori` analyzer and the custom filter chains that `gen.sh` runs.

use super::{
    Compare, TokenizerConfig, assert_cases_match, chain, read_cases, read_testdata,
    read_token_golden, run_analyzer, run_chain, run_tokenizer,
};

fn tokens(case: &str, config: &str) {
    let inputs = read_cases(&read_testdata(&format!("cases/{case}.txt")));
    let golden = read_token_golden(&read_testdata(&format!("golden/{case}.{config}.tokens")));
    let config = TokenizerConfig::named(config);
    assert_cases_match(
        &format!("cases/{case}.txt"),
        &inputs,
        &golden,
        |input| run_tokenizer(input, config),
        Compare::TOKENIZER,
    );
}

fn analyze(case: &str, config: &str) {
    let inputs = read_cases(&read_testdata(&format!("cases/{case}.txt")));
    let golden = read_token_golden(&read_testdata(&format!("golden/{case}.{config}.analyze")));
    assert_cases_match(
        &format!("cases/{case}.txt"),
        &inputs,
        &golden,
        |input| run_analyzer(input, config),
        Compare::ANALYZER,
    );
}

fn chained(case: &str, name: &str) {
    let inputs = read_cases(&read_testdata(&format!("cases/{case}.txt")));
    let golden = read_token_golden(&read_testdata(&format!("golden/{case}.{name}.chain")));
    let (config, filters) = chain(name);
    let compare = Compare {
        attributes: !filters.contains(&super::Filter::Number),
        lowercases: filters.contains(&super::Filter::Lowercase),
    };
    assert_cases_match(
        &format!("cases/{case}.txt"),
        &inputs,
        &golden,
        |input| run_chain(input, config, &filters),
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
    tokens_upstream_mixed = ("upstream", "mixed"),
    tokens_upstream_none = ("upstream", "none"),
    tokens_upstream_punct = ("upstream", "punct"),
    tokens_upstream_mixed_punct = ("upstream", "mixed_punct"),
    tokens_upstream_unigrams = ("upstream", "unigrams"),
    tokens_upstream_userdict = ("upstream", "userdict"),
    tokens_upstream_userdict_mixed_punct = ("upstream", "userdict_mixed_punct"),
    tokens_edge_default = ("edge", "default"),
    tokens_edge_mixed = ("edge", "mixed"),
    tokens_edge_none = ("edge", "none"),
    tokens_edge_punct = ("edge", "punct"),
    tokens_edge_mixed_punct = ("edge", "mixed_punct"),
    tokens_edge_unigrams = ("edge", "unigrams"),
    tokens_edge_userdict = ("edge", "userdict"),
    tokens_edge_userdict_mixed_punct = ("edge", "userdict_mixed_punct"),
    tokens_fuzz_default = ("fuzz", "default"),
    tokens_fuzz_mixed_punct = ("fuzz", "mixed_punct"),
    tokens_fuzz_unigrams = ("fuzz", "unigrams"),
    tokens_fuzz_userdict_mixed_punct = ("fuzz", "userdict_mixed_punct"),
    tokens_wiki_ko_default = ("wiki_ko", "default"),
}

golden_tests! { analyze:
    analyze_upstream_default = ("upstream", "default"),
    analyze_upstream_mixed = ("upstream", "mixed"),
    analyze_upstream_stoptags = ("upstream", "stoptags"),
    analyze_upstream_userdict = ("upstream", "userdict"),
    analyze_edge_default = ("edge", "default"),
    analyze_edge_mixed = ("edge", "mixed"),
    analyze_edge_stoptags = ("edge", "stoptags"),
    analyze_edge_userdict = ("edge", "userdict"),
    analyze_fuzz_default = ("fuzz", "default"),
    analyze_fuzz_mixed = ("fuzz", "mixed"),
}

golden_tests! { chained:
    chain_upstream_reading = ("upstream", "reading"),
    chain_upstream_number = ("upstream", "number"),
    chain_upstream_number_punct = ("upstream", "number_punct"),
    chain_upstream_number_mixed_punct = ("upstream", "number_mixed_punct"),
    chain_upstream_pos_custom = ("upstream", "pos_custom"),
    chain_upstream_graph = ("upstream", "graph"),
    chain_upstream_lowercase = ("upstream", "lowercase"),
    chain_edge_reading = ("edge", "reading"),
    chain_edge_number = ("edge", "number"),
    chain_edge_number_punct = ("edge", "number_punct"),
    chain_edge_number_mixed_punct = ("edge", "number_mixed_punct"),
    chain_edge_pos_custom = ("edge", "pos_custom"),
    chain_edge_graph = ("edge", "graph"),
    chain_edge_lowercase = ("edge", "lowercase"),
    chain_fuzz_reading = ("fuzz", "reading"),
    chain_fuzz_number_mixed_punct = ("fuzz", "number_mixed_punct"),
    chain_fuzz_graph = ("fuzz", "graph"),
}

/// Ad-hoc large differential run over any case file + default-configuration tokenizer golden,
/// given by `NORI_CASES` and `NORI_TOKENS` (see `testdata/nori/README.md`). Ignored by default.
#[test]
#[ignore]
fn tokens_large() {
    let cases_path = std::env::var("NORI_CASES").expect("set NORI_CASES to a case file");
    let golden_path =
        std::env::var("NORI_TOKENS").expect("set NORI_TOKENS to its default .tokens golden");
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
