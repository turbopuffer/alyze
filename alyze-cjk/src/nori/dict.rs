//! The system dictionaries: mecab-ko-dic as compiled into Lucene's binary files, converted once
//! into the blobs in `data/nori/` (see `examples/nori_convert_dict.rs` for the formats).
//!
//! - [`TokenInfoDict`]: every surface form with its words (a surface form can have several
//!   entries); per word the connection ids, cost, part of speech, reading and morphemes.
//! - [`UnknownDict`]: one entry per character class, used for words not in any dictionary.
//! - [`ConnectionCosts`]: the cost of putting a word with a given left id after a word with a
//!   given right id.
//!
//! All surface forms are handled as UTF-16 code units, like Lucene: the lattice is built per code
//! unit and the term index is keyed by code unit. Everything is read in place from the embedded
//! blobs; nothing is copied at load time.

use std::ops::Range;
use std::sync::OnceLock;

use fst::raw::Fst;

pub(crate) use crate::morph::term_index::TermIndex;

use super::Morpheme;
use super::char_def::CharClass;
use super::pos;

/// Everything the Viterbi search needs to know about one dictionary word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WordInfo {
    pub left_id: u16,
    pub right_id: u16,
    pub cost: i16,
    pub pos_type: pos::Type,
    pub left_pos: pos::Tag,
    pub right_pos: pos::Tag,
}

impl WordInfo {
    /// Decodes an 8-byte word record.
    fn read(record: &[u8]) -> WordInfo {
        let left = u16::from_le_bytes([record[0], record[1]]);
        WordInfo {
            left_id: left & 0x3FFF,
            right_id: u16::from_le_bytes([record[2], record[3]]) & 0x3FFF,
            cost: i16::from_le_bytes([record[4], record[5]]),
            pos_type: match left >> 14 {
                0 => pos::Type::Morpheme,
                1 => pos::Type::Compound,
                2 => pos::Type::Inflect,
                _ => pos::Type::Preanalysis,
            },
            left_pos: pos::Tag::from_index(record[6]).expect("bad tag"),
            right_pos: pos::Tag::from_index(record[7]).expect("bad tag"),
        }
    }
}

/// A word id: an index into the token-info dictionary's records.
pub(crate) type WordId = u32;
/// The words of a term, consecutive ids in Lucene's order (which decides ties in the lattice).
pub(crate) type WordIds = Range<WordId>;

const RECORD_LEN: usize = 8;
/// Flags in a record's right-id word: the word has a reading / has morphemes in the extras.
const HAS_READING: u16 = 1 << 14;
const HAS_MORPHEMES: u16 = 1 << 15;

pub(crate) struct TokenInfoDict {
    terms: TermIndex<&'static [u8]>,
    /// Word-index bounds per term: term k's words are `bounds[k]..bounds[k+1]`.
    bounds: &'static [u8],
    records: &'static [u8],
    /// `(word id, offset into extras)` pairs sorted by word id.
    extras_index: &'static [u8],
    extras: &'static [u8],
}

static TERMS_FST: &[u8] = include_bytes!("../../data/nori/terms.fst");
static WORDS_BIN: &[u8] = include_bytes!("../../data/nori/words.bin");
static UNK_BIN: &[u8] = include_bytes!("../../data/nori/unk.bin");
static COSTS_BIN: &[u8] = include_bytes!("../../data/nori/costs.bin");

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

impl TokenInfoDict {
    pub fn get() -> &'static TokenInfoDict {
        static DICT: OnceLock<TokenInfoDict> = OnceLock::new();
        DICT.get_or_init(|| {
            let fst = Fst::new(TERMS_FST).expect("corrupt terms.fst");
            let terms = u32_at(WORDS_BIN, 0) as usize;
            let words = u32_at(WORDS_BIN, 4) as usize;
            let bounds_start = 8;
            let records_start = bounds_start + (terms + 1) * 4;
            let extras_count_at = records_start + words * RECORD_LEN;
            let extras_count = u32_at(WORDS_BIN, extras_count_at) as usize;
            let extras_index_start = extras_count_at + 4;
            let extras_start = extras_index_start + extras_count * 8;
            TokenInfoDict {
                terms: TermIndex::new(fst),
                bounds: &WORDS_BIN[bounds_start..records_start],
                records: &WORDS_BIN[records_start..extras_count_at],
                extras_index: &WORDS_BIN[extras_index_start..extras_start],
                extras: &WORDS_BIN[extras_start..],
            }
        })
    }

    fn words_of(&self, ord: u64) -> WordIds {
        let k = ord as usize * 4;
        u32_at(self.bounds, k)..u32_at(self.bounds, k + 4)
    }

    /// The words of the surface form `surface`, in dictionary order, or `None` if it isn't a
    /// dictionary term.
    #[cfg(test)]
    pub fn lookup(&self, surface: &[u16]) -> Option<WordIds> {
        self.terms.lookup(surface).map(|ord| self.words_of(ord))
    }

    /// Calls `f(length, words)` for every dictionary term that is a prefix of `text`, shortest
    /// first (what Lucene's FST walk from one position yields). Returns whether any matched.
    pub fn for_each_prefix(&self, text: &[u16], mut f: impl FnMut(usize, WordIds)) -> bool {
        self.terms
            .for_each_prefix(text, |len, ord| f(len, self.words_of(ord)))
    }

    pub fn word(&self, id: WordId) -> WordInfo {
        let at = id as usize * RECORD_LEN;
        WordInfo::read(&self.records[at..at + RECORD_LEN])
    }

    fn flags(&self, id: WordId) -> u16 {
        u16_at(self.records, id as usize * RECORD_LEN + 2) & (HAS_READING | HAS_MORPHEMES)
    }

    /// The extras block of a word (reading, morphemes), if it has one.
    fn extras_of(&self, id: WordId) -> Option<&'static [u8]> {
        let count = self.extras_index.len() / 8;
        let mut lo = 0usize;
        let mut hi = count;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let mid_id = u32_at(self.extras_index, mid * 8);
            match mid_id.cmp(&id) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => {
                    let offset = u32_at(self.extras_index, mid * 8 + 4) as usize;
                    return Some(&self.extras[offset..]);
                }
            }
        }
        None
    }

    /// The Hangul reading of a Hanja word, as UTF-16 code units (little-endian bytes decoded
    /// into `out`, which is cleared first). Returns whether there was one.
    pub fn reading(&self, id: WordId, out: &mut Vec<u16>) -> bool {
        out.clear();
        if self.flags(id) & HAS_READING == 0 {
            return false;
        }
        let Some(extras) = self.extras_of(id) else {
            return false;
        };
        let len = extras[0] as usize;
        if len == 0 {
            return false;
        }
        out.extend((0..len).map(|i| u16_at(extras, 1 + i * 2)));
        true
    }

    /// The morphemes of a compound, inflected or pre-analysed word (`None` for a plain morpheme
    /// or an entry without a decomposition).
    pub fn morphemes(&self, id: WordId) -> Option<Vec<Morpheme>> {
        if self.flags(id) & HAS_MORPHEMES == 0 {
            return None;
        }
        let extras = self.extras_of(id)?;
        let mut at = 1 + extras[0] as usize * 2;
        let count = extras[at] as usize;
        at += 1;
        if count == 0 {
            return None;
        }
        let mut morphemes = Vec::with_capacity(count);
        let mut units = Vec::new();
        for _ in 0..count {
            let tag = pos::Tag::from_index(extras[at]).expect("bad tag");
            let len = extras[at + 1] as usize;
            at += 2;
            units.clear();
            units.extend((0..len).map(|i| u16_at(extras, at + i * 2)));
            at += len * 2;
            morphemes.push(Morpheme {
                tag,
                text: String::from_utf16(&units).expect("bad morpheme text"),
            });
        }
        Some(morphemes)
    }

    /// Calls `f(surface, words)` for every term in lookup order (surface forms sorted by UTF-16
    /// code units), for tests.
    #[cfg(test)]
    pub fn for_each_term(&self, mut f: impl FnMut(&[u16], WordIds)) {
        self.terms
            .for_each_term(|surface, ord| f(surface, self.words_of(ord)));
    }
}

pub(crate) struct UnknownDict {
    /// Entries per character class, in class order.
    entries: Vec<Vec<WordInfo>>,
}

impl UnknownDict {
    pub fn get() -> &'static UnknownDict {
        static DICT: OnceLock<UnknownDict> = OnceLock::new();
        DICT.get_or_init(|| {
            let classes = UNK_BIN[0] as usize;
            assert_eq!(classes, CharClass::ALL.len());
            let mut at = 1;
            let mut entries = Vec::with_capacity(classes);
            for _ in 0..classes {
                let count = UNK_BIN[at] as usize;
                at += 1;
                let mut words = Vec::with_capacity(count);
                for _ in 0..count {
                    words.push(WordInfo::read(&UNK_BIN[at..at + RECORD_LEN]));
                    at += RECORD_LEN;
                }
                entries.push(words);
            }
            UnknownDict { entries }
        })
    }

    /// The entries for unknown words made of characters of `class` (mecab-ko-dic has exactly one
    /// per class, but Lucene allows several).
    pub fn words(&self, class: CharClass) -> &[WordInfo] {
        &self.entries[class as usize]
    }
}

/// Stored left-id major, like Lucene: `cost` is called in a loop over the arcs arriving at a
/// position (varying right ids) for one word (fixed left id), so those reads are contiguous.
pub(crate) struct ConnectionCosts {
    right_ids: usize,
    #[cfg(test)]
    left_ids: usize,
    matrix: &'static [u8],
}

impl ConnectionCosts {
    pub fn get() -> &'static ConnectionCosts {
        static COSTS: OnceLock<ConnectionCosts> = OnceLock::new();
        COSTS.get_or_init(|| {
            let right_ids = u32_at(COSTS_BIN, 0) as usize;
            let left_ids = u32_at(COSTS_BIN, 4) as usize;
            let matrix = &COSTS_BIN[8..];
            assert_eq!(matrix.len(), right_ids * left_ids * 2);
            ConnectionCosts {
                right_ids,
                #[cfg(test)]
                left_ids,
                matrix,
            }
        })
    }

    /// Number of right ids (rows) and left ids (columns).
    #[cfg(test)]
    pub fn dimensions(&self) -> (usize, usize) {
        (self.right_ids, self.left_ids)
    }

    /// The cost of a word with `left_id` following a word with `right_id` (0 is BOS/EOS).
    #[inline]
    pub fn cost(&self, right_id: u16, left_id: u16) -> i16 {
        let at = (left_id as usize * self.right_ids + right_id as usize) * 2;
        i16::from_le_bytes([self.matrix[at], self.matrix[at + 1]])
    }
}
