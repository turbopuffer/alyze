//! Sentence boundaries as the JDK's `BreakIterator.getSentenceInstance(Locale.ROOT)` finds them,
//! which is what Lucene's `HMMChineseTokenizer` segments on.
//!
//! The JDK's rules predate UAX #29 and differ from it in ways that show up on real text (most
//! visibly: a line break is whitespace, not a sentence boundary), so `alyze`'s UAX #29 sentence
//! breaker can't be used. The JDK compiles its rules (`sun.text.resources.BreakIteratorRules`)
//! into a character-category map and a state table; [`jdk_sentence_tables`](super::jdk_sentence_tables)
//! holds those tables and this module ports the loop that runs them,
//! `RuleBasedBreakIterator.handleNext`.

use super::jdk_sentence_tables::{
    CATEGORY_RANGES, END_STATES, IGNORE, LOOKAHEAD_STATES, STATE_TABLE,
};

const START_STATE: usize = 1;
const STOP_STATE: usize = 0;

/// Invokes `on_boundary` with the byte offset of every sentence boundary in `text`, in order,
/// starting with 0 and ending with `text.len()` (so an empty text yields just 0). Returning `false`
/// stops iteration.
pub(crate) fn for_each_boundary(text: &str, mut on_boundary: impl FnMut(usize) -> bool) {
    if !on_boundary(0) {
        return;
    }
    let mut position = 0;
    while position < text.len() {
        position = next_boundary(text, position);
        if !on_boundary(position) {
            return;
        }
    }
}

/// Java's `CharacterIterator` reports the end of the text as this character, so the JDK treats a
/// literal U+FFFF in the text as the end of it too.
const END_OF_TEXT: char = '\u{FFFF}';

/// Runs the state machine from `start` until it stops or the text ends, remembering the last
/// accepting position seen. Lookahead states (rules with a `/`) record a provisional position
/// that becomes the result once the lookahead part of the rule is matched.
fn next_boundary(text: &str, start: usize) -> usize {
    let mut result = after_char(text, start);
    let mut lookahead_result = 0;
    let mut state = START_STATE;
    let mut position = start;
    let at_end = |position: usize| position >= text.len() || char_at(text, position) == END_OF_TEXT;
    while !at_end(position) && state != STOP_STATE {
        let category = category_of(char_at(text, position));
        if category != IGNORE {
            state = usize::from(STATE_TABLE[state][category as usize]);
        }
        if LOOKAHEAD_STATES[state] {
            if END_STATES[state] {
                result = lookahead_result;
            } else {
                lookahead_result = after_char(text, position);
            }
        } else if END_STATES[state] {
            result = after_char(text, position);
        }
        position = after_char(text, position);
    }
    if at_end(position) && lookahead_result == text.len() {
        result = lookahead_result;
    }
    result
}

fn char_at(text: &str, position: usize) -> char {
    text[position..].chars().next().unwrap()
}

fn after_char(text: &str, position: usize) -> usize {
    position + char_at(text, position).len_utf8()
}

fn category_of(c: char) -> i8 {
    let cp = c as u32;
    let index = CATEGORY_RANGES.partition_point(|&(_, last, _)| last < cp);
    CATEGORY_RANGES[index].2
}
