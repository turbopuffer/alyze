//! The character classes must agree with Lucene's `CharacterDefinition` for every UTF-16 code
//! unit, and so must the per-class invoke/group flags.

use super::read_testdata;
use crate::kuromoji::char_def::{self, CharClass, is_kanji};

#[test]
fn every_code_unit_and_flags() {
    let golden = read_testdata("golden/chardef.txt");
    let mut covered = 0u32;
    let mut classes_seen = 0;
    let mut mismatches = Vec::new();
    for line in golden.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields[0] == "class" {
            let class = CharClass::from_name(fields[1]).unwrap();
            assert_eq!(class as usize, classes_seen, "class order");
            classes_seen += 1;
            assert_eq!(
                char_def::invoke(class),
                fields[2] == "1",
                "invoke({class:?})"
            );
            assert_eq!(char_def::group(class), fields[3] == "1", "group({class:?})");
            continue;
        }
        let lo = u32::from_str_radix(fields[0], 16).unwrap();
        let hi = u32::from_str_radix(fields[1], 16).unwrap();
        let expected = CharClass::from_name(fields[2]).unwrap();
        assert_eq!(lo, covered, "golden runs must be contiguous");
        covered = hi + 1;
        for cu in lo..=hi {
            let actual = char_def::class(cu as u16);
            if actual != expected {
                if mismatches.len() < 20 {
                    mismatches.push(format!("U+{cu:04X}: expected {expected:?}, got {actual:?}"));
                } else {
                    mismatches.push(String::new());
                }
            }
        }
    }
    assert_eq!(classes_seen, CharClass::ALL.len());
    assert_eq!(covered, 0x10000, "golden must cover every code unit");
    assert!(
        mismatches.is_empty(),
        "{} mismatches, first:\n{}",
        mismatches.len(),
        mismatches
            .iter()
            .filter(|m| !m.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// `isKanji` (what the search-mode penalty counts) is the KANJI and KANJINUMERIC classes.
#[test]
fn kanji() {
    assert!(is_kanji(char_def::class('日' as u16)));
    assert!(is_kanji(char_def::class('一' as u16)));
    assert!(is_kanji(char_def::class('々' as u16)));
    assert!(!is_kanji(char_def::class('あ' as u16)));
    assert!(!is_kanji(char_def::class('ア' as u16)));
    assert!(!is_kanji(char_def::class('1' as u16)));
}
