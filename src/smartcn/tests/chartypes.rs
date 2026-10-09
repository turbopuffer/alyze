//! `char_type` must agree with `Utility.getCharType` for every UTF-16 code unit.

use super::read_testdata;
use crate::smartcn::char_type::{CharType, char_type};

fn parse_type(name: &str) -> CharType {
    match name {
        "DELIMITER" => CharType::Delimiter,
        "LETTER" => CharType::Letter,
        "DIGIT" => CharType::Digit,
        "HANZI" => CharType::Hanzi,
        "SPACE_LIKE" => CharType::SpaceLike,
        "FULLWIDTH_LETTER" => CharType::FullwidthLetter,
        "FULLWIDTH_DIGIT" => CharType::FullwidthDigit,
        "OTHER" => CharType::Other,
        "SURROGATE" => CharType::Surrogate,
        other => panic!("unknown char type {other}"),
    }
}

#[test]
fn every_bmp_code_unit() {
    let golden = read_testdata("golden/chartypes.txt");
    let mut covered = 0u32;
    let mut mismatches = Vec::new();
    for line in golden.lines() {
        let mut fields = line.split('\t');
        let lo = u32::from_str_radix(fields.next().unwrap(), 16).unwrap();
        let hi = u32::from_str_radix(fields.next().unwrap(), 16).unwrap();
        let expected = parse_type(fields.next().unwrap());
        assert_eq!(lo, covered, "golden runs must be contiguous");
        covered = hi + 1;
        for cp in lo..=hi {
            let actual = char_type(cp);
            if actual != expected && mismatches.len() < 20 {
                mismatches.push(format!("U+{cp:04X}: expected {expected:?}, got {actual:?}"));
            }
            if actual != expected {
                // keep counting cheaply
                mismatches.push(String::new());
            }
        }
    }
    assert_eq!(covered, 0x10000, "golden must cover the whole BMP");
    let count = mismatches.iter().filter(|m| m.is_empty()).count();
    let shown: Vec<&String> = mismatches.iter().filter(|m| !m.is_empty()).collect();
    assert!(
        count == 0,
        "{count} code units classified differently from Lucene, e.g.:\n{}",
        shown
            .iter()
            .map(|s| format!("  {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Lucene sees supplementary code points as two SURROGATE code units; the port classifies code
/// points, so every supplementary code point must be `Surrogate`.
#[test]
fn supplementary_code_points() {
    for cp in [
        0x10000, 0x1F600, 0x20000, 0x2A6DF, 0x2CB3B, 0xE0001, 0x10FFFF,
    ] {
        assert_eq!(char_type(cp), CharType::Surrogate, "U+{cp:X}");
    }
}
