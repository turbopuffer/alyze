//! The Unicode character properties nori's unknown-word grouping and punctuation test consult,
//! from the shared table (see `morph::unicode`), plus nori's script and punctuation predicates.

pub(crate) use crate::morph::unicode::{GeneralCategory, Script, props};

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
