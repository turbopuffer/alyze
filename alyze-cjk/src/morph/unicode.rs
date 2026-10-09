//! The Unicode character properties the analyzers consult per UTF-16 code unit. Lucene asks the
//! JDK (`Character.getType`, `Character.UnicodeScript.of`, `Character.isDigit`); the ports ask
//! the pinned ICU property data alyze already ships, so the answers don't change under the
//! tokenizers when the toolchain's Unicode tables do. The two must agree for every code unit,
//! which each analyzer's `tests::unicode` checks against a JDK dump.

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

/// The general category of one code unit (`Character.getType`).
#[inline]
pub(crate) fn category(code_unit: u16) -> GeneralCategory {
    props(code_unit).category
}
