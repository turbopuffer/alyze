//! The converted dictionaries must contain exactly what Lucene's do (checksums over a canonical
//! dump) and answer lookups identically (probes).

use super::{read_testdata, unescape};
use crate::smartcn::dict::{BigramDict, CoreDict};

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

struct Golden {
    core_entries: usize,
    core_checksum: u64,
    bigram_entries: usize,
    bigram_checksum: u64,
    /// `(is_core, word or pair, freq)`
    probes: Vec<(bool, String, i32)>,
}

fn read_golden() -> Golden {
    let mut g = Golden {
        core_entries: 0,
        core_checksum: 0,
        bigram_entries: 0,
        bigram_checksum: 0,
        probes: Vec::new(),
    };
    for line in read_testdata("golden/dict.txt").lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields[0] {
            "core.entries" => g.core_entries = fields[1].parse().unwrap(),
            "core.checksum" => g.core_checksum = u64::from_str_radix(fields[1], 16).unwrap(),
            "bigram.entries" => g.bigram_entries = fields[1].parse().unwrap(),
            "bigram.checksum" => g.bigram_checksum = u64::from_str_radix(fields[1], 16).unwrap(),
            "core" | "bigram" => g.probes.push((
                fields[0] == "core",
                unescape(fields[1]),
                fields[2].parse().unwrap(),
            )),
            other => panic!("unknown line {other:?}"),
        }
    }
    g
}

/// Canonical dump of the core dictionary: `(word, freq)` sorted by word as UTF-16 code units (the
/// order Java's `String.compareTo` gives), hashed as `"<word>\t<freq>\n"` in UTF-8.
#[test]
fn core_dictionary_complete() {
    let golden = read_golden();
    let mut entries: Vec<(Vec<u16>, i32)> = Vec::new();
    CoreDict::get().for_each_entry(|word, freq| entries.push((word.to_vec(), freq)));
    entries.sort();
    assert_eq!(
        entries.len(),
        golden.core_entries,
        "core dictionary entry count"
    );
    let mut checksum = FNV_OFFSET;
    for (word, freq) in &entries {
        let line = format!("{}\t{freq}\n", String::from_utf16(word).unwrap());
        checksum = fnv1a(checksum, line.as_bytes());
    }
    assert_eq!(checksum, golden.core_checksum, "core dictionary checksum");
}

/// Canonical dump of the bigram table: `(hash, freq)` sorted by hash as a signed 64-bit integer
/// (Java's `Long.compare`), hashed as little-endian `i64` then `i32`.
#[test]
fn bigram_dictionary_complete() {
    let golden = read_golden();
    let mut entries: Vec<(i64, i32)> = Vec::new();
    BigramDict::get().for_each_entry(|hash, freq| entries.push((hash as i64, freq)));
    entries.sort();
    assert_eq!(
        entries.len(),
        golden.bigram_entries,
        "bigram dictionary entry count"
    );
    let mut checksum = FNV_OFFSET;
    for (hash, freq) in &entries {
        let mut bytes = [0u8; 12];
        bytes[..8].copy_from_slice(&hash.to_le_bytes());
        bytes[8..].copy_from_slice(&freq.to_le_bytes());
        checksum = fnv1a(checksum, &bytes);
    }
    assert_eq!(
        checksum, golden.bigram_checksum,
        "bigram dictionary checksum"
    );
}

#[test]
fn probes() {
    let golden = read_golden();
    assert!(golden.probes.len() > 50);
    let mut failures = Vec::new();
    for (is_core, word, expected) in &golden.probes {
        let units: Vec<u16> = word.encode_utf16().collect();
        let actual = if *is_core {
            CoreDict::get().frequency(&units)
        } else {
            BigramDict::get().frequency(&units)
        };
        if actual != *expected {
            failures.push(format!(
                "{} {word:?}: expected {expected}, got {actual}",
                if *is_core { "core" } else { "bigram" }
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} probes differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
