//! User dictionary parsing: what Lucene's `UserDictionary.open` builds (goldens), plus the
//! duplicate handling Elasticsearch layers on top.

use super::read_testdata;
use crate::nori::{UserDictionary, UserDictionaryError};

/// Parses `golden/userdict.<name>.txt`: `empty`, `error\t<message>`, or entry lines.
enum Golden {
    Empty,
    Error(String),
    Entries(Vec<(String, u16, Option<Vec<String>>)>),
}

fn read_golden(name: &str) -> Golden {
    let contents = read_testdata(&format!("golden/userdict.{name}.txt"));
    let mut entries = Vec::new();
    for line in contents.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields[0] {
            "empty" => return Golden::Empty,
            "error" => return Golden::Error(super::unescape(fields[1])),
            "entry" => {
                assert_eq!(
                    fields[2].parse::<usize>().unwrap(),
                    entries.len(),
                    "ords in order"
                );
                entries.push((
                    super::unescape(fields[1]),
                    fields[3].parse().unwrap(),
                    (fields[4] != "-").then(|| fields[4].split('+').map(super::unescape).collect()),
                ));
            }
            other => panic!("unknown line {other:?}"),
        }
    }
    Golden::Entries(entries)
}

/// Lenient parsing reproduces Lucene exactly (it silently keeps the first of duplicate rules).
fn assert_matches_lucene(name: &str) {
    let rules = read_testdata(&format!("userdict/{name}.txt"));
    let golden = read_golden(name);
    let parsed = UserDictionary::parse(&rules, true);
    match golden {
        Golden::Empty => assert!(
            parsed.unwrap().is_empty(),
            "userdict/{name}.txt should be empty"
        ),
        Golden::Error(message) => {
            let err = parsed
                .err()
                .unwrap_or_else(|| panic!("userdict/{name}.txt should fail: {message}"));
            assert!(
                matches!(err, UserDictionaryError::SegmentationTooLong { .. }),
                "userdict/{name}.txt: {err}"
            );
        }
        Golden::Entries(expected) => {
            let dict = parsed.unwrap_or_else(|e| panic!("userdict/{name}.txt: {e}"));
            let actual: Vec<_> = dict
                .entries()
                .into_iter()
                .map(|e| (e.surface, e.right_id, e.segmentation))
                .collect();
            assert_eq!(actual, expected, "userdict/{name}.txt");
        }
    }
}

#[test]
fn lucene_test_dictionary() {
    assert_matches_lucene("lucene");
}

#[test]
fn elasticsearch_test_dictionary() {
    assert_matches_lucene("es");
}

#[test]
fn edge_dictionary() {
    assert_matches_lucene("edge");
}

#[test]
fn duplicates_lenient() {
    assert_matches_lucene("dups");
}

#[test]
fn invalid_segmentation() {
    assert_matches_lucene("invalid");
    let err = UserDictionary::parse("세종시 세종 시청", false).unwrap_err();
    assert_eq!(
        err,
        UserDictionaryError::SegmentationTooLong {
            rule: "세종시 세종 시청".to_owned(),
            line: 1
        }
    );
}

#[test]
fn only_comments() {
    assert_matches_lucene("empty");
    assert!(UserDictionary::parse("", false).unwrap().is_empty());
    assert!(
        UserDictionary::parse("# just a comment\n\n", false)
            .unwrap()
            .is_empty()
    );
}

/// Elasticsearch (8.13+, `NoriAnalysisTests.testNoriAnalyzerDuplicateUserDictRule`) rejects a
/// duplicate rule, naming the term and its line.
#[test]
fn duplicates_strict() {
    let rules = read_testdata("userdict/dups.txt");
    let err = UserDictionary::parse(&rules, false).unwrap_err();
    assert_eq!(
        err,
        UserDictionaryError::Duplicate {
            surface: "세종".to_owned(),
            line: 5
        }
    );
    assert!(err.to_string().contains("[세종]") && err.to_string().contains("[5]"));

    // The ES test's rule list (no leading comment line).
    let err = UserDictionary::parse("c++\nC쁠쁠\n세종\n세종\n세종시 세종 시", false).unwrap_err();
    assert_eq!(
        err,
        UserDictionaryError::Duplicate {
            surface: "세종".to_owned(),
            line: 4
        }
    );
    // A duplicate surface form with a different segmentation is still a duplicate.
    let err = UserDictionary::parse("세종시 세종 시\n세종시", false).unwrap_err();
    assert!(matches!(
        err,
        UserDictionaryError::Duplicate { line: 2, .. }
    ));
}

/// Lucene's `TestKoreanTokenizer.testDuplicate`: with duplicates, the dictionary has exactly as
/// many entries as distinct surface forms.
#[test]
fn duplicates_collapse() {
    for rules in [
        "c++\nC쁠쁠\n세종\n세종\n세종시 세종 시",
        "c++\nC쁠쁠\n세종\n세종\n세종시 세종 시\n세종시 세종 시",
    ] {
        let dict = UserDictionary::parse(rules, true).unwrap();
        assert_eq!(dict.entries().len(), 4);
    }
}

/// Lucene's `TestUserDictionary.testLookup`, on its test dictionary.
#[test]
fn lucene_lookup() {
    let dict = UserDictionary::parse(&read_testdata("userdict/lucene.txt"), false).unwrap();
    let entries = dict.entries();
    let find = |s: &str| {
        entries
            .iter()
            .find(|e| e.surface == s)
            .unwrap_or_else(|| panic!("{s}"))
    };
    assert_eq!(find("세종").segmentation, None);
    assert_eq!(
        find("세종시").segmentation,
        Some(vec!["세종".to_owned(), "시".to_owned()])
    );
    assert_eq!(find("c++").segmentation, None);
    assert_eq!(entries.len(), 9);
}

/// Right connection ids follow Lucene: 3535 after a Hangul syllable with a final consonant, 3534
/// after one without, 3533 after anything else, judged on the rule's last character (including a
/// segmentation's last part).
#[test]
fn right_ids() {
    let dict = UserDictionary::parse(
        "세종\n서울\nc++\n세종시 세종 시\n트위터\n인스타그램 인스타 그램",
        false,
    )
    .unwrap();
    let entries = dict.entries();
    let right_id = |s: &str| entries.iter().find(|e| e.surface == s).unwrap().right_id;
    assert_eq!(right_id("세종"), 3535);
    assert_eq!(right_id("서울"), 3535);
    assert_eq!(right_id("c++"), 3533);
    assert_eq!(right_id("세종시"), 3534);
    assert_eq!(right_id("트위터"), 3534);
    assert_eq!(right_id("인스타그램"), 3535);
}

/// Lucene quirks, as the `userdict.edge.txt` golden shows: the right id comes from the rule's last
/// character even when that is a trailing space left by comment removal, `hasCoda` runs on
/// compatibility jamo too, and a segmentation only contributes its parts' lengths.
#[test]
fn lucene_quirks() {
    let dict = UserDictionary::parse(
        "세종시 세종 시   # comment\nㅋㅋㅋ\n서울특별시 서울 시\n세종시청 세종 시청",
        false,
    )
    .unwrap();
    let entries = dict.entries();
    let find = |s: &str| {
        entries
            .iter()
            .find(|e| e.surface == s)
            .unwrap_or_else(|| panic!("{s}"))
    };
    assert_eq!(
        find("세종시").right_id,
        3533,
        "trailing space after the comment"
    );
    assert_eq!(
        find("ㅋㅋㅋ").right_id,
        3535,
        "jamo: negative Java remainder is never 0"
    );
    assert_eq!(
        find("세종시청").right_id,
        3535,
        "last part's last character 청 has a coda"
    );
    assert_eq!(
        find("서울특별시").segmentation,
        Some(vec!["서울".to_owned(), "특".to_owned()])
    );
}

/// `#` starts a comment anywhere on a line; whitespace-only lines are skipped; segmentation parts
/// are whitespace-separated.
#[test]
fn comments_and_whitespace() {
    let dict =
        UserDictionary::parse("# head\n\n  \n세종시 세종 시 # tail\nc++\t\n", false).unwrap();
    let surfaces: Vec<_> = dict.entries().into_iter().map(|e| e.surface).collect();
    assert_eq!(surfaces, ["c++", "세종시"]);
}

/// Splitting follows Java, not Unicode: `\s` is ASCII, so a no-break space is part of a word;
/// `String.trim` skips lines of characters at or below U+0020; a leading separator makes
/// `String.split` yield an empty surface form, which Lucene rejects; lines end at a lone CR too.
#[test]
fn java_whitespace_semantics() {
    assert_matches_lucene("whitespace");
    let dict = UserDictionary::parse("서울\u{a0}특별시\n\u{b}\nx\u{c}y\ra\r\nb\n", false).unwrap();
    let entries = dict.entries();
    let surfaces: Vec<_> = entries.iter().map(|e| e.surface.as_str()).collect();
    assert_eq!(surfaces, ["a", "b", "x", "서울\u{a0}특별시"]);
    assert_eq!(entries[2].segmentation, Some(vec!["x".to_owned()]));

    // A leading tab: Java splits off an empty first token and then fails the length check.
    let err = UserDictionary::parse("세종\n\tc++\n", false).unwrap_err();
    assert_eq!(
        err,
        UserDictionaryError::SegmentationTooLong {
            rule: "c++".to_owned(),
            line: 2
        }
    );
}

/// A segmentation whose parts cut a surrogate pair is rejected (Lucene emits half characters).
#[test]
fn segmentation_inside_surrogate_pair() {
    for rules in ["😀a x y", "😀a x", "a😀b ab c"] {
        let err = UserDictionary::parse(rules, true).unwrap_err();
        assert!(
            matches!(
                err,
                UserDictionaryError::SegmentationSplitsCharacter { line: 1, .. }
            ),
            "{rules:?}: {err}"
        );
    }
    let dict = UserDictionary::parse("😀a 😀 a\na😀b a 😀", false).unwrap();
    let entries = dict.entries();
    assert_eq!(
        entries[0].segmentation,
        Some(vec!["a".to_owned(), "😀".to_owned()])
    );
    assert_eq!(
        entries[1].segmentation,
        Some(vec!["😀".to_owned(), "a".to_owned()])
    );
}
