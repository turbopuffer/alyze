//! Sentence boundaries as the JDK's `BreakIterator.getSentenceInstance(Locale.ROOT)` finds them,
//! which is what Lucene's `HMMChineseTokenizer` segments on.
//!
//! The JDK's rules predate UAX #29 and differ from it in ways that show up on real text (most
//! visibly: a line break is whitespace, not a sentence boundary), so `alyze`'s UAX #29 sentence
//! breaker can't be used as-is. The rules are in `~/Src/smartcn/jdk/BreakIteratorRules.java`;
//! the golden files in `testdata/smartcn/golden/*.sentences` are the ground truth.

/// Invokes `on_boundary` with the byte offset of every sentence boundary in `text`, in order,
/// starting with 0 and ending with `text.len()` (so an empty text yields just 0). Returning `false`
/// stops iteration.
pub(crate) fn for_each_boundary(text: &str, on_boundary: impl FnMut(usize) -> bool) {
    // Stub: the port has not been written yet.
    let _ = (text, on_boundary);
}
