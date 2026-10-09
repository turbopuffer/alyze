//! The converted dictionaries must contain exactly what Lucene's do (checksums over a canonical
//! dump) and answer lookups identically (probes).

use super::{NULL, escape, read_testdata, unescape};
use crate::kuromoji::char_def::CharClass;
use crate::kuromoji::dict::{ConnectionCosts, TokenInfoDict, UnknownDict, WordId};
use crate::testutil::{FNV_OFFSET, fnv1a};

struct WordLine {
    left_id: u16,
    right_id: u16,
    cost: i16,
    part_of_speech: String,
    base_form: Option<String>,
    reading: Option<String>,
    pronunciation: Option<String>,
    inflection_type: Option<String>,
    inflection_form: Option<String>,
}

#[derive(Default)]
struct Golden {
    terms: usize,
    words: usize,
    checksum: u64,
    unk: Vec<(CharClass, u16, u16, i16, String)>,
    costs_forward: usize,
    costs_backward: usize,
    costs_checksum: u64,
    /// `term` probes: surface → the words, or `None` for a miss.
    terms_probes: Vec<(String, Option<Vec<WordLine>>)>,
    /// `prefix` probes: text → the dictionary terms that are prefixes of it, shortest first.
    prefix_probes: Vec<(String, Vec<String>)>,
    /// `cost` probes: (right id, left id, cost).
    cost_probes: Vec<(u16, u16, i16)>,
}

fn optional(field: &str) -> Option<String> {
    (field != NULL).then(|| unescape(field))
}

fn parse_word_line(fields: &[&str]) -> WordLine {
    WordLine {
        left_id: fields[0].parse().unwrap(),
        right_id: fields[1].parse().unwrap(),
        cost: fields[2].parse().unwrap(),
        part_of_speech: unescape(fields[3]),
        base_form: optional(fields[4]),
        reading: optional(fields[5]),
        pronunciation: optional(fields[6]),
        inflection_type: optional(fields[7]),
        inflection_form: optional(fields[8]),
    }
}

fn read_golden() -> Golden {
    let mut g = Golden::default();
    for line in read_testdata("golden/dict.txt").lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields[0] {
            "tokeninfo.terms" => g.terms = fields[1].parse().unwrap(),
            "tokeninfo.words" => g.words = fields[1].parse().unwrap(),
            "tokeninfo.checksum" => g.checksum = u64::from_str_radix(fields[1], 16).unwrap(),
            "unk" => g.unk.push((
                CharClass::from_name(fields[1]).unwrap(),
                fields[2].parse().unwrap(),
                fields[3].parse().unwrap(),
                fields[4].parse().unwrap(),
                unescape(fields[5]),
            )),
            "costs.forward" => g.costs_forward = fields[1].parse().unwrap(),
            "costs.backward" => g.costs_backward = fields[1].parse().unwrap(),
            "costs.checksum" => g.costs_checksum = u64::from_str_radix(fields[1], 16).unwrap(),
            "miss" => g.terms_probes.push((unescape(fields[1]), None)),
            "term" => g.terms_probes.push((
                unescape(fields[1]),
                Some(Vec::with_capacity(fields[2].parse().unwrap())),
            )),
            "word" => {
                let (surface, words) = g.terms_probes.last_mut().unwrap();
                assert_eq!(*surface, unescape(fields[1]));
                let words = words.as_mut().unwrap();
                assert_eq!(fields[2].parse::<usize>().unwrap(), words.len());
                words.push(parse_word_line(&fields[3..]));
            }
            "prefix" => g.prefix_probes.push((
                unescape(fields[1]),
                fields[2..].iter().map(|s| unescape(s)).collect(),
            )),
            "cost" => g.cost_probes.push((
                fields[1].parse().unwrap(),
                fields[2].parse().unwrap(),
                fields[3].parse().unwrap(),
            )),
            other => panic!("unknown line {other:?}"),
        }
    }
    g
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn nullable(s: Option<&str>) -> String {
    s.map(escape).unwrap_or_else(|| NULL.to_owned())
}

/// The canonical word line the Java generator checksums, built from the port's view of a word.
fn canonical_line(dict: &TokenInfoDict, surface: &[u16], id: WordId) -> String {
    let info = dict.word(id);
    let text = String::from_utf16(surface).unwrap();
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
        escape(&text),
        info.left_id,
        info.right_id,
        info.cost,
        escape(dict.part_of_speech(id)),
        nullable(dict.base_form(id, surface).as_deref()),
        escape(&dict.reading(id, surface)),
        escape(&dict.pronunciation(id, surface)),
        nullable(dict.inflection_type(id)),
        nullable(dict.inflection_form(id)),
    )
}

fn expected_line(surface: &str, w: &WordLine) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
        escape(surface),
        w.left_id,
        w.right_id,
        w.cost,
        escape(&w.part_of_speech),
        nullable(w.base_form.as_deref()),
        nullable(w.reading.as_deref()),
        nullable(w.pronunciation.as_deref()),
        nullable(w.inflection_type.as_deref()),
        nullable(w.inflection_form.as_deref()),
    )
}

/// Every term in lookup order, every word in dictionary order, hashed exactly like the generator.
#[test]
fn token_info_dictionary_complete() {
    let golden = read_golden();
    let dict = TokenInfoDict::get();
    let mut terms = 0usize;
    let mut words = 0usize;
    let mut checksum = FNV_OFFSET;
    let mut last: Vec<u16> = Vec::new();
    dict.for_each_term(|surface, ids| {
        assert!(
            terms == 0 || last.as_slice() < surface,
            "terms out of order"
        );
        last = surface.to_vec();
        terms += 1;
        for id in ids {
            words += 1;
            checksum = fnv1a(checksum, canonical_line(dict, surface, id).as_bytes());
        }
    });
    assert_eq!(terms, golden.terms, "term count");
    assert_eq!(words, golden.words, "word count");
    assert_eq!(checksum, golden.checksum, "token-info dictionary checksum");
}

/// mecab-ipadic's left and right ids are always equal (Lucene's builder asserts it).
#[test]
fn left_and_right_ids_agree() {
    let dict = TokenInfoDict::get();
    let mut checked = 0;
    dict.for_each_term(|_, ids| {
        for id in ids {
            let info = dict.word(id);
            assert_eq!(info.left_id, info.right_id);
            checked += 1;
        }
    });
    assert!(checked > 300_000);
}

#[test]
fn unknown_dictionary_complete() {
    let golden = read_golden();
    let dict = UnknownDict::get();
    let mut actual = Vec::new();
    for &class in CharClass::ALL {
        for w in dict.words(class) {
            actual.push((
                class,
                w.left_id,
                w.right_id,
                w.cost,
                w.part_of_speech.to_owned(),
            ));
        }
    }
    assert_eq!(actual, golden.unk);
}

#[test]
fn connection_costs_complete() {
    let golden = read_golden();
    let costs = ConnectionCosts::get();
    assert_eq!(
        costs.dimensions(),
        (golden.costs_forward, golden.costs_backward),
        "(right ids, left ids)"
    );
    let mut checksum = FNV_OFFSET;
    for right in 0..golden.costs_forward {
        for left in 0..golden.costs_backward {
            let cost = costs.cost(right as u16, left as u16);
            checksum = fnv1a(checksum, &cost.to_le_bytes());
        }
    }
    assert_eq!(checksum, golden.costs_checksum, "connection costs checksum");
}

#[test]
fn term_probes() {
    let golden = read_golden();
    let dict = TokenInfoDict::get();
    for (surface, expected) in &golden.terms_probes {
        let units = utf16(surface);
        let actual = dict.lookup(&units);
        match expected {
            None => assert!(
                actual.is_none(),
                "{surface:?} should not be a dictionary term"
            ),
            Some(expected) => {
                let ids =
                    actual.unwrap_or_else(|| panic!("{surface:?} should be a dictionary term"));
                let actual_lines: Vec<String> =
                    ids.map(|id| canonical_line(dict, &units, id)).collect();
                let expected_lines: Vec<String> =
                    expected.iter().map(|w| expected_line(surface, w)).collect();
                assert_eq!(actual_lines, expected_lines, "words of {surface:?}");
            }
        }
    }
}

#[test]
fn prefix_probes() {
    let golden = read_golden();
    let dict = TokenInfoDict::get();
    for (text, expected) in &golden.prefix_probes {
        let units = utf16(text);
        let mut actual = Vec::new();
        dict.for_each_prefix(&units, |len, ids| {
            assert!(!ids.is_empty(), "term without words");
            actual.push(String::from_utf16(&units[..len]).unwrap());
        });
        assert_eq!(&actual, expected, "dictionary prefixes of {text:?}");
    }
}

#[test]
fn cost_probes() {
    let golden = read_golden();
    let costs = ConnectionCosts::get();
    for &(right, left, expected) in &golden.cost_probes {
        assert_eq!(costs.cost(right, left), expected, "cost({right}, {left})");
    }
}
