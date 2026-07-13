use alyze::analyze::{
    AnalysisOptions, Analyzer, LanguageWithStopwords, ReusableBuffer, StemmingLanguage,
    StopwordRemoval, TokenizerOptions,
};
use alyze::uax29;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};

#[path = "../dev/wikipedia.rs"]
#[allow(dead_code)]
mod wikipedia_data;
use wikipedia_data::{CHUNK_BYTES, load_n_bytes};

criterion_group!(benches, wikipedia_benchmark, analysis_benchmark);
criterion_main!(benches);

pub fn wikipedia_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("wikipedia");

    let n_bytes = CHUNK_BYTES;
    let texts = load_n_bytes(n_bytes);

    group.throughput(Throughput::Bytes(n_bytes));
    group.sample_size(16);

    group.bench_function("word break", |b| {
        b.iter(|| {
            let mut count = 0;
            for text in &texts {
                uax29::word::tokenize(text, uax29::word::Options::default(), |_, _| {
                    count += 1;
                    true
                });
            }
            std::hint::black_box(&count);
        })
    });

    // When `props` is unused, LLVM will optimize it away (which is amazing!), but we also want
    // to benchmark the cost of computing and using this word-like property.
    group.bench_function("word break + word_like", |b| {
        b.iter(|| {
            let mut count = 0;
            let mut word_like = 0;
            for text in &texts {
                uax29::word::tokenize(text, uax29::word::Options::default(), |_, props| {
                    count += 1;
                    if props.is_word_like() {
                        word_like += 1;
                    }
                    true
                });
            }
            std::hint::black_box((&count, &word_like));
        })
    });

    group.bench_function("sentence break", |b| {
        b.iter(|| {
            let mut count = 0;
            for text in &texts {
                uax29::sentence::tokenize(text, uax29::sentence::Options::default(), |_| {
                    count += 1;
                    true
                });
            }
            std::hint::black_box(&count);
        })
    });

    group.finish();
}

pub fn analysis_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("analysis");

    let n_bytes = CHUNK_BYTES;
    let texts = load_n_bytes(n_bytes);

    group.throughput(Throughput::Bytes(n_bytes));
    group.sample_size(16);

    let base = AnalysisOptions {
        tokenizer: TokenizerOptions::UAX29Word(uax29::word::Options::default()),
        maximum_token_length: None,
        case_sensitive: false,
        stopword_removal: None,
        stemming: None,
        ascii_folding: false,
    };

    // Each config exercises an additional stage of the analysis pipeline, so the
    // deltas between rows approximate the marginal cost of each filter.
    let configs: &[(&str, AnalysisOptions)] = &[
        (
            "tokenize only (case sensitive)",
            AnalysisOptions {
                case_sensitive: true,
                ..base
            },
        ),
        ("+ lowercase", base),
        (
            "+ stopwords",
            AnalysisOptions {
                stopword_removal: Some(StopwordRemoval::ForLanguage(
                    LanguageWithStopwords::English,
                )),
                ..base
            },
        ),
        (
            "+ stemming",
            AnalysisOptions {
                stemming: Some(StemmingLanguage::English),
                ..base
            },
        ),
        (
            "full pipeline",
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

    for (name, options) in configs {
        assert!(options.valid(), "invalid options for benchmark '{name}'");
        let analyzer = Analyzer::new(*options);
        let mut buffer = ReusableBuffer::new();
        group.bench_function(*name, |b| {
            b.iter(|| {
                let mut count = 0;
                for text in &texts {
                    analyzer.analyze(text, &mut buffer, |token| {
                        count += 1;
                        std::hint::black_box(&token.text);
                        true
                    });
                }
                std::hint::black_box(&count);
            })
        });
        group.bench_function(format!("{name} (stream)"), |b| {
            b.iter(|| {
                let mut count = 0;
                for text in &texts {
                    let mut stream = analyzer.token_stream(text, &mut buffer);
                    while let Some(token) = stream.next_token() {
                        count += 1;
                        std::hint::black_box(&token.text);
                    }
                }
                std::hint::black_box(&count);
            })
        });
    }

    group.finish();
}
