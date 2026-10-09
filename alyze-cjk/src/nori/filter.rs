//! Token filters, applied in place to a [`Tokens`] buffer: Elasticsearch's `nori_part_of_speech`,
//! `nori_readingform` and `nori_number`, plus the lowercasing the `nori` analyzer ends with. They
//! compose in any order, like Elasticsearch filter chains.

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
        if text.chars().all(|c| !c.is_alphabetic() || c.is_lowercase()) {
            continue;
        }
        lowered.clear();
        lowercase_into(text, &mut lowered);
        if lowered != text {
            let range = tokens.push_text(&lowered);
            tokens.items[i].text = range;
        }
    }
}

/// The mapping [`lowercase`] applies, on one string.
#[cfg(test)]
pub(crate) fn lowercase_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    lowercase_into(text, &mut out);
    out
}

fn lowercase_into(text: &str, out: &mut String) {
    for c in text.chars() {
        out.extend(crate::unicode_lower::unicode_v17_char_to_lower(c));
    }
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

fn is_numeral(s: &str) -> bool {
    s.chars().all(is_numeral_char)
}

fn is_numeral_char(c: char) -> bool {
    is_arabic_numeral(c) || hangul_numeral_value(c).is_some() || exponent(c) > 0
}

fn is_numeral_punctuation(s: &str) -> bool {
    s.chars()
        .all(|c| is_decimal_point(c) || is_thousand_separator(c))
}

fn is_arabic_numeral(c: char) -> bool {
    c.is_ascii_digit() || ('０'..='９').contains(&c)
}

fn arabic_numeral_value(c: char) -> u8 {
    if c.is_ascii_digit() {
        c as u8 - b'0'
    } else {
        (c as u32 - '０' as u32) as u8
    }
}

fn hangul_numeral_value(c: char) -> Option<u8> {
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

fn is_decimal_point(c: char) -> bool {
    c == '.' || c == '．'
}

fn is_thousand_separator(c: char) -> bool {
    c == ',' || c == '，'
}

/// Lucene's `normalizeNumber`: the plain decimal value of a Korean number, or the input unchanged
/// when it doesn't parse.
pub(crate) fn normalize_number(number: &str) -> String {
    let chars: Vec<char> = number.chars().collect();
    let mut buffer = NumberBuffer {
        chars: &chars,
        position: 0,
    };
    match parse_number(&mut buffer) {
        Ok(Some(value)) => value.to_plain_string(),
        _ => number.to_owned(),
    }
}

struct NumberBuffer<'a> {
    chars: &'a [char],
    position: usize,
}

/// `Err(())` is Java's `NumberFormatException` (malformed input); `Ok(None)` is "nothing here".
type Parsed = Result<Option<Decimal>, ()>;

fn parse_number(buffer: &mut NumberBuffer<'_>) -> Parsed {
    let mut sum = Decimal::zero();
    let Some(mut result) = parse_large_pair(buffer)? else {
        return Ok(None);
    };
    loop {
        sum = sum.add(&result);
        match parse_large_pair(buffer)? {
            Some(next) => result = next,
            None => return Ok(Some(sum)),
        }
    }
}

/// A pair whose second factor is 만 or larger.
fn parse_large_pair(buffer: &mut NumberBuffer<'_>) -> Parsed {
    let first = parse_medium_number(buffer)?;
    let second = parse_large_hangul_numeral(buffer);
    Ok(match (first, second) {
        (None, None) => None,
        (Some(first), None) => Some(first),
        (None, Some(second)) => Some(Decimal::one().times_power_of_ten(second)),
        (Some(first), Some(second)) => Some(first.times_power_of_ten(second)),
    })
}

fn parse_medium_number(buffer: &mut NumberBuffer<'_>) -> Parsed {
    let mut sum = Decimal::zero();
    let Some(mut result) = parse_medium_pair(buffer)? else {
        return Ok(None);
    };
    loop {
        sum = sum.add(&result);
        match parse_medium_pair(buffer)? {
            Some(next) => result = next,
            None => return Ok(Some(sum)),
        }
    }
}

/// A pair whose second factor is at most 천.
fn parse_medium_pair(buffer: &mut NumberBuffer<'_>) -> Parsed {
    let first = parse_basic_number(buffer)?;
    let second = parse_medium_hangul_numeral(buffer);
    Ok(match (first, second) {
        (None, None) => None,
        (Some(first), None) => Some(first),
        (None, Some(second)) => Some(Decimal::one().times_power_of_ten(second)),
        (Some(first), Some(second)) => Some(first.times_power_of_ten(second)),
    })
}

/// A run of Arabic numerals, Hangul digits, decimal points and (skipped) thousands separators,
/// parsed like `new BigDecimal(String)`.
fn parse_basic_number(buffer: &mut NumberBuffer<'_>) -> Parsed {
    let mut text = String::new();
    while buffer.position < buffer.chars.len() {
        let c = buffer.chars[buffer.position];
        if is_arabic_numeral(c) {
            text.push((b'0' + arabic_numeral_value(c)) as char);
        } else if let Some(v) = hangul_numeral_value(c) {
            text.push((b'0' + v) as char);
        } else if is_decimal_point(c) {
            text.push('.');
        } else if is_thousand_separator(c) {
            // skipped
        } else {
            break;
        }
        buffer.position += 1;
    }
    if text.is_empty() {
        return Ok(None);
    }
    Decimal::parse(&text).map(Some)
}

fn parse_large_hangul_numeral(buffer: &mut NumberBuffer<'_>) -> Option<u32> {
    let c = *buffer.chars.get(buffer.position)?;
    let power = exponent(c);
    if power > 3 {
        buffer.position += 1;
        Some(power)
    } else {
        None
    }
}

fn parse_medium_hangul_numeral(buffer: &mut NumberBuffer<'_>) -> Option<u32> {
    let c = *buffer.chars.get(buffer.position)?;
    let power = exponent(c);
    if (1..=3).contains(&power) {
        buffer.position += 1;
        Some(power)
    } else {
        None
    }
}

/// Just enough of `java.math.BigDecimal` for the number filter: a non-negative unscaled integer
/// (decimal digits, most significant first) times ten to the minus `scale`.
#[derive(Clone, Debug)]
struct Decimal {
    digits: Vec<u8>,
    scale: i64,
}

impl Decimal {
    fn zero() -> Decimal {
        Decimal {
            digits: vec![0],
            scale: 0,
        }
    }

    fn one() -> Decimal {
        Decimal {
            digits: vec![1],
            scale: 0,
        }
    }

    /// `new BigDecimal(text)` for the texts `parse_basic_number` builds: digits with at most one
    /// decimal point and at least one digit.
    fn parse(text: &str) -> Result<Decimal, ()> {
        let (int_part, frac_part) = match text.split_once('.') {
            Some((i, f)) => (i, f),
            None => (text, ""),
        };
        if int_part.is_empty() && frac_part.is_empty() || frac_part.contains('.') {
            return Err(());
        }
        let digits: Vec<u8> = int_part
            .bytes()
            .chain(frac_part.bytes())
            .map(|b| b - b'0')
            .collect();
        Ok(Decimal {
            digits,
            scale: frac_part.len() as i64,
        }
        .normalized())
    }

    /// Strips leading zeros (keeping one digit).
    fn normalized(mut self) -> Decimal {
        let leading = self.digits.iter().take_while(|&&d| d == 0).count();
        let keep = leading.min(self.digits.len() - 1);
        self.digits.drain(..keep);
        self
    }

    fn times_power_of_ten(mut self, power: u32) -> Decimal {
        self.digits.extend(std::iter::repeat_n(0, power as usize));
        self.normalized()
    }

    fn add(&self, other: &Decimal) -> Decimal {
        let scale = self.scale.max(other.scale);
        let a = self.rescaled(scale);
        let b = other.rescaled(scale);
        let width = a.len().max(b.len());
        let mut digits = vec![0u8; width + 1];
        let mut carry = 0u8;
        for i in 0..width {
            let da = if i < a.len() { a[a.len() - 1 - i] } else { 0 };
            let db = if i < b.len() { b[b.len() - 1 - i] } else { 0 };
            let sum = da + db + carry;
            digits[width - i] = sum % 10;
            carry = sum / 10;
        }
        digits[0] = carry;
        Decimal { digits, scale }.normalized()
    }

    /// The unscaled digits after raising the scale to `scale` (appending zeros).
    fn rescaled(&self, scale: i64) -> Vec<u8> {
        let mut digits = self.digits.clone();
        digits.extend(std::iter::repeat_n(0, (scale - self.scale) as usize));
        digits
    }

    /// `stripTrailingZeros().toPlainString()`.
    fn to_plain_string(&self) -> String {
        let mut digits = self.digits.clone();
        let mut scale = self.scale;
        if digits.iter().all(|&d| d == 0) {
            return "0".to_owned();
        }
        while digits.len() > 1 && *digits.last().unwrap() == 0 {
            digits.pop();
            scale -= 1;
        }
        let mut s = String::with_capacity(digits.len() + 2);
        if scale <= 0 {
            s.extend(digits.iter().map(|&d| (b'0' + d) as char));
            s.extend(std::iter::repeat_n('0', (-scale) as usize));
        } else {
            let scale = scale as usize;
            let int_len = digits.len().saturating_sub(scale);
            if int_len == 0 {
                s.push('0');
            } else {
                s.extend(digits[..int_len].iter().map(|&d| (b'0' + d) as char));
            }
            s.push('.');
            s.extend(std::iter::repeat_n('0', scale.saturating_sub(digits.len())));
            s.extend(digits[int_len..].iter().map(|&d| (b'0' + d) as char));
        }
        s
    }
}
