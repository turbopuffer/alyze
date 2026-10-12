//! Converts the dump of Lucene's nori dictionaries (written by `testdata/nori/gen.sh dump`) into
//! the blobs in `data/nori/` that `nori::dict` and `nori::char_def` load. Only needs rerunning if
//! Lucene's dictionaries change (they are built from mecab-ko-dic 2.1.1, unchanged since 2018).
//!
//!     alyze-cjk/testdata/nori/gen.sh dump
//!     cargo run -p alyze-cjk --example nori_convert_dict -- ~/Src/nori/dump
//!
//! Blob formats (all little-endian; text as UTF-16 code units, like Lucene):
//!
//! - `terms.fst`: an `fst::Map` from each surface form (UTF-16BE bytes, so byte order equals
//!   code-unit order) to its term ordinal, 0-based in sorted order.
//! - `words.bin`: `u32` term count T, `u32` word count W; `(T+1) × u32` word-index bounds per
//!   term (the words of term k are `bounds[k]..bounds[k+1]`, in Lucene's order); `W × 8` word
//!   records: `u16` left id | POS type << 14, `u16` right id | has-reading << 14 | has-morphemes
//!   << 15, `i16` cost, `u8` left POS, `u8` right POS; `u32` extras count E; `E × (u32 word id,
//!   u32 offset)` sorted by word id; then the extras: `u8` reading length (0 = none) and that
//!   many `u16`, `u8` morpheme count and per morpheme `u8` tag, `u8` length, that many `u16`.
//! - `unk.bin`: `u8` class count, then per class `u8` entry count and that many 8-byte records.
//! - `costs.bin`: `u32` right-id count R, `u32` left-id count L, `L × R` `i16`, left-id major
//!   (like Lucene: the Viterbi's inner loop varies the right id, so those reads are contiguous).
//! - `chardef.bin`: `u8` class count C, `65536` class bytes (one per code unit), `C` flag bytes
//!   (bit 0 invoke, bit 1 group), from `testdata/nori/golden/chardef.txt`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn main() {
    let dump = std::env::args()
        .nth(1)
        .expect("path to the dictionary dump directory");
    let dump = Path::new(&dump);
    let root = env!("CARGO_MANIFEST_DIR");
    let out = Path::new(root).join("data/nori");
    fs::create_dir_all(&out).unwrap();

    convert_token_info(&dump.join("tokeninfo.tsv"), &out);
    convert_unknown(&dump.join("unk.tsv"), &out.join("unk.bin"));
    convert_costs(&dump.join("costs.bin"), &out.join("costs.bin"));
    convert_chardef(
        &Path::new(root).join("testdata/nori/golden/chardef.txt"),
        &out.join("chardef.bin"),
    );
    for name in [
        "terms.fst",
        "words.bin",
        "unk.bin",
        "costs.bin",
        "chardef.bin",
    ] {
        eprintln!(
            "{name}: {} bytes",
            fs::metadata(out.join(name)).unwrap().len()
        );
    }
}

const TAGS: &[&str] = &[
    "EP", "EF", "EC", "ETN", "ETM", "IC", "JKS", "JKC", "JKG", "JKO", "JKB", "JKV", "JKQ", "JX",
    "JC", "MAG", "MAJ", "MM", "NNG", "NNP", "NNB", "NNBC", "NP", "NR", "SF", "SH", "SL", "SN",
    "SP", "SSC", "SSO", "SC", "SY", "SE", "VA", "VCN", "VCP", "VV", "VX", "XPN", "XR", "XSA",
    "XSN", "XSV", "UNKNOWN", "UNA", "NA", "VSV",
];
const POS_TYPES: &[&str] = &["MORPHEME", "COMPOUND", "INFLECT", "PREANALYSIS"];
const CLASSES: &[&str] = &[
    "NGRAM",
    "DEFAULT",
    "SPACE",
    "SYMBOL",
    "NUMERIC",
    "ALPHA",
    "CYRILLIC",
    "GREEK",
    "HIRAGANA",
    "KATAKANA",
    "KANJI",
    "HANGUL",
    "HANJA",
    "HANJANUMERIC",
];

fn index_of(table: &[&str], name: &str) -> u8 {
    table
        .iter()
        .position(|t| *t == name)
        .unwrap_or_else(|| panic!("unknown name {name:?}")) as u8
}

/// A word record: `left id | POS type << 14`, `right id | flags`, cost, left POS, right POS.
fn record(fields: &[&str], flags: u16, out: &mut Vec<u8>) {
    let left_id: u16 = fields[0].parse().unwrap();
    let right_id: u16 = fields[1].parse().unwrap();
    let cost: i16 = fields[2].parse().unwrap();
    let pos_type = index_of(POS_TYPES, fields[3]);
    assert!(left_id < 1 << 14 && right_id < 1 << 14);
    out.extend_from_slice(&(left_id | u16::from(pos_type) << 14).to_le_bytes());
    out.extend_from_slice(&(right_id | flags).to_le_bytes());
    out.extend_from_slice(&cost.to_le_bytes());
    out.push(index_of(TAGS, fields[4]));
    out.push(index_of(TAGS, fields[5]));
}

fn convert_token_info(tsv: &Path, out: &Path) {
    let text = fs::read_to_string(tsv).unwrap();
    let mut fst = fst::MapBuilder::memory();
    // bounds[k] is the first word index of term k; the last entry is the word count.
    let mut bounds: Vec<u32> = Vec::new();
    let mut records: Vec<u8> = Vec::new();
    let mut extras_index: Vec<(u32, u32)> = Vec::new();
    let mut extras: Vec<u8> = Vec::new();
    let mut last_surface: Option<Vec<u16>> = None;
    let mut word_id = 0u32;
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 9, "bad line {line:?}");
        let surface = utf16(&unescape(fields[0]));
        if last_surface.as_ref() != Some(&surface) {
            if let Some(last) = &last_surface {
                assert!(last < &surface, "terms out of order at {line:?}");
            }
            fst.insert(utf16be(&surface), bounds.len() as u64).unwrap();
            bounds.push(word_id);
            last_surface = Some(surface);
        }
        let reading = (fields[7] != "-").then(|| utf16(&unescape(fields[7])));
        let morphemes: Vec<(u8, Vec<u16>)> = if fields[8] == "-" {
            Vec::new()
        } else {
            fields[8]
                .split('+')
                .map(|m| {
                    let (text, tag) = m.rsplit_once('/').unwrap();
                    (index_of(TAGS, tag), utf16(&unescape(text)))
                })
                .collect()
        };
        let flags = (reading.is_some() as u16) << 14 | (!morphemes.is_empty() as u16) << 15;
        record(&fields[1..7], flags, &mut records);
        if reading.is_some() || !morphemes.is_empty() {
            extras_index.push((word_id, extras.len() as u32));
            let reading = reading.unwrap_or_default();
            extras.push(u8::try_from(reading.len()).unwrap());
            for u in &reading {
                extras.extend_from_slice(&u.to_le_bytes());
            }
            extras.push(u8::try_from(morphemes.len()).unwrap());
            for (tag, text) in &morphemes {
                extras.push(*tag);
                extras.push(u8::try_from(text.len()).unwrap());
                for u in text {
                    extras.extend_from_slice(&u.to_le_bytes());
                }
            }
        }
        word_id += 1;
    }
    bounds.push(word_id);
    let terms = bounds.len() - 1;

    fs::write(out.join("terms.fst"), fst.into_inner().unwrap()).unwrap();
    let mut words: Vec<u8> = Vec::new();
    words.extend_from_slice(&(terms as u32).to_le_bytes());
    words.extend_from_slice(&word_id.to_le_bytes());
    for b in &bounds {
        words.extend_from_slice(&b.to_le_bytes());
    }
    words.extend_from_slice(&records);
    words.extend_from_slice(&(extras_index.len() as u32).to_le_bytes());
    for (id, offset) in &extras_index {
        words.extend_from_slice(&id.to_le_bytes());
        words.extend_from_slice(&offset.to_le_bytes());
    }
    words.extend_from_slice(&extras);
    fs::write(out.join("words.bin"), words).unwrap();
    eprintln!(
        "{terms} terms, {word_id} words, {} with extras",
        extras_index.len()
    );
}

fn convert_unknown(tsv: &Path, out: &Path) {
    let text = fs::read_to_string(tsv).unwrap();
    let mut by_class: BTreeMap<u8, Vec<Vec<u8>>> = BTreeMap::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 5, "bad line {line:?}");
        let class = index_of(CLASSES, fields[0]);
        let mut rec = Vec::new();
        // class, leftId, rightId, wordCost, leftPOS: a MORPHEME with left POS == right POS.
        record(
            &[
                fields[1], fields[2], fields[3], "MORPHEME", fields[4], fields[4],
            ],
            0,
            &mut rec,
        );
        by_class.entry(class).or_default().push(rec);
    }
    let mut blob = vec![CLASSES.len() as u8];
    for class in 0..CLASSES.len() as u8 {
        let entries = by_class.get(&class).map(Vec::as_slice).unwrap_or(&[]);
        blob.push(entries.len() as u8);
        for e in entries {
            blob.extend_from_slice(e);
        }
    }
    fs::write(out, blob).unwrap();
}

fn convert_costs(dump: &Path, out: &Path) {
    let matrix = fs::read(dump).unwrap();
    // The dump has no header; the dimensions are mecab-ko-dic's (right-id.def / left-id.def).
    let (right, left) = (3822usize, 2693usize);
    assert_eq!(matrix.len(), right * left * 2, "unexpected matrix size");
    let mut blob = Vec::with_capacity(matrix.len() + 8);
    blob.extend_from_slice(&(right as u32).to_le_bytes());
    blob.extend_from_slice(&(left as u32).to_le_bytes());
    // Transpose: the dump is right-id major, the blob left-id major.
    for l in 0..left {
        for r in 0..right {
            let at = (r * left + l) * 2;
            blob.extend_from_slice(&matrix[at..at + 2]);
        }
    }
    fs::write(out, blob).unwrap();
}

fn convert_chardef(golden: &Path, out: &Path) {
    let text = fs::read_to_string(golden).unwrap();
    let mut classes = vec![0u8; 0x10000];
    let mut flags = vec![0u8; CLASSES.len()];
    let mut covered = 0usize;
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields[0] == "class" {
            let class = index_of(CLASSES, fields[1]);
            flags[class as usize] = (fields[2] == "1") as u8 | ((fields[3] == "1") as u8) << 1;
            continue;
        }
        let lo = usize::from_str_radix(fields[0], 16).unwrap();
        let hi = usize::from_str_radix(fields[1], 16).unwrap();
        assert_eq!(lo, covered);
        let class = index_of(CLASSES, fields[2]);
        classes[lo..=hi].fill(class);
        covered = hi + 1;
    }
    assert_eq!(covered, 0x10000);
    let mut blob = vec![CLASSES.len() as u8];
    blob.extend_from_slice(&classes);
    blob.extend_from_slice(&flags);
    fs::write(out, blob).unwrap();
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn utf16be(units: &[u16]) -> Vec<u8> {
    units.iter().flat_map(|u| u.to_be_bytes()).collect()
}

/// Inverse of the generator's escaping (`\\`, `\n`, `\r`, `\t`, `\u{hex}`).
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                assert_eq!(chars.next(), Some('{'));
                let hex: String = chars.by_ref().take_while(|&c| c != '}').collect();
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).unwrap());
            }
            other => panic!("bad escape {other:?} in {s:?}"),
        }
    }
    out
}
