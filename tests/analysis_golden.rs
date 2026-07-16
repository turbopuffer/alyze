use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use alyze::{
    analyze::{
        AnalysisOptions, Analyzer, LanguageWithStopwords, ReusableBuffer, StemmingLanguage,
        StopwordRemoval, Token, TokenizerOptions,
    },
    uax29,
};

#[path = "../dev/wikipedia.rs"]
mod wikipedia_data;
use wikipedia_data::{CHUNK_BYTES, load_n_bytes};

fn hash_token(token: Token<'_>, hasher: &mut impl Hasher) {
    token.text.hash(hasher);
    token.position.hash(hasher);
    token.byte_range.hash(hasher);
    token.input_index.hash(hasher);
}

fn base_options() -> AnalysisOptions {
    AnalysisOptions {
        tokenizer: TokenizerOptions::UAX29Word(uax29::word::Options::default()),
        maximum_token_length: None,
        case_sensitive: false,
        stopword_removal: None,
        stemming: None,
        ascii_folding: false,
    }
}

fn analysis_options() -> Vec<(&'static str, AnalysisOptions)> {
    let base = base_options();
    let mut options = vec![
        (
            "case-sensitive",
            AnalysisOptions {
                case_sensitive: true,
                ..base
            },
        ),
        ("lowercase", base),
        (
            "maximum-length-1",
            AnalysisOptions {
                maximum_token_length: Some(1),
                ..base
            },
        ),
        (
            "maximum-length-4",
            AnalysisOptions {
                maximum_token_length: Some(4),
                ..base
            },
        ),
        (
            "ascii-folding",
            AnalysisOptions {
                ascii_folding: true,
                ..base
            },
        ),
        (
            "full-english",
            AnalysisOptions {
                maximum_token_length: Some(40),
                stopword_removal: Some(StopwordRemoval::ForLanguage(
                    LanguageWithStopwords::English,
                )),
                stemming: Some(StemmingLanguage::English),
                ascii_folding: true,
                ..base
            },
        ),
    ];

    for (name, language) in [
        ("stopwords-danish", LanguageWithStopwords::Danish),
        ("stopwords-dutch", LanguageWithStopwords::Dutch),
        ("stopwords-english", LanguageWithStopwords::English),
        ("stopwords-finnish", LanguageWithStopwords::Finnish),
        ("stopwords-french", LanguageWithStopwords::French),
        ("stopwords-german", LanguageWithStopwords::German),
        ("stopwords-hungarian", LanguageWithStopwords::Hungarian),
        ("stopwords-italian", LanguageWithStopwords::Italian),
        ("stopwords-norwegian", LanguageWithStopwords::Norwegian),
        ("stopwords-portuguese", LanguageWithStopwords::Portuguese),
        ("stopwords-russian", LanguageWithStopwords::Russian),
        ("stopwords-spanish", LanguageWithStopwords::Spanish),
        ("stopwords-swedish", LanguageWithStopwords::Swedish),
    ] {
        options.push((
            name,
            AnalysisOptions {
                stopword_removal: Some(StopwordRemoval::ForLanguage(language)),
                ..base
            },
        ));
    }

    for (name, language) in [
        ("stemming-arabic", StemmingLanguage::Arabic),
        ("stemming-danish", StemmingLanguage::Danish),
        ("stemming-dutch", StemmingLanguage::Dutch),
        ("stemming-english", StemmingLanguage::English),
        ("stemming-finnish", StemmingLanguage::Finnish),
        ("stemming-french", StemmingLanguage::French),
        ("stemming-german", StemmingLanguage::German),
        ("stemming-greek", StemmingLanguage::Greek),
        ("stemming-hungarian", StemmingLanguage::Hungarian),
        ("stemming-italian", StemmingLanguage::Italian),
        ("stemming-norwegian", StemmingLanguage::Norwegian),
        ("stemming-portuguese", StemmingLanguage::Portuguese),
        ("stemming-romanian", StemmingLanguage::Romanian),
        ("stemming-russian", StemmingLanguage::Russian),
        ("stemming-spanish", StemmingLanguage::Spanish),
        ("stemming-swedish", StemmingLanguage::Swedish),
        ("stemming-tamil", StemmingLanguage::Tamil),
        ("stemming-turkish", StemmingLanguage::Turkish),
    ] {
        options.push((
            name,
            AnalysisOptions {
                stemming: Some(language),
                ..base
            },
        ));
    }

    options
}

fn benchmark_options() -> Vec<(&'static str, AnalysisOptions)> {
    let base = base_options();
    vec![
        (
            "case-sensitive",
            AnalysisOptions {
                case_sensitive: true,
                ..base
            },
        ),
        ("lowercase", base),
        (
            "stopwords",
            AnalysisOptions {
                stopword_removal: Some(StopwordRemoval::ForLanguage(
                    LanguageWithStopwords::English,
                )),
                ..base
            },
        ),
        (
            "stemming",
            AnalysisOptions {
                stemming: Some(StemmingLanguage::English),
                ..base
            },
        ),
        (
            "full-pipeline",
            AnalysisOptions {
                maximum_token_length: Some(40),
                stopword_removal: Some(StopwordRemoval::ForLanguage(
                    LanguageWithStopwords::English,
                )),
                stemming: Some(StemmingLanguage::English),
                ascii_folding: true,
                ..base
            },
        ),
    ]
}

fn callback_digest(options: AnalysisOptions, inputs: &[String]) -> (usize, u64) {
    let mut hasher = DefaultHasher::new();
    let mut count = 0;
    Analyzer::new(options).analyze_inputs(
        inputs.iter().map(String::as_str),
        &mut ReusableBuffer::new(),
        |token| {
            hash_token(token, &mut hasher);
            count += 1;
            true
        },
    );
    (count, hasher.finish())
}

fn stream_digest(options: AnalysisOptions, inputs: &[String]) -> (usize, u64) {
    let mut hasher = DefaultHasher::new();
    let mut count = 0;
    let mut buffer = ReusableBuffer::new();
    let analyzer = Analyzer::new(options);
    let mut stream = analyzer.token_stream_inputs(inputs.iter().map(String::as_str), &mut buffer);
    while let Some(token) = stream.next_token() {
        hash_token(token, &mut hasher);
        count += 1;
    }
    (count, hasher.finish())
}

fn corpus_digest(inputs: &[String], options: Vec<(&'static str, AnalysisOptions)>) -> (usize, u64) {
    let mut total_count = 0;
    let mut total_hasher = DefaultHasher::new();
    for (name, options) in options {
        let (count, digest) = callback_digest(options, inputs);
        assert_eq!(
            stream_digest(options, inputs),
            (count, digest),
            "stream output differs from callback output for {name}"
        );
        total_count += count;
        name.hash(&mut total_hasher);
        count.hash(&mut total_hasher);
        digest.hash(&mut total_hasher);
    }
    (total_count, total_hasher.finish())
}

fn conformance_inputs() -> Vec<String> {
    let mut inputs = Vec::new();
    for line in include_str!("../testdata/WordBreakTest.txt").lines() {
        let source = line.split('#').next().unwrap().trim();
        if source.is_empty() {
            continue;
        }
        let text = source
            .split_whitespace()
            .filter(|part| *part != "÷" && *part != "×")
            .map(|part| char::from_u32(u32::from_str_radix(part, 16).unwrap()).unwrap())
            .collect();
        inputs.push(text);
    }
    inputs.extend(
        [
            "",
            "THE café runners' coöperation isn't naïve",
            "العَرَبِيَّة واللغات كلمات",
            "Dansk nederlands English suomalainen français Deutsch",
            "Ελληνικά magyar italiano norsk português română русский",
            "Español svenska தமிழ் Türkçe İstanbul Iİıi",
            "中文 日本語 한국어 ไทย 👨\u{200d}👩\u{200d}👧\u{200d}👦",
            "one.two 1,234.56 foo_bar can't 3:00 A\u{308}\u{300}",
            "\0\r\n\u{ad}\u{2060}\u{200d}\u{1f1e6}\u{1f1e7}",
        ]
        .map(str::to_owned),
    );
    inputs
}

#[test]
fn analysis_matches_golden() {
    let actual = corpus_digest(&conformance_inputs(), analysis_options());
    assert_eq!(actual, (62_198, 13_013_864_069_378_490_261));
}

#[test]
fn wikipedia_analysis_matches_golden() {
    let actual = corpus_digest(&load_n_bytes(CHUNK_BYTES), benchmark_options());
    assert_eq!(actual, (46_721_103, 11_235_000_508_967_425_851));
}
