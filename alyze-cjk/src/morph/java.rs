//! Java library behaviours the ports have to match exactly.

/// `BufferedReader.readLine`: lines end at `\n`, `\r` or `\r\n`; no line after the last
/// terminator unless there is text.
pub(crate) fn lines(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let end = rest.find(['\n', '\r']).unwrap_or(rest.len());
        let line = &rest[..end];
        let skip = if rest[end..].starts_with("\r\n") {
            2
        } else if end < rest.len() {
            1
        } else {
            0
        };
        rest = &rest[end + skip..];
        Some(line)
    })
}

/// Java's `\s` (without `UNICODE_CHARACTER_CLASS`): ASCII whitespace only.
pub(crate) fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0B' | '\x0C' | '\r')
}

/// `String.trim`: strips characters at or below U+0020 from both ends.
pub(crate) fn trim(s: &str) -> &str {
    s.trim_matches(|c| c <= ' ')
}
