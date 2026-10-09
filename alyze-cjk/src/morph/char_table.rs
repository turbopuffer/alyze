//! A MeCab `char.def` as compiled into Lucene's `CharacterDefinition.dat` and converted into a
//! `chardef.bin` blob: `u8` class count C, 65536 class bytes (one per UTF-16 code unit), C flag
//! bytes (bit 0: invoke unknown-word processing even when dictionary words match; bit 1: group
//! consecutive characters of the class into one unknown word). Each analyzer wraps it with its
//! own class enum.

pub(crate) struct CharTable {
    bin: &'static [u8],
}

impl CharTable {
    pub const fn new(bin: &'static [u8]) -> CharTable {
        CharTable { bin }
    }

    pub fn class_count(&self) -> usize {
        self.bin[0] as usize
    }

    /// The class byte of a code unit.
    #[inline]
    pub fn class_index(&self, code_unit: u16) -> u8 {
        self.bin[1 + code_unit as usize]
    }

    #[inline]
    fn flags(&self, class: u8) -> u8 {
        self.bin[1 + 0x10000 + class as usize]
    }

    #[inline]
    pub fn invoke(&self, class: u8) -> bool {
        self.flags(class) & 1 != 0
    }

    #[inline]
    pub fn group(&self, class: u8) -> bool {
        self.flags(class) & 2 != 0
    }
}
