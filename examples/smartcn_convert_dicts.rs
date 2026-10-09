//! Converts Lucene's smartcn dictionaries (`coredict.mem`, `bigramdict.mem`: Java-serialized
//! primitive arrays) into the compact blobs that `alyze-cjk` ships and `alyze::smartcn::dict`
//! parses. Only needs rerunning if Lucene's dictionaries change (they haven't in over a decade).
//!
//!     cargo run -p alyze --example smartcn_convert_dicts -- \
//!         ~/Src/smartcn/lucene/analysis/smartcn/src/resources/org/apache/lucene/analysis/cn/smart/hhmm
//!
//! Core dictionary blob (little-endian): `u32` head count, then per head character (ascending)
//! `u16` head, `u32` entry count, then per entry `u8` suffix length, that many `u16` code units,
//! `i32` frequency. The word is head + suffix. Entries are sorted by suffix and unique.
//!
//! Bigram dictionary blob (little-endian): `u32` count, then that many (`u64` hash, `i32`
//! frequency) pairs sorted by hash. Lucene only stores the hash of `word1 '@' word2`.

use std::collections::BTreeMap;
use std::path::Path;

fn main() {
    let source_dir = std::env::args()
        .nth(1)
        .expect("path to Lucene's hhmm resource directory");
    let out_dir = format!("{}/alyze-cjk/data", env!("CARGO_MANIFEST_DIR"));

    let core = convert_core(&std::fs::read(Path::new(&source_dir).join("coredict.mem")).unwrap());
    std::fs::write(format!("{out_dir}/coredict.bin"), &core).unwrap();
    let bigram =
        convert_bigram(&std::fs::read(Path::new(&source_dir).join("bigramdict.mem")).unwrap());
    std::fs::write(format!("{out_dir}/bigramdict.bin"), &bigram).unwrap();
    eprintln!(
        "wrote {out_dir}/coredict.bin ({} bytes) and bigramdict.bin ({} bytes)",
        core.len(),
        bigram.len()
    );
}

fn convert_core(mem: &[u8]) -> Vec<u8> {
    let mut reader = JavaObjectReader::new(mem);
    let JavaValue::Shorts(word_index_table) = reader.read_object() else {
        panic!("expected short[]")
    };
    let JavaValue::Chars(char_index_table) = reader.read_object() else {
        panic!("expected char[]")
    };
    let JavaValue::Array(word_tables) = reader.read_object() else {
        panic!("expected char[][][]")
    };
    let JavaValue::Array(freq_tables) = reader.read_object() else {
        panic!("expected int[][]")
    };

    let mut heads: BTreeMap<u16, Vec<(Vec<u16>, i32)>> = BTreeMap::new();
    for (slot, &head) in char_index_table.iter().enumerate() {
        if head == 0 {
            continue;
        }
        let row = usize::try_from(word_index_table[slot]).unwrap();
        let JavaValue::Array(words) = &word_tables[row] else {
            continue; // Lucene nulls out the row it expanded punctuation from, but leaves its head
        };
        let JavaValue::Ints(freqs) = &freq_tables[row] else {
            panic!("row {row} has words but no freqs")
        };
        let mut entries = Vec::with_capacity(words.len());
        for (word, freq) in words.iter().zip(freqs) {
            let suffix = match word {
                JavaValue::Null => Vec::new(),
                JavaValue::Chars(c) => c.clone(),
                other => panic!("unexpected {other:?}"),
            };
            entries.push((suffix, *freq));
        }
        assert!(
            entries.windows(2).all(|w| w[0].0 < w[1].0),
            "row for U+{head:04X} not sorted/unique"
        );
        assert!(heads.insert(head, entries).is_none());
    }

    let mut out = Vec::new();
    push_u32(&mut out, heads.len() as u32);
    let mut entry_count = 0;
    for (head, entries) in &heads {
        out.extend_from_slice(&head.to_le_bytes());
        push_u32(&mut out, entries.len() as u32);
        for (suffix, freq) in entries {
            out.push(u8::try_from(suffix.len()).unwrap());
            for unit in suffix {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            out.extend_from_slice(&freq.to_le_bytes());
            entry_count += 1;
        }
    }
    eprintln!("core: {} heads, {entry_count} entries", heads.len());
    out
}

fn convert_bigram(mem: &[u8]) -> Vec<u8> {
    let mut reader = JavaObjectReader::new(mem);
    let JavaValue::Longs(hashes) = reader.read_object() else {
        panic!("expected long[]")
    };
    let JavaValue::Ints(freqs) = reader.read_object() else {
        panic!("expected int[]")
    };
    let mut pairs: Vec<(u64, i32)> = hashes
        .iter()
        .zip(&freqs)
        .filter(|(hash, _)| **hash != 0)
        .map(|(&hash, &freq)| (hash as u64, freq))
        .collect();
    pairs.sort_unstable();
    assert!(pairs.windows(2).all(|w| w[0].0 != w[1].0));
    let mut out = Vec::new();
    push_u32(&mut out, pairs.len() as u32);
    for (hash, freq) in &pairs {
        out.extend_from_slice(&hash.to_le_bytes());
        out.extend_from_slice(&freq.to_le_bytes());
    }
    eprintln!("bigram: {} entries", pairs.len());
    out
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Just enough of the Java serialization format to read primitive arrays and arrays of them.
struct JavaObjectReader<'a> {
    bytes: &'a [u8],
    pos: usize,
    /// Class names by handle (arrays also take a handle slot, as `None`).
    handles: Vec<Option<String>>,
}

#[derive(Debug)]
enum JavaValue {
    Null,
    Shorts(Vec<i16>),
    Chars(Vec<u16>),
    Ints(Vec<i32>),
    Longs(Vec<i64>),
    Array(Vec<JavaValue>),
}

const TC_NULL: u8 = 0x70;
const TC_REFERENCE: u8 = 0x71;
const TC_CLASSDESC: u8 = 0x72;
const TC_ARRAY: u8 = 0x75;
const TC_ENDBLOCKDATA: u8 = 0x78;
const BASE_HANDLE: u32 = 0x7E0000;

impl<'a> JavaObjectReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        assert_eq!(
            &bytes[..4],
            &[0xAC, 0xED, 0x00, 0x05],
            "not a Java serialization stream"
        );
        Self {
            bytes,
            pos: 4,
            handles: Vec::new(),
        }
    }

    fn u8(&mut self) -> u8 {
        let v = self.bytes[self.pos];
        self.pos += 1;
        v
    }

    fn u16(&mut self) -> u16 {
        u16::from_be_bytes(self.take::<2>())
    }

    fn i32(&mut self) -> i32 {
        i32::from_be_bytes(self.take::<4>())
    }

    fn take<const N: usize>(&mut self) -> [u8; N] {
        let v: [u8; N] = self.bytes[self.pos..self.pos + N].try_into().unwrap();
        self.pos += N;
        v
    }

    fn read_object(&mut self) -> JavaValue {
        match self.u8() {
            TC_NULL => JavaValue::Null,
            TC_ARRAY => {
                let class_name = self.read_class_desc();
                self.handles.push(None);
                let len = usize::try_from(self.i32()).unwrap();
                match class_name.as_str() {
                    "[S" => JavaValue::Shorts((0..len).map(|_| self.u16() as i16).collect()),
                    "[C" => JavaValue::Chars((0..len).map(|_| self.u16()).collect()),
                    "[I" => JavaValue::Ints((0..len).map(|_| self.i32()).collect()),
                    "[J" => JavaValue::Longs(
                        (0..len)
                            .map(|_| i64::from_be_bytes(self.take::<8>()))
                            .collect(),
                    ),
                    name if name.starts_with("[[") => {
                        JavaValue::Array((0..len).map(|_| self.read_object()).collect())
                    }
                    other => panic!("unsupported array class {other}"),
                }
            }
            tag => panic!("unsupported type code {tag:#x} at {}", self.pos - 1),
        }
    }

    fn read_class_desc(&mut self) -> String {
        match self.u8() {
            TC_REFERENCE => {
                let handle = u32::from_be_bytes(self.take::<4>());
                self.handles[(handle - BASE_HANDLE) as usize]
                    .clone()
                    .expect("reference to a non-class handle")
            }
            TC_CLASSDESC => {
                let len = self.u16() as usize;
                let name =
                    String::from_utf8(self.bytes[self.pos..self.pos + len].to_vec()).unwrap();
                self.pos += len;
                self.pos += 8; // serialVersionUID
                self.u8(); // flags
                assert_eq!(self.u16(), 0, "arrays have no fields");
                assert_eq!(self.u8(), TC_ENDBLOCKDATA);
                assert_eq!(
                    self.u8(),
                    TC_NULL,
                    "array classes have no superclass descriptor"
                );
                self.handles.push(Some(name.clone()));
                name
            }
            tag => panic!("unexpected type code {tag:#x} in class descriptor"),
        }
    }
}
