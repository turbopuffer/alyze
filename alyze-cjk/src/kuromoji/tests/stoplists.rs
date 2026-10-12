//! The default stop tags and stop words embedded in the port must be exactly the jar's
//! `stoptags.txt` and `stopwords.txt` (`golden/stoplists.txt`).

use super::{read_testdata, unescape};
use crate::kuromoji::filter::{DEFAULT_STOP_TAGS, DEFAULT_STOP_WORDS, StopTags, StopWords};

fn golden() -> (Vec<String>, Vec<String>) {
    let mut tags = Vec::new();
    let mut words = Vec::new();
    for line in read_testdata("golden/stoplists.txt").lines() {
        let (kind, value) = line.split_once('\t').unwrap();
        match kind {
            "tag" => tags.push(unescape(value)),
            "word" => words.push(unescape(value)),
            other => panic!("bad line kind {other:?}"),
        }
    }
    (tags, words)
}

#[test]
fn default_stop_tags() {
    let (tags, _) = golden();
    let mut ours: Vec<String> = DEFAULT_STOP_TAGS.iter().map(|s| s.to_string()).collect();
    ours.sort();
    assert_eq!(ours, tags);
    let set = StopTags::defaults();
    for tag in &tags {
        assert!(set.contains(tag));
    }
    // Whole-tag matching, like Lucene: a prefix is not a match.
    assert!(!set.contains("助詞-格助詞-一般-x"));
    assert!(!set.contains("名詞"));
    assert!(set.contains("助詞"));
}

#[test]
fn default_stop_words() {
    let (_, words) = golden();
    let mut ours: Vec<String> = DEFAULT_STOP_WORDS.iter().map(|s| s.to_string()).collect();
    ours.sort();
    assert_eq!(ours, words);
    let set = StopWords::japanese();
    for word in &words {
        assert!(set.contains(word));
    }
    assert!(!set.contains("寿司"));
}

/// Lucene's default set is case-insensitive; a custom set is whatever it was built as.
#[test]
fn stop_words_case() {
    let sensitive = StopWords::new(["Culture", "test"], false);
    assert!(sensitive.contains("Culture") && !sensitive.contains("culture"));
    assert!(sensitive.contains("test") && !sensitive.contains("TEST"));
    let insensitive = StopWords::new(["Culture", "test"], true);
    assert!(insensitive.contains("culture") && insensitive.contains("CULTURE"));
    assert!(insensitive.contains("TEST"));
    assert!(StopWords::new(Vec::<String>::new(), false).is_empty());
}
