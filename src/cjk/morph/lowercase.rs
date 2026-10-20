//! Lucene-compatible simple lowercasing using alyze's pinned Unicode mapping.

pub(crate) fn lowercase_into(text: &str, out: &mut String) {
    for c in text.chars() {
        if c == '\u{0130}' {
            out.push('i');
        } else {
            out.extend(crate::analyze::u17_to_lower::unicode_v17_char_to_lower(c));
        }
    }
}

/// Whether `text` can't change under [`lowercase_into`] without consulting any table (ASCII
/// without uppercase letters). Only the pinned table decides otherwise: no `char::is_lowercase`,
/// whose Unicode version is the toolchain's.
pub(crate) fn is_lowercase_ascii(text: &str) -> bool {
    text.is_ascii() && !text.bytes().any(|b| b.is_ascii_uppercase())
}

pub(crate) fn lowercase_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    lowercase_into(text, &mut out);
    out
}
