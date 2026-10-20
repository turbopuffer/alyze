//! Helpers shared by the smartcn and nori test suites: the escaped case-file format the Java
//! golden generators use, UTF-16 offset conversion, and the FNV-1a checksum the dictionary goldens
//! use.

use std::fmt::Write as _;

/// Inverse of the escaping the generators use: `\\`, `\n`, `\r`, `\t` and `\u{hex}`.
pub(crate) fn unescape(line: &str) -> String {
    let (text, lone_surrogate) = unescape_allowing_lone_surrogate(line);
    assert!(
        lone_surrogate.is_none(),
        "unexpected lone surrogate in {line:?}"
    );
    text
}

/// [`unescape`], but a line consisting of exactly one escaped lone surrogate (`\u{d800}` to
/// `\u{dfff}`, which Java's tokenizers emit when a UTF-16 cut lands inside a surrogate pair) is
/// returned as `("", Some(unit))` instead of panicking.
pub(crate) fn unescape_allowing_lone_surrogate(line: &str) -> (String, Option<u16>) {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                assert_eq!(chars.next(), Some('{'), "bad escape in {line:?}");
                let hex: String = chars.by_ref().take_while(|&c| c != '}').collect();
                let cp = u32::from_str_radix(&hex, 16)
                    .unwrap_or_else(|_| panic!("bad escape in {line:?}"));
                match char::from_u32(cp) {
                    Some(c) => out.push(c),
                    None if (0xD800..=0xDFFF).contains(&cp)
                        && out.is_empty()
                        && chars.as_str().is_empty() =>
                    {
                        return (out, Some(cp as u16));
                    }
                    None => panic!("bad code point in {line:?}"),
                }
            }
            other => panic!("bad escape {other:?} in {line:?}"),
        }
    }
    (out, None)
}

/// Unescapes a line that may contain lone surrogates anywhere, as UTF-16 code units.
pub(crate) fn unescape_utf16(line: &str) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.extend(c.encode_utf16(&mut [0; 2]).iter());
            continue;
        }
        match chars.next() {
            Some('\\') => out.push(u16::from(b'\\')),
            Some('n') => out.push(u16::from(b'\n')),
            Some('r') => out.push(u16::from(b'\r')),
            Some('t') => out.push(u16::from(b'\t')),
            Some('u') => {
                assert_eq!(chars.next(), Some('{'), "bad escape in {line:?}");
                let hex: String = chars.by_ref().take_while(|&c| c != '}').collect();
                let cp = u32::from_str_radix(&hex, 16)
                    .unwrap_or_else(|_| panic!("bad escape in {line:?}"));
                match char::from_u32(cp) {
                    Some(c) => out.extend(c.encode_utf16(&mut [0; 2]).iter()),
                    None => out.push(u16::try_from(cp).expect("lone surrogate")),
                }
            }
            other => panic!("bad escape {other:?} in {line:?}"),
        }
    }
    out
}

/// Same escaping as the generators, for messages.
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20
                || ((c as u32) >= 0x7F && (c as u32) < 0xA0)
                || c == '\u{2028}'
                || c == '\u{2029}' =>
            {
                write!(out, "\\u{{{:x}}}", c as u32).unwrap();
            }
            c => out.push(c),
        }
    }
    out
}

/// Reads a case file: one (escaped) input per line.
pub(crate) fn read_cases(contents: &str) -> Vec<String> {
    contents.lines().map(unescape).collect()
}

/// Byte offset of a UTF-16 offset into `text`, for expectations transcribed from Lucene's tests.
pub(crate) fn utf16_to_byte_offset(text: &str, utf16_offset: usize) -> usize {
    let mut units = 0;
    for (byte_offset, c) in text.char_indices() {
        if units == utf16_offset {
            return byte_offset;
        }
        units += c.len_utf16();
    }
    assert_eq!(
        units, utf16_offset,
        "UTF-16 offset {utf16_offset} out of range for {text:?}"
    );
    text.len()
}

pub(crate) const FNV_OFFSET: u64 = 0xcbf29ce484222325;
pub(crate) const FNV_PRIME: u64 = 0x100000001b3;

/// FNV-1a 64, matching the Java generators' checksums.
pub(crate) fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}
