//! Romanization against `golden/romaji.txt`: `ToStringUtil.getRomanization` (the reading-form
//! filter's romaji) and `KatakanaRomanizer` (the completion filter's keystrokes) for every line of
//! `cases/romaji.txt`.

use super::{escape, read_cases, read_testdata, unescape};
use crate::kuromoji::romaji;

struct Golden {
    hepburn: String,
    /// `None` when the input isn't katakana plus ASCII lowercase (the romanizer's precondition).
    keystrokes: Option<Vec<String>>,
}

fn read_golden() -> Vec<Golden> {
    let mut out: Vec<Golden> = Vec::new();
    for line in read_testdata("golden/romaji.txt").lines() {
        if let Some(index) = line.strip_prefix("# ") {
            assert_eq!(index.parse::<usize>().unwrap(), out.len());
            out.push(Golden {
                hepburn: String::new(),
                keystrokes: None,
            });
        } else if let Some(h) = line.strip_prefix("hepburn\t") {
            out.last_mut().unwrap().hepburn = unescape(h);
        } else if let Some(k) = line.strip_prefix("keystrokes\t") {
            if k != "!" {
                let fields: Vec<&str> = k.split('\t').collect();
                let n: usize = fields[0].parse().unwrap();
                assert_eq!(fields.len(), n + 1);
                out.last_mut().unwrap().keystrokes =
                    Some(fields[1..].iter().map(|s| unescape(s)).collect());
            }
        } else {
            panic!("bad line {line:?}");
        }
    }
    out
}

#[test]
fn hepburn() {
    let inputs = read_cases(&read_testdata("cases/romaji.txt"));
    let golden = read_golden();
    assert_eq!(inputs.len(), golden.len());
    let mut failures = Vec::new();
    for (input, g) in inputs.iter().zip(&golden) {
        let actual = romaji::hepburn(input);
        if actual != g.hepburn {
            failures.push(format!(
                "{}: expected {:?}, got {actual:?}",
                escape(input),
                g.hepburn
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

/// Keystroke lists are compared in order: Lucene's order is deterministic (longest match first,
/// alternatives in `romaji_map.txt` order) and the completion filter emits them in it.
#[test]
fn keystrokes() {
    let inputs = read_cases(&read_testdata("cases/romaji.txt"));
    let golden = read_golden();
    let mut failures = Vec::new();
    let mut checked = 0;
    for (input, g) in inputs.iter().zip(&golden) {
        let Some(expected) = &g.keystrokes else {
            continue;
        };
        checked += 1;
        let actual = romaji::keystrokes(input);
        if &actual != expected {
            failures.push(format!(
                "{}: expected {expected:?}, got {actual:?}",
                escape(input)
            ));
        }
    }
    assert!(checked > 100, "too few romanizable inputs");
    assert!(
        failures.is_empty(),
        "{} of {checked} inputs differ:\n{}",
        failures.len(),
        failures[..failures.len().min(20)].join("\n")
    );
}
