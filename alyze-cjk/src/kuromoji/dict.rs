//! The system dictionaries, converted from Lucene's binary `TokenInfoDictionary`,
//! `UnknownDictionary` and `ConnectionCosts` (see `testdata/kuromoji/README.md` for how the
//! blobs under `data/kuromoji/` are produced).

use super::char_def::CharClass;

/// A word's id: its index in the dictionary's word records.
pub(crate) type WordId = u32;

/// The lattice-relevant part of a word record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WordInfo {
    /// Left and right connection ids are always equal in mecab-ipadic; both kept for symmetry
    /// with the Java API.
    pub left_id: u16,
    pub right_id: u16,
    pub cost: i16,
}

/// The token-info (known word) dictionary: a term index over surface forms, each mapping to one
/// or more word records.
pub(crate) struct TokenInfoDict {}

impl TokenInfoDict {
    pub fn get() -> &'static TokenInfoDict {
        todo!()
    }

    /// The word ids of `surface` (UTF-16), in dictionary order, if it is a term.
    pub fn lookup(&self, surface: &[u16]) -> Option<&[WordId]> {
        let _ = surface;
        todo!()
    }

    /// Calls `f(len, ids)` for every dictionary term that is a prefix of `text`, shortest first.
    pub fn for_each_prefix(&self, text: &[u16], f: impl FnMut(usize, &[WordId])) {
        let _ = (text, f);
        todo!()
    }

    /// Calls `f(surface, ids)` for every term, in term-index order.
    pub fn for_each_term(&self, f: impl FnMut(&[u16], &[WordId])) {
        let _ = f;
        todo!()
    }

    pub fn word(&self, id: WordId) -> WordInfo {
        let _ = id;
        todo!()
    }

    pub fn part_of_speech(&self, id: WordId) -> &'static str {
        let _ = id;
        todo!()
    }

    /// `None` when the base form equals the surface form.
    pub fn base_form(&self, id: WordId, surface: &[u16]) -> Option<String> {
        let _ = (id, surface);
        todo!()
    }

    /// The reading, or the surface form with hiragana shifted to katakana when there is none.
    pub fn reading(&self, id: WordId, surface: &[u16]) -> String {
        let _ = (id, surface);
        todo!()
    }

    /// The pronunciation, or the reading when there is none.
    pub fn pronunciation(&self, id: WordId, surface: &[u16]) -> String {
        let _ = (id, surface);
        todo!()
    }

    pub fn inflection_type(&self, id: WordId) -> Option<&'static str> {
        let _ = id;
        todo!()
    }

    pub fn inflection_form(&self, id: WordId) -> Option<&'static str> {
        let _ = id;
        todo!()
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
pub(crate) struct UnknownDict {}

impl UnknownDict {
    pub fn get() -> &'static UnknownDict {
        todo!()
    }

    pub fn words(&self, class: CharClass) -> &[UnknownWord] {
        let _ = class;
        todo!()
    }
}

/// The connection-cost matrix, indexed by (right id of the previous word, left id of the next).
pub(crate) struct ConnectionCosts {}

impl ConnectionCosts {
    pub fn get() -> &'static ConnectionCosts {
        todo!()
    }

    /// (right id count, left id count).
    pub fn dimensions(&self) -> (usize, usize) {
        todo!()
    }

    pub fn cost(&self, right_id: u16, left_id: u16) -> i16 {
        let _ = (right_id, left_id);
        todo!()
    }
}
