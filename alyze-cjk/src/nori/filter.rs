//! Token filters, applied in place to a [`Tokens`] buffer: Elasticsearch's `nori_part_of_speech`,
//! `nori_readingform` and `nori_number`, plus the lowercasing the `nori` analyzer ends with. They
//! compose in any order, like Elasticsearch filter chains.

use crate::morph::lowercase;
use crate::morph::number::{self, Numerals};

use super::pos::TagSet;
use super::{TokenData, Tokens};

/// Removes tokens whose `left_pos` is in `stop_tags` (Lucene's `KoreanPartOfSpeechStopFilter`).
/// Like Lucene's filtering filters, the removed tokens' positions are kept as gaps: the next
/// surviving token's `position_increment` grows by the increments of the removed ones.
pub fn part_of_speech_stop(tokens: &mut Tokens, stop_tags: TagSet) {
    let mut skipped_increment = 0u32;
    tokens.items.retain_mut(|token| {
        if stop_tags.contains(token.left_pos) {
            skipped_increment += token.position_increment;
            false
        } else {
            token.position_increment += skipped_increment;
            skipped_increment = 0;
            true
        }
    });
}

/// Replaces the text of tokens that have a reading (Hanja entries) with that reading (Lucene's
/// `KoreanReadingFormFilter`).
pub fn reading_form(tokens: &mut Tokens) {
    for token in &mut tokens.items {
        if let Some(reading) = &token.reading {
            token.text = reading.clone();
        }
    }
}

/// Lowercases every token's text, with alyze's pinned Unicode lowercase mapping (Lucene's
/// `LowerCaseFilter` uses Java's simple mapping; see the module docs for the difference).
pub fn lowercase(tokens: &mut Tokens) {
    let mut lowered = String::new();
    for i in 0..tokens.items.len() {
        let text = &tokens.text[tokens.items[i].text.clone()];
        if lowercase::is_lowercase_ascii(text) {
            continue;
        }
        lowered.clear();
        lowercase::lowercase_into(text, &mut lowered);
        if lowered != text {
            let range = tokens.push_text(&lowered);
            tokens.items[i].text = range;
        }
    }
}

/// The mapping [`lowercase`] applies, on one string.
#[cfg(test)]
pub(crate) fn lowercase_text(text: &str) -> String {
    lowercase::lowercase_text(text)
}

// ------------------------------------------------------------------------------------------------
// Number filter

/// Normalizes Korean numbers (Lucene's `KoreanNumberFilter`): consecutive tokens made of Arabic or
/// Hangul numerals, powers of ten (십, 백, 천, 만, 억, 조, 경, 해) and decimal points / thousands
/// separators are merged into one token holding the plain decimal value (십만이천오백 → 102500,
/// ３．２천 → 3200). The merged token spans the merged tokens' offsets.
///
/// This is a literal port of Lucene's stateful filter, quirks included: the merged token takes
/// every attribute but text and offsets from the token that ended the run, a stacked token
/// (position increment 0) inside a run makes the filter give up on that run and emit its first
/// token unchanged, and the numeral buffer is only cleared after a successful merge. One quirk is
/// not reproduced: when a run ends the stream, Lucene's merged token takes its attributes from the
/// last token the tokenizer produced, even one a preceding stop filter discarded (a trailing
/// space, say); here it takes the last numeral's.
pub fn number(tokens: &mut Tokens) {
    let input = std::mem::take(&mut tokens.items);
    let mut filter = NumberFilter {
        input: &input,
        next: 0,
        state: None,
        numeral: String::new(),
        fall_through_tokens: 0,
        exhausted: false,
    };
    let mut output = Vec::with_capacity(input.len());
    while let Some(token) = filter.increment_token(tokens) {
        output.push(token);
    }
    tokens.items = output;
}

struct NumberFilter<'a> {
    input: &'a [TokenData],
    next: usize,
    /// A token read past the end of a run, to emit on the next call.
    state: Option<TokenData>,
    numeral: String,
    fall_through_tokens: u32,
    exhausted: bool,
}

impl NumberFilter<'_> {
    /// Lucene's `input.incrementToken()`: the next token's attributes, or `None` at the end.
    fn next_input(&mut self) -> Option<TokenData> {
        let token = self.input.get(self.next)?.clone();
        self.next += 1;
        Some(token)
    }

    fn increment_token(&mut self, tokens: &mut Tokens) -> Option<TokenData> {
        // Emit a previously captured token we read past earlier.
        if let Some(state) = self.state.take() {
            return Some(state);
        }
        if self.exhausted {
            return None;
        }
        let Some(mut current) = self.next_input() else {
            self.exhausted = true;
            return None;
        };
        if self.fall_through_tokens > 0 {
            self.fall_through_tokens -= 1;
            return Some(current);
        }
        if current.position_increment == 0 {
            self.fall_through_tokens = current.position_length.saturating_sub(1);
            return Some(current);
        }

        let mut more_tokens = true;
        let mut composed = false;
        let mut start_offset = 0;
        let mut end_offset = 0;
        let pre_composition = current.clone();
        let mut term = tokens.text[current.text.clone()].to_owned();
        let mut numeral_term = is_numeral(&term);
        while more_tokens && numeral_term {
            if !composed {
                start_offset = current.byte_range.start;
                composed = true;
            }
            end_offset = current.byte_range.end;
            match self.next_input() {
                Some(next) => current = next,
                None => {
                    more_tokens = false;
                    self.exhausted = true;
                }
            }
            if current.position_increment == 0 {
                // A stacked token: capture it (and let the tokens under it through), and emit the
                // run's first token as it was.
                self.fall_through_tokens = current.position_length.saturating_sub(1);
                self.state = Some(current);
                return more_tokens.then_some(pre_composition);
            }
            self.numeral.push_str(&term);
            if more_tokens {
                term = tokens.text[current.text.clone()].to_owned();
                numeral_term = is_numeral(&term) || is_numeral_punctuation(&term);
            }
        }
        if composed {
            if more_tokens {
                // Read past the numerals: emit this token on the next call.
                self.state = Some(current.clone());
            }
            let normalized = normalize_number(&self.numeral);
            let mut merged = current;
            merged.text = tokens.push_text(&normalized);
            merged.byte_range = start_offset..end_offset;
            self.numeral.clear();
            return Some(merged);
        }
        more_tokens.then_some(current)
    }
}

/// Hangul numerals, for the shared number parser.
struct Hangul;

impl Numerals for Hangul {
    fn digit(c: char) -> Option<u8> {
        Some(match c {
            '영' => 0,
            '일' => 1,
            '이' => 2,
            '삼' => 3,
            '사' => 4,
            '오' => 5,
            '육' => 6,
            '칠' => 7,
            '팔' => 8,
            '구' => 9,
            _ => return None,
        })
    }

    /// The power of ten a Hangul numeral denotes (0 for anything else).
    fn exponent(c: char) -> u32 {
        match c {
            '십' => 1,
            '백' => 2,
            '천' => 3,
            '만' => 4,
            '억' => 8,
            '조' => 12,
            '경' => 16,
            '해' => 20,
            _ => 0,
        }
    }
}

fn is_numeral(s: &str) -> bool {
    number::is_numeral::<Hangul>(s)
}

fn is_numeral_punctuation(s: &str) -> bool {
    number::is_numeral_punctuation(s)
}

/// Lucene's `normalizeNumber`: the plain decimal value of a Korean number, or the input unchanged
/// when it doesn't parse.
pub(crate) fn normalize_number(number: &str) -> String {
    number::normalize_number::<Hangul>(number)
}
