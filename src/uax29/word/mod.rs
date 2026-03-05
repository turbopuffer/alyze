pub(crate) mod properties;
pub(crate) mod transitions;

use crate::uax29::Action;
use properties::{ASCII_WORD_BREAK_PROP, WordBreakProperty, lookup_word_break_property_from_dictionary};
use transitions::{State, TABLE, Transition};

/// For backwards compatibility, require caller to pass in options struct.
#[derive(Default, Clone)]
#[non_exhaustive]
pub struct Options {}

/// A tokenizer that implements UAX #29 word boundary rules, using a deterministic finite automaton
/// (DFA) to efficiently determine word boundaries in Unicode text. Includes a number of fast-paths
/// for common cases, e.g. ASCII.
pub fn tokenize(text: &str, breakpoints: &mut Vec<usize>, _options: Options) {
    if text.is_empty() {
        return;
    }
    let bytes = text.as_bytes();

    let mut state = State::StartOfText;
    let mut deferred_break_pos = None;
    let mut pos = 0;

    // WB4 says: X (Extend | Format | ZWJ)*	→	X
    // To avoid adding _many_ `_AfterZWJ` variant states, we'll cheat a little by keeping track
    // of this condition with a bool. More specifically, we need to conditionally break based on
    // whether the previous character was a ZWJ.
    //
    // Example:
    // 'a 🛑' -> break (ALetter -> Other)
    // 'a ZWJ 🛑' -> no break (WB4)
    let mut last_was_zwj = false;

    while pos < text.len() {
        // Fast path for ASCII, e.g. skip DFA all together when possible.
        // Roughly a ~2x speedup on English Wikipedia.
        if matches!(
            state,
            State::ALetter | State::Numeric | State::ExtendNumLet | State::HLetter
        ) {
            let scan_start = pos;
            while pos < text.len() && bytes[pos] < 0x80 && WORD_CONTINUE[bytes[pos] as usize] {
                pos += 1;
            }
            if pos > scan_start {
                let last = bytes[pos - 1]; // Safe because we're not in State::StartOfText.
                state = match last {
                    b'0'..=b'9' => State::Numeric,
                    b'_' => State::ExtendNumLet,
                    _ => State::ALetter,
                };
                last_was_zwj = false;
                continue;
            }
        }

        // Fast path for ASCII, e.g. avoid chars().next(), and lookup word property from table.
        let b = bytes[pos];
        let (c, prop, char_len) = if b < 0x80 {
            (b as char, ASCII_WORD_BREAK_PROP[b as usize], 1usize)
        } else {
            let c = text[pos..].chars().next().unwrap();
            (
                c,
                lookup_word_break_property_from_dictionary(c),
                c.len_utf8(),
            )
        };

        // Each iteration, we consult the transition table to determine the next state
        // and whether to emit a breakpoint.
        let Transition(next_state, action) = TABLE[state as usize][prop as usize];
        match action {
            Action::Break => {
                let boundary = pos;
                pos += char_len;
                if last_was_zwj {
                    last_was_zwj = false;
                    if WordBreakProperty::is_ext_pictographic(c) {
                        continue; // transparent
                    }
                }
                last_was_zwj = prop == WordBreakProperty::ZWJ;
                state = next_state;
                breakpoints.push(boundary);
                continue;
            }
            Action::NoBreak => {
                last_was_zwj = false;
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
            Action::DeferredBreak => {
                last_was_zwj = false;
                let boundary = deferred_break_pos.take().unwrap();
                state = next_state;
                // Notably, we don't advance `pos` here, current char re-examined
                // after the next call to `next()`.
                breakpoints.push(boundary);
                continue;
            }
            Action::Transparent => {
                last_was_zwj = prop == WordBreakProperty::ZWJ;
                // State doesn't change, but we still consume the character.
                pos += char_len;
            }
        }
    }

    // Deferred state at EOT - defer failed
    if state.is_deferred() {
        breakpoints.push(deferred_break_pos.take().unwrap());
    }

    // WB2: Any ÷ eot — emit final segment
    breakpoints.push(text.len());
}

// Lookup table for ASCII characters, which can be processed without the DFA.
// Specifically, if we see a given ASCII character, can we just immediately skip to the next one?
const WORD_CONTINUE: [bool; 128] = {
    let mut t = [false; 128];
    let mut i = 0u8;
    loop {
        t[i as usize] = match i {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' => true,
            _ => false,
        };
        if i == 127 {
            break;
        }
        i += 1;
    }
    t
};

#[cfg(test)]
mod tests {
    use super::{Options, tokenize};
    use crate::uax29::test_helpers::test_against_uax29_break_tests;

    #[test]
    fn test_word_break_against_uax29_tests() {
        let (passed, failed) =
            test_against_uax29_break_tests("testdata/WordBreakTest.txt", |s, breakpoints| {
                tokenize(s, breakpoints, Options::default())
            });
        assert_eq!(
            (1944, 0),
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
        assert_breaks("\n", vec![0, 1]);

        // WB5: ALetter × ALetter
        assert_breaks("hello", vec![0, 5]);

        // WB8: Numeric × Numeric
        assert_breaks("123", vec![0, 3]);

        // WB9/WB10: ALetter × Numeric, Numeric × ALetter
        assert_breaks("abc123", vec![0, 6]);
        assert_breaks("123abc", vec![0, 6]);
        assert_breaks("a1b2", vec![0, 4]);

        // WB3: CR × LF (stay together)
        assert_breaks("\r\n", vec![0, 2]);
        assert_breaks("\r\n\r\n", vec![0, 2, 4]);

        // CR and LF alone break normally
        assert_breaks("\r", vec![0, 1]);
        assert_breaks("\n\n", vec![0, 1, 2]);

        // Mixed with newlines
        assert_breaks("a\r\nb", vec![0, 1, 3, 4]);
        assert_breaks("ab\r\ncd", vec![0, 2, 4, 6]);

        // Keep horizontal whitespace together (WB3d)
        assert_breaks("a   c", vec![0, 1, 4, 5]);

        // Do not break letters across certain punctuation, such as within "e.g." or "example.com".
        // TODO make sure this is the right breakpoints for "e.g."
        assert_breaks("e.g. hello", vec![0, 3, 4, 5, 10]);
        assert_breaks("example.com", vec![0, 11]);
        assert_breaks("won't", vec![0, 5]);

        // WB13a/WB13b: ExtendNumLet connects letters, numbers, katakana
        assert_breaks("a_1", vec![0, 3]);
        assert_breaks("_a", vec![0, 2]);

        // Edge cases with deferred breaks.
        assert_breaks("can'", vec![0, 3, 4]);
        assert_breaks("can' hi", vec![0, 3, 4, 5, 7]);

        // WB3c: ZWJ × Extended_Pictographic (emoji ZWJ sequences)
        assert_breaks("👨\u{200D}👩", vec![0, 11]);
        assert_breaks("👨👩", vec![0, 4, 8]);

        // Weird edge case: Letters that are also extended pictographic
        assert_breaks("🇦", vec![0, 4]);
        assert_breaks("🇦🇦", vec![0, 8]);
        assert_breaks("🇦🇦🇦", vec![0, 8, 12]);

        // Circled letters
        assert_breaks("\u{200d}Ⓜ", vec![0, 6]);
    }
}
