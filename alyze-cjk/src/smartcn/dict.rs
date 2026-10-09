//! The smartcn dictionaries, parsed on first use from the blobs in `data/`, which
//! `examples/smartcn_convert_dicts.rs` produces from Lucene's `coredict.mem` and `bigramdict.mem`
//! (ICTCLAS data, Apache License 2.0) and whose formats it documents.
//!
//! Words are handled as UTF-16 code units throughout, like the Java implementation: every
//! dictionary word is in the BMP, and the bigram table only stores a 64-bit hash of
//! `word1 '@' word2` computed over code units, so the hash has to run on exactly the same units
//! to find anything.

use std::sync::OnceLock;

const CORE_DICT: &[u8] = include_bytes!("../../data/coredict.bin");
const BIGRAM_DICT: &[u8] = include_bytes!("../../data/bigramdict.bin");

/// The core word dictionary: every known word with its frequency, grouped by first code unit.
pub(crate) struct CoreDict {
    row_of_head: Box<[u32; 0x10000]>,
    rows: Vec<Vec<Entry>>,
}

/// A word in the core dictionary, stored without its first code unit (the row's head).
pub(crate) struct Entry {
    pub(crate) suffix: Box<[u16]>,
    pub(crate) freq: i32,
}

const NO_ROW: u32 = u32::MAX;

impl CoreDict {
    pub(crate) fn get() -> &'static CoreDict {
        static CORE: OnceLock<CoreDict> = OnceLock::new();
        CORE.get_or_init(|| CoreDict::parse(CORE_DICT))
    }

    fn parse(blob: &[u8]) -> CoreDict {
        let mut reader = BlobReader { bytes: blob };
        let head_count = reader.u32();
        let mut row_of_head = Box::new([NO_ROW; 0x10000]);
        let mut rows = Vec::with_capacity(head_count as usize);
        for _ in 0..head_count {
            let head = reader.u16();
            let entry_count = reader.u32();
            let mut entries = Vec::with_capacity(entry_count as usize);
            for _ in 0..entry_count {
                let suffix_len = reader.u8() as usize;
                let suffix = (0..suffix_len).map(|_| reader.u16()).collect();
                let freq = reader.i32();
                entries.push(Entry { suffix, freq });
            }
            row_of_head[head as usize] = rows.len() as u32;
            rows.push(entries);
        }
        assert!(
            reader.bytes.is_empty(),
            "trailing bytes in core dictionary blob"
        );
        CoreDict { row_of_head, rows }
    }

    /// Every word starting with `head`, sorted by suffix.
    pub(crate) fn row(&self, head: u16) -> Option<&[Entry]> {
        match self.row_of_head[head as usize] {
            NO_ROW => None,
            row => Some(&self.rows[row as usize]),
        }
    }

    /// Frequency of `word`, or 0 if it isn't in the dictionary.
    pub(crate) fn frequency(&self, word: &[u16]) -> i32 {
        let Some((&head, suffix)) = word.split_first() else {
            return 0;
        };
        let Some(row) = self.row(head) else {
            return 0;
        };
        match row.binary_search_by(|entry| entry.suffix[..].cmp(suffix)) {
            Ok(index) => row[index].freq,
            Err(_) => 0,
        }
    }

    /// Index of the first entry of `row`, at or after `from`, whose suffix starts with `prefix`.
    pub(crate) fn first_with_prefix(row: &[Entry], prefix: &[u16], from: usize) -> Option<usize> {
        let index = from + row[from..].partition_point(|entry| entry.suffix[..] < *prefix);
        row.get(index)
            .filter(|entry| entry.suffix.starts_with(prefix))
            .map(|_| index)
    }

    #[cfg(test)]
    /// Visits every `(word, frequency)` entry, in no particular order.
    pub(crate) fn for_each_entry(&self, mut f: impl FnMut(&[u16], i32)) {
        let mut word = Vec::new();
        for (head, &row) in self.row_of_head.iter().enumerate() {
            if row == NO_ROW {
                continue;
            }
            for entry in &self.rows[row as usize] {
                word.clear();
                word.push(head as u16);
                word.extend_from_slice(&entry.suffix);
                f(&word, entry.freq);
            }
        }
    }
}

/// The bigram dictionary: the frequency of every known word pair, keyed by [`bigram_hash`] of
/// `word1 '@' word2`. Lucene stores only the hash, so pairs whose hashes collide share a
/// frequency; that is part of the behaviour being reproduced.
pub(crate) struct BigramDict {
    hashes: Vec<u64>,
    freqs: Vec<i32>,
}

impl BigramDict {
    pub(crate) fn get() -> &'static BigramDict {
        static BIGRAM: OnceLock<BigramDict> = OnceLock::new();
        BIGRAM.get_or_init(|| BigramDict::parse(BIGRAM_DICT))
    }

    fn parse(blob: &[u8]) -> BigramDict {
        let mut reader = BlobReader { bytes: blob };
        let count = reader.u32() as usize;
        let mut hashes = Vec::with_capacity(count);
        let mut freqs = Vec::with_capacity(count);
        for _ in 0..count {
            hashes.push(reader.u64());
            freqs.push(reader.i32());
        }
        assert!(
            reader.bytes.is_empty(),
            "trailing bytes in bigram dictionary blob"
        );
        assert!(hashes.is_sorted());
        BigramDict { hashes, freqs }
    }

    /// Frequency of the pair `word1 '@' word2` (passed already joined), or 0 if unknown.
    pub(crate) fn frequency(&self, pair: &[u16]) -> i32 {
        match self.hashes.binary_search(&bigram_hash(pair)) {
            Ok(index) => self.freqs[index],
            Err(_) => 0,
        }
    }

    #[cfg(test)]
    /// Visits every `(hash, frequency)` entry, in no particular order.
    pub(crate) fn for_each_entry(&self, mut f: impl FnMut(u64, i32)) {
        for (&hash, &freq) in self.hashes.iter().zip(&self.freqs) {
            f(hash, freq);
        }
    }
}

/// Lucene's `AbstractDictionary.hash1(char[])`: FNV-1a-like, one byte at a time per code unit.
pub(crate) fn bigram_hash(units: &[u16]) -> u64 {
    const PRIME: u64 = 1099511628211;
    let mut hash: u64 = 0xcbf29ce484222325;
    for &unit in units {
        hash = (hash ^ u64::from(unit & 0xFF)).wrapping_mul(PRIME);
        hash = (hash ^ u64::from(unit >> 8)).wrapping_mul(PRIME);
    }
    hash
}

struct BlobReader<'a> {
    bytes: &'a [u8],
}

impl BlobReader<'_> {
    fn take<const N: usize>(&mut self) -> [u8; N] {
        let (head, rest) = self.bytes.split_at(N);
        self.bytes = rest;
        head.try_into().unwrap()
    }

    fn u8(&mut self) -> u8 {
        self.take::<1>()[0]
    }

    fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.take())
    }

    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take())
    }

    fn i32(&mut self) -> i32 {
        i32::from_le_bytes(self.take())
    }

    fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.take())
    }
}
