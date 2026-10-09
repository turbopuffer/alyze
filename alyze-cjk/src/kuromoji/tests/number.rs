//! `normalize_number` against `golden/numbers.txt`: Lucene's `JapaneseNumberFilter.normalizeNumber`
//! for every line of `cases/numbers.txt` (the token-level behaviour is covered by the `number`
//! chains in `golden`).

use super::{escape, read_cases, read_testdata, unescape};
use crate::kuromoji::filter::normalize_number;

#[test]
fn normalize() {
    let inputs = read_cases(&read_testdata("cases/numbers.txt"));
    let expected: Vec<String> = read_testdata("golden/numbers.txt")
        .lines()
        .map(unescape)
        .collect();
    assert_eq!(inputs.len(), expected.len());
    let mut failures = Vec::new();
    for (input, expected) in inputs.iter().zip(&expected) {
        let actual = normalize_number(input);
        if &actual != expected {
            failures.push(format!(
                "{}: expected {expected:?}, got {actual:?}",
                escape(input)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} inputs differ:\n{}",
        failures.len(),
        inputs.len(),
        failures[..failures.len().min(20)].join("\n")
    );
}

/// Inputs that don't parse come back unchanged, including the empty string.
#[test]
fn passthrough() {
    for s in ["", "abc", "一二三abc", "、", "十万円"] {
        assert_eq!(normalize_number(s), s);
    }
}
