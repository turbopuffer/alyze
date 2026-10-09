//! mecab-ipadic's character classes (`char.def`), as compiled into Lucene's
//! `CharacterDefinition.dat`: a class per UTF-16 code unit, and per class whether unknown-word
//! processing is invoked even when dictionary words match (`invoke`) and whether consecutive
//! characters of the class are grouped into one unknown word (`group`). Loaded from
//! `data/kuromoji/chardef.bin` (see `examples/kuromoji_convert_dict.rs`).

use crate::morph::char_table::CharTable;

/// A character class, in `char.def` / Lucene ordinal order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub(crate) enum CharClass {
    Ngram,
    Default,
    Space,
    Symbol,
    Numeric,
    Alpha,
    Cyrillic,
    Greek,
    Hiragana,
    Katakana,
    Kanji,
    KanjiNumeric,
}

impl CharClass {
    pub const ALL: &'static [CharClass] = &[
        CharClass::Ngram,
        CharClass::Default,
        CharClass::Space,
        CharClass::Symbol,
        CharClass::Numeric,
        CharClass::Alpha,
        CharClass::Cyrillic,
        CharClass::Greek,
        CharClass::Hiragana,
        CharClass::Katakana,
        CharClass::Kanji,
        CharClass::KanjiNumeric,
    ];

    #[cfg(test)]
    pub fn name(self) -> &'static str {
        match self {
            CharClass::Ngram => "NGRAM",
            CharClass::Default => "DEFAULT",
            CharClass::Space => "SPACE",
            CharClass::Symbol => "SYMBOL",
            CharClass::Numeric => "NUMERIC",
            CharClass::Alpha => "ALPHA",
            CharClass::Cyrillic => "CYRILLIC",
            CharClass::Greek => "GREEK",
            CharClass::Hiragana => "HIRAGANA",
            CharClass::Katakana => "KATAKANA",
            CharClass::Kanji => "KANJI",
            CharClass::KanjiNumeric => "KANJINUMERIC",
        }
    }

    #[cfg(test)]
    pub fn from_name(name: &str) -> Option<CharClass> {
        CharClass::ALL.iter().copied().find(|c| c.name() == name)
    }
}

static TABLE: CharTable = CharTable::new(include_bytes!("../../data/kuromoji/chardef.bin"));

/// The class of a UTF-16 code unit. Supplementary characters are classified by their surrogate
/// halves, which `char.def` doesn't list, so they are `Default`.
#[inline]
pub(crate) fn class(code_unit: u16) -> CharClass {
    debug_assert_eq!(TABLE.class_count(), CharClass::ALL.len());
    CharClass::ALL[TABLE.class_index(code_unit) as usize]
}

/// Whether unknown-word processing runs at a character of this class even when dictionary words
/// match there.
#[inline]
pub(crate) fn invoke(class: CharClass) -> bool {
    TABLE.invoke(class as u8)
}

/// Whether consecutive characters of this class are grouped into one unknown word.
#[inline]
pub(crate) fn group(class: CharClass) -> bool {
    TABLE.group(class as u8)
}

/// Lucene's `isKanji`: `Kanji` or `KanjiNumeric` (what the search-mode penalty counts).
#[inline]
pub(crate) fn is_kanji(class: CharClass) -> bool {
    matches!(class, CharClass::Kanji | CharClass::KanjiNumeric)
}
