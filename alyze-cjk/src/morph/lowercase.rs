//! The lowercasing the analyzers end with: alyze's pinned Unicode mapping (Lucene's
//! `LowerCaseFilter` uses Java's simple one-to-one mapping; the two differ only where the full
//! mapping expands, e.g. U+0130).

pub(crate) fn lowercase_into(text: &str, out: &mut String) {
    for c in text.chars() {
        out.extend(crate::unicode_lower::unicode_v17_char_to_lower(c));
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
