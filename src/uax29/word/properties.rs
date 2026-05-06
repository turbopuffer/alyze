use tpuf_icu_properties_211::{
    CodePointMapData, CodePointMapDataBorrowed, CodePointSetData, CodePointSetDataBorrowed,
    props::{ExtendedPictographic, GeneralCategory, Ideographic, Script, WordBreak},
};

use crate::uax29::break_property_enum;

// Property values for the Word_Break property, as defined in
// [UAX #29](https://www.unicode.org/reports/tr29/#Word_Break_Property).
//
// Each character has an associated Word_Break property value.
break_property_enum! {
    WordBreakProperty {
        Other, ALetter, Format, Katakana, MidLetter, MidNum, Numeric,
        ExtendNumLet, CR, Extend, LF, MidNumLet, Newline,
        RegionalIndicator, HebrewLetter, SingleQuote, DoubleQuote, ZWJ, WSegSpace,
    }
}

/// ASCII characters are very common, so we pre-compute a lookup table for the first 128 code points.
/// A test below verifies that this table is correct against the full dictionary-based lookup.
pub(crate) const ASCII_WORD_BREAK_PROP: [WordBreakProperty; 128] = [
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::LF,
    WordBreakProperty::Newline,
    WordBreakProperty::Newline,
    WordBreakProperty::CR,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::WSegSpace,
    WordBreakProperty::Other,
    WordBreakProperty::DoubleQuote,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::SingleQuote,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::MidNum,
    WordBreakProperty::Other,
    WordBreakProperty::MidNumLet,
    WordBreakProperty::Other,
    WordBreakProperty::Numeric,
    WordBreakProperty::Numeric,
    WordBreakProperty::Numeric,
    WordBreakProperty::Numeric,
    WordBreakProperty::Numeric,
    WordBreakProperty::Numeric,
    WordBreakProperty::Numeric,
    WordBreakProperty::Numeric,
    WordBreakProperty::Numeric,
    WordBreakProperty::Numeric,
    WordBreakProperty::MidLetter,
    WordBreakProperty::MidNum,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::ExtendNumLet,
    WordBreakProperty::Other,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::ALetter,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
    WordBreakProperty::Other,
];

impl WordBreakProperty {
    /// Checks if the property is ExtPictographic.
    /// Notably, a character can be both ExtPictographic and have another Word_Break property value.
    pub fn is_ext_pictographic(c: char) -> bool {
        EXT_PICT.contains(c)
    }
}

/// A dictionary mapping Unicode code points to their corresponding Word_Break property values.
/// This is used to efficiently look up the Word_Break property for any given character.
const WORD_BREAK_PROP: CodePointMapDataBorrowed<'static, WordBreak> =
    CodePointMapData::<WordBreak>::new();
const EXT_PICT: CodePointSetDataBorrowed<'static> = CodePointSetData::new::<ExtendedPictographic>();

/// Lazily-built union of every codepoint whose "strict" word-like check returns true:
/// `ExtendedPictographic ∪ Ideographic ∪ {c | Script(c) ∉ {Common,Inherited,Unknown}} ∪
/// {c | GeneralCategory(c) == OtherNumber}`.
///
/// Stored as a sorted slice of inclusive `(start, end)` ranges so `is_word_like_strict` is a
/// single binary search per char instead of up to four ICU trie lookups. Built on first use.
static WORD_LIKE_STRICT_RANGES: std::sync::OnceLock<Box<[(u32, u32)]>> = std::sync::OnceLock::new();

// See `TokenProperties::is_word_like()` for a detailed explanation of this.
fn word_like_strict_ranges() -> &'static [(u32, u32)] {
    WORD_LIKE_STRICT_RANGES.get_or_init(|| {
        let ideographic = CodePointSetData::new::<Ideographic>();
        let script = CodePointMapData::<Script>::new();
        let gc = CodePointMapData::<GeneralCategory>::new();

        let mut ranges: Vec<(u32, u32)> = Vec::new();
        for r in EXT_PICT.iter_ranges() {
            ranges.push((*r.start(), *r.end()));
        }
        for r in ideographic.iter_ranges() {
            ranges.push((*r.start(), *r.end()));
        }

        // r.value is true for any range where the script is not Common/Inherited/Unknown.
        // E.g. r.value=true when Script is a reasonable writing system (Greek, Thai, Cyrillic, etc.)
        // This operates similar to Rust's .iter().map().filter().
        for r in script.iter_ranges_mapped(|s| {
            !matches!(s, Script::Common | Script::Inherited | Script::Unknown)
        }) {
            if r.value {
                ranges.push((*r.range.start(), *r.range.end()));
            }
        }

        for r in gc.iter_ranges_for_value(GeneralCategory::OtherNumber) {
            ranges.push((*r.start(), *r.end()));
        }

        // Sort + merge adjacent/overlapping ranges so binary search has clean buckets.
        ranges.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
        for (s, e) in ranges {
            if let Some(last) = merged.last_mut() {
                if s <= last.1.saturating_add(1) {
                    last.1 = last.1.max(e);
                    continue;
                }
            }
            merged.push((s, e));
        }
        merged.into_boxed_slice()
    })
}

/// "Strict" word-like check for chars whose `WordBreakProperty` alone didn't classify them as
/// word-like. Mirrors the residual conditions of turbopuffer's `is_word_like_token_char` /
/// `should_include_token_span` for non-ASCII chars not already covered by `ALetter` /
/// `HebrewLetter` / `Numeric`.
#[inline]
pub(crate) fn is_word_like_strict(c: char) -> bool {
    let cp = c as u32;
    word_like_strict_ranges()
        .binary_search_by(|&(s, e)| {
            if cp < s {
                std::cmp::Ordering::Greater
            } else if cp > e {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Helper function to look up the Word_Break property for a given character using the pre-computed dictionary.
#[inline(never)]
pub(crate) fn lookup_word_break_property_from_dictionary(c: char) -> WordBreakProperty {
    match WORD_BREAK_PROP.get(c) {
        WordBreak::ALetter => WordBreakProperty::ALetter,
        WordBreak::Format => WordBreakProperty::Format,
        WordBreak::Katakana => WordBreakProperty::Katakana,
        WordBreak::MidLetter => WordBreakProperty::MidLetter,
        WordBreak::MidNum => WordBreakProperty::MidNum,
        WordBreak::Numeric => WordBreakProperty::Numeric,
        WordBreak::ExtendNumLet => WordBreakProperty::ExtendNumLet,
        WordBreak::CR => WordBreakProperty::CR,
        WordBreak::Extend => WordBreakProperty::Extend,
        WordBreak::LF => WordBreakProperty::LF,
        WordBreak::MidNumLet => WordBreakProperty::MidNumLet,
        WordBreak::Newline => WordBreakProperty::Newline,
        WordBreak::RegionalIndicator => WordBreakProperty::RegionalIndicator,
        WordBreak::HebrewLetter => WordBreakProperty::HebrewLetter,
        WordBreak::SingleQuote => WordBreakProperty::SingleQuote,
        WordBreak::DoubleQuote => WordBreakProperty::DoubleQuote,
        WordBreak::ZWJ => WordBreakProperty::ZWJ,
        WordBreak::WSegSpace => WordBreakProperty::WSegSpace,
        WordBreak::Other => WordBreakProperty::Other,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ASCII_WORD_BREAK_PROP, WordBreakProperty, lookup_word_break_property_from_dictionary,
    };

    #[test]
    fn test_ascii_table_correct() {
        let mut expected = [WordBreakProperty::Other; 128];
        for c in 0..=0x7F {
            expected[c as usize] = lookup_word_break_property_from_dictionary(c as u8 as char);
        }

        // If it doesn't match, print the correct table in an easy-to-copy-paste way.
        if ASCII_WORD_BREAK_PROP != expected {
            for i in 0..8 {
                let row = &expected[i * 16..(i + 1) * 16];
                let row_str = row
                    .iter()
                    .map(|p| format!("WordBreakProperty::{:?}", p))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("    {},", row_str);
            }
            assert!(false);
        }
    }
}
