//! The Unicode character property kuromoji's punctuation test consults. Lucene asks the JDK
//! (`Character.getType`, per UTF-16 code unit); the port asks the pinned ICU property data alyze
//! already ships, so the answers don't change under the tokenizer when the toolchain's Unicode
//! tables do. The two must agree for every code unit, which `tests::unicode` checks against a
//! JDK dump.

pub(crate) use tpuf_icu_properties_211::props::GeneralCategory;

/// The general category of one UTF-16 code unit (a surrogate code unit is `Surrogate`, like the
/// JDK reports for a lone surrogate).
#[inline]
pub(crate) fn category(code_unit: u16) -> GeneralCategory {
    let _ = code_unit;
    todo!()
}

/// Lucene's `isPunctuation`: separators, controls, format characters, and all punctuation and
/// symbol categories.
#[inline]
pub(crate) fn is_punctuation(category: GeneralCategory) -> bool {
    matches!(
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
