use super::*;

#[derive(Debug, PartialEq, Eq)]
struct OwnedToken {
    text: String,
    position: usize,
    position_length: usize,
    byte_range: Range<usize>,
    input_index: usize,
}

impl From<Token<'_>> for OwnedToken {
    fn from(token: Token<'_>) -> Self {
        Self {
            text: token.text.to_owned(),
            position: token.position,
            position_length: token.position_length,
            byte_range: token.byte_range,
            input_index: token.input_index,
        }
    }
}

fn collect(
    analyzer: Analyzer<'_>,
    inputs: &[&str],
    buffer: &mut ReusableBuffer,
) -> Vec<OwnedToken> {
    let mut tokens = Vec::new();
    analyzer.analyze_inputs(inputs.iter().copied(), buffer, |token| {
        tokens.push(token.into());
        true
    });
    tokens
}

fn analyzers() -> Vec<(Analyzer<'static>, &'static str)> {
    vec![
        (Analyzer::new(super::tests::opts()), "Hello world"),
        #[cfg(feature = "smartcn")]
        (
            Analyzer::smartcn(Default::default()),
            "我购买了道具和服装。",
        ),
        #[cfg(feature = "nori")]
        (Analyzer::nori(Default::default()), "한국어를 분석합니다"),
        #[cfg(feature = "kuromoji")]
        (Analyzer::kuromoji(Default::default()), "日本語を解析します"),
    ]
}

#[test]
fn cancellation_stops_before_consuming_the_next_input() {
    for (analyzer, input) in analyzers() {
        let mut buffer = ReusableBuffer::new();
        let inputs = std::iter::once(input).chain(std::iter::once_with(|| {
            panic!("read another input after cancellation")
        }));
        let mut calls = 0;
        analyzer.analyze_inputs(inputs, &mut buffer, |_| {
            calls += 1;
            false
        });
        assert_eq!(calls, 1);
        assert_eq!(
            collect(analyzer, &[input], &mut buffer),
            collect(analyzer, &[input], &mut ReusableBuffer::new())
        );
    }
}

#[test]
fn buffers_can_be_reused_reset_and_cloned_across_pipelines() {
    let mut buffer = ReusableBuffer::new();
    for _ in 0..2 {
        for (analyzer, input) in analyzers() {
            let expected = collect(analyzer, &[input], &mut ReusableBuffer::new());
            assert!(!expected.is_empty());
            assert_eq!(collect(analyzer, &[input], &mut buffer), expected);
            assert_eq!(collect(analyzer, &[input], &mut buffer.clone()), expected);
            buffer.reset_keep_stemming_cache();
            assert_eq!(collect(analyzer, &[input], &mut buffer), expected);
            assert!(collect(analyzer, &["", ""], &mut buffer).is_empty());
        }
    }
}

#[cfg(feature = "smartcn")]
#[test]
fn smartcn_preserves_stemming_offsets_and_position_gaps() {
    let input = "我购买了道具和服装。 Running, jumped!";
    let options = Default::default();
    let mut expected = Vec::new();
    crate::cjk::smartcn::analyze(input, options, &mut String::new(), |token| {
        expected.push(OwnedToken {
            text: token.text.to_owned(),
            position: token.position,
            position_length: 1,
            byte_range: token.byte_range,
            input_index: 0,
        });
        true
    });
    let actual = collect(
        Analyzer::smartcn(options),
        &[input],
        &mut ReusableBuffer::new(),
    );
    assert_eq!(actual, expected);
    assert!(
        actual
            .windows(2)
            .any(|pair| pair[1].position > pair[0].position + 1)
    );
    assert!(
        actual
            .iter()
            .any(|token| token.text == "run" && &input[token.byte_range.clone()] == "Running")
    );
}

#[cfg(feature = "nori")]
#[test]
fn nori_preserves_compound_graphs_and_borrowed_user_dictionaries() {
    use crate::cjk::nori;

    let dictionary = nori::UserDictionary::parse("세종시 세종 시", false).unwrap();
    let mut options = nori::AnalyzerOptions::default();
    options.decompound_mode = nori::DecompoundMode::Mixed;
    options.user_dictionary = Some(&dictionary);
    let analyzer = Analyzer::nori(options);
    let inputs = ["세종시", "", "세종시"];
    let tokens = collect(analyzer, &inputs, &mut ReusableBuffer::new());
    let actual: Vec<_> = tokens
        .iter()
        .map(|token| {
            (
                token.text.as_str(),
                token.position,
                token.position_length,
                token.input_index,
            )
        })
        .collect();
    assert_eq!(
        actual,
        [
            ("세종시", 0, 2, 0),
            ("세종", 0, 1, 0),
            ("시", 1, 1, 0),
            ("세종시", 2, 2, 2),
            ("세종", 2, 1, 2),
            ("시", 3, 1, 2),
        ]
    );
    for token in tokens {
        assert_eq!(&inputs[token.input_index][token.byte_range], token.text);
    }
}

#[cfg(feature = "nori")]
#[test]
fn nori_preserves_reading_forms_and_filtered_positions() {
    use crate::cjk::nori;

    let input = "나는 韓國에서 한국어를 분석합니다";
    let options = nori::AnalyzerOptions::default();
    let mut tokens = nori::Tokens::new();
    nori::analyze(input, options, &mut tokens);
    let mut position = 0;
    let expected: Vec<_> = tokens
        .iter()
        .map(|token| {
            position += token.position_increment as usize;
            OwnedToken {
                text: token.text.to_owned(),
                position: position - 1,
                position_length: token.position_length as usize,
                byte_range: token.byte_range,
                input_index: 0,
            }
        })
        .collect();
    assert_eq!(
        collect(
            Analyzer::nori(options),
            &[input],
            &mut ReusableBuffer::new()
        ),
        expected
    );
    assert!(
        expected
            .windows(2)
            .any(|pair| pair[1].position > pair[0].position + 1)
    );
    assert!(
        expected
            .iter()
            .any(|token| token.text == "한국" && &input[token.byte_range.clone()] == "韓國")
    );
}

#[cfg(feature = "kuromoji")]
#[test]
fn kuromoji_preserves_original_offsets_after_width_normalization() {
    use crate::cjk::kuromoji;

    let dictionary = kuromoji::UserDictionary::parse(
        "東京スカイツリー,東京 スカイツリー,トウキョウ スカイツリー,カスタム名詞",
        false,
    )
    .unwrap();
    let mut options = kuromoji::AnalyzerOptions::default();
    options.user_dictionary = Some(&dictionary);
    let input = "東京スカイツリーでｶﾞｯﾂﾎﾟｰｽﾞ ＡＢＣ";
    let mut tokens = kuromoji::Tokens::new();
    kuromoji::analyze(input, options, &mut tokens);
    let mut position = 0;
    let expected: Vec<_> = tokens
        .iter()
        .map(|token| {
            position += token.position_increment as usize;
            OwnedToken {
                text: token.text.to_owned(),
                position: position - 1,
                position_length: token.position_length as usize,
                byte_range: token.byte_range,
                input_index: 0,
            }
        })
        .collect();
    let actual = collect(
        Analyzer::kuromoji(options),
        &[input],
        &mut ReusableBuffer::new(),
    );
    assert_eq!(actual, expected);
    assert!(
        actual
            .iter()
            .any(|token| token.text == "abc" && &input[token.byte_range.clone()] == "ＡＢＣ")
    );
    assert!(
        actual
            .iter()
            .any(|token| input[token.byte_range.clone()].starts_with("ｶﾞ"))
    );
}

#[test]
fn positions_and_input_indices_continue_across_empty_inputs() {
    for (analyzer, input) in analyzers() {
        let mut buffer = ReusableBuffer::new();
        let mut expected = collect(analyzer, &[input], &mut buffer);
        let end = expected
            .iter()
            .map(|token| token.position + token.position_length)
            .max()
            .unwrap();
        let mut second = collect(analyzer, &[input], &mut buffer);
        for token in &mut second {
            token.position += end;
            token.input_index = 2;
        }
        expected.extend(second);
        assert_eq!(
            collect(analyzer, &[input, "", input], &mut buffer),
            expected
        );
    }
}

#[cfg(feature = "smartcn")]
#[test]
fn cjk_input_boundaries_follow_emitted_tokens() {
    let tokens = collect(
        Analyzer::smartcn(Default::default()),
        &["running,", "!!!", "", "jumped"],
        &mut ReusableBuffer::new(),
    );
    let actual: Vec<_> = tokens
        .iter()
        .map(|token| (token.text.as_str(), token.position, token.input_index))
        .collect();
    assert_eq!(actual, [("run", 0, 0), ("jump", 1, 3)]);
}
