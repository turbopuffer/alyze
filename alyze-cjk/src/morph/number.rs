//! The number normalization shared by Lucene's `KoreanNumberFilter` and `JapaneseNumberFilter`
//! (the Japanese one is the original; the Korean one is its port with Hangul numerals): the
//! recursive-descent parse of a number written with Arabic or native numerals, powers of ten,
//! decimal points and thousands separators, into a `BigDecimal`, printed plainly with trailing
//! zeros stripped. Each analyzer supplies its numerals through [`Numerals`] and keeps its own
//! token-level filter loop.

/// A language's numeral characters.
pub(crate) trait Numerals {
    /// The value of a native digit character (〇一二三四五六七八九 / 영일이삼사오육칠팔구).
    fn digit(c: char) -> Option<u8>;
    /// The power of ten a native numeral denotes (十百千万億兆京垓 / 십백천만억조경해), 0 for
    /// anything else.
    fn exponent(c: char) -> u32;
}

pub(crate) fn is_arabic_numeral(c: char) -> bool {
    c.is_ascii_digit() || ('０'..='９').contains(&c)
}

fn arabic_numeral_value(c: char) -> u8 {
    if c.is_ascii_digit() {
        c as u8 - b'0'
    } else {
        (c as u32 - '０' as u32) as u8
    }
}

pub(crate) fn is_numeral_char<N: Numerals>(c: char) -> bool {
    is_arabic_numeral(c) || N::digit(c).is_some() || N::exponent(c) > 0
}

/// Lucene's `isNumeral(String)`: every character is a numeral (true for the empty string).
pub(crate) fn is_numeral<N: Numerals>(s: &str) -> bool {
    s.chars().all(is_numeral_char::<N>)
}

pub(crate) fn is_decimal_point(c: char) -> bool {
    c == '.' || c == '．'
}

pub(crate) fn is_thousand_separator(c: char) -> bool {
    c == ',' || c == '，'
}

/// Lucene's `isNumeralPunctuation(String)`.
pub(crate) fn is_numeral_punctuation(s: &str) -> bool {
    s.chars()
        .all(|c| is_decimal_point(c) || is_thousand_separator(c))
}

/// Lucene's `normalizeNumber`: the plain decimal value of a number, or the input unchanged when
/// it doesn't parse.
pub(crate) fn normalize_number<N: Numerals>(number: &str) -> String {
    let chars: Vec<char> = number.chars().collect();
    let mut buffer = NumberBuffer {
        chars: &chars,
        position: 0,
    };
    match parse_number::<N>(&mut buffer) {
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

fn parse_number<N: Numerals>(buffer: &mut NumberBuffer<'_>) -> Parsed {
    let mut sum = Decimal::zero();
    let Some(mut result) = parse_large_pair::<N>(buffer)? else {
        return Ok(None);
    };
    loop {
        sum = sum.add(&result);
        match parse_large_pair::<N>(buffer)? {
            Some(next) => result = next,
            None => return Ok(Some(sum)),
        }
    }
}

/// A pair whose second factor is 万 or larger.
fn parse_large_pair<N: Numerals>(buffer: &mut NumberBuffer<'_>) -> Parsed {
    let first = parse_medium_number::<N>(buffer)?;
    let second = parse_large_numeral::<N>(buffer);
    Ok(match (first, second) {
        (None, None) => None,
        (Some(first), None) => Some(first),
        (None, Some(second)) => Some(Decimal::one().times_power_of_ten(second)),
        (Some(first), Some(second)) => Some(first.times_power_of_ten(second)),
    })
}

fn parse_medium_number<N: Numerals>(buffer: &mut NumberBuffer<'_>) -> Parsed {
    let mut sum = Decimal::zero();
    let Some(mut result) = parse_medium_pair::<N>(buffer)? else {
        return Ok(None);
    };
    loop {
        sum = sum.add(&result);
        match parse_medium_pair::<N>(buffer)? {
            Some(next) => result = next,
            None => return Ok(Some(sum)),
        }
    }
}

/// A pair whose second factor is at most 千.
fn parse_medium_pair<N: Numerals>(buffer: &mut NumberBuffer<'_>) -> Parsed {
    let first = parse_basic_number::<N>(buffer)?;
    let second = parse_medium_numeral::<N>(buffer);
    Ok(match (first, second) {
        (None, None) => None,
        (Some(first), None) => Some(first),
        (None, Some(second)) => Some(Decimal::one().times_power_of_ten(second)),
        (Some(first), Some(second)) => Some(first.times_power_of_ten(second)),
    })
}

/// A run of Arabic numerals, native digits, decimal points and (skipped) thousands separators,
/// parsed like `new BigDecimal(String)`.
fn parse_basic_number<N: Numerals>(buffer: &mut NumberBuffer<'_>) -> Parsed {
    let mut text = String::new();
    while buffer.position < buffer.chars.len() {
        let c = buffer.chars[buffer.position];
        if is_arabic_numeral(c) {
            text.push((b'0' + arabic_numeral_value(c)) as char);
        } else if let Some(v) = N::digit(c) {
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

fn parse_large_numeral<N: Numerals>(buffer: &mut NumberBuffer<'_>) -> Option<u32> {
    let c = *buffer.chars.get(buffer.position)?;
    let power = N::exponent(c);
    if power > 3 {
        buffer.position += 1;
        Some(power)
    } else {
        None
    }
}

fn parse_medium_numeral<N: Numerals>(buffer: &mut NumberBuffer<'_>) -> Option<u32> {
    let c = *buffer.chars.get(buffer.position)?;
    let power = N::exponent(c);
    if (1..=3).contains(&power) {
        buffer.position += 1;
        Some(power)
    } else {
        None
    }
}

/// Just enough of `java.math.BigDecimal` for the number filters: a non-negative unscaled integer
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
