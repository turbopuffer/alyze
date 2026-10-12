//! Throughput of the kuromoji tokenizer and analyzer over Japanese Wikipedia articles.
//!
//!     cargo bench -p alyze-cjk --bench wikipedia_ja
//!
//! Reads the parquet shard the `kuromoji_wiki_sample` example uses (see its docs for the download);
//! falls back to the committed 512 KiB sample if the shard isn't there.

use std::fs::File;
use std::hint::black_box;
use std::time::Duration;

use alyze_cjk::kuromoji;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::RowAccessor;
use parquet::record::reader::RowIter;
use parquet::schema::types::Type;

const TARGET_BYTES: usize = 16 * 1024 * 1024;

fn benchmark(c: &mut Criterion) {
    let articles = load_articles(TARGET_BYTES);
    let total_bytes: usize = articles.iter().map(String::len).sum();
    eprintln!(
        "benchmarking over {} articles, {} bytes",
        articles.len(),
        total_bytes
    );

    let mut group = c.benchmark_group("wikipedia_ja");
    group.throughput(Throughput::Bytes(total_bytes as u64));
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));

    group.bench_function("tokenize", |b| {
        let mut tokens = kuromoji::Tokens::new();
        b.iter(|| {
            let mut count = 0usize;
            for article in &articles {
                kuromoji::tokenize(article, kuromoji::Options::default(), &mut tokens);
                count += tokens.len();
            }
            black_box(count)
        })
    });

    group.bench_function("analyze", |b| {
        let mut tokens = kuromoji::Tokens::new();
        b.iter(|| {
            let mut count = 0usize;
            for article in &articles {
                kuromoji::analyze(article, kuromoji::AnalyzerOptions::default(), &mut tokens);
                count += tokens.len();
            }
            black_box(count)
        })
    });

    group.finish();
}

fn load_articles(target_bytes: usize) -> Vec<String> {
    let root = env!("CARGO_MANIFEST_DIR");
    let shard = format!("{root}/../.cache/wikipedia_ja/train-00014-of-00015.parquet");
    let Ok(file) = File::open(&shard) else {
        eprintln!("{shard} not found, using the committed sample");
        return load_sample(&format!("{root}/testdata/kuromoji/cases/wiki_ja.txt"));
    };
    let reader = SerializedFileReader::new(file).expect("failed to create parquet reader");
    let fields = reader.metadata().file_metadata().schema().get_fields();
    let mut selected = fields.to_vec();
    selected.retain(|f| f.name() == "text");
    let projection = Type::group_type_builder("schema")
        .with_fields(selected)
        .build()
        .unwrap();
    let mut articles = Vec::new();
    let mut bytes = 0;
    for row in RowIter::from_file_into(Box::new(reader))
        .project(Some(projection))
        .unwrap()
    {
        let text = row.unwrap().get_string(0).unwrap().clone();
        bytes += text.len();
        articles.push(text);
        if bytes >= target_bytes {
            break;
        }
    }
    articles
}

fn load_sample(path: &str) -> Vec<String> {
    std::fs::read_to_string(path)
        .expect("committed sample missing")
        .lines()
        .map(|line| {
            line.replace("\\n", "\n")
                .replace("\\t", "\t")
                .replace("\\\\", "\\")
        })
        .collect()
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
