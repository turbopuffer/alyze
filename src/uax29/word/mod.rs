pub(crate) mod properties;
pub(crate) mod transitions;

use crate::uax29::Action;
use properties::{
    ASCII_WORD_BREAK_PROP, WordBreakProperty, is_word_like_strict,
    lookup_word_break_property_from_dictionary,
};
use std::arch::aarch64::*;
use transitions::{State, TABLE, Transition};

/// For backwards compatibility, require caller to pass in options struct.
#[derive(Default, Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Options {}

/// For a given span, extracts info from the DFA state to provide useful information upstream, e.g.
/// whether the span was "word-like", ascii, etc
#[derive(Copy, Clone, Default, Debug, Eq, PartialEq)]
pub struct TokenProperties(u8);

impl TokenProperties {
    const WORD_LIKE_MASK: u8 = 0b0000_0001;
    const NON_ASCII_MASK: u8 = 0b0000_0010;
    const HAS_ASCII_UPPER_MASK: u8 = 0b0000_0100;

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

    // Stored disjunctively: a single ASCII uppercase byte (A–Z) in the span sets this bit.
    // `has_ascii_upper()` returns true when the bit is set (vacuously false for the empty span).
    pub fn has_ascii_upper(&self) -> bool {
        self.0 & Self::HAS_ASCII_UPPER_MASK != 0
    }
}

impl std::ops::BitOrAssign for TokenProperties {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

pub struct WindowTokens {
    pub breaks: u32,
    pub word_like: u32,
    pub ascii_upper: u32,
}

#[inline(always)]
pub fn table_lookup32(top: uint8x16x4_t, bottom: uint8x16x4_t, bytes: &[u8]) -> uint8x16_t {
    unsafe {
        let input: uint8x16_t = vld1q_u8(bytes.as_ptr());
        let top_tokens = vqtbl4q_u8(top, input);
        let bottom_tokens = vqtbl4q_u8(bottom, vsubq_u8(input, vdupq_n_u8(64)));
        vorrq_u8(top_tokens, bottom_tokens)
    }
}

#[inline(always)]
pub fn load_byte_info(top: uint8x16x4_t, bottom: uint8x16x4_t, bytes: &[u8]) -> (uint8x16_t, uint8x16_t) {
    unsafe {
        let first = table_lookup32(top, bottom, &bytes[0..16]);
        let second = table_lookup32(top, bottom, &bytes[16..32]);
        
        let first_even_tokens = vuzp1q_u8(first, first);
        let first_odd_tokens = vuzp2q_u8(first, first);
        let first_lo = vsliq_n_u8::<4>(first_even_tokens, first_odd_tokens);
        let first_hi = vsriq_n_u8::<4>(first_odd_tokens, first_even_tokens);

        let second_even_tokens = vuzp1q_u8(second, second);
        let second_odd_tokens = vuzp2q_u8(second, second);
        let second_lo = vsliq_n_u8::<4>(second_even_tokens, second_odd_tokens);
        let second_hi = vsriq_n_u8::<4>(second_odd_tokens, second_even_tokens);

        let shifted_first_lo = vextq_u8::<8>(vdupq_n_u8(0), first_lo);
        let lo = vextq_u8::<8>(shifted_first_lo, second_lo);

        let shifted_first_hi = vextq_u8::<8>(vdupq_n_u8(0), first_hi);
        let hi = vextq_u8::<8>(shifted_first_hi, second_hi);
        
        (lo, hi)
    }
}

//Takes a mask of low nibbles and returns a 32 bit packed mask 
#[inline(always)]
pub unsafe fn move_nibble_mask(input: uint8x16_t) -> u32 {
    let mut mask = vgetq_lane_u64::<0>(vreinterpretq_u64_u8(input));
    mask = (mask | (mask >> 3)) & 0x0303030303030303;
    mask = (mask | (mask >> 6)) & 0x000F000F000F000F;
    mask = (mask | (mask >> 12)) & 0x00FF00FF00FF00FF;
    mask = (mask >> 24) | mask;
    let lo_mask = mask & 0xFFFF;

    mask = vgetq_lane_u64::<1>(vreinterpretq_u64_u8(input));
    mask = (mask | (mask >> 3)) & 0x0303030303030303;
    mask = (mask | (mask >> 6)) & 0x000F000F000F000F;
    mask = (mask | (mask >> 12)) & 0x00FF00FF00FF00FF;
    mask = (mask >> 24) | mask;
    let hi_mask = mask & 0xFFFF;
    ((hi_mask << 16) | lo_mask) as u32
}

#[inline(always)]
pub unsafe fn shl_nibble(v: uint8x16_t) -> uint8x16_t {
    let nxt = vextq_u8::<15>(vdupq_n_u8(0), v);
    vsriq_n_u8::<4>(vshlq_n_u8::<4>(v), nxt)
}

#[inline(always)]
pub unsafe fn shr_nibble(v: uint8x16_t) -> uint8x16_t {
    let prv = vextq_u8::<1>(v, vdupq_n_u8(0));
    vsliq_n_u8::<4>(vshrq_n_u8::<4>(v), prv)
}

#[inline(always)]
pub fn maybe_process_ascii_window_neon32(
    bytes: &[u8],
    pos: usize,
    top: uint8x16x4_t,
    bottom: uint8x16x4_t
) -> Option<WindowTokens> {
    let mut high_bit_acc: u8 = 0;
    for i in 0..32 {
        let b = bytes[pos - 2 + i];
        high_bit_acc |= b;
    }

    if high_bit_acc & 0x80 != 0 {
        return None;
    }

    unsafe {
        let (full_lo, full_hi) = load_byte_info(top, bottom, &bytes[pos-2..(pos -2 + 32)]);

        /* Word break rules */

        // let wb6 = (is_letter << 1) & is_mid_let & (is_letter >> 1);
        let wb6 = vandq_u8(
            shr_nibble(full_lo),
            vandq_u8(vshrq_n_u8(full_lo, 3), shl_nibble(full_lo)),
        );

        // let wb6wb7 = wb6 | (wb6 << 1);
        let wb6wb7 = vorrq_u8(wb6, shl_nibble(wb6));

        // let wb12 = (is_numeric << 1) & is_mid_num & (is_numeric >> 1);
        let wb12 = vandq_u8(
            shr_nibble(full_hi),
            vandq_u8(vshrq_n_u8(full_lo, 1), shl_nibble(full_hi)),
        );

        // let wb11wb12 = wb12 | (wb12 << 1);
        let wb11wb12 = vorrq_u8(wb12, shl_nibble(wb12));

        /*
        let wbex1 = is_extend & (is_extend << 1);
        let wbwseg = is_wseg & (is_wseg << 1);
        let wbcrlf = is_lf & (is_cr << 1);
         */
        let wbex1 = vshrq_n_u8(vandq_u8(full_lo, shl_nibble(full_lo)), 2);
        let wbwseg = vshrq_n_u8(vandq_u8(full_hi, shl_nibble(full_hi)), 3);
        let wbcrlf = vandq_u8(vshrq_n_u8(full_hi, 2), vshrq_n_u8(shl_nibble(full_hi), 1));

        let a = vorrq_u8(wb6wb7, wb11wb12);
        let b = vorrq_u8(wbex1, wbwseg);
        let do_not_break = vorrq_u8(vorrq_u8(a, b), wbcrlf);

        // Break is the inverse of do not break
        let mut breaks = vandq_u8(vmvnq_u8(do_not_break), vdupq_n_u8(0x11));
        // we need to shift off the prev1 and prev2 bits
        breaks = vextq_u8::<1>(breaks, vdupq_n_u8(0));
        let full_mask = move_nibble_mask(breaks);

        /* Check if positions are word like, we check of is_letter or is_numeric */
        
        let mut word_like_vector = vorrq_u8(full_hi, full_lo);
        word_like_vector = vandq_u8(word_like_vector, vdupq_n_u8(0x11));
        // remove 1 byte of previous
        word_like_vector = vextq_u8::<1>(word_like_vector, vdupq_n_u8(0));
        let word_like = move_nibble_mask(word_like_vector);

        /* Check if positions are upper case */
        
        let window_bytes = &bytes[pos..(pos + 32)];
        let first = vld1q_u8(window_bytes.as_ptr());
        let second = vld1q_u8(window_bytes.as_ptr().add(16));
        let mut is_first_upper = vcleq_u8(vsubq_u8(first, vdupq_n_u8(b'A')), vdupq_n_u8(b'Z' - b'A'));
        is_first_upper = vandq_u8(is_first_upper, vdupq_n_u8(0x01));
        let first_even_tokens = vuzp1q_u8(is_first_upper, is_first_upper);
        let first_odd_tokens = vuzp2q_u8(is_first_upper, is_first_upper);
        is_first_upper = vsliq_n_u8::<4>(first_even_tokens, first_odd_tokens);
        
        let mut is_second_upper = vcleq_u8(vsubq_u8(second, vdupq_n_u8(b'A')), vdupq_n_u8(b'Z' - b'A'));
        is_second_upper = vandq_u8(is_second_upper, vdupq_n_u8(0x01));
        let second_even_tokens = vuzp1q_u8(is_second_upper, is_second_upper);
        let second_odd_tokens = vuzp2q_u8(is_second_upper, is_second_upper);
        is_second_upper = vsliq_n_u8::<4>(second_even_tokens, second_odd_tokens);

        let full_upper =  vcombine_u8(vget_low_u8(is_first_upper), vget_low_u8(is_second_upper));
        let ascii_upper = move_nibble_mask(full_upper);
        
        Some(WindowTokens {
            breaks: (full_mask  & 0x1FFFFFFF) as u32,
            word_like: (word_like & 0x1FFFFFFF),
            ascii_upper: (ascii_upper & 0x1FFFFFFF),
        })
    }
}

pub fn tokenize_windowed(
    text: &str,
    options: Options,
    on_breakpoint: impl FnMut(usize, TokenProperties) -> bool,
) {
    #[cfg(target_arch = "aarch64")]
    tokenize_windowed_with::<Neon32, _>(text, options, on_breakpoint);
    #[cfg(not(target_arch = "aarch64"))]
    tokenize(text, options, on_breakpoint);
}

pub fn tokenize_windowed_with<P: WindowProcessor, F: FnMut(usize, TokenProperties) -> bool>(
    text: &str,
    _options: Options,
    mut on_breakpoint: F,
) {
    if text.is_empty() {
        return;
    }
    let bytes = text.as_bytes();

    let mut state = State::StartOfText;
    let mut deferred_break_pos = None;
    let mut pos = 0;

    let mut last_was_zwj = false;
    let mut token_props = TokenProperties::default();
    let mut deferred_props = TokenProperties::default();
    let mut processor = P::new();

    while pos < text.len() {
        // ACII Windowed fast path
        // We only accept all ascii windows
        // We dont handoff state from the scalar parsing to the window
        if pos >= P::MIN_POS
            && pos + P::WINDOW_SIZE + 3 < bytes.len()
            && deferred_break_pos.is_none()
            && last_was_zwj == false
            && bytes[pos - 1] < 0x80
            && bytes[pos - 2] < 0x80
        {
            let mut processed_window = false;
            let mut last_break = 0;
            while pos + P::WINDOW_SIZE + 3 < bytes.len() {
                if let Some(res) = processor.process(bytes, pos) {
                    let mut breaks = res.breaks;

                    let mut start = 0;
                    while breaks != 0 {
                        let next_break = breaks.trailing_zeros();
                        let prop_mask: u32 = (1u32 << next_break) - (1u32 << start);

                        token_props.0 |= ((res.word_like & prop_mask != 0) as u8).wrapping_neg()
                            & TokenProperties::WORD_LIKE_MASK;
                        token_props.0 |= ((res.ascii_upper & prop_mask != 0) as u8).wrapping_neg()
                            & TokenProperties::HAS_ASCII_UPPER_MASK;

                        if !on_breakpoint(
                            pos + next_break as usize,
                            std::mem::take(&mut token_props),
                        ) {
                            return;
                        }

                        start = next_break;
                        breaks &= breaks - 1;
                        last_break = pos + next_break as usize;
                    }

                    // handoff to the next window, we will need to handoff tokenprops that cross window boundaries
                    let prop_mask: u32 = ((1u32 << 29) - (1u32 << start)) as u32;
                    token_props.0 |= ((res.word_like & prop_mask != 0) as u8).wrapping_neg()
                        & TokenProperties::WORD_LIKE_MASK;
                    token_props.0 |= ((res.ascii_upper & prop_mask != 0) as u8).wrapping_neg()
                        & TokenProperties::HAS_ASCII_UPPER_MASK;

                    pos += P::WINDOW_SIZE;
                    processed_window = true;
                } else {
                    break;
                }
            }

            if processed_window {
                let previous_state = TABLE[State::Any as usize][ASCII_WORD_BREAK_PROP[bytes[pos-2] as usize] as usize].0;
                state = TABLE[previous_state as usize][ASCII_WORD_BREAK_PROP[bytes[pos-1] as usize] as usize].0;
                let next_state = TABLE[state as usize][ASCII_WORD_BREAK_PROP[bytes[pos] as usize] as usize].0;
                if state.is_deferred() {
                    if last_break != pos-1 {
                        deferred_break_pos = Some(pos-1);
                        deferred_props.0 = ASCII_BYTE_INFO[bytes[pos-1] as usize] & !ASCII_WORD_CONTINUE;
                    } else {
                        //We have already consumed this deferred break, advance to the next state.
                        state = next_state;
                    }
                }
            }
        }

        // Handle unicode & Scalar runs, scalar runs happen at the edges between unicode and ascii
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
/// - Low bits: the byte's `TokenProperties` contribution (`WORD_LIKE_MASK` for `[a-zA-Z0-9]`,
///   since underscore continues the run but isn't itself word-like, plus
///   `HAS_ASCII_UPPER_MASK` for `[A-Z]`).
const ASCII_WORD_CONTINUE: u8 = 0b1000_0000;
const ASCII_BYTE_INFO: [u8; 128] = {
    let mut t = [0u8; 128];
    let mut i = 0u8;
    loop {
        t[i as usize] = match i {
            b'a'..=b'z' | b'0'..=b'9' => ASCII_WORD_CONTINUE | TokenProperties::WORD_LIKE_MASK,
            b'A'..=b'Z' => {
                ASCII_WORD_CONTINUE
                    | TokenProperties::WORD_LIKE_MASK
                    | TokenProperties::HAS_ASCII_UPPER_MASK
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

static ASCII_BREAK_MAP: [u8; 128] = {
    let mut t = [0u8; 128];
    let mut i = 0u8;
    loop {
        t[i as usize] = {
            let mid_let = matches!(
                ASCII_WORD_BREAK_PROP[i as usize],
                WordBreakProperty::MidLetter
                    | WordBreakProperty::MidNumLet
                    | WordBreakProperty::SingleQuote
            ) as u8;
            let mid_num = matches!(
                ASCII_WORD_BREAK_PROP[i as usize],
                WordBreakProperty::MidNum
                    | WordBreakProperty::MidNumLet
                    | WordBreakProperty::SingleQuote
            ) as u8;
            let extend = matches!(
                ASCII_WORD_BREAK_PROP[i as usize],
                WordBreakProperty::ALetter
                    | WordBreakProperty::Numeric
                    | WordBreakProperty::ExtendNumLet
            ) as u8;
            let letter = matches!(
                ASCII_WORD_BREAK_PROP[i as usize],
                WordBreakProperty::ALetter
            ) as u8;
            let numeric = matches!(
                ASCII_WORD_BREAK_PROP[i as usize],
                WordBreakProperty::Numeric
            ) as u8;
            let cr = matches!(ASCII_WORD_BREAK_PROP[i as usize], WordBreakProperty::CR) as u8;
            let lf = matches!(ASCII_WORD_BREAK_PROP[i as usize], WordBreakProperty::LF) as u8;
            let wseg = matches!(
                ASCII_WORD_BREAK_PROP[i as usize],
                WordBreakProperty::WSegSpace
            ) as u8;
            
            letter
                | (mid_num << 1)
                | (extend << 2)
                | (mid_let << 3)
                | (numeric << 4)
                | (cr << 5)
                | (lf << 6)
                | (wseg << 7)
        };

        if i == 127 {
            break;
        }
        i += 1;
    }
    t
};

pub trait WindowProcessor: Copy {
    /// Bytes of left context `process` reads before `pos`; the caller must not call `process`
    /// with a smaller `pos`.
    const MIN_POS: usize;
    const WINDOW_SIZE: usize;

    fn new() -> Self;
    fn process(&mut self, bytes: &[u8], pos: usize) -> Option<WindowTokens>;
}

#[cfg(target_arch = "aarch64")]
#[derive(Clone, Copy)]
pub struct Neon32 {
    top: uint8x16x4_t,
    bottom: uint8x16x4_t,
}

#[cfg(target_arch = "aarch64")]
impl WindowProcessor for Neon32 {
    // `process` table-looks-up the preceding window to seed prev_lo/prev_hi.
    const MIN_POS: usize = 2;
    const WINDOW_SIZE: usize = 29;

    #[inline(always)]
    fn new() -> Self {
        // SAFETY: table is 128 bytes; NEON is baseline on aarch64.
        unsafe {
            let p = ASCII_BREAK_MAP.as_ptr();
            Neon32 {
                top: vld1q_u8_x4(p),
                bottom: vld1q_u8_x4(p.add(64))
            }
        }
    }

    #[inline(always)]
    fn process(&mut self, bytes: &[u8], pos: usize) -> Option<WindowTokens> {
        maybe_process_ascii_window_neon32(bytes, pos, self.top, self.bottom)
    }
}

#[cfg(test)]
mod tests {
    use super::{Options, tokenize};
    use crate::uax29::{test_helpers::test_against_uax29_break_tests, word::tokenize_windowed};

    //Sanity checks for wordbreak rules.
    #[cfg(target_arch = "aarch64")]
    fn check_kernel_rules<P: crate::uax29::word::WindowProcessor>(
        name: &str,
        failures: &mut Vec<String>,
    ) {
        const CASES: &[(&str, &str)] = &[
            ("WB6/WB7   ALetter x MidLetter x ALetter", "can't stop o'neill won't go now"),
            ("WB6/WB7   MidNumLet between letters", "a.b.c d.e.f g.h.i j.k.l m.n o.p"),
            ("WB11/WB12 Numeric x MidNum x Numeric", "1,234 5.67 89,012 3.4 56,78 9,01"),
            ("WB13a/b   ExtendNumLet adjacency", "foo_bar_baz a1b2c3 x_1 y_2 z_3 w_4"),
            ("WB3       CR x LF", "a\r\nb c\r\nd e\r\nf g\r\nh i\r\nj k\r\nl m"),
            ("WB3d      WSegSpace x WSegSpace", "a  b   c    d  e   f    g  h   i  j"),
            ("mixed     rules meeting at the edge", "ab.cd 12,34 ef_gh  ij\r\nkl mn.op qr,st"),
        ];

        for (rule, body) in CASES {
            let text = format!("{body} {body} {body}");
            let bytes = text.as_bytes();

            let mut is_break = vec![false; text.len() + 1];
            tokenize(&text, Options::default(), |bp, _| {
                is_break[bp] = true;
                true
            });

            let mut processor = P::new();
            let last = bytes.len().saturating_sub(P::WINDOW_SIZE + 4);
            for pos in P::MIN_POS.max(2)..last {
                let Some(got) = processor.process(bytes, pos) else {
                    continue;
                };

                let mut want = 0u32;
                for j in 0..P::WINDOW_SIZE {
                    if is_break[pos + j] {
                        want |= 1 << j;
                    }
                }
                let got = got.breaks as u32;
                if got == want {
                    continue;
                }

                let diff = got ^ want;
                let offsets: Vec<usize> =
                    (0..P::WINDOW_SIZE).filter(|j| (diff >> j) & 1 == 1).collect();
                let chars: String = offsets.iter().map(|j| bytes[pos + j] as char).collect();
                failures.push(format!(
                    "  [{name}] {rule}\n    pos={pos} {:?}\n    dfa  {want:032b}\n    got  {got:032b}\n    differs at offsets {offsets:?} (chars {chars:?})",
                    &text[pos..(pos + P::WINDOW_SIZE).min(text.len())]
                ));
                // One report per rule per processor is enough to work from.
                break;
            }
        }
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn neon_kernel_matches_dfa_rules() {
        use crate::uax29::word::Neon32;

        let mut failures = Vec::new();
        check_kernel_rules::<Neon32>("Neon32", &mut failures);

        assert!(
            failures.is_empty(),
            "window rules disagree with the DFA:\n{}",
            failures.join("\n")
        );
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn load_byte_info_packs_register_order() {
        use super::{ASCII_BREAK_MAP, load_byte_info};
        use std::arch::aarch64::*;

        // a A 1 _ . , : ; ' CR LF SP b B 2 c 3 D 4 e 5 F 6 g 7 H 8 i 9 j K -
        let input: &[u8; 32] = &[
            0x61, 0x41, 0x31, 0x5F, 0x2E, 0x2C, 0x3A, 0x3B,
            0x27, 0x0D, 0x0A, 0x20, 0x62, 0x42, 0x32, 0x63,
            0x33, 0x44, 0x34, 0x65, 0x35, 0x46, 0x36, 0x67,
            0x37, 0x48, 0x38, 0x69, 0x39, 0x6A, 0x4B, 0x2D,
        ];

        // `ASCII_CUSTOM_BYTE` for each input byte — the intermediate the packing consumes.
        // bit 0 letter, 1 mid_num, 2 extend, 3 mid_let, 4 numeric, 5 cr, 6 lf, 7 wseg
        const EXPECTED_TOKENS: [u8; 32] = [
            0x05, 0x05, 0x14, 0x04, 0x0A, 0x02, 0x08, 0x02,
            0x0A, 0x20, 0x40, 0x80, 0x05, 0x05, 0x14, 0x05,
            0x14, 0x05, 0x14, 0x05, 0x14, 0x05, 0x14, 0x05,
            0x14, 0x05, 0x14, 0x05, 0x14, 0x05, 0x05, 0x00,
        ];

        const EXPECTED_LO: [u8; 16] = [
            0x55, 0x44, 0x2A, 0x28, 0x0A, 0x00, 0x55, 0x54,
            0x54, 0x54, 0x54, 0x54, 0x54, 0x54, 0x54, 0x05,
        ];

        const EXPECTED_HI: [u8; 16] = [
            0x00, 0x01, 0x00, 0x00, 0x20, 0x84, 0x00, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00,
        ];

        // Pin the table first: if an entry moves, the packed expectations below are stale and
        // this says so directly rather than surfacing as a confusing packing failure.
        for i in 0..32 {
            assert_eq!(
                ASCII_BREAK_MAP[input[i] as usize],
                EXPECTED_TOKENS[i],
                "ASCII_CUSTOM_BYTE[{:?}] changed; regenerate EXPECTED_LO / EXPECTED_HI",
                input[i] as char
            );
        }

        let (top, bottom) = unsafe {
            let p = ASCII_BREAK_MAP.as_ptr();
            (vld1q_u8_x4(p), vld1q_u8_x4(p.add(64)))
        };

        let (lo, hi) = load_byte_info(top, bottom, input);
        let (mut got_lo, mut got_hi) = ([0u8; 16], [0u8; 16]);
        unsafe {
            vst1q_u8(got_lo.as_mut_ptr(), lo);
            vst1q_u8(got_hi.as_mut_ptr(), hi);
        }

        // Render each half as aligned rows so the two can be compared by eye, with carets
        // under the lanes that differ.
        let render = |name: &str, got: &[u8; 16], want: &[u8; 16]| -> String {
            let cells = |v: &[u8; 16]| -> String {
                v.iter().map(|b| format!("{b:#04X} ")).collect::<String>()
            };
            let index: String = (0..16).map(|i| format!("{i:>4} ")).collect();
            let marks: String = (0..16)
                .map(|i| if got[i] == want[i] { "     " } else { "  ^^ " })
                .collect();
            format!(
                "  {name} lane {index}\n  {name}  got {}\n  {name} want {}\n  {name}      {marks}",
                cells(got),
                cells(want),
            )
        };

        let wrong = (0..16)
            .filter(|i| got_lo[*i] != EXPECTED_LO[*i] || got_hi[*i] != EXPECTED_HI[*i])
            .count();

        assert!(
            got_lo == EXPECTED_LO && got_hi == EXPECTED_HI,
            "load_byte_info: {wrong} of 16 lanes packed wrong\n{}\n{}",
            render("lo", &got_lo, &EXPECTED_LO),
            render("hi", &got_hi, &EXPECTED_HI),
        );
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn nibble_shifts_move_one_position() {
        use super::{shl_nibble, shr_nibble};
        use std::arch::aarch64::*;

        // nibbles 1,2,3,...,F,0 then 0,1,2,...,F, low nibble of each lane first.
        const INPUT: [u8; 16] = [
            0x21, 0x43, 0x65, 0x87, 0xA9, 0xCB, 0xED, 0x0F, 0x10, 0x32, 0x54, 0x76, 0x98, 0xBA,
            0xDC, 0xFE,
        ];
        // Every nibble one place up; position 0 zero-filled, the old position 31 dropped.
        const WANT_SHL: [u8; 16] = [
            0x10, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE, 0x00, 0x21, 0x43, 0x65, 0x87, 0xA9,
            0xCB, 0xED,
        ];
        // Every nibble one place down; position 31 zero-filled, the old position 0 dropped.
        const WANT_SHR: [u8; 16] = [
            0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE, 0x00, 0x21, 0x43, 0x65, 0x87, 0xA9, 0xCB,
            0xED, 0x0F,
        ];

        let apply = |f: unsafe fn(uint8x16_t) -> uint8x16_t| -> [u8; 16] {
            let mut out = [0u8; 16];
            unsafe {
                vst1q_u8(out.as_mut_ptr(), f(vld1q_u8(INPUT.as_ptr())));
            }
            out
        };

        assert_eq!(
            apply(shl_nibble),
            WANT_SHL,
            "shl_nibble did not move every nibble one position up\n  in   {INPUT:02X?}\n  want {WANT_SHL:02X?}"
        );
        assert_eq!(
            apply(shr_nibble),
            WANT_SHR,
            "shr_nibble did not move every nibble one position down\n  in   {INPUT:02X?}\n  want {WANT_SHR:02X?}"
        );
    }

    /// Mid-class punctuation (`,` `.` `:` `'`) sitting next to non-ASCII, which makes a window
    /// reject and hand back to the DFA
    #[test]
    fn windowed_deferred_break_next_to_non_ascii() {
        const BODIES: &[&str] = &[
            "February 12, 1809\u{a0}\u{2013} April 15, 1865 and so on",
            "the Egyptian temples.\n\nMythology \u{2013} Zosimos wrote it",
            "British standard. The digits 0\u{2013}9 are printed here",
            "a, b \u{2013} c, d \u{2013} e, f \u{2013} g, h \u{2013} i, j \u{2013} k, l",
            "x.y \u{2013} z.w \u{2013} p.q \u{2013} r.s \u{2013} t.u \u{2013} v.w \u{2013} aa",
            "it's \u{2013} a \u{2013} test's \u{2013} of \u{2013} quotes' \u{2013} and more",
            "1,234 \u{a0}5,678 \u{a0}9,012 \u{a0}3,456 \u{a0}7,890 \u{a0}1,234",
        ];

        let mut failures = Vec::new();
        for body in BODIES {
            for pad in 0..40 {
                let input = format!("{}{body}", "a".repeat(pad));

                let mut want = Vec::new();
                tokenize(&input, Options::default(), |bp, _| {
                    want.push(bp);
                    true
                });

                let mut got = Vec::new();
                tokenize_windowed(&input, Options::default(), |bp, _| {
                    got.push(bp);
                    true
                });

                // Breakpoints must be strictly increasing. A repeat means one boundary was
                // reported twice, which produces an empty token downstream.
                if let Some(w) = got.windows(2).find(|w| w[1] <= w[0]) {
                    if failures.len() < 10 {
                        failures.push(format!(
                            "  pad={pad} breakpoint {} repeated in {input:?}\n    got {got:?}",
                            w[0]
                        ));
                    }
                    continue;
                }

                if want != got && failures.len() < 10 {
                    failures.push(format!(
                        "  pad={pad} {input:?}\n    want {want:?}\n    got  {got:?}"
                    ));
                }
            }
        }

        assert!(
            failures.is_empty(),
            "windowed tokenizer disagrees with the DFA around deferred breaks:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn test_windowed_break_against_uax29_tests() {
        let (passed, failed) =
            test_against_uax29_break_tests("testdata/WordBreakTest.txt", |s, breakpoints| {
                tokenize_windowed(s, Options::default(), |bp, _props| {
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
    fn windowed_matches_dfa_on_padded_corpus() {
        use crate::uax29::test_helpers::load_break_tests;
        use crate::uax29::word::{Neon32, WindowProcessor};

        // Prefix lengths, chosen to land the body at every offset mod WINDOW and to straddle one,
        // two, and three window boundaries.
        const PADS: &[usize] = &[0, 1, 2, 3, 5, 8, 13, 15, 16, 17, 31, 33];
        // Long enough on its own to satisfy `pos + WINDOW < len` after the body ends.
        const TAIL: &str = " the quick brown fox jumps over it";
        assert!(
            TAIL.len() > Neon32::WINDOW_SIZE + 2,
            "tail too short to reach the fast path for the widest window"
        );

        let mut mismatches = Vec::new();
        let mut total_mismatches = 0usize;
        let mut checked = 0usize;

        for case in load_break_tests("testdata/WordBreakTest.txt") {
            let body = case.codepoints_as_string();
            for &pad in PADS {
                let input = format!("{}{body}{TAIL}", "a".repeat(pad));

                let mut want = Vec::new();
                tokenize(&input, Options::default(), |bp, _props| {
                    want.push(bp);
                    true
                });
                let mut got = Vec::new();
                tokenize_windowed(&input, Options::default(), |bp, _props| {
                    got.push(bp);
                    true
                });

                checked += 1;
                if want != got {
                    total_mismatches += 1;
                    if mismatches.len() < 15 {
                        mismatches.push(format!(
                            "  pad={pad} {input:?}\n      want {want:?}\n       got {got:?}"
                        ));
                    }
                }
            }
        }

        assert!(
            mismatches.is_empty(),
            "{}\n\n{total_mismatches} / {checked} padded inputs disagree with the DFA \
             (showing first {})",
            mismatches.join("\n"),
            mismatches.len()
        );
    }

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

    #[test]
    fn tokenizer_has_ascii_upper_sanity() {
        // Each emit reports properties of the span just closed; the leading boundary at 0 has
        // no preceding span, so has_ascii_upper is vacuously false.
        fn assert_has_ascii_upper(s: &str, expected: Vec<(usize, bool)>) {
            let mut got: Vec<(usize, bool)> = Vec::new();
            tokenize(s, Options::default(), |bp, props| {
                got.push((bp, props.has_ascii_upper()));
                true
            });
            assert_eq!(got, expected, "input: {:?}", s);
        }

        assert_has_ascii_upper("hello", vec![(0, false), (5, false)]);
        assert_has_ascii_upper("Hello", vec![(0, false), (5, true)]);
        assert_has_ascii_upper("HELLO", vec![(0, false), (5, true)]);
        assert_has_ascii_upper("aB", vec![(0, false), (2, true)]);
        assert_has_ascii_upper("123", vec![(0, false), (3, false)]);

        // The breaking char is non-ASCII but starts the *next* token, so "ab" must still
        // report has_ascii_upper=false.
        assert_has_ascii_upper("ab🛑", vec![(0, false), (2, false), (6, false)]);
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

    #[test]
    fn windowed_token_props_on_padded_ascii() {
        // Bodies from `tokenizer_word_like_ascii_sanity`, plus uppercase and boundary-straddling
        // cases so `has_ascii_upper` and the cross-window carry are exercised too.
        //
        // The multi-word uppercase bodies matter beyond the short ones: `has_ascii_upper` is
        // accumulated per token with `(mask & prop_mask) != 0`, so a bit sitting at the wrong
        // offset still gives the right answer while it stays inside the same token. Only a
        // capital far enough into the window for the error to cross a token boundary shows it.
        const BODIES: &[&str] = &[
            "hello", "123", "abc123", "won't", "___", "   ", "!!!", "Hello", "aB", "HELLO",
            "a_b_c", "3.14", "e.g.", "x", "",
            "the 1 brown 22 jumps 333 over 4444 lazy 55555 again 6 now 77",
            "a 1.5 b 2,000 c 3.14.15 d 42 e 7 f 99 g 100 h 8 i 0 j 12",
            "123 !!! 456 ___ 789 ... 012 --- 345 ??? 678 ,,, 901 ;;; 23",
            "x!y 12!34 ab!!cd 5?6 ef...gh 7--8 ij__kl 9 mn 0 op 3.4 qr",
            "A1 b2 C3 d4 E5 f6 G7 h8 I9 j0 K1 l2 M3 n4 O5 p6 Q7 r8 S9",
            "the Quick brown Fox jumps over the lazy Dog again now",
            "one Two three Four five Six seven Eight nine Ten more",
            "a B c D e F g H i J k L m N o P q R s T u V w X y Z b",
            "ABC def GHI jkl MNO pqr STU vwx YZa bcd EFG hij KLM no",
            "hello WORLD hello WORLD hello WORLD hello WORLD hello",
            "alpha Bravo charlie delta Echo foxtrot golf é hotel India juliet kilo Lima mike now",
            "Quick brown fox jumps over the lazy Dog ☃ again and again now and then again",
            // The non-ASCII inside a token rather than between two, so the handoff lands mid-token
            // and the scalar path has to pick up a token the window had already started.
            "aaaa Bbbb cccc dddd eeee ffff gggg hhhhé iiii Jjjj kkkk llll mmmm nnnn oooo pppp",
            "ABCDEFGH ijklmnop QRSTUVWX yzabcdef ghijklmn é GHIJKLMN opqrstuv WXYZabcd MNOP",
            // Two islands, so the fast path has to re-engage twice rather than once.
            "one Two three é four Five six seven Eight nine ☃ ten Eleven twelve thirteen Fourteen",
        ];
        const PADS: &[usize] = &[0, 1, 2, 3, 5, 8, 13, 15, 16, 17, 31, 33];
        const TAIL: &str = " the quick brown fox jumps over it";

        use super::TokenProperties;

        fn run(
            tok: impl Fn(&str, Options, &mut dyn FnMut(usize, TokenProperties) -> bool),
            s: &str,
        ) -> Vec<(usize, u8)> {
            let mut out = Vec::new();
            tok(s, Options::default(), &mut |bp, props| {
                out.push((bp, props.0));
                true
            });
            out
        }

        const SWEEP_LEN: usize = 70;

        // Windows only kick in after 2 bytes so we lead with this prefix
        const LEAD: &str = "ab ";

        let capital_sweep = (0..SWEEP_LEN)
            .map(|k| format!("{LEAD}{}B{}", "a".repeat(k), "c".repeat(SWEEP_LEN - 1 - k)));

        let two_capitals = (0..SWEEP_LEN - 24).map(|k| {
            format!(
                "{LEAD}{}B{}C{}",
                "a".repeat(k),
                "d".repeat(22),
                "e".repeat(SWEEP_LEN - 24 - k)
            )
        });

        let bodies: Vec<String> = BODIES
            .iter()
            .map(|b| (*b).to_string())
            .chain(capital_sweep)
            .chain(two_capitals)
            .collect();

        let mut failures = Vec::new();
        let mut checked = 0usize;
        for body in &bodies {
            for &pad in PADS {
                let input = format!("{}{body}{TAIL}", "a".repeat(pad));
                let want = run(|s, o, cb| tokenize(s, o, cb), &input);
                let got = run(|s, o, cb| tokenize_windowed(s, o, cb), &input);
                checked += 1;
                if want != got {
                    let fmt = |e: Option<&(usize, u8)>| match e {
                        Some((bp, bits)) => format!("bp={bp} props={bits:#07b}"),
                        None => "<no emit>".to_string(),
                    };
                    let mut report = String::new();
                    let mut prev = 0;
                    for i in 0..want.len().max(got.len()) {
                        let (w, g) = (want.get(i), got.get(i));
                        let tok = match w {
                            Some(&(bp, _)) => &input[std::mem::replace(&mut prev, bp)..bp],
                            None => "",
                        };
                        if w == g {
                            continue;
                        }
                        report.push_str(&format!(
                            "      #{i} {tok:?}\n        want {}\n         got {}\n",
                            fmt(w),
                            fmt(g),
                        ));
                    }
                    failures.push(format!(
                        "  body={body:?} pad={pad} {input:?}\n{report}"
                    ));
                }
            }
        }

        assert!(
            failures.is_empty(),
            "{}\n\n{} / {checked} padded inputs disagree on token properties",
            failures
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n"),
            failures.len(),
        );
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
                ("\u{05F4}".len(), true),                 // leading ״
                ("\u{05F4}אקספרס".len(), true),           // אקספרס
                ("\u{05F4}אקספרס\u{05F4}".len(), true),   // trailing ״
                ("\u{05F4}אקספרס\u{05F4} ".len(), false), // space
                (s.len(), true),                          // מהיום
            ],
        );
    }
}
