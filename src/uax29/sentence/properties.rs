use icu_properties::{CodePointMapData, CodePointMapDataBorrowed, props::SentenceBreak};

use crate::uax29::break_property_enum;

// Property values for the Sentence_Break property, as defined in
// [UAX #29](https://www.unicode.org/reports/tr29/#Table_Sentence_Break_Property_Values).
//
// Each character has an associated Sentence_Break property value.
break_property_enum! {
    SentenceBreakProperty {
        Other, ATerm, Close, Format, Lower, Numeric, OLetter, Sep, Sp, STerm,
        Upper, CR, Extend, LF, SContinue,
    }
}

/// ASCII characters are very common, so we pre-compute a lookup table for the first 128 code points.
/// A test below verifies that this table is correct against the full dictionary-based lookup.
pub(crate) const ASCII_SENTENCE_BREAK_PROP: [SentenceBreakProperty; 128] = [
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Sp,
    SentenceBreakProperty::LF,
    SentenceBreakProperty::Sp,
    SentenceBreakProperty::Sp,
    SentenceBreakProperty::CR,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Sp,
    SentenceBreakProperty::STerm,
    SentenceBreakProperty::Close,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Close,
    SentenceBreakProperty::Close,
    SentenceBreakProperty::Close,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::SContinue,
    SentenceBreakProperty::SContinue,
    SentenceBreakProperty::ATerm,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Numeric,
    SentenceBreakProperty::Numeric,
    SentenceBreakProperty::Numeric,
    SentenceBreakProperty::Numeric,
    SentenceBreakProperty::Numeric,
    SentenceBreakProperty::Numeric,
    SentenceBreakProperty::Numeric,
    SentenceBreakProperty::Numeric,
    SentenceBreakProperty::Numeric,
    SentenceBreakProperty::Numeric,
    SentenceBreakProperty::SContinue,
    SentenceBreakProperty::SContinue,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::STerm,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Upper,
    SentenceBreakProperty::Close,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Close,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Lower,
    SentenceBreakProperty::Close,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Close,
    SentenceBreakProperty::Other,
    SentenceBreakProperty::Other,
];

/// A dictionary mapping Unicode code points to their corresponding Sentence_Break property values.
/// This is used to efficiently look up the Sentence_Break property for any given character.
const SENTENCE_BREAK_PROP: CodePointMapDataBorrowed<'static, SentenceBreak> =
    CodePointMapData::<SentenceBreak>::new();

/// Uses the icu_properties data to look up the Sentence_Break property for a given character.
pub(crate) fn lookup_sentence_break_property(c: char) -> SentenceBreakProperty {
    match SENTENCE_BREAK_PROP.get(c) {
        SentenceBreak::Other => SentenceBreakProperty::Other,
        SentenceBreak::ATerm => SentenceBreakProperty::ATerm,
        SentenceBreak::Close => SentenceBreakProperty::Close,
        SentenceBreak::Format => SentenceBreakProperty::Format,
        SentenceBreak::Lower => SentenceBreakProperty::Lower,
        SentenceBreak::Numeric => SentenceBreakProperty::Numeric,
        SentenceBreak::OLetter => SentenceBreakProperty::OLetter,
        SentenceBreak::Sep => SentenceBreakProperty::Sep,
        SentenceBreak::Sp => SentenceBreakProperty::Sp,
        SentenceBreak::STerm => SentenceBreakProperty::STerm,
        SentenceBreak::Upper => SentenceBreakProperty::Upper,
        SentenceBreak::CR => SentenceBreakProperty::CR,
        SentenceBreak::Extend => SentenceBreakProperty::Extend,
        SentenceBreak::LF => SentenceBreakProperty::LF,
        SentenceBreak::SContinue => SentenceBreakProperty::SContinue,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::{ASCII_SENTENCE_BREAK_PROP, SentenceBreakProperty, lookup_sentence_break_property};

    #[test]
    fn test_ascii_table_correct() {
        let mut expected = [SentenceBreakProperty::Other; 128];
        for c in 0..=0x7F {
            expected[c as usize] = lookup_sentence_break_property(c as u8 as char);
        }

        // If it doesn't match, print the correct table in an easy-to-copy-paste way.
        if ASCII_SENTENCE_BREAK_PROP != expected {
            for i in 0..8 {
                let row = &expected[i * 16..(i + 1) * 16];
                let row_str = row
                    .iter()
                    .map(|p| format!("SentenceBreakProperty::{:?}", p))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("    {},", row_str);
            }
            assert!(false);
        }
    }
}
