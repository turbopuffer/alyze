//! Char filters: text rewrites applied before tokenizing, with the offset bookkeeping that maps
//! token offsets back to the original input (Lucene's `CharFilter.correctOffset`).

use super::Tokens;

/// The output of a char filter: the rewritten text and the map back to the original offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filtered {
    text: String,
    /// `(filtered byte offset, original byte offset)` at every point the two diverge, ascending.
    corrections: Vec<(usize, usize)>,
}

impl Filtered {
    /// The rewritten text, to tokenize.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The original byte offset of a byte offset into [`Filtered::text`] (which must be on a
    /// char boundary of it).
    pub fn correct_offset(&self, byte_offset: usize) -> usize {
        let _ = byte_offset;
        todo!()
    }

    /// Rewrites the byte ranges of tokens produced from [`Filtered::text`] to ranges of the
    /// original input.
    pub fn correct_tokens(&self, tokens: &mut Tokens) {
        let _ = tokens;
        todo!()
    }
}

/// Lucene's `JapaneseIterationMarkCharFilter` (Elasticsearch's `kuromoji_iteration_mark`):
/// expands the horizontal iteration marks 々 (kanji), ゝゞ (hiragana) and ヽヾ (katakana) to the
/// character they repeat, voicing or unvoicing kana as the mark says. Runs of marks repeat the
/// run before them; a mark with nothing to repeat (at the start, after another run, after 。
/// which flushes the filter's buffer, or after a supplementary character) is left alone. The
/// text length never changes.
pub fn iteration_mark(text: &str, normalize_kanji: bool, normalize_kana: bool) -> Filtered {
    let _ = (text, normalize_kanji, normalize_kana);
    todo!()
}

/// Lucene's `CJKWidthCharFilter` (what the `kuromoji` and `kuromoji_completion` analyzers apply
/// first): folds fullwidth ASCII variants to ASCII and halfwidth katakana to katakana, combining
/// a halfwidth voiced or semi-voiced sound mark into the preceding kana where a precomposed
/// character exists (ﾊﾟ → パ), and otherwise mapping it to the combining mark (U+3099/U+309A).
pub fn cjk_width(text: &str) -> Filtered {
    let _ = text;
    todo!()
}
