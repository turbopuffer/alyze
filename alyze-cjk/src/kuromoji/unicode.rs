//! The Unicode character property kuromoji's punctuation test consults (`Character.getType` per
//! UTF-16 code unit in Lucene), from the shared table (see `morph::unicode`), and the test itself.

pub(crate) use crate::morph::unicode::{GeneralCategory, category};

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
