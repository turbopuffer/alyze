//! Extracts a deterministic sample of Chinese Wikipedia articles for the smartcn differential
//! tests, one article per line in the escaped case-file format.
//!
//!     cargo run -p alyze-cjk --example smartcn_wiki_sample [-- --bytes N --out PATH --parquet PATH]
//!
//! Defaults produce `testdata/smartcn/cases/wiki_zh.txt` (committed, ~512 KiB) from the first
//! articles of one shard of the `wikimedia/wikipedia` `20231101.zh` dataset. For a large ad-hoc
//! differential run, point `--bytes` at tens of megabytes and `--out` outside the repo, generate
//! the golden with `testdata/smartcn/gen.sh tokens IN OUT`, then run the ignored
//! `smartcn::tests::golden::tokens_large` test with `SMARTCN_CASES`/`SMARTCN_TOKENS` set.
//!
//! The shard is not downloaded automatically; fetch it once (into the repository's ignored
//! `.cache/`) with
//!
//!     curl -L -o .cache/wikipedia_zh/train-00002-of-00006.parquet \
//!       'https://huggingface.co/datasets/wikimedia/wikipedia/resolve/main/20231101.zh/train-00002-of-00006.parquet?download=true'

use std::fmt::Write as _;
use std::fs::File;

use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::reader::RowIter;
use parquet::record::{Row, RowAccessor};
use parquet::schema::types::Type;

fn main() {
    let root = env!("CARGO_MANIFEST_DIR");
    let mut bytes = 512 * 1024usize;
    let mut out = format!("{root}/testdata/smartcn/cases/wiki_zh.txt");
    let mut parquet = format!("{root}/../.cache/wikipedia_zh/train-00002-of-00006.parquet");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next().expect("missing value");
        match arg.as_str() {
            "--bytes" => bytes = value.parse().unwrap(),
            "--out" => out = value,
            "--parquet" => parquet = value,
            _ => panic!("unknown argument {arg}"),
        }
    }

    let file = File::open(&parquet).unwrap_or_else(|e| panic!("open {parquet}: {e} (see --help)"));
    let reader = SerializedFileReader::new(file).expect("failed to create parquet reader");
    let mut file_out = String::new();
    let mut articles = 0usize;
    for row in iter_parquet_rows(Box::new(reader), &["text"]) {
        let text = row.get_string(0).unwrap();
        // Keep the sample to moderately sized articles so one line stays reviewable, but well
        // past the tokenizer's 1024-char read buffer.
        if text.len() > 32 * 1024 {
            continue;
        }
        escape_line(text, &mut file_out);
        file_out.push('\n');
        articles += 1;
        if file_out.len() >= bytes {
            break;
        }
    }
    std::fs::write(&out, &file_out).unwrap();
    eprintln!(
        "wrote {articles} articles ({} bytes) to {out}",
        file_out.len()
    );
}

fn iter_parquet_rows(
    reader: Box<dyn FileReader>,
    column_names: &[&str],
) -> impl Iterator<Item = Row> {
    let fields = reader.metadata().file_metadata().schema().get_fields();
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

/// Same escaping as the Java side: backslash, CR, LF, TAB, other C0/C1 controls, DEL, LS, PS.
fn escape_line(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20
                || ((c as u32) >= 0x7F && (c as u32) < 0xA0)
                || c == '\u{2028}'
                || c == '\u{2029}' =>
            {
                write!(out, "\\u{{{:x}}}", c as u32).unwrap();
            }
            c => out.push(c),
        }
    }
}
