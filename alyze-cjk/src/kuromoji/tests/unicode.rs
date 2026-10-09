//! The pinned ICU property the port uses must agree with the JDK's `Character.getType` for every
//! UTF-16 code unit (the JDK is what Lucene consults), the punctuation test must match Lucene's
//! category list, and the port's lowercasing must match Java's except where documented.

use super::read_testdata;
use crate::kuromoji::unicode::{self, GeneralCategory};

/// `java.lang.Character` type constants.
fn java_type(category: GeneralCategory) -> u8 {
    match category {
        GeneralCategory::Unassigned => 0,
        GeneralCategory::UppercaseLetter => 1,
        GeneralCategory::LowercaseLetter => 2,
        GeneralCategory::TitlecaseLetter => 3,
        GeneralCategory::ModifierLetter => 4,
        GeneralCategory::OtherLetter => 5,
        GeneralCategory::NonspacingMark => 6,
        GeneralCategory::EnclosingMark => 7,
        GeneralCategory::SpacingMark => 8,
        GeneralCategory::DecimalNumber => 9,
        GeneralCategory::LetterNumber => 10,
        GeneralCategory::OtherNumber => 11,
        GeneralCategory::SpaceSeparator => 12,
        GeneralCategory::LineSeparator => 13,
        GeneralCategory::ParagraphSeparator => 14,
        GeneralCategory::Control => 15,
        GeneralCategory::Format => 16,
        GeneralCategory::PrivateUse => 18,
        GeneralCategory::Surrogate => 19,
        GeneralCategory::DashPunctuation => 20,
        GeneralCategory::OpenPunctuation => 21,
        GeneralCategory::ClosePunctuation => 22,
        GeneralCategory::ConnectorPunctuation => 23,
        GeneralCategory::OtherPunctuation => 24,
        GeneralCategory::MathSymbol => 25,
        GeneralCategory::CurrencySymbol => 26,
        GeneralCategory::ModifierSymbol => 27,
        GeneralCategory::OtherSymbol => 28,
        GeneralCategory::InitialPunctuation => 29,
        GeneralCategory::FinalPunctuation => 30,
    }
}

/// What Lucene's `isPunctuation` says, derived from the JDK type in the golden.
fn lucene_is_punctuation(java_type: u8) -> bool {
    matches!(java_type, 12..=16 | 20..=30)
}

#[test]
fn every_code_unit() {
    let golden = read_testdata("golden/unicode.txt");
    let mut covered = 0u32;
    let mut mismatches = Vec::new();
    let mut report = |m: String| {
        if mismatches.len() < 30 {
            mismatches.push(m);
        } else {
            mismatches.push(String::new());
        }
    };
    for line in golden.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let lo = u32::from_str_radix(fields[0], 16).unwrap();
        let hi = u32::from_str_radix(fields[1], 16).unwrap();
        let expected_type: u8 = fields[2].parse().unwrap();
        assert_eq!(lo, covered, "golden runs must be contiguous");
        covered = hi + 1;
        for cu in lo..=hi {
            let category = unicode::category(cu as u16);
            let actual_type = java_type(category);
            if actual_type != expected_type {
                report(format!(
                    "U+{cu:04X}: type expected {expected_type}, got {actual_type} ({category:?})"
                ));
            }
            let punct = unicode::is_punctuation(category);
            if punct != lucene_is_punctuation(expected_type) {
                report(format!(
                    "U+{cu:04X}: isPunctuation expected {}, got {punct}",
                    !punct
                ));
            }
        }
    }
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

/// The port lowercases with alyze's pinned full Unicode mapping, Lucene with Java's simple
/// mapping. They are allowed to differ only where the full mapping expands to several code points
/// (U+0130 is the only unconditional case in Unicode's SpecialCasing); everywhere else they must
/// agree, so that analyzer output matches Elasticsearch for all ordinary text.
#[test]
fn lowercase_matches_java_except_expansions() {
    const ALLOWED: &[u32] = &[0x0130];
    let mut differing = Vec::new();
    for cp in 0..=0x10FFFFu32 {
        let Some(c) = char::from_u32(cp) else {
            continue;
        };
        let java = super::java_simple_lowercase(c).to_string();
        let port = crate::kuromoji::filter::lowercase_text(&c.to_string());
        if java != port && !ALLOWED.contains(&cp) {
            differing.push(format!("U+{cp:04X}: java {java:?}, port {port:?}"));
        }
    }
    assert!(
        differing.is_empty(),
        "{} code points lowercase differently:\n{}",
        differing.len(),
        differing[..differing.len().min(30)].join("\n")
    );
}
