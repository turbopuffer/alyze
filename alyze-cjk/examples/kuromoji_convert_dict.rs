//! Converts the dump of Lucene's kuromoji dictionaries (written by `testdata/kuromoji/gen.sh
//! dump`) into the blobs in `data/kuromoji/` that `kuromoji::dict` and `kuromoji::char_def` load.
//! Only needs rerunning if Lucene's dictionaries change (they are built from mecab-ipadic
//! 2.7.0-20070801, with Lucene's own patches, unchanged for years).
//!
//!     alyze-cjk/testdata/kuromoji/gen.sh dump
//!     cargo run -p alyze-cjk --example kuromoji_convert_dict -- ~/Src/kuromoji/dump
//!
//! Blob formats (all little-endian; text as UTF-16 code units, like Lucene):
//!
//! - `terms.fst`: an `fst::Map` from each surface form (UTF-16BE bytes, so byte order equals
//!   code-unit order) to its term ordinal, 0-based in sorted order.
//! - `words.bin`: `u32` term count T, `u32` word count W; `(T+1) × u32` word-index bounds per
//!   term (the words of term k are `bounds[k]..bounds[k+1]`, in Lucene's order); `W × 8` word
//!   records: `u16` connection id (left and right ids are equal in mecab-ipadic), `i16` cost,
//!   `u32` offset of the word's extras; `u32` extras length E; E bytes of extras. A word's
//!   extras are `u8` flags (bit 0 base form, bit 1 reading, bit 2 pronunciation, bit 3 / bit 4:
//!   the reading / pronunciation is stored as kana bytes), then, as flagged: base form as `u8`
//!   prefix length shared with the surface form, `u8` suffix length, suffix `u16`s; reading and
//!   pronunciation each as `u8` length then that many bytes (each `0x30A0 + b`) or `u16`s.
//!   A reading is omitted when it is the surface form with hiragana shifted to katakana, a
//!   pronunciation when it equals the reading, a base form when it equals the surface form
//!   (Lucene's `getReading` / `getPronunciation` / `getBaseForm` fallbacks); every word with
//!   nothing to store points at the shared zero-flag blob at offset 0.
//! - `ids.bin`: `u16` count N, then per connection id three strings (part of speech,
//!   inflection type, inflection form; empty = none), each `u8` length + UTF-8. Lucene keys
//!   these by left id too (`posDict[leftId]`).
//! - `unk.bin`: `u8` class count, then per class `u8` entry count and per entry `u16` id, `i16`
//!   cost, `u8` POS length + UTF-8.
//! - `costs.bin`: `u32` right-id count R, `u32` left-id count L, `L × R` `i16`, left-id major
//!   (like Lucene: the Viterbi's inner loop varies the right id, so those reads are contiguous).
//! - `chardef.bin`: `u8` class count C, `65536` class bytes (one per code unit), `C` flag bytes
//!   (bit 0 invoke, bit 1 group), from `testdata/kuromoji/golden/chardef.txt`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn main() {
    let dump = std::env::args()
        .nth(1)
        .expect("path to the dictionary dump directory");
    let dump = Path::new(&dump);
    let root = env!("CARGO_MANIFEST_DIR");
    let out = Path::new(root).join("data/kuromoji");
    fs::create_dir_all(&out).unwrap();

    convert_token_info(&dump.join("tokeninfo.tsv"), &out);
    convert_unknown(&dump.join("unk.tsv"), &out.join("unk.bin"));
    convert_costs(&dump.join("costs.bin"), &out.join("costs.bin"));
    convert_chardef(
        &Path::new(root).join("testdata/kuromoji/golden/chardef.txt"),
        &out.join("chardef.bin"),
    );
    for name in [
        "terms.fst",
        "words.bin",
        "ids.bin",
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
    "KANJINUMERIC",
];

const NULL: &str = "\\N";

fn index_of(table: &[&str], name: &str) -> u8 {
    table
        .iter()
        .position(|t| *t == name)
        .unwrap_or_else(|| panic!("unknown name {name:?}")) as u8
}

/// Lucene's `TokenInfoMorphData.getReading` fallback: hiragana shifted to katakana.
fn default_reading(surface: &[u16]) -> Vec<u16> {
    surface
        .iter()
        .map(|&u| {
            if u > 0x3040 && u < 0x3097 {
                u + 0x60
            } else {
                u
            }
        })
        .collect()
}

/// Appends a kana string (length, then bytes if every unit is in U+30A0..U+30FF, else units);
/// returns whether the kana encoding was used.
fn push_kana_string(units: &[u16], out: &mut Vec<u8>) -> bool {
    out.push(u8::try_from(units.len()).expect("string too long"));
    if units.iter().all(|&u| (0x30A0..0x30A0 + 0x100).contains(&u)) {
        out.extend(units.iter().map(|&u| (u - 0x30A0) as u8));
        true
    } else {
        for u in units {
            out.extend_from_slice(&u.to_le_bytes());
        }
        false
    }
}

fn convert_token_info(tsv: &Path, out: &Path) {
    let text = fs::read_to_string(tsv).unwrap();
    let mut fst = fst::MapBuilder::memory();
    // bounds[k] is the first word index of term k; the last entry is the word count.
    let mut bounds: Vec<u32> = Vec::new();
    let mut records: Vec<u8> = Vec::new();
    let mut extras: Vec<u8> = vec![0]; // offset 0: the shared "nothing stored" blob
    let mut ids: BTreeMap<u16, (String, String, String)> = BTreeMap::new();
    let mut last_surface: Option<Vec<u16>> = None;
    let mut word_id = 0u32;
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 10, "bad line {line:?}");
        let surface = utf16(&unescape(fields[0]));
        if last_surface.as_ref() != Some(&surface) {
            if let Some(last) = &last_surface {
                assert!(last < &surface, "terms out of order at {line:?}");
            }
            fst.insert(utf16be(&surface), bounds.len() as u64).unwrap();
            bounds.push(word_id);
            last_surface = Some(surface.clone());
        }
        let left_id: u16 = fields[1].parse().unwrap();
        let right_id: u16 = fields[2].parse().unwrap();
        assert_eq!(left_id, right_id, "left and right ids differ at {line:?}");
        let cost: i16 = fields[3].parse().unwrap();
        let strings = (
            unescape(fields[4]),
            optional(fields[8]),
            optional(fields[9]),
        );
        match ids.get(&left_id) {
            Some(existing) => assert_eq!(existing, &strings, "id {left_id} is ambiguous"),
            None => {
                ids.insert(left_id, strings);
            }
        }

        // Extras: only what the fallbacks can't reconstruct.
        let base_form = (fields[5] != NULL).then(|| utf16(&unescape(fields[5])));
        let reading = utf16(&unescape(fields[6]));
        let pronunciation = utf16(&unescape(fields[7]));
        let base_form = base_form.filter(|b| b != &surface);
        let reading = (reading != default_reading(&surface)).then_some(reading);
        let pronunciation = match &reading {
            Some(r) => (pronunciation != *r).then_some(pronunciation),
            None => (pronunciation != default_reading(&surface)).then_some(pronunciation),
        };
        let offset = if base_form.is_none() && reading.is_none() && pronunciation.is_none() {
            0
        } else {
            let offset = extras.len() as u32;
            let flags_at = extras.len();
            extras.push(0);
            let mut flags = 0u8;
            if let Some(base) = &base_form {
                flags |= 1;
                let prefix = surface.iter().zip(base).take_while(|(a, b)| a == b).count();
                extras.push(u8::try_from(prefix).unwrap());
                extras.push(u8::try_from(base.len() - prefix).unwrap());
                for u in &base[prefix..] {
                    extras.extend_from_slice(&u.to_le_bytes());
                }
            }
            if let Some(reading) = &reading {
                flags |= 2;
                if push_kana_string(reading, &mut extras) {
                    flags |= 8;
                }
            }
            if let Some(pronunciation) = &pronunciation {
                flags |= 4;
                if push_kana_string(pronunciation, &mut extras) {
                    flags |= 16;
                }
            }
            extras[flags_at] = flags;
            offset
        };
        records.extend_from_slice(&left_id.to_le_bytes());
        records.extend_from_slice(&cost.to_le_bytes());
        records.extend_from_slice(&offset.to_le_bytes());
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
    words.extend_from_slice(&(extras.len() as u32).to_le_bytes());
    words.extend_from_slice(&extras);
    fs::write(out.join("words.bin"), words).unwrap();

    let count = ids.keys().next_back().map_or(0, |&id| id as usize + 1);
    let mut blob: Vec<u8> = Vec::new();
    blob.extend_from_slice(&(count as u16).to_le_bytes());
    for id in 0..count as u16 {
        let (pos, infl_type, infl_form) = ids.get(&id).cloned().unwrap_or_default();
        for s in [pos, infl_type, infl_form] {
            blob.push(u8::try_from(s.len()).unwrap());
            blob.extend_from_slice(s.as_bytes());
        }
    }
    fs::write(out.join("ids.bin"), blob).unwrap();
    eprintln!(
        "{terms} terms, {word_id} words, {} bytes of extras, {} connection ids",
        extras.len(),
        ids.len()
    );
}

fn optional(field: &str) -> String {
    if field == NULL {
        String::new()
    } else {
        unescape(field)
    }
}

fn convert_unknown(tsv: &Path, out: &Path) {
    let text = fs::read_to_string(tsv).unwrap();
    let mut by_class: BTreeMap<u8, Vec<Vec<u8>>> = BTreeMap::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 5, "bad line {line:?}");
        let class = index_of(CLASSES, fields[0]);
        let left_id: u16 = fields[1].parse().unwrap();
        let right_id: u16 = fields[2].parse().unwrap();
        assert_eq!(left_id, right_id);
        let cost: i16 = fields[3].parse().unwrap();
        let pos = unescape(fields[4]);
        let mut rec = Vec::new();
        rec.extend_from_slice(&left_id.to_le_bytes());
        rec.extend_from_slice(&cost.to_le_bytes());
        rec.push(u8::try_from(pos.len()).unwrap());
        rec.extend_from_slice(pos.as_bytes());
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
    // The dump has no header; the matrix is square (mecab-ipadic's left-id.def / right-id.def
    // both have 1316 ids).
    let side = 1316usize;
    assert_eq!(matrix.len(), side * side * 2, "unexpected matrix size");
    let (right, left) = (side, side);
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
