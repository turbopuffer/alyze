//! Token filters, applied in place to a [`Tokens`] buffer: Elasticsearch's `nori_part_of_speech`,
//! `nori_readingform` and `nori_number`, plus the lowercasing the `nori` analyzer ends with. They
//! compose in any order, like Elasticsearch filter chains.

use super::Tokens;
use super::pos::TagSet;

/// Removes tokens whose `left_pos` is in `stop_tags` (Lucene's `KoreanPartOfSpeechStopFilter`).
/// Like Lucene's filtering filters, the removed tokens' positions are kept as gaps: the next
/// surviving token's `position_increment` grows by the increments of the removed ones.
pub fn part_of_speech_stop(tokens: &mut Tokens, stop_tags: TagSet) {
    let _ = (tokens, stop_tags);
    todo!("nori part-of-speech stop filter")
}

/// Replaces the text of tokens that have a reading (Hanja entries) with that reading (Lucene's
/// `KoreanReadingFormFilter`).
pub fn reading_form(tokens: &mut Tokens) {
    let _ = tokens;
    todo!("nori reading-form filter")
}

/// Normalizes Korean numbers (Lucene's `KoreanNumberFilter`): consecutive tokens made of Arabic or
/// Hangul numerals, powers of ten (십, 백, 천, 만, 억, 조, 경, 해) and decimal points / thousands
/// separators are merged into one token holding the plain decimal value (십만이천오백 → 102500,
/// ３．２천 → 3200). The merged token spans the merged tokens' offsets.
pub fn number(tokens: &mut Tokens) {
    let _ = tokens;
    todo!("nori number filter")
}

/// Lowercases every token's text, with alyze's pinned Unicode lowercase mapping (Lucene's
/// `LowerCaseFilter` uses Java's simple mapping; see the module docs for the difference).
pub fn lowercase(tokens: &mut Tokens) {
    let _ = tokens;
    todo!("nori lowercase filter")
}

/// The mapping [`lowercase`] applies, on one string.
pub(crate) fn lowercase_text(text: &str) -> String {
    let _ = text;
    todo!("nori lowercase filter")
}
