pub(crate) mod properties;
pub(crate) mod transitions;

use crate::uax29::Action;
use properties::{ASCII_SENTENCE_BREAK_PROP, lookup_sentence_break_property};
use transitions::{State, TRANSITION_TABLE, Transition};

#[derive(Default)]
#[non_exhaustive]
pub struct Options {}

pub fn tokenize(text: &str, breakpoints: &mut Vec<usize>, _options: Options) {
    if text.is_empty() {
        return;
    }
    let bytes = text.as_bytes();
    let mut state = State::StartOfText;
    let mut deferred_break_pos = None;
    let mut pos = 0;
    while pos < text.len() {
        let b = bytes[pos];
        let (prop, char_len) = if b < 0x80 {
            (ASCII_SENTENCE_BREAK_PROP[b as usize], 1usize)
        } else {
            let c = text[pos..].chars().next().unwrap();
            (lookup_sentence_break_property(c), c.len_utf8())
        };
        let Transition(next_state, action) = TRANSITION_TABLE[state as usize][prop as usize];
        match action {
            Action::Break => {
                state = next_state;
                breakpoints.push(pos);
                pos += char_len;
                continue;
            }
            Action::NoBreak => {
                if next_state.is_deferred() {
                    if deferred_break_pos.is_none() {
                        deferred_break_pos = Some(pos);
                    }
                } else {
                    deferred_break_pos = None;
                }
                state = next_state;
                pos += char_len;
            }
            Action::Transparent => {
                // State doesn't change, but we still consume the character.
                pos += char_len;
            }
            Action::DeferredBreak => {
                let boundary = deferred_break_pos.take().unwrap();
                state = next_state;
                // Don't advance pos — re-examine current char in new state.
                breakpoints.push(boundary);
                continue;
            }
        }
    }

    // Deferred state at EOT — defer failed, confirm break
    if state.is_deferred() {
        breakpoints.push(deferred_break_pos.take().unwrap());
    }

    // SB2: Any	÷ eot (break at end of text)
    breakpoints.push(text.len());
}

#[cfg(test)]
mod tests {
    use super::{Options, tokenize};
    use crate::uax29::test_helpers::test_against_uax29_break_tests;

    #[test]
    fn test_sentence_break_against_uax29_tests() {
        let (passed, failed) =
            test_against_uax29_break_tests("testdata/SentenceBreakTest.txt", |s, breakpoints| {
                tokenize(s, breakpoints, Options::default())
            });
        assert_eq!(
            (512, 0),
            (passed, failed),
            "{} / {} tests passed",
            passed,
            passed + failed
        );
    }

    #[test]
    fn tokenizer_sanity() {
        fn assert_breaks(s: &str, expected: Vec<usize>) {
            let mut breakpoints = Vec::new();
            tokenize(s, &mut breakpoints, Options::default());
            assert_eq!(breakpoints, expected, "input: {:?}", s);
        }

        // Empty string yields no breakpoints.
        assert_breaks("", vec![]);

        // Non-empty strings break at the start & end.
        assert_breaks("a", vec![0, 1]);
        assert_breaks(".", vec![0, 1]);

        // SB998: don't break within a sentence.
        assert_breaks("Hello world", vec![0, 11]);

        // SB3: CR × LF (don't break between CR and LF)
        assert_breaks("\r\n", vec![0, 2]);

        // SB4: Break after paragraph separators (Sep, CR, LF).
        assert_breaks("a\nb", vec![0, 2, 3]);
        assert_breaks("a\r\nb", vec![0, 3, 4]);
        assert_breaks("a\rb", vec![0, 2, 3]);

        // SB5: Extend and Format are transparent.
        assert_breaks("a\u{0308}b", vec![0, 4]); // a + combining diaeresis + b

        // SB6: ATerm × Numeric — don't break between "." and a digit.
        assert_breaks("3.4", vec![0, 3]);

        // SB7: (Upper | Lower) ATerm × Upper — abbreviations like U.S.A.
        assert_breaks("U.S.A.", vec![0, 6]);
        assert_breaks("U.S.", vec![0, 4]);
        assert_breaks("c.D", vec![0, 3]);

        // SB8: ATerm Close* Sp* × (¬(OLetter|Upper|Lower|ParaSep|SATerm))* Lower
        // Don't break after "." when eventually followed by a lowercase letter.
        assert_breaks("c.d", vec![0, 3]);
        assert_breaks("etc. the", vec![0, 8]);
        assert_breaks("the resp. leaders are", vec![0, 21]);

        // SB8: with Close and Sp between ATerm and Lower.
        assert_breaks("etc.)'\u{a0}the", vec![0, 11]);

        // SB8a: SATerm Close* Sp* × (SContinue | SATerm)
        // Don't break before continuation punctuation after sentence terminators.
        assert_breaks(".,", vec![0, 2]); // ATerm × SContinue
        assert_breaks("..", vec![0, 2]); // ATerm × ATerm
        assert_breaks("!,", vec![0, 2]); // STerm × SContinue
        assert_breaks("!.", vec![0, 2]); // STerm × ATerm

        // SB9/SB10/SB11: Break after sentence terminators,
        // but include trailing Close, Sp, and ParaSep in the sentence.
        assert_breaks("Hello. World", vec![0, 7, 12]);
        assert_breaks("Hello!) World", vec![0, 8, 13]);
        assert_breaks("Hello.  World", vec![0, 8, 13]);
        assert_breaks("Hello.\nWorld", vec![0, 7, 12]);

        // SB11: STerm breaks even when followed by lowercase.
        assert_breaks("Hello! world", vec![0, 7, 12]);

        // SB8 vs SB11: ATerm followed by OLetter or Upper DOES break (SB8 fails).
        assert_breaks("Hello. World", vec![0, 7, 12]);

        // Figures 3 & 4 from the spec:
        // Figure 3: Forbidden breaks on "." (should NOT break)
        assert_breaks("c.d", vec![0, 3]);
        assert_breaks("3.4", vec![0, 3]);
        assert_breaks("U.S.", vec![0, 4]);
        assert_breaks("the resp. leaders are", vec![0, 21]);
        assert_breaks("etc.)\u{2019}\u{a0}\u{2018}(the", vec![0, 17]);

        // Figure 4: Allowed breaks on "." (SHOULD break)
        assert_breaks(
            "She said \"See spot run.\" John shook his head.",
            vec![0, 25, 45],
        );
    }
}
