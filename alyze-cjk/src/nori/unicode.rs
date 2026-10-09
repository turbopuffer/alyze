//! The Unicode character properties nori's unknown-word grouping and punctuation test consult.
//! Lucene asks the JDK (`Character.getType`, `Character.UnicodeScript.of`, `Character.isDigit`),
//! per UTF-16 code unit; the port asks the pinned ICU property data alyze already ships, so the
//! answers don't change under the tokenizer when the toolchain's Unicode tables do. The two must
//! agree for every code unit, which `tests::unicode` checks against a JDK dump.

use std::sync::OnceLock;

use tpuf_icu_properties_211::CodePointMapData;
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

/// One table lookup per code unit; the table (256 KiB) is built from the ICU data on first use.
#[inline]
pub(crate) fn props(code_unit: u16) -> CodeUnitProps {
    static TABLE: OnceLock<Box<[CodeUnitProps]>> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let categories = CodePointMapData::<GeneralCategory>::new();
        let scripts = CodePointMapData::<Script>::new();
        (0..=0xFFFFu32)
            .map(|cp| {
                let category = categories.get32(cp);
                CodeUnitProps {
                    category,
                    script: scripts.get32(cp),
                    is_digit: category == GeneralCategory::DecimalNumber,
                }
            })
            .collect()
    });
    table[code_unit as usize]
}

/// Lucene's `isPunctuation`: separators, controls, format characters, all punctuation and symbol
/// categories, plus U+318D (Hangul letter araea, used as an interpunct).
#[inline]
pub(crate) fn is_punctuation(code_unit: u16, category: GeneralCategory) -> bool {
    code_unit == 0x318D
        || matches!(
            category,
            GeneralCategory::SpaceSeparator
                | GeneralCategory::LineSeparator
                | GeneralCategory::ParagraphSeparator
                | GeneralCategory::Control
                | GeneralCategory::Format
                | GeneralCategory::DashPunctuation
                | GeneralCategory::OpenPunctuation
                | GeneralCategory::ClosePunctuation
                | GeneralCategory::ConnectorPunctuation
                | GeneralCategory::OtherPunctuation
                | GeneralCategory::MathSymbol
                | GeneralCategory::CurrencySymbol
                | GeneralCategory::ModifierSymbol
                | GeneralCategory::OtherSymbol
                | GeneralCategory::InitialPunctuation
                | GeneralCategory::FinalPunctuation
        )
}

/// Lucene's `isCommonOrInherited`.
#[inline]
pub(crate) fn is_common_or_inherited(script: Script) -> bool {
    script == Script::Common || script == Script::Inherited
}

/// Lucene's `isSameScript`: equal, or either side Common/Inherited.
#[inline]
pub(crate) fn is_same_script(a: Script, b: Script) -> bool {
    a == b || is_common_or_inherited(a) || is_common_or_inherited(b)
}
