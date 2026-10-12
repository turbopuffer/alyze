//! mecab-ko-dic's character classes (`char.def`), as compiled into Lucene's
//! `CharacterDefinition.dat`: a class per UTF-16 code unit, and per class whether unknown-word
//! processing is invoked even when dictionary words match (`invoke`) and whether consecutive
//! characters of the class are grouped into one unknown word (`group`). Loaded from
//! `data/nori/chardef.bin` (see `examples/nori_convert_dict.rs`).

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
    Hangul,
    Hanja,
    HanjaNumeric,
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
        CharClass::Hangul,
        CharClass::Hanja,
        CharClass::HanjaNumeric,
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
            CharClass::Hangul => "HANGUL",
            CharClass::Hanja => "HANJA",
            CharClass::HanjaNumeric => "HANJANUMERIC",
        }
    }

    #[cfg(test)]
    pub fn from_name(name: &str) -> Option<CharClass> {
        CharClass::ALL.iter().copied().find(|c| c.name() == name)
    }
}

static CHARDEF_BIN: &[u8] = include_bytes!("../../data/nori/chardef.bin");
const CLASS_COUNT: usize = CharClass::ALL.len();

/// The 65536 class bytes.
#[inline]
fn classes() -> &'static [u8] {
    &CHARDEF_BIN[1..1 + 0x10000]
}

fn flags(class: CharClass) -> u8 {
    debug_assert_eq!(CHARDEF_BIN[0] as usize, CLASS_COUNT);
    CHARDEF_BIN[1 + 0x10000 + class as usize]
}

/// The class of a UTF-16 code unit. Supplementary characters are classified by their surrogate
/// halves, which `char.def` doesn't list, so they are `Default`.
#[inline]
pub(crate) fn class(code_unit: u16) -> CharClass {
    CharClass::ALL[classes()[code_unit as usize] as usize]
}

/// Whether unknown-word processing runs at a character of this class even when dictionary words
/// match there.
#[inline]
pub(crate) fn invoke(class: CharClass) -> bool {
    flags(class) & 1 != 0
}

/// Whether consecutive characters of this class are grouped into one unknown word.
#[inline]
pub(crate) fn group(class: CharClass) -> bool {
    flags(class) & 2 != 0
}

/// Whether a Hangul syllable (U+AC00..U+D7A3) has a final consonant (Lucene's `hasCoda`). Lucene
/// also calls this for other HANGUL-class characters (compatibility jamo like ㅋ), where the Java
/// arithmetic goes negative; `i32` reproduces that (the remainder is then never 0).
pub(crate) fn has_coda(code_unit: u16) -> bool {
    (i32::from(code_unit) - 0xAC00) % 0x1C != 0
}
