//! The system dictionaries: mecab-ko-dic as compiled into Lucene's binary files, converted once
//! into the blobs in `data/nori/` (see `examples/nori_convert_dict.rs`).
//!
//! - [`TokenInfoDict`]: every surface form with its words (a surface form can have several
//!   entries); per word the connection ids, cost, part of speech, reading and morphemes.
//! - [`UnknownDict`]: one entry per character class, used for words not in any dictionary.
//! - [`ConnectionCosts`]: the cost of putting a word with a given left id after a word with a
//!   given right id.
//!
//! All surface forms are handled as UTF-16 code units, like Lucene: the lattice is built per code
//! unit and the lookup structure is keyed by code unit.

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

/// A word id: an index into the token-info dictionary.
pub(crate) type WordId = u32;

pub(crate) struct TokenInfoDict {
    _private: (),
}

impl TokenInfoDict {
    pub fn get() -> &'static TokenInfoDict {
        todo!("nori token-info dictionary")
    }

    /// The words of the surface form `surface`, in dictionary order (the order Lucene adds them
    /// to the lattice, which decides ties), or `None` if it isn't a dictionary term.
    pub fn lookup(&self, surface: &[u16]) -> Option<&[WordId]> {
        let _ = surface;
        todo!("nori token-info dictionary")
    }

    /// Calls `f(length, words)` for every dictionary term that is a prefix of `text`, shortest
    /// first (what Lucene's FST walk from one position yields).
    pub fn for_each_prefix(&self, text: &[u16], f: impl FnMut(usize, &[WordId])) {
        let _ = (text, f);
        todo!("nori token-info dictionary")
    }

    pub fn word(&self, id: WordId) -> WordInfo {
        let _ = id;
        todo!("nori token-info dictionary")
    }

    /// The Hangul reading of a Hanja word, as UTF-16.
    pub fn reading(&self, id: WordId) -> Option<&[u16]> {
        let _ = id;
        todo!("nori token-info dictionary")
    }

    /// The morphemes of a compound, inflected or pre-analysed word with surface form `surface`
    /// (compound morphemes are slices of the surface form; inflected ones are stored).
    pub fn morphemes(&self, id: WordId, surface: &[u16]) -> Option<Vec<Morpheme>> {
        let _ = (id, surface);
        todo!("nori token-info dictionary")
    }

    /// Calls `f(surface, words)` for every term in lookup order (surface forms sorted by UTF-16
    /// code units), for tests.
    #[cfg(test)]
    pub fn for_each_term(&self, f: impl FnMut(&[u16], &[WordId])) {
        let _ = f;
        todo!("nori token-info dictionary")
    }
}

pub(crate) struct UnknownDict {
    _private: (),
}

impl UnknownDict {
    pub fn get() -> &'static UnknownDict {
        todo!("nori unknown dictionary")
    }

    /// The entries for unknown words made of characters of `class` (mecab-ko-dic has exactly one
    /// per class, but Lucene allows several).
    pub fn words(&self, class: CharClass) -> &[WordInfo] {
        let _ = class;
        todo!("nori unknown dictionary")
    }
}

pub(crate) struct ConnectionCosts {
    _private: (),
}

impl ConnectionCosts {
    pub fn get() -> &'static ConnectionCosts {
        todo!("nori connection costs")
    }

    /// Number of right ids (rows) and left ids (columns).
    pub fn dimensions(&self) -> (usize, usize) {
        todo!("nori connection costs")
    }

    /// The cost of a word with `left_id` following a word with `right_id` (0 is BOS/EOS).
    pub fn cost(&self, right_id: u16, left_id: u16) -> i16 {
        let _ = (right_id, left_id);
        todo!("nori connection costs")
    }
}
