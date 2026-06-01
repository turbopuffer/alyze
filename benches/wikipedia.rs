use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use alyze::analyze::{
    AnalysisOptions, Analyzer, LanguageWithStopwords, ReusableBuffer, StemmingLanguage,
    StopwordRemoval, TokenizerOptions,
};
use alyze::uax29;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use parquet::{
    file::reader::{FileReader, SerializedFileReader},
    record::{Row, RowAccessor, reader::RowIter},
    schema::types::Type,
};

criterion_group!(benches, wikipedia_benchmark, analysis_benchmark);
criterion_main!(benches);

pub fn wikipedia_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("wikipedia");

    let n_bytes = 64 << 20; // 64 MiB
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

    let n_bytes = 64 << 20; // 64 MiB
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
        ("tokenize only (case sensitive)", AnalysisOptions {
            case_sensitive: true,
            ..base
        }),
        ("+ lowercase", base),
        ("+ stopwords", AnalysisOptions {
            stopword_removal: Some(StopwordRemoval::ForLanguage(LanguageWithStopwords::English)),
            ..base
        }),
        ("+ stemming", AnalysisOptions {
            stemming: Some(StemmingLanguage::English),
            ..base
        }),
        ("full pipeline", AnalysisOptions {
            maximum_token_length: Some(40),
            stopword_removal: Some(StopwordRemoval::ForLanguage(LanguageWithStopwords::English)),
            stemming: Some(StemmingLanguage::English),
            ascii_folding: true,
            ..base
        }),
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
    }

    group.finish();
}

fn load_n_bytes(n: u64) -> Vec<String> {
    let cache_dir = cache_dir();
    let files_and_urls = parquet_files_and_urls();
    let mut texts = Vec::new();
    let mut total_bytes = 0;
    for (file_name, url) in files_and_urls {
        let file = download_file_with_cache(&file_name, &url, &cache_dir);
        let reader = SerializedFileReader::new(file).expect("failed to create parquet reader");
        let rows = iter_parquet_rows(Box::new(reader), &["text"]);
        for row in rows {
            let text = row.get_string(0).cloned().unwrap();
            total_bytes += text.len() as u64;
            texts.push(text);
            if total_bytes >= n {
                return texts;
            }
        }
    }
    panic!("not enough data in parquet files to reach {} bytes", n);
}

fn cache_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/wikipedia");
    std::fs::create_dir_all(&dir).expect("failed to create cache directory");
    dir
}

fn parquet_files_and_urls() -> Vec<(String, String)> {
    let mut files_and_urls = Vec::new();
    for i in 0..41 {
        let file = format!("train-{:05}-of-00041.parquet", i);
        let url = format!(
            "https://huggingface.co/datasets/wikimedia/wikipedia/resolve/main/20231101.en/{}?download=true",
            file
        );
        files_and_urls.push((file, url));
    }
    files_and_urls
}

fn download_file_with_cache(file_name: &str, url: &str, cache_dir: &Path) -> File {
    let cache_file = cache_dir.join(file_name);
    if !cache_file.exists() {
        println!(
            "wikipedia: downloading '{}' (from {}) for benchmark",
            file_name, url
        );
        let response = ureq::get(url).call().expect("failed to download file");
        let mut tmp_file = tempfile::Builder::new()
            .tempfile_in(cache_dir)
            .expect("failed to create temporary file");
        std::io::copy(&mut response.into_body().into_reader(), &mut tmp_file)
            .expect("failed to write response body to temporary file");
        tmp_file
            .as_file_mut()
            .flush()
            .expect("failed to flush temporary file");
        tmp_file
            .persist(&cache_file)
            .expect("rename failed to move temporary file to cache");
    }
    File::open(cache_file).expect("failed to open cached file")
}

fn iter_parquet_rows(
    reader: Box<dyn FileReader>,
    column_names: &[&str],
) -> impl Iterator<Item = Row> {
    let parquet_metadata = reader.metadata();
    let fields = parquet_metadata.file_metadata().schema().get_fields();
    let mut selected_fields = fields.to_vec();
    selected_fields.retain(|f| column_names.contains(&f.name()));
    let schema_proj = Type::group_type_builder("schema")
        .with_fields(selected_fields)
        .build()
        .unwrap();
    RowIter::from_file_into(reader)
        .project(Some(schema_proj))
        .unwrap()
        .map(|result| result.unwrap())
}
