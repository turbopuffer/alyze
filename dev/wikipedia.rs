use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use parquet::{
    file::reader::{FileReader, SerializedFileReader},
    record::{Row, RowAccessor, reader::RowIter},
    schema::types::Type,
};

pub const SHARD_COUNT: usize = 41;
pub const CHUNK_BYTES: u64 = 64 << 20;

pub fn load_n_bytes(n: u64) -> Vec<String> {
    let mut texts = Vec::new();
    let mut total_bytes = 0;
    visit_texts(0..SHARD_COUNT, |text| {
        total_bytes += text.len() as u64;
        texts.push(text);
        total_bytes < n
    });
    assert!(
        total_bytes >= n,
        "Wikipedia dataset is smaller than {n} bytes"
    );
    texts
}

#[allow(unused)]
pub fn visit_chunks(
    files: impl IntoIterator<Item = usize>,
    chunk_bytes: u64,
    mut visit: impl FnMut(usize, usize, &[String]) -> bool,
) {
    for file_index in files {
        let mut texts = Vec::new();
        let mut total_bytes = 0;
        let mut chunk_index = 0;
        let mut keep_going = true;
        visit_texts(file_index..file_index + 1, |text| {
            total_bytes += text.len() as u64;
            texts.push(text);
            if total_bytes >= chunk_bytes {
                keep_going = visit(file_index, chunk_index, &texts);
                texts.clear();
                total_bytes = 0;
                chunk_index += 1;
            }
            keep_going
        });
        if !keep_going {
            return;
        }
        if !texts.is_empty() && !visit(file_index, chunk_index, &texts) {
            return;
        }
    }
}

fn visit_texts(files: impl IntoIterator<Item = usize>, mut visit: impl FnMut(String) -> bool) {
    for index in files {
        assert!(index < SHARD_COUNT, "invalid Wikipedia shard {index}");
        let reader = SerializedFileReader::new(wikipedia_file(index))
            .expect("failed to create Wikipedia parquet reader");
        for row in text_rows(Box::new(reader)) {
            if !visit(row.get_string(0).unwrap().clone()) {
                return;
            }
        }
    }
}

fn wikipedia_file(index: usize) -> File {
    let file_name = format!("train-{index:05}-of-{SHARD_COUNT:05}.parquet");
    let cache_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/wikipedia");
    std::fs::create_dir_all(&cache_dir).expect("failed to create Wikipedia cache directory");
    let path = cache_dir.join(&file_name);
    if !path.exists() {
        download(&file_name, &path);
    }
    File::open(path).expect("failed to open cached Wikipedia shard")
}

fn download(file_name: &str, path: &Path) {
    let url = format!(
        "https://huggingface.co/datasets/wikimedia/wikipedia/resolve/main/20231101.en/{file_name}?download=true"
    );
    println!("wikipedia: downloading '{file_name}' from {url}");
    let response = ureq::get(&url)
        .call()
        .expect("failed to download Wikipedia shard");
    let mut temp = tempfile::Builder::new()
        .tempfile_in(path.parent().unwrap())
        .expect("failed to create temporary Wikipedia cache file");
    std::io::copy(&mut response.into_body().into_reader(), &mut temp)
        .expect("failed to write Wikipedia shard");
    temp.as_file_mut()
        .flush()
        .expect("failed to flush Wikipedia shard");
    temp.persist(path)
        .expect("failed to move Wikipedia shard into cache");
}

fn text_rows(reader: Box<dyn FileReader>) -> impl Iterator<Item = Row> {
    let metadata = reader.metadata();
    let fields = metadata.file_metadata().schema().get_fields();
    let text_fields = fields
        .iter()
        .filter(|field| field.name() == "text")
        .cloned()
        .collect();
    let projection = Type::group_type_builder("schema")
        .with_fields(text_fields)
        .build()
        .unwrap();
    RowIter::from_file_into(reader)
        .project(Some(projection))
        .unwrap()
        .map(Result::unwrap)
}
