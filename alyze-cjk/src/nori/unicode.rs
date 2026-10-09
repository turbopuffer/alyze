//! The Unicode character properties nori's unknown-word grouping and punctuation test consult.
//! Lucene asks the JDK (`Character.getType`, `Character.UnicodeScript.of`, `Character.isDigit`),
//! per UTF-16 code unit; the port asks the pinned ICU property data alyze already ships, so the
//! answers don't change under the tokenizer when the toolchain's Unicode tables do. The two must
//! agree for every code unit, which `tests::unicode` checks against a JDK dump.

pub(crate) use tpuf_icu_properties_211::props::{GeneralCategory, Script};

/// Properties of one UTF-16 code unit. A surrogate code unit is `Surrogate` / `Unknown` / not a
/// digit, like the JDK reports for a lone surrogate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CodeUnitProps {
    pub category: GeneralCategory,
    pub script: Script,
    /// `Character.isDigit`: general category Nd.
    pub is_digit: bool,
}

pub(crate) fn props(code_unit: u16) -> CodeUnitProps {
    let _ = code_unit;
    todo!("nori unicode properties")
}

/// Lucene's `isPunctuation`: separators, controls, format characters, all punctuation and symbol
/// categories, plus U+318D (Hangul letter araea, used as an interpunct).
pub(crate) fn is_punctuation(code_unit: u16, category: GeneralCategory) -> bool {
    let _ = (code_unit, category);
    todo!("nori unicode properties")
}
