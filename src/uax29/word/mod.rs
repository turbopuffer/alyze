pub(crate) mod properties;
pub(crate) mod transitions;

use crate::uax29::Action;
use properties::{
    ASCII_WORD_BREAK_PROP, WordBreakProperty, is_word_like_strict,
    lookup_word_break_property_from_dictionary,
};
use transitions::{State, TABLE, Transition};

/// For backwards compatibility, require caller to pass in options struct.
#[derive(Default, Clone)]
#[non_exhaustive]
pub struct Options {}

/// For a given span, extracts info from the DFA state to provide useful information upstream, e.g.
/// whether the span was "word-like", ascii, etc
#[derive(Copy, Clone, Default, Debug, Eq, PartialEq)]
pub struct TokenProperties(u8);

impl TokenProperties {
    const WORD_LIKE_MASK: u8 = 0b0000_0001;
    const NON_ASCII_MASK: u8 = 0b0000_0010;

    pub(crate) const NON_ASCII: Self = Self(Self::NON_ASCII_MASK);
    pub(crate) const WORD_LIKE: Self = Self(Self::WORD_LIKE_MASK);

    // A token is "word-like" if it contains any char that is:
    // - ALetter, HebrewLetter, or Numeric (this is a fast-path from our DFA WordBreakProperty lookup)
    // - Ideographic or Extended_Pictographic (e.g. CJK chars, emoji)
    // - Other_Number general category (⑦, ², ¼)
    // - A character whose Script is something meaningful (e.g. belonging to a real writing system),
    //   as opposed to Script=Common/Inherited/Unknown (e.g. punctuation, symbols, emoji modifiers).
    pub fn is_word_like(&self) -> bool {
        self.0 & Self::WORD_LIKE_MASK != 0
    }

    // Stored disjunctively: a single non-ASCII char in the span sets this bit.
    // `is_ascii()` returns true when the bit is unset (vacuously true for the empty span).
    pub fn is_ascii(&self) -> bool {
        self.0 & Self::NON_ASCII_MASK == 0
    }
}

impl std::ops::BitOrAssign for TokenProperties {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// A tokenizer that implements UAX #29 word boundary rules, using a deterministic finite automaton
/// (DFA) to efficiently determine word boundaries in Unicode text. Includes a number of fast-paths
/// for common cases, e.g. ASCII.
pub fn tokenize(
    text: &str,
    _options: Options,
    mut on_breakpoint: impl FnMut(usize, TokenProperties) -> bool,
) {
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

    // Maintain properties of the current token, which are reset on each break and can be used by the caller
    // to more efficiently determine what type of token was just emitted, e.g. whether it's "word-like" or ascii.
    let mut token_props = TokenProperties::default();

    // Properties of chars consumed while in a deferred state. Held aside from `token_props`
    // because we don't yet know which token they belong to: if the deferred state resolves
    // via `DeferredBreak`, these chars start the *next* token (so their contribution must
    // not leak into the in-progress one); if it resolves via `NoBreak` exiting deferred,
    // they fold into the current token. Tracked by `deferred_break_pos.is_some()`.
    let mut deferred_props = TokenProperties::default();

    while pos < text.len() {
        // Fast path for ASCII, e.g. skip DFA all together when possible.
        // Roughly a ~2x speedup on English Wikipedia.
        if matches!(
            state,
            State::ALetter | State::Numeric | State::ExtendNumLet | State::HLetter
        ) {
            let scan_start = pos;
            let mut fast_acc: u8 = 0;
            while pos < text.len() && bytes[pos] < 0x80 {
                let info = ASCII_BYTE_INFO[bytes[pos] as usize];
                if info & ASCII_WORD_CONTINUE == 0 {
                    break;
                }
                fast_acc |= info;
                pos += 1;
            }
            if pos > scan_start {
                token_props.0 |= fast_acc & !ASCII_WORD_CONTINUE;
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
        // `char_props` is this char's contribution to the enclosing token's properties; it's
        // applied to `token_props` per-arm below, since `Action::Break` treats the breaking char
        // as the first char of the *next* token (the contribution lands there, not in the token
        // being emitted).
        let b = bytes[pos];
        let (c, prop, char_len, char_props) = if b < 0x80 {
            (
                b as char,
                ASCII_WORD_BREAK_PROP[b as usize],
                1usize,
                TokenProperties(ASCII_BYTE_INFO[b as usize] & !ASCII_WORD_CONTINUE),
            )
        } else {
            let c = text[pos..].chars().next().unwrap();
            let prop = lookup_word_break_property_from_dictionary(c);
            // Cheap path covers ALetter / HebrewLetter / Numeric. For everything else, fall back
            // to the strict per-char check (ExtPict / Ideographic / Script / OtherNumber).
            let mut char_props = TokenProperties::NON_ASCII;
            char_props |= WORD_BREAK_CONTRIB[prop as usize];
            if !char_props.is_word_like() && is_word_like_strict(c) {
                char_props |= TokenProperties::WORD_LIKE;
            }
            (c, prop, c.len_utf8(), char_props)
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
                        // Transparent: char joins the in-progress token instead of breaking.
                        token_props |= char_props;
                        continue;
                    }
                }
                last_was_zwj = prop == WordBreakProperty::ZWJ;
                state = next_state;
                if !on_breakpoint(boundary, std::mem::take(&mut token_props)) {
                    return;
                }
                // Breaking char starts the next token; apply its contribution after the take.
                token_props |= char_props;
                continue;
            }
            Action::NoBreak => {
                last_was_zwj = false;
                if next_state.is_deferred() {
                    if deferred_break_pos.is_none() {
                        deferred_break_pos = Some(pos);
                    }
                    deferred_props |= char_props;
                } else {
                    if deferred_break_pos.take().is_some() {
                        // Word resumed: deferred chars belong to the in-progress token.
                        token_props |= std::mem::take(&mut deferred_props);
                    }
                    token_props |= char_props;
                }
                state = next_state;
                pos += char_len;
            }
            Action::DeferredBreak => {
                last_was_zwj = false;
                let boundary = deferred_break_pos.take().unwrap();
                state = next_state;
                // Notably, we don't advance `pos` here; the current char is re-examined on the
                // next iteration and will accumulate its props then — don't apply char_props here.
                if !on_breakpoint(boundary, std::mem::take(&mut token_props)) {
                    return;
                }
                // Deferred chars start the next token.
                token_props |= std::mem::take(&mut deferred_props);
                continue;
            }
            Action::Transparent => {
                last_was_zwj = prop == WordBreakProperty::ZWJ;
                // State doesn't change, but we still consume the character.
                pos += char_len;
                if deferred_break_pos.is_some() {
                    deferred_props |= char_props;
                } else {
                    token_props |= char_props;
                }
            }
        }
    }

    // Deferred state at EOT - defer failed
    if state.is_deferred() {
        let breakpoint = deferred_break_pos.take().unwrap();
        if !on_breakpoint(breakpoint, std::mem::take(&mut token_props)) {
            return;
        }
        // Deferred chars become the trailing token.
        token_props |= std::mem::take(&mut deferred_props);
    }

    // WB2: Any ÷ eot — emit final segment
    _ = on_breakpoint(text.len(), token_props);
}

/// Cheap-path `TokenProperties` contribution for each `WordBreakProperty` value. Covers the
/// signals that fall out of WordBreak alone — letters and digits. Katakana is intentionally
/// **not** included: its set mixes Katakana letters (word-like) with the prolonged-sound mark
/// `ー` (Script=Common, not word-like). Those split is resolved via `is_word_like_strict`.
const WORD_BREAK_CONTRIB: [TokenProperties; WordBreakProperty::NUM_VARIANTS] = {
    let mut t = [TokenProperties(0); WordBreakProperty::NUM_VARIANTS];
    t[WordBreakProperty::ALetter as usize] = TokenProperties::WORD_LIKE;
    t[WordBreakProperty::HebrewLetter as usize] = TokenProperties::WORD_LIKE;
    t[WordBreakProperty::Numeric as usize] = TokenProperties::WORD_LIKE;
    t
};

/// Per-ASCII-byte info for the fast-path scan and the single-char branch.
/// - Bit 7 (`ASCII_WORD_CONTINUE`): byte is part of a word-like run (`[a-zA-Z0-9_]`).
/// - Low bits: the byte's `TokenProperties` contribution (currently just `WORD_LIKE_MASK` for
///   `[a-zA-Z0-9]`, since underscore continues the run but isn't itself word-like).
const ASCII_WORD_CONTINUE: u8 = 0b1000_0000;
const ASCII_BYTE_INFO: [u8; 128] = {
    let mut t = [0u8; 128];
    let mut i = 0u8;
    loop {
        t[i as usize] = match i {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' => {
                ASCII_WORD_CONTINUE | TokenProperties::WORD_LIKE_MASK
            }
            b'_' => ASCII_WORD_CONTINUE,
            _ => 0,
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
                tokenize(s, Options::default(), |bp, _props| {
                    breakpoints.push(bp);
                    true
                });
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
            tokenize(s, Options::default(), |bp, _props| {
                breakpoints.push(bp);
                true
            });
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
        assert_breaks("e.g. hello", vec![0, 3, 4, 5, 10]);
        assert_breaks("example.com", vec![0, 11]);
        assert_breaks("won't", vec![0, 5]);

        // WB13a/WB13b: ExtendNumLet connects letters, numbers, katakana
        assert_breaks("a_1", vec![0, 3]);
        assert_breaks("_a", vec![0, 2]);

        // Edge cases with deferred breaks.
        assert_breaks("can'", vec![0, 3, 4]);
        assert_breaks("can' hi", vec![0, 3, 4, 5, 7]);

        // WB7a and WB6/WB7 with Hebrew_Letter and Single_Quote.
        assert_breaks("א'", vec![0, "א'".len()]);
        assert_breaks("א'א", vec![0, "א'א".len()]);
        assert_breaks("א'\u{2060}א", vec![0, "א'\u{2060}א".len()]);
        assert_breaks("א'a", vec![0, "א'a".len()]);
        assert_breaks("הצ'קרות", vec![0, "הצ'קרות".len()]);
        assert_breaks(
            "לייף אנרג'י",
            vec![0, "לייף".len(), "לייף ".len(), "לייף אנרג'י".len()],
        );

        // WB7b/WB7c: Hebrew_Letter × Double_Quote × Hebrew_Letter (gershayim acronyms
        // like צה״ל). With letters on both sides the gershayim is absorbed into the
        // word; with whitespace on either side it must emit as its own standalone
        // token (UAX #29 prescribes a break — no MidLetter/DoubleQuote rule applies).
        assert_breaks("צה\u{05F4}ל", vec![0, "צה\u{05F4}ל".len()]);
        // Closing gershayim followed by space: standalone token.
        assert_breaks(
            "אקספרס\u{05F4} מהיום",
            vec![
                0,
                "אקספרס".len(),
                "אקספרס\u{05F4}".len(),
                "אקספרס\u{05F4} ".len(),
                "אקספרס\u{05F4} מהיום".len(),
            ],
        );
        // Full quoted-word pattern: both opening and closing gershayim are standalone.
        assert_breaks(
            "\u{05F4}אקספרס\u{05F4} מהיום",
            vec![
                0,
                "\u{05F4}".len(),
                "\u{05F4}אקספרס".len(),
                "\u{05F4}אקספרס\u{05F4}".len(),
                "\u{05F4}אקספרס\u{05F4} ".len(),
                "\u{05F4}אקספרס\u{05F4} מהיום".len(),
            ],
        );

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

    #[test]
    fn tokenizer_properties_sanity() {
        // Each emit reports properties of the span just closed; the leading boundary at 0 has
        // no preceding span, so it carries default props.
        fn assert_props(s: &str, expected: Vec<(usize, bool)>) {
            let mut got: Vec<(usize, bool)> = Vec::new();
            tokenize(s, Options::default(), |bp, props| {
                got.push((bp, props.is_ascii()));
                true
            });
            assert_eq!(got, expected, "input: {:?}", s);
        }

        // Leading boundary at 0 is vacuously is_ascii=true.
        assert_props("hello", vec![(0, true), (5, true)]);
        assert_props("🛑", vec![(0, true), (4, false)]);

        // The sharp case: the breaking char is non-ASCII but starts the *next* token, so "ab"
        // must still report is_ascii=true and "🛑" must report is_ascii=false.
        assert_props("ab🛑", vec![(0, true), (2, true), (6, false)]);
    }

    fn assert_word_like(s: &str, expected: Vec<(usize, bool)>) {
        let mut got: Vec<(usize, bool)> = Vec::new();
        tokenize(s, Options::default(), |bp, props| {
            got.push((bp, props.is_word_like()));
            true
        });
        assert_eq!(got, expected, "input: {:?}", s);
    }

    /// ASCII subset of the word-like contract: any token containing an ASCII letter or digit is
    /// word-like; pure-connector / whitespace / punctuation tokens are not. The leading boundary
    /// at 0 has no preceding span, so word_like is vacuously false.
    #[test]
    fn tokenizer_word_like_ascii_sanity() {
        // ASCII letters / digits / mixed / contractions.
        assert_word_like("hello", vec![(0, false), (5, true)]);
        assert_word_like("123", vec![(0, false), (3, true)]);
        assert_word_like("abc123", vec![(0, false), (6, true)]);
        assert_word_like("won't", vec![(0, false), (5, true)]);

        // Connectors only (ExtendNumLet) — `_` is not a letter or digit.
        assert_word_like("___", vec![(0, false), (3, false)]);
        // Whitespace only.
        assert_word_like("   ", vec![(0, false), (3, false)]);
        // ASCII punctuation: each '!' breaks separately, none word-like.
        assert_word_like("!!!", vec![(0, false), (1, false), (2, false), (3, false)]);
    }

    /// Strict cases that need Script / Ideographic / OtherNumber / ExtPict lookups beyond the
    /// WordBreak property.
    #[test]
    fn tokenizer_word_like_strict_sanity() {
        // Hebrew (HebrewLetter prop)
        assert_word_like("ש", vec![(0, false), (2, true)]);

        // CJK ideograph: WordBreak=Other, Script=Han.
        assert_word_like("中", vec![(0, false), (3, true)]);
        // Ideographic iteration mark: WordBreak=Other, Script=Common, Ideographic=true.
        assert_word_like("々", vec![(0, false), (3, true)]);
        // Circled digit: WordBreak=Other, GeneralCategory=OtherNumber.
        assert_word_like("①", vec![(0, false), (3, true)]);
        // Devanagari letter: WordBreak=Other, Script=Devanagari.
        assert_word_like("अ", vec![(0, false), (3, true)]);
        // Thai letter: WordBreak=Other, Script=Thai.
        assert_word_like("ก", vec![(0, false), (3, true)]);
        // Emoji: WordBreak=Other (or ExtPict), Script=Common, ExtendedPictographic=true.
        assert_word_like("👍", vec![(0, false), (4, true)]);

        // Real Katakana letter: WordBreak=Katakana, Script=Katakana → word-like.
        assert_word_like("リ", vec![(0, false), (3, true)]);
        // Katakana-Hiragana extender: WordBreak=Katakana, Script=Common → NOT word-like.
        // Locks in why we can't just OR `WordBreakProperty::Katakana → WORD_LIKE`; we need to
        // additionally check the char's Script.
        assert_word_like("ー", vec![(0, false), (3, false)]);

        // HEBREW PUNCTUATION GERSHAYIM (U+05F4): WordBreak=DoubleQuote (not word-like via
        // cheap path), Script=Hebrew (word-like via strict). Standalone token is word-like.
        assert_word_like("\u{05F4}", vec![(0, false), ("\u{05F4}".len(), true)]);
    }

    /// A deferred break must not strand the deferred char's properties on the preceding
    /// token. For `אקספרס״ `, the closing gershayim emerges as a standalone token via
    /// `DeferredBreak` from `HLetterDQ`; its `WORD_LIKE` bit (via Script=Hebrew) belongs
    /// to that standalone token, not to the Hebrew word that precedes it.
    #[test]
    fn deferred_break_does_not_misattribute_props() {
        let s = "אקספרס\u{05F4} ";
        assert_word_like(
            s,
            vec![
                (0, false),
                ("אקספרס".len(), true),         // אקספרס (Hebrew letters)
                ("אקספרס\u{05F4}".len(), true), // ״ standalone — Script=Hebrew
                (s.len(), false),               // trailing space
            ],
        );

        // Same shape with Hebrew word after the space — the four-quote pattern from
        // real Common Crawl docs (`״אקספרס״ מהיום …`). All four standalone gershayim
        // tokens must be word-like; the asymmetry-bug case is the trailing one.
        let s = "\u{05F4}אקספרס\u{05F4} מהיום";
        assert_word_like(
            s,
            vec![
                (0, false),
                ("\u{05F4}".len(), true),                          // leading ״
                ("\u{05F4}אקספרס".len(), true),                    // אקספרס
                ("\u{05F4}אקספרס\u{05F4}".len(), true),            // trailing ״
                ("\u{05F4}אקספרס\u{05F4} ".len(), false),          // space
                (s.len(), true),                                   // מהיום
            ],
        );
    }
}
