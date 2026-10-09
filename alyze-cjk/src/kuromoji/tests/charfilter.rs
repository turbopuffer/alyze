//! Char filters against the `charfilter.<case>.<name>.txt` goldens: the filtered text and the
//! offset map back to the original input, for every input of the case files.

use super::{CharFilter, escape, read_cases, read_testdata, unescape};

struct Golden {
    text: String,
    /// `(filtered byte offset, original byte offset)` at every code-point boundary, ascending.
    map: Vec<(usize, usize)>,
}

fn read_golden(case: &str, name: &str) -> Vec<Golden> {
    let contents = read_testdata(&format!("golden/charfilter.{case}.{name}.txt"));
    let mut out: Vec<Golden> = Vec::new();
    for line in contents.lines() {
        if let Some(index) = line.strip_prefix("# ") {
            assert_eq!(index.parse::<usize>().unwrap(), out.len());
            out.push(Golden {
                text: String::new(),
                map: Vec::new(),
            });
        } else if let Some(text) = line.strip_prefix("text\t") {
            out.last_mut().unwrap().text = unescape(text);
        } else if let Some(map) = line.strip_prefix("map") {
            out.last_mut().unwrap().map = map
                .split('\t')
                .filter(|s| !s.is_empty())
                .map(|pair| {
                    let (f, o) = pair.split_once(':').unwrap();
                    (f.parse().unwrap(), o.parse().unwrap())
                })
                .collect();
        } else {
            panic!("bad line {line:?}");
        }
    }
    out
}

fn check(case: &str, name: &str) {
    let inputs = read_cases(&read_testdata(&format!("cases/{case}.txt")));
    let golden = read_golden(case, name);
    assert_eq!(inputs.len(), golden.len());
    let filter = CharFilter::named(name);
    let mut failures = Vec::new();
    for (i, (input, g)) in inputs.iter().zip(&golden).enumerate() {
        let filtered = filter.apply(input);
        if filtered.text() != g.text {
            failures.push(format!(
                "case {i}: text\n  input:    {}\n  expected: {}\n  actual:   {}",
                escape(input),
                escape(&g.text),
                escape(filtered.text())
            ));
            continue;
        }
        let boundaries: Vec<usize> = g.text.char_indices().map(|(b, _)| b).collect();
        assert_eq!(
            g.map.iter().map(|m| m.0).collect::<Vec<_>>(),
            boundaries
                .iter()
                .copied()
                .chain(std::iter::once(g.text.len()))
                .collect::<Vec<_>>(),
            "case {i}: golden map keys are the code-point boundaries"
        );
        for &(filtered_offset, original) in &g.map {
            let actual = filtered.correct_offset(filtered_offset);
            if actual != original {
                failures.push(format!(
                    "case {i}: correct_offset({filtered_offset}) = {actual}, expected {original}\n  input: {}",
                    escape(input)
                ));
                break;
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ for charfilter.{case}.{name}:\n{}",
        failures.len(),
        inputs.len(),
        failures[..failures.len().min(5)].join("\n")
    );
}

macro_rules! charfilter_tests {
    ($( $test:ident = ($case:literal, $name:literal), )*) => {
        $(
            #[test]
            fn $test() {
                check($case, $name);
            }
        )*
    };
}

charfilter_tests! {
    itermark_upstream = ("upstream", "itermark"),
    itermark_kanji_upstream = ("upstream", "itermark_kanji"),
    itermark_kana_upstream = ("upstream", "itermark_kana"),
    width_upstream = ("upstream", "width"),
    itermark_edge = ("edge", "itermark"),
    itermark_kanji_edge = ("edge", "itermark_kanji"),
    itermark_kana_edge = ("edge", "itermark_kana"),
    width_edge = ("edge", "width"),
    itermark_long = ("long", "itermark"),
    width_long = ("long", "width"),
    itermark_fuzz = ("fuzz", "itermark"),
    width_fuzz = ("fuzz", "width"),
}

/// The iteration-mark filter never changes the length, so its offset map is the identity; with
/// both flags off it is the identity on the text too.
#[test]
fn iteration_mark_preserves_length() {
    for input in read_cases(&read_testdata("cases/edge.txt")) {
        let filtered = CharFilter::named("itermark").apply(&input);
        assert_eq!(filtered.text().chars().count(), input.chars().count());
        for (b, _) in filtered.text().char_indices() {
            assert_eq!(
                filtered.correct_offset(b),
                super::utf16_to_byte_offset(&input, filtered.text()[..b].encode_utf16().count())
            );
        }
        let none = CharFilter::named("itermark_none").apply(&input);
        assert_eq!(none.text(), input);
    }
}
