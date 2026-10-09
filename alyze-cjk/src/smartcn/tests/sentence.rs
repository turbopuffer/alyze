//! Sentence boundaries must match the JDK's `BreakIterator` on every case file.

use super::{escape, read_cases, read_sentence_golden, read_testdata};
use crate::smartcn::sentence::for_each_boundary;

fn boundaries(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for_each_boundary(text, |b| {
        assert!(
            text.is_char_boundary(b),
            "boundary {b} not on a char boundary"
        );
        if let Some(&last) = out.last() {
            assert!(b > last, "boundaries must be strictly increasing");
        }
        out.push(b);
        true
    });
    out
}

fn check(case: &str) {
    let inputs = read_cases(&read_testdata(&format!("cases/{case}.txt")));
    let golden = read_sentence_golden(&read_testdata(&format!("golden/{case}.sentences")));
    assert_eq!(inputs.len(), golden.len());
    let mut failures = 0;
    let mut report = String::new();
    for (i, (input, expected)) in inputs.iter().zip(&golden).enumerate() {
        let actual = boundaries(input);
        if &actual == expected {
            continue;
        }
        failures += 1;
        if failures <= 5 {
            let mut shown = escape(input);
            if shown.chars().count() > 300 {
                shown = shown.chars().take(300).collect::<String>() + "…";
            }
            report +=
                &format!("case {i}: {shown}\n  expected {expected:?}\n  actual   {actual:?}\n");
        }
    }
    assert!(
        failures == 0,
        "cases/{case}.txt: {failures} of {} cases split differently from the JDK\n{report}",
        inputs.len()
    );
}

#[test]
fn upstream() {
    check("upstream");
}

#[test]
fn edge() {
    check("edge");
}

#[test]
fn fuzz() {
    check("fuzz");
}

#[test]
fn wiki_zh() {
    check("wiki_zh");
}

#[test]
fn empty_text_has_one_boundary() {
    assert_eq!(boundaries(""), [0]);
}
