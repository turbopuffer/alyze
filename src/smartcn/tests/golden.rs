//! Differential tests against the golden files: every case file at the tokenizer stage (exact)
//! and the analyzer stage (exact except for English stems).

use super::{
    TextMatch, assert_cases_match, read_cases, read_testdata, read_token_golden, run_analyzer,
    run_tokenizer,
};

fn tokens(case: &str) {
    let inputs = read_cases(&read_testdata(&format!("cases/{case}.txt")));
    let golden = read_token_golden(&read_testdata(&format!("golden/{case}.tokens")));
    assert_cases_match(
        &format!("cases/{case}.txt"),
        &inputs,
        &golden,
        run_tokenizer,
        TextMatch::Exact,
    );
}

fn analyze(case: &str) {
    let inputs = read_cases(&read_testdata(&format!("cases/{case}.txt")));
    let golden = read_token_golden(&read_testdata(&format!("golden/{case}.analyze")));
    assert_cases_match(
        &format!("cases/{case}.txt"),
        &inputs,
        &golden,
        run_analyzer,
        TextMatch::StemsMayDiffer,
    );
}

#[test]
fn tokens_upstream() {
    tokens("upstream");
}

#[test]
fn tokens_edge() {
    tokens("edge");
}

#[test]
fn tokens_fuzz() {
    tokens("fuzz");
}

#[test]
fn tokens_wiki_zh() {
    tokens("wiki_zh");
}

#[test]
fn analyze_upstream() {
    analyze("upstream");
}

#[test]
fn analyze_edge() {
    analyze("edge");
}

#[test]
fn analyze_fuzz() {
    analyze("fuzz");
}

/// Ad-hoc large differential run over any case file + tokenizer golden, given by `SMARTCN_CASES`
/// and `SMARTCN_TOKENS` (see `testdata/smartcn/README.md`). Ignored by default.
#[test]
#[ignore]
fn tokens_large() {
    let cases_path = std::env::var("SMARTCN_CASES").expect("set SMARTCN_CASES to a case file");
    let golden_path =
        std::env::var("SMARTCN_TOKENS").expect("set SMARTCN_TOKENS to its .tokens golden");
    let inputs = read_cases(&std::fs::read_to_string(&cases_path).unwrap());
    let golden = read_token_golden(&std::fs::read_to_string(&golden_path).unwrap());
    assert_cases_match(
        &cases_path,
        &inputs,
        &golden,
        run_tokenizer,
        TextMatch::Exact,
    );
}
