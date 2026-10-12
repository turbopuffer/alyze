//! The system dictionaries: mecab-ipadic as compiled into Lucene's binary files, converted once
//! into the blobs in `data/kuromoji/` (see `examples/kuromoji_convert_dict.rs` for the formats).
//!
//! - [`TokenInfoDict`]: every surface form with its words (a surface form can have several
//!   entries); per word the connection id, cost, and the part of speech, base form, reading,
//!   pronunciation and inflection Lucene's `TokenInfoMorphData` exposes.
//! - [`UnknownDict`]: the entries per character class, used for words not in any dictionary.
//! - [`ConnectionCosts`]: the cost of putting a word with a given left id after a word with a
//!   given right id.
//!
//! All surface forms are handled as UTF-16 code units, like Lucene: the lattice is built per code
//! unit and the term index is keyed by code unit. Everything is read in place from the embedded
//! blobs; nothing is copied at load time.

use std::ops::Range;
use std::sync::OnceLock;

use fst::raw::Fst;

use super::char_def::CharClass;
use crate::morph::term_index::TermIndex;

/// A word id: an index into the token-info dictionary's records.
pub(crate) type WordId = u32;
/// The words of a term, consecutive ids in Lucene's order (which decides ties in the lattice).
pub(crate) type WordIds = Range<WordId>;

/// The lattice-relevant part of a word record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WordInfo {
    /// Left and right connection ids are always equal in mecab-ipadic; both kept for symmetry
    /// with the Java API.
    pub left_id: u16,
    pub right_id: u16,
    pub cost: i16,
}

const RECORD_LEN: usize = 8;
const HAS_BASE_FORM: u8 = 1;
const HAS_READING: u8 = 2;
const HAS_PRONUNCIATION: u8 = 4;
const READING_IS_KANA: u8 = 8;
const PRONUNCIATION_IS_KANA: u8 = 16;

static TERMS_FST: &[u8] = include_bytes!("../../data/kuromoji/terms.fst");
static WORDS_BIN: &[u8] = include_bytes!("../../data/kuromoji/words.bin");
static IDS_BIN: &[u8] = include_bytes!("../../data/kuromoji/ids.bin");
static UNK_BIN: &[u8] = include_bytes!("../../data/kuromoji/unk.bin");
static COSTS_BIN: &[u8] = include_bytes!("../../data/kuromoji/costs.bin");

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

/// Reads a `u8`-length-prefixed UTF-8 string from a static blob.
fn str_at(bytes: &'static [u8], offset: usize) -> (&'static str, usize) {
    let len = bytes[offset] as usize;
    let s = std::str::from_utf8(&bytes[offset + 1..offset + 1 + len]).expect("bad UTF-8");
    (s, offset + 1 + len)
}

/// Appends UTF-16 units to a string (a lone surrogate becomes U+FFFD; dictionary text never has
/// one).
fn push_units(units: impl Iterator<Item = u16>, out: &mut String) {
    out.extend(char::decode_utf16(units).map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER)));
}

/// Lucene's `TokenInfoMorphData.getReading` fallback: the surface form with hiragana shifted to
/// katakana.
fn push_default_reading(surface: &[u16], out: &mut String) {
    push_units(
        surface.iter().map(|&u| {
            if u > 0x3040 && u < 0x3097 {
                u + 0x60
            } else {
                u
            }
        }),
        out,
    );
}

/// Appends a kana / UTF-16 string as the converter writes it; returns the offset after it.
fn push_string(extras: &[u8], at: usize, kana: bool, out: &mut String) -> usize {
    let len = extras[at] as usize;
    let at = at + 1;
    if kana {
        push_units(
            extras[at..at + len].iter().map(|&b| 0x30A0 + u16::from(b)),
            out,
        );
        at + len
    } else {
        push_units((0..len).map(|i| u16_at(extras, at + i * 2)), out);
        at + len * 2
    }
}

/// The length in bytes of a kana / UTF-16 string as the converter writes it, to skip it.
fn string_len(extras: &[u8], at: usize, kana: bool) -> usize {
    let len = extras[at] as usize;
    1 + if kana { len } else { len * 2 }
}

/// The strings keyed by connection id: part of speech, inflection type, inflection form.
struct IdStrings {
    part_of_speech: &'static str,
    inflection_type: Option<&'static str>,
    inflection_form: Option<&'static str>,
}

pub(crate) struct TokenInfoDict {
    terms: TermIndex<&'static [u8]>,
    /// Word-index bounds per term: term k's words are `bounds[k]..bounds[k+1]`.
    bounds: &'static [u8],
    records: &'static [u8],
    extras: &'static [u8],
    ids: Vec<IdStrings>,
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
            let extras_len_at = records_start + words * RECORD_LEN;
            let extras_len = u32_at(WORDS_BIN, extras_len_at) as usize;
            let extras_start = extras_len_at + 4;
            let count = u16_at(IDS_BIN, 0) as usize;
            let mut ids = Vec::with_capacity(count);
            let mut at = 2;
            for _ in 0..count {
                let (pos, next) = str_at(IDS_BIN, at);
                let (infl_type, next) = str_at(IDS_BIN, next);
                let (infl_form, next) = str_at(IDS_BIN, next);
                at = next;
                ids.push(IdStrings {
                    part_of_speech: pos,
                    inflection_type: (!infl_type.is_empty()).then_some(infl_type),
                    inflection_form: (!infl_form.is_empty()).then_some(infl_form),
                });
            }
            TokenInfoDict {
                terms: TermIndex::new(fst),
                bounds: &WORDS_BIN[bounds_start..records_start],
                records: &WORDS_BIN[records_start..extras_len_at],
                extras: &WORDS_BIN[extras_start..extras_start + extras_len],
                ids,
            }
        })
    }

    fn words_of(&self, ord: u64) -> WordIds {
        let k = ord as usize * 4;
        u32_at(self.bounds, k)..u32_at(self.bounds, k + 4)
    }

    /// The word ids of `surface` (UTF-16), in dictionary order, if it is a term.
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

    /// Calls `f(surface, words)` for every term in lookup order (surface forms sorted by UTF-16
    /// code units), for tests.
    #[cfg(test)]
    pub fn for_each_term(&self, mut f: impl FnMut(&[u16], WordIds)) {
        self.terms
            .for_each_term(|surface, ord| f(surface, self.words_of(ord)));
    }

    #[inline]
    fn record(&self, id: WordId) -> &'static [u8] {
        let at = id as usize * RECORD_LEN;
        &self.records[at..at + RECORD_LEN]
    }

    #[inline]
    pub fn word(&self, id: WordId) -> WordInfo {
        let r = self.record(id);
        let conn = u16::from_le_bytes([r[0], r[1]]);
        WordInfo {
            left_id: conn,
            right_id: conn,
            cost: i16::from_le_bytes([r[2], r[3]]),
        }
    }

    /// The word's extras blob (flags byte first).
    fn extras_of(&self, id: WordId) -> &'static [u8] {
        let r = self.record(id);
        let offset = u32::from_le_bytes([r[4], r[5], r[6], r[7]]) as usize;
        &self.extras[offset..]
    }

    fn id_strings(&self, id: WordId) -> &IdStrings {
        &self.ids[self.word(id).left_id as usize]
    }

    /// Lucene's `getPartOfSpeech`: keyed by the connection id.
    pub fn part_of_speech(&self, id: WordId) -> &'static str {
        self.id_strings(id).part_of_speech
    }

    pub fn inflection_type(&self, id: WordId) -> Option<&'static str> {
        self.id_strings(id).inflection_type
    }

    pub fn inflection_form(&self, id: WordId) -> Option<&'static str> {
        self.id_strings(id).inflection_form
    }

    /// Appends the base form when it differs from the surface form; returns whether it did.
    pub fn push_base_form(&self, id: WordId, surface: &[u16], out: &mut String) -> bool {
        let extras = self.extras_of(id);
        if extras[0] & HAS_BASE_FORM == 0 {
            return false;
        }
        let prefix = extras[1] as usize;
        let suffix = extras[2] as usize;
        push_units(
            surface[..prefix]
                .iter()
                .copied()
                .chain((0..suffix).map(|i| u16_at(extras, 3 + i * 2))),
            out,
        );
        true
    }

    /// `None` when the base form equals the surface form.
    #[cfg(test)]
    pub fn base_form(&self, id: WordId, surface: &[u16]) -> Option<String> {
        let mut out = String::new();
        self.push_base_form(id, surface, &mut out).then_some(out)
    }

    /// Offset of the reading string within the extras (after the base form, if any).
    fn reading_at(extras: &[u8]) -> usize {
        if extras[0] & HAS_BASE_FORM != 0 {
            3 + extras[2] as usize * 2
        } else {
            1
        }
    }

    /// Appends the reading, or the surface form with hiragana shifted to katakana when there is
    /// none.
    pub fn push_reading(&self, id: WordId, surface: &[u16], out: &mut String) {
        let extras = self.extras_of(id);
        if extras[0] & HAS_READING == 0 {
            push_default_reading(surface, out);
        } else {
            push_string(
                extras,
                Self::reading_at(extras),
                extras[0] & READING_IS_KANA != 0,
                out,
            );
        }
    }

    /// The reading, or the surface form with hiragana shifted to katakana when there is none.
    #[cfg(test)]
    pub fn reading(&self, id: WordId, surface: &[u16]) -> String {
        let mut out = String::new();
        self.push_reading(id, surface, &mut out);
        out
    }

    /// Appends the pronunciation, or the reading when there is none.
    pub fn push_pronunciation(&self, id: WordId, surface: &[u16], out: &mut String) {
        let extras = self.extras_of(id);
        if extras[0] & HAS_PRONUNCIATION == 0 {
            return self.push_reading(id, surface, out);
        }
        let mut at = Self::reading_at(extras);
        if extras[0] & HAS_READING != 0 {
            at += string_len(extras, at, extras[0] & READING_IS_KANA != 0);
        }
        push_string(extras, at, extras[0] & PRONUNCIATION_IS_KANA != 0, out);
    }

    /// The pronunciation, or the reading when there is none.
    #[cfg(test)]
    pub fn pronunciation(&self, id: WordId, surface: &[u16]) -> String {
        let mut out = String::new();
        self.push_pronunciation(id, surface, &mut out);
        out
    }
}

/// An unknown-word entry: the lattice data plus the part of speech it is tagged with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UnknownWord {
    pub left_id: u16,
    pub right_id: u16,
    pub cost: i16,
    pub part_of_speech: &'static str,
}

/// The unknown-word dictionary: entries per character class.
pub(crate) struct UnknownDict {
    /// Entries per character class, in class order.
    entries: Vec<Vec<UnknownWord>>,
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
                    let id = u16_at(UNK_BIN, at);
                    let cost = i16::from_le_bytes([UNK_BIN[at + 2], UNK_BIN[at + 3]]);
                    let (pos, next) = str_at(UNK_BIN, at + 4);
                    at = next;
                    words.push(UnknownWord {
                        left_id: id,
                        right_id: id,
                        cost,
                        part_of_speech: pos,
                    });
                }
                entries.push(words);
            }
            UnknownDict { entries }
        })
    }

    /// The entries for unknown words made of characters of `class`, in Lucene's order.
    #[inline]
    pub fn words(&self, class: CharClass) -> &[UnknownWord] {
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
