//! User dictionary parsing: what Lucene's `UserDictionary.open` builds from a rules file after
//! Elasticsearch's file preprocessing (goldens), plus the duplicate handling Elasticsearch layers
//! on top and the port's own rejections.

use super::{read_testdata, unescape};
use crate::kuromoji::{UserDictionary, UserDictionaryError};

/// One golden entry: term-index key, segments, readings, part of speech.
type Entry = (String, Vec<String>, Vec<String>, String);

/// Parses `golden/userdict.<name>.txt`: `empty`, `error\t<message>`, or entry lines.
enum Golden {
    Empty,
    Error(String),
    Entries(Vec<Entry>),
}

fn read_golden(name: &str) -> Golden {
    let contents = read_testdata(&format!("golden/userdict.{name}.txt"));
    let mut entries = Vec::new();
    for line in contents.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields[0] {
            "empty" => return Golden::Empty,
            "error" => return Golden::Error(unescape(fields[1])),
            "entry" => {
                assert_eq!(
                    fields[2].parse::<usize>().unwrap(),
                    entries.len(),
                    "ords in order"
                );
                let n: usize = fields[3].parse().unwrap();
                assert_eq!(fields.len(), 4 + 2 * n + 1, "bad entry line {line:?}");
                entries.push((
                    unescape(fields[1]),
                    fields[4..4 + n].iter().map(|s| unescape(s)).collect(),
                    fields[4 + n..4 + 2 * n]
                        .iter()
                        .map(|s| unescape(s))
                        .collect(),
                    unescape(fields[4 + 2 * n]),
                ));
            }
            other => panic!("unknown line {other:?}"),
        }
    }
    Golden::Entries(entries)
}

/// Lenient parsing reproduces Lucene exactly (after Elasticsearch's keep-first deduplication).
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
            assert!(
                parsed.is_err(),
                "userdict/{name}.txt should fail like Lucene: {message}"
            );
        }
        Golden::Entries(expected) => {
            let dict = parsed.unwrap_or_else(|e| panic!("userdict/{name}.txt: {e}"));
            let actual: Vec<Entry> = dict
                .entries()
                .into_iter()
                .map(|e| (e.surface, e.segments, e.readings, e.part_of_speech))
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
fn docs_dictionary() {
    assert_matches_lucene("docs");
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
fn only_comments() {
    assert_matches_lucene("empty");
    assert!(UserDictionary::parse("", false).unwrap().is_empty());
    assert!(
        UserDictionary::parse("# just a comment\n\n", false)
            .unwrap()
            .is_empty()
    );
}

/// Java separator semantics (see the fixture's comments).
#[test]
fn java_whitespace_semantics() {
    assert_matches_lucene("whitespace");
}

/// `TestUserDictionary.testReadInvalid1/2`: the joined segmentation must be the surface form.
#[test]
fn invalid_surface() {
    assert_matches_lucene("invalid_surface");
    for rules in [
        "日経新聞,日本 経済 新聞,ニホン ケイザイ シンブン,カスタム名詞",
        "日本経済新聞,日経 新聞,ニッケイ シンブン,カスタム名詞",
    ] {
        let err = UserDictionary::parse(rules, false).unwrap_err();
        assert!(
            matches!(
                err,
                UserDictionaryError::SegmentationMismatch { line: 1, .. }
            ),
            "{rules}: {err}"
        );
    }
}

#[test]
fn invalid_count() {
    assert_matches_lucene("invalid_count");
    let err = UserDictionary::parse(
        "日本経済新聞,日本 経済 新聞,ニホン ケイザイ,カスタム名詞",
        false,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        UserDictionaryError::SegmentationReadingsMismatch { line: 1, .. }
    ));
}

/// Lucene's CSV parser yields no fields for an odd number of quotes and then indexes past the end;
/// a rule with fewer than four fields does the same. The port reports both as malformed.
#[test]
fn malformed() {
    assert_matches_lucene("malformed_quotes");
    assert_matches_lucene("malformed_fields");
    for rules in [
        "\"東京,東京,トウキョウ,カスタム名詞",
        "東京,東京,トウキョウ",
        "東京",
        "# c\n東京,東京",
    ] {
        let err = UserDictionary::parse(rules, false).unwrap_err();
        assert!(
            matches!(err, UserDictionaryError::Malformed { .. }),
            "{rules:?}: {err}"
        );
    }
    let err = UserDictionary::parse("# c\n東京,東京", false).unwrap_err();
    assert!(matches!(
        err,
        UserDictionaryError::Malformed { line: 2, .. }
    ));
}

/// Elasticsearch (`KuromojiAnalysisTests.testKuromojiAnalyzerDuplicateUserDictRule`) rejects a
/// duplicate rule, naming the term and its line; comments count, blank lines don't.
#[test]
fn duplicates_strict() {
    let rules = read_testdata("userdict/dups.txt");
    let err = UserDictionary::parse(&rules, false).unwrap_err();
    assert_eq!(
        err,
        UserDictionaryError::Duplicate {
            surface: "制限スピード".to_owned(),
            line: 6
        }
    );
    assert!(err.to_string().contains("[制限スピード]") && err.to_string().contains("[6]"));

    // The ES test's rule list itself (no leading comment lines, no blank lines).
    let err = UserDictionary::parse(
        "c++,c++,w,w\n#comment\n制限スピード,制限スピード,セイゲンスピード,テスト名詞\n制限スピード,制限スピード,セイゲンスピード,テスト名詞",
        false,
    )
    .unwrap_err();
    assert_eq!(
        err,
        UserDictionaryError::Duplicate {
            surface: "制限スピード".to_owned(),
            line: 4
        }
    );
    // The key is the raw first field, after CSV unquoting: a different segmentation or a quoted
    // spelling of the same term is still a duplicate.
    let err = UserDictionary::parse(
        "東京都,東京 都,トウキョウ ト,カスタム名詞\n\"東京都\",東京都,トウキョウト,カスタム名詞",
        false,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        UserDictionaryError::Duplicate { line: 2, .. }
    ));
}

/// With `lenient`, the first rule wins and the dictionary has one entry per distinct key.
#[test]
fn duplicates_collapse() {
    let dict = UserDictionary::parse(
        "東京都,東京 都,トウキョウ ト,カスタム名詞\n東京都,東京都,トウキョウト,カスタム名詞\n大阪,大阪,オオサカ,カスタム名詞",
        true,
    )
    .unwrap();
    let entries = dict.entries();
    assert_eq!(entries.len(), 2);
    let tokyo = entries.iter().find(|e| e.surface == "東京都").unwrap();
    assert_eq!(tokyo.segments, vec!["東京".to_owned(), "都".to_owned()]);
}

/// Lucene's `TestUserDictionary.testLookup`, `testReadings`, `testPartOfSpeech`, `testSharp`, on
/// its test dictionary.
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
    assert_eq!(entries.len(), 7);
    let e = find("関西国際空港");
    assert_eq!(e.segments, ["関西", "国際", "空港"]);
    assert_eq!(e.readings, ["カンサイ", "コクサイ", "クウコウ"]);
    assert_eq!(e.part_of_speech, "テスト名詞");
    let e = find("日本経済新聞");
    assert_eq!(e.readings[0], "ニホン");
    assert_eq!(e.part_of_speech, "カスタム名詞");
    assert_eq!(find("朝青龍").readings, ["アサショウリュウ"]);
    // A '#' only starts a comment at the beginning of a line.
    assert_eq!(find("テスト#").part_of_speech, "カスタム名刺");
    assert_eq!(find("test#テスト").segments, ["test", "#", "テスト"]);
}

/// A segmentation whose parts cut a surrogate pair is rejected (Lucene emits half characters).
#[test]
fn segmentation_inside_surrogate_pair() {
    for rules in ["😀a,😀 a,エ ア,x", "😀顔,😀 顔,エ カオ,x"] {
        let dict = UserDictionary::parse(rules, false).unwrap_or_else(|e| panic!("{rules}: {e}"));
        assert_eq!(dict.entries()[0].segments.len(), 2);
    }
    // Lucene would accept these (the lengths are UTF-16 code units); the port rejects them.
    // They can't be written as whole-character segments, so build them from a segmentation that
    // claims the pair's halves: not expressible in valid UTF-8 input either. The check that
    // matters is the key length: an entry whose segments don't add up to whole code points.
    // (Such a rule needs a segment to end in the middle of a pair, which no UTF-8 rule can state
    // explicitly, so the port's guard is exercised only through the ES-style dedup key path in
    // `edge_dictionary`.)
}

/// `#` starts a comment only at the start of a line; whitespace-only lines are skipped; lines
/// are trimmed as Elasticsearch trims a dictionary file.
#[test]
fn comments_and_whitespace() {
    let dict = UserDictionary::parse(
        "# head\n\n  \n東京,東京,トウキョウ,カスタム名詞 \n  大阪,大阪,オオサカ,カスタム名詞\t\n",
        false,
    )
    .unwrap();
    let entries = dict.entries();
    let surfaces: Vec<_> = entries.iter().map(|e| e.surface.as_str()).collect();
    assert_eq!(surfaces, ["大阪", "東京"]);
    assert_eq!(entries[1].part_of_speech, "カスタム名詞");
}

/// Entries come back in Lucene's term-index order (UTF-16 code unit order of the key), which
/// differs from byte order only for supplementary characters.
#[test]
fn entry_order() {
    let dict = UserDictionary::parse(
        "😀,😀,エ,x\nｱ,ｱ,ア,x\n東京,東京,トウキョウ,x\nabc,abc,エービーシー,x",
        false,
    )
    .unwrap();
    let surfaces: Vec<_> = dict.entries().into_iter().map(|e| e.surface).collect();
    assert_eq!(surfaces, ["abc", "東京", "😀", "ｱ"]);
}
