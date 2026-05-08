// Vendored from the Rust standard library, nightly-2026-04-01.
// It's important that we avoid changing tokenization after-the-fact
// (e.g. by lowercasing with a different Unicode version).

use std::{fmt::Write, iter::FusedIterator, ops::RangeInclusive};

struct L1Lut {
    l2_luts: [L2Lut; 2],
}

struct L2Lut {
    singles: &'static [(Range, i16)],
    multis: &'static [(u16, [u16; 3])],
}

#[derive(Copy, Clone)]
struct Range {
    start: u16,
    len: u8,
    parity: bool,
}

impl Range {
    const fn new(range: RangeInclusive<u16>, parity: bool) -> Self {
        let start = *range.start();
        let end = *range.end();
        assert!(start <= end);

        let len = end - start;
        assert!(len <= 255);

        Self {
            start,
            len: len as u8,
            parity,
        }
    }

    const fn singleton(start: u16) -> Self {
        Self::new(start..=start, false)
    }

    const fn step_by_1(range: RangeInclusive<u16>) -> Self {
        Self::new(range, false)
    }

    const fn step_by_2(range: RangeInclusive<u16>) -> Self {
        Self::new(range, true)
    }

    const fn start(&self) -> u16 {
        self.start
    }

    const fn end(&self) -> u16 {
        self.start + self.len as u16
    }
}

fn deconstruct(c: char) -> (u16, u16) {
    let c = c as u32;
    let plane = (c >> 16) as u16;
    let low = c as u16;
    (plane, low)
}

unsafe fn reconstruct(plane: u16, low: u16) -> char {
    // SAFETY: The caller must ensure that the result is a valid `char`.
    unsafe { char::from_u32_unchecked(((plane as u32) << 16) | (low as u32)) }
}

fn lookup(input: char, l1_lut: &L1Lut) -> Option<[char; 3]> {
    let (input_high, input_low) = deconstruct(input);
    let Some(l2_lut) = l1_lut.l2_luts.get(input_high as usize) else {
        return None;
    };

    let idx = l2_lut.singles.binary_search_by(|(range, _)| {
        use std::cmp::Ordering;

        if input_low < range.start() {
            Ordering::Greater
        } else if input_low > range.end() {
            Ordering::Less
        } else {
            Ordering::Equal
        }
    });

    if let Ok(idx) = idx {
        // SAFETY: binary search guarantees that the index is in bounds.
        let &(range, output_delta) = unsafe { l2_lut.singles.get_unchecked(idx) };
        let mask = range.parity as u16;
        if input_low & mask == range.start() & mask {
            let output_low = input_low.wrapping_add_signed(output_delta);
            // SAFETY: Table data are guaranteed to be valid Unicode.
            let output = unsafe { reconstruct(input_high, output_low) };
            return Some([output, '\0', '\0']);
        }
    };

    if let Ok(idx) = l2_lut.multis.binary_search_by_key(&input_low, |&(p, _)| p) {
        // SAFETY: binary search guarantees that the index is in bounds.
        let &(_, output_lows) = unsafe { l2_lut.multis.get_unchecked(idx) };
        // SAFETY: Table data are guaranteed to be valid Unicode.
        let output = output_lows.map(|output_low| unsafe { reconstruct(input_high, output_low) });
        return Some(output);
    };

    None
}

fn to_lower(c: char) -> [char; 3] {
    // https://util.unicode.org/UnicodeJsps/list-unicodeset.jsp?a=%5B%253AChanges_When_Lowercased%253A%5D-%5B%253AASCII%253A%5D&abb=on
    if c < '\u{C0}' {
        return [c.to_ascii_lowercase(), '\0', '\0'];
    }

    lookup(c, &LOWERCASE_LUT).unwrap_or([c, '\0', '\0'])
}

static LOWERCASE_LUT: L1Lut = L1Lut {
    l2_luts: [
        L2Lut {
            singles: &[
                // 172 entries, 1032 bytes
                (Range::step_by_1(0x00c0..=0x00d6), 32),
                (Range::step_by_1(0x00d8..=0x00de), 32),
                (Range::step_by_2(0x0100..=0x012e), 1),
                (Range::step_by_2(0x0132..=0x0136), 1),
                (Range::step_by_2(0x0139..=0x0147), 1),
                (Range::step_by_2(0x014a..=0x0176), 1),
                (Range::singleton(0x0178), -121),
                (Range::step_by_2(0x0179..=0x017d), 1),
                (Range::singleton(0x0181), 210),
                (Range::step_by_2(0x0182..=0x0184), 1),
                (Range::singleton(0x0186), 206),
                (Range::singleton(0x0187), 1),
                (Range::step_by_1(0x0189..=0x018a), 205),
                (Range::singleton(0x018b), 1),
                (Range::singleton(0x018e), 79),
                (Range::singleton(0x018f), 202),
                (Range::singleton(0x0190), 203),
                (Range::singleton(0x0191), 1),
                (Range::singleton(0x0193), 205),
                (Range::singleton(0x0194), 207),
                (Range::singleton(0x0196), 211),
                (Range::singleton(0x0197), 209),
                (Range::singleton(0x0198), 1),
                (Range::singleton(0x019c), 211),
                (Range::singleton(0x019d), 213),
                (Range::singleton(0x019f), 214),
                (Range::step_by_2(0x01a0..=0x01a4), 1),
                (Range::singleton(0x01a6), 218),
                (Range::singleton(0x01a7), 1),
                (Range::singleton(0x01a9), 218),
                (Range::singleton(0x01ac), 1),
                (Range::singleton(0x01ae), 218),
                (Range::singleton(0x01af), 1),
                (Range::step_by_1(0x01b1..=0x01b2), 217),
                (Range::step_by_2(0x01b3..=0x01b5), 1),
                (Range::singleton(0x01b7), 219),
                (Range::singleton(0x01b8), 1),
                (Range::singleton(0x01bc), 1),
                (Range::singleton(0x01c4), 2),
                (Range::singleton(0x01c5), 1),
                (Range::singleton(0x01c7), 2),
                (Range::singleton(0x01c8), 1),
                (Range::singleton(0x01ca), 2),
                (Range::step_by_2(0x01cb..=0x01db), 1),
                (Range::step_by_2(0x01de..=0x01ee), 1),
                (Range::singleton(0x01f1), 2),
                (Range::step_by_2(0x01f2..=0x01f4), 1),
                (Range::singleton(0x01f6), -97),
                (Range::singleton(0x01f7), -56),
                (Range::step_by_2(0x01f8..=0x021e), 1),
                (Range::singleton(0x0220), -130),
                (Range::step_by_2(0x0222..=0x0232), 1),
                (Range::singleton(0x023a), 10795),
                (Range::singleton(0x023b), 1),
                (Range::singleton(0x023d), -163),
                (Range::singleton(0x023e), 10792),
                (Range::singleton(0x0241), 1),
                (Range::singleton(0x0243), -195),
                (Range::singleton(0x0244), 69),
                (Range::singleton(0x0245), 71),
                (Range::step_by_2(0x0246..=0x024e), 1),
                (Range::step_by_2(0x0370..=0x0372), 1),
                (Range::singleton(0x0376), 1),
                (Range::singleton(0x037f), 116),
                (Range::singleton(0x0386), 38),
                (Range::step_by_1(0x0388..=0x038a), 37),
                (Range::singleton(0x038c), 64),
                (Range::step_by_1(0x038e..=0x038f), 63),
                (Range::step_by_1(0x0391..=0x03a1), 32),
                (Range::step_by_1(0x03a3..=0x03ab), 32),
                (Range::singleton(0x03cf), 8),
                (Range::step_by_2(0x03d8..=0x03ee), 1),
                (Range::singleton(0x03f4), -60),
                (Range::singleton(0x03f7), 1),
                (Range::singleton(0x03f9), -7),
                (Range::singleton(0x03fa), 1),
                (Range::step_by_1(0x03fd..=0x03ff), -130),
                (Range::step_by_1(0x0400..=0x040f), 80),
                (Range::step_by_1(0x0410..=0x042f), 32),
                (Range::step_by_2(0x0460..=0x0480), 1),
                (Range::step_by_2(0x048a..=0x04be), 1),
                (Range::singleton(0x04c0), 15),
                (Range::step_by_2(0x04c1..=0x04cd), 1),
                (Range::step_by_2(0x04d0..=0x052e), 1),
                (Range::step_by_1(0x0531..=0x0556), 48),
                (Range::step_by_1(0x10a0..=0x10c5), 7264),
                (Range::singleton(0x10c7), 7264),
                (Range::singleton(0x10cd), 7264),
                (Range::step_by_1(0x13a0..=0x13ef), -26672),
                (Range::step_by_1(0x13f0..=0x13f5), 8),
                (Range::singleton(0x1c89), 1),
                (Range::step_by_1(0x1c90..=0x1cba), -3008),
                (Range::step_by_1(0x1cbd..=0x1cbf), -3008),
                (Range::step_by_2(0x1e00..=0x1e94), 1),
                (Range::singleton(0x1e9e), -7615),
                (Range::step_by_2(0x1ea0..=0x1efe), 1),
                (Range::step_by_1(0x1f08..=0x1f0f), -8),
                (Range::step_by_1(0x1f18..=0x1f1d), -8),
                (Range::step_by_1(0x1f28..=0x1f2f), -8),
                (Range::step_by_1(0x1f38..=0x1f3f), -8),
                (Range::step_by_1(0x1f48..=0x1f4d), -8),
                (Range::step_by_2(0x1f59..=0x1f5f), -8),
                (Range::step_by_1(0x1f68..=0x1f6f), -8),
                (Range::step_by_1(0x1f88..=0x1f8f), -8),
                (Range::step_by_1(0x1f98..=0x1f9f), -8),
                (Range::step_by_1(0x1fa8..=0x1faf), -8),
                (Range::step_by_1(0x1fb8..=0x1fb9), -8),
                (Range::step_by_1(0x1fba..=0x1fbb), -74),
                (Range::singleton(0x1fbc), -9),
                (Range::step_by_1(0x1fc8..=0x1fcb), -86),
                (Range::singleton(0x1fcc), -9),
                (Range::step_by_1(0x1fd8..=0x1fd9), -8),
                (Range::step_by_1(0x1fda..=0x1fdb), -100),
                (Range::step_by_1(0x1fe8..=0x1fe9), -8),
                (Range::step_by_1(0x1fea..=0x1feb), -112),
                (Range::singleton(0x1fec), -7),
                (Range::step_by_1(0x1ff8..=0x1ff9), -128),
                (Range::step_by_1(0x1ffa..=0x1ffb), -126),
                (Range::singleton(0x1ffc), -9),
                (Range::singleton(0x2126), -7517),
                (Range::singleton(0x212a), -8383),
                (Range::singleton(0x212b), -8262),
                (Range::singleton(0x2132), 28),
                (Range::step_by_1(0x2160..=0x216f), 16),
                (Range::singleton(0x2183), 1),
                (Range::step_by_1(0x24b6..=0x24cf), 26),
                (Range::step_by_1(0x2c00..=0x2c2f), 48),
                (Range::singleton(0x2c60), 1),
                (Range::singleton(0x2c62), -10743),
                (Range::singleton(0x2c63), -3814),
                (Range::singleton(0x2c64), -10727),
                (Range::step_by_2(0x2c67..=0x2c6b), 1),
                (Range::singleton(0x2c6d), -10780),
                (Range::singleton(0x2c6e), -10749),
                (Range::singleton(0x2c6f), -10783),
                (Range::singleton(0x2c70), -10782),
                (Range::singleton(0x2c72), 1),
                (Range::singleton(0x2c75), 1),
                (Range::step_by_1(0x2c7e..=0x2c7f), -10815),
                (Range::step_by_2(0x2c80..=0x2ce2), 1),
                (Range::step_by_2(0x2ceb..=0x2ced), 1),
                (Range::singleton(0x2cf2), 1),
                (Range::step_by_2(0xa640..=0xa66c), 1),
                (Range::step_by_2(0xa680..=0xa69a), 1),
                (Range::step_by_2(0xa722..=0xa72e), 1),
                (Range::step_by_2(0xa732..=0xa76e), 1),
                (Range::step_by_2(0xa779..=0xa77b), 1),
                (Range::singleton(0xa77d), 30204),
                (Range::step_by_2(0xa77e..=0xa786), 1),
                (Range::singleton(0xa78b), 1),
                (Range::singleton(0xa78d), 23256),
                (Range::step_by_2(0xa790..=0xa792), 1),
                (Range::step_by_2(0xa796..=0xa7a8), 1),
                (Range::singleton(0xa7aa), 23228),
                (Range::singleton(0xa7ab), 23217),
                (Range::singleton(0xa7ac), 23221),
                (Range::singleton(0xa7ad), 23231),
                (Range::singleton(0xa7ae), 23228),
                (Range::singleton(0xa7b0), 23278),
                (Range::singleton(0xa7b1), 23254),
                (Range::singleton(0xa7b2), 23275),
                (Range::singleton(0xa7b3), 928),
                (Range::step_by_2(0xa7b4..=0xa7c2), 1),
                (Range::singleton(0xa7c4), -48),
                (Range::singleton(0xa7c5), 23229),
                (Range::singleton(0xa7c6), 30152),
                (Range::step_by_2(0xa7c7..=0xa7c9), 1),
                (Range::singleton(0xa7cb), 23193),
                (Range::step_by_2(0xa7cc..=0xa7da), 1),
                (Range::singleton(0xa7dc), 22975),
                (Range::singleton(0xa7f5), 1),
                (Range::step_by_1(0xff21..=0xff3a), 32),
            ],
            multis: &[
                // 1 entries, 8 bytes
                (0x0130, [0x0069, 0x0307, 0x0000]),
            ],
        },
        L2Lut {
            singles: &[
                // 12 entries, 72 bytes
                (Range::step_by_1(0x0400..=0x0427), 40),
                (Range::step_by_1(0x04b0..=0x04d3), 40),
                (Range::step_by_1(0x0570..=0x057a), 39),
                (Range::step_by_1(0x057c..=0x058a), 39),
                (Range::step_by_1(0x058c..=0x0592), 39),
                (Range::step_by_1(0x0594..=0x0595), 39),
                (Range::step_by_1(0x0c80..=0x0cb2), 64),
                (Range::step_by_1(0x0d50..=0x0d65), 32),
                (Range::step_by_1(0x18a0..=0x18bf), 32),
                (Range::step_by_1(0x6e40..=0x6e5f), 32),
                (Range::step_by_1(0x6ea0..=0x6eb8), 27),
                (Range::step_by_1(0xe900..=0xe921), 34),
            ],
            multis: &[ // 0 entries, 0 bytes
                ],
        },
    ],
};

#[derive(Debug, Clone)]
pub struct CaseMappingIter(core::array::IntoIter<char, 3>);

impl CaseMappingIter {
    #[inline]
    fn new(chars: [char; 3]) -> CaseMappingIter {
        let mut iter = chars.into_iter();

        if chars[2] == '\0' {
            iter.next_back();

            if chars[1] == '\0' {
                iter.next_back();

                // Deliberately don't check `chars[0]`,
                // as '\0' lowercases to itself.
            }
        }

        CaseMappingIter(iter)
    }
}

impl Iterator for CaseMappingIter {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl DoubleEndedIterator for CaseMappingIter {
    fn next_back(&mut self) -> Option<char> {
        self.0.next_back()
    }
}

impl ExactSizeIterator for CaseMappingIter {
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl FusedIterator for CaseMappingIter {}

impl std::fmt::Display for CaseMappingIter {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for c in self.0.clone() {
            f.write_char(c)?;
        }
        Ok(())
    }
}

pub fn unicode_v17_char_to_lower(c: char) -> CaseMappingIter {
    CaseMappingIter::new(to_lower(c))
}
