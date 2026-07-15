//! WebAssembly bindings for [`alyze`].
//!
//! Exposes the analysis pipeline that powers turbopuffer's `word_v4` tokenizer
//! so it can run client-side (e.g. an interactive "analyze a string" widget in
//! the docs). The option set and validation rules mirror the BM25 full-text
//! search parameters users configure against the turbopuffer API.

use alyze::analyze::{
    AnalysisOptions, Analyzer, LanguageWithStopwords, ReusableBuffer, StemmingLanguage,
    StopwordRemoval, TokenizerOptions,
};
use alyze::uax29;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Analysis options, mirroring the subset of turbopuffer's full-text search
/// parameters that affect tokenization. Field names match the API
/// (`snake_case`) so a config object can be passed straight through.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Options {
    /// When false (the default), tokens are lowercased.
    case_sensitive: bool,
    /// Fold non-ASCII characters to an ASCII equivalent where one exists
    /// (e.g. `à` -> `a`).
    ascii_folding: bool,
    /// Apply Snowball stemming for `language`. Requires `case_sensitive: false`.
    stemming: bool,
    /// Drop stopwords for `language`. Requires `case_sensitive: false`.
    remove_stopwords: bool,
    /// Language used for stemming and stopword removal (e.g. `"english"`).
    language: Option<String>,
    /// Drop tokens longer than this many bytes. Defaults to 39.
    max_token_length: Option<usize>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            ascii_folding: false,
            stemming: false,
            remove_stopwords: false,
            language: Some("english".to_string()),
            max_token_length: Some(39),
        }
    }
}

/// A single emitted token.
#[derive(Serialize)]
struct Token {
    /// The final token text, after the full pipeline (lowercasing, stopword
    /// removal, stemming, ASCII folding).
    text: String,
    /// Token position. Every word-like token consumes a position, even if a
    /// later filter (length, stopword) drops it — which keeps phrase distances
    /// accurate. Positions can therefore have gaps.
    position: usize,
    /// UTF-8 byte range of the raw token in the input text, before any
    /// normalization. Always on char boundaries.
    start: usize,
    end: usize,
}

/// Describes a language's capabilities, so the UI can build its language picker
/// from a single source of truth.
#[derive(Serialize)]
struct LanguageInfo {
    id: &'static str,
    label: &'static str,
    stemming: bool,
    stopwords: bool,
}

/// Analyze `text` and return the resulting tokens.
///
/// `options` is a plain JS object matching [`Options`]. On an invalid
/// configuration (e.g. stemming with case sensitivity, or an unsupported
/// language) this throws an `Error` whose message matches what the turbopuffer
/// API would return.
#[wasm_bindgen]
pub fn analyze(text: &str, options: JsValue) -> Result<JsValue, JsValue> {
    let options: Options = if options.is_undefined() || options.is_null() {
        Options::default()
    } else {
        serde_wasm_bindgen::from_value(options).map_err(|e| js_error(&e.to_string()))?
    };

    let analysis_options = build_options(&options)?;
    let analyzer = Analyzer::new(analysis_options);

    let mut buffer = ReusableBuffer::new();
    let mut tokens = Vec::new();
    analyzer.analyze(text, &mut buffer, |token| {
        tokens.push(Token {
            text: token.text.to_string(),
            position: token.position,
            start: token.byte_range.start,
            end: token.byte_range.end,
        });
        true
    });

    serde_wasm_bindgen::to_value(&tokens).map_err(|e| js_error(&e.to_string()))
}

/// A sentence's UTF-8 byte range in the input text.
#[derive(Serialize)]
struct SentenceRange {
    start: usize,
    end: usize,
}

/// Segment `text` into sentences (UAX #29), returning each sentence's byte
/// range. Ranges are contiguous and cover the whole input; trailing whitespace
/// attaches to the preceding sentence.
#[wasm_bindgen]
pub fn sentences(text: &str) -> Result<JsValue, JsValue> {
    let mut ranges = Vec::new();
    let mut prev: Option<usize> = None;
    uax29::sentence::tokenize(text, uax29::sentence::Options::default(), |idx| {
        if let Some(start) = prev {
            ranges.push(SentenceRange { start, end: idx });
        }
        prev = Some(idx);
        true
    });
    serde_wasm_bindgen::to_value(&ranges).map_err(|e| js_error(&e.to_string()))
}

/// Returns the supported languages and whether each supports stemming and/or
/// stopword removal.
#[wasm_bindgen]
pub fn languages() -> Result<JsValue, JsValue> {
    let langs: Vec<LanguageInfo> = LANGUAGES
        .iter()
        .map(|&(id, label)| LanguageInfo {
            id,
            label,
            stemming: stemming_language(id).is_some(),
            stopwords: stopword_language(id).is_some(),
        })
        .collect();
    serde_wasm_bindgen::to_value(&langs).map_err(|e| js_error(&e.to_string()))
}

/// Translate the user-facing options into validated [`AnalysisOptions`],
/// applying the same validation rules as the turbopuffer API.
fn build_options(options: &Options) -> Result<AnalysisOptions, JsValue> {
    // Mirrors `Params::validate` in the turbopuffer codebase.
    if options.stemming && options.case_sensitive {
        return Err(js_error(
            "stemming is not supported when case sensitivity is enabled",
        ));
    }
    if options.remove_stopwords && options.case_sensitive {
        return Err(js_error(
            "stopword removal is not supported when case sensitivity is enabled",
        ));
    }
    if (options.stemming || options.remove_stopwords) && options.language.is_none() {
        return Err(js_error(
            "language must be provided when stemming or stopword removal is enabled",
        ));
    }

    let stemming = if options.stemming {
        let language = options.language.as_deref().unwrap();
        Some(stemming_language(language).ok_or_else(|| {
            js_error(&format!(
                "stemming is not supported for language: {language}"
            ))
        })?)
    } else {
        None
    };

    let stopword_removal = if options.remove_stopwords {
        let language = options.language.as_deref().unwrap();
        let language = stopword_language(language).ok_or_else(|| {
            js_error(&format!(
                "stopword removal is not supported for language: {language}"
            ))
        })?;
        Some(StopwordRemoval::ForLanguage(language))
    } else {
        None
    };

    let max_token_length = options.max_token_length.unwrap_or(39);
    if max_token_length == 0 || max_token_length > 255 {
        return Err(js_error("max_token_length must be between 1 and 255"));
    }

    Ok(AnalysisOptions {
        tokenizer: TokenizerOptions::UAX29Word(uax29::word::Options::default()),
        maximum_token_length: Some(max_token_length),
        case_sensitive: options.case_sensitive,
        stopword_removal,
        stemming,
        ascii_folding: options.ascii_folding,
    })
}

/// All languages turbopuffer recognizes, with display labels. Capability flags
/// (stemming/stopwords) are derived from the mapping functions below.
const LANGUAGES: &[(&str, &str)] = &[
    ("arabic", "Arabic"),
    ("danish", "Danish"),
    ("dutch", "Dutch"),
    ("english", "English"),
    ("finnish", "Finnish"),
    ("french", "French"),
    ("german", "German"),
    ("greek", "Greek"),
    ("hungarian", "Hungarian"),
    ("italian", "Italian"),
    ("norwegian", "Norwegian"),
    ("portuguese", "Portuguese"),
    ("romanian", "Romanian"),
    ("russian", "Russian"),
    ("spanish", "Spanish"),
    ("swedish", "Swedish"),
    ("tamil", "Tamil"),
    ("turkish", "Turkish"),
];

/// Mirrors `language_into_alyze_stemming_language` in the turbopuffer codebase.
fn stemming_language(language: &str) -> Option<StemmingLanguage> {
    Some(match language {
        "arabic" => StemmingLanguage::Arabic,
        "danish" => StemmingLanguage::Danish,
        "dutch" => StemmingLanguage::Dutch,
        "english" => StemmingLanguage::English,
        "finnish" => StemmingLanguage::Finnish,
        "french" => StemmingLanguage::French,
        "german" => StemmingLanguage::German,
        "greek" => StemmingLanguage::Greek,
        "hungarian" => StemmingLanguage::Hungarian,
        "italian" => StemmingLanguage::Italian,
        "norwegian" => StemmingLanguage::Norwegian,
        "portuguese" => StemmingLanguage::Portuguese,
        "romanian" => StemmingLanguage::Romanian,
        "russian" => StemmingLanguage::Russian,
        "spanish" => StemmingLanguage::Spanish,
        "swedish" => StemmingLanguage::Swedish,
        "tamil" => StemmingLanguage::Tamil,
        "turkish" => StemmingLanguage::Turkish,
        _ => return None,
    })
}

/// Mirrors `language_into_alyze_stopword_language` in the turbopuffer codebase.
/// Some languages have no stopword list.
fn stopword_language(language: &str) -> Option<LanguageWithStopwords> {
    Some(match language {
        "danish" => LanguageWithStopwords::Danish,
        "dutch" => LanguageWithStopwords::Dutch,
        "english" => LanguageWithStopwords::English,
        "finnish" => LanguageWithStopwords::Finnish,
        "french" => LanguageWithStopwords::French,
        "german" => LanguageWithStopwords::German,
        "hungarian" => LanguageWithStopwords::Hungarian,
        "italian" => LanguageWithStopwords::Italian,
        "norwegian" => LanguageWithStopwords::Norwegian,
        "portuguese" => LanguageWithStopwords::Portuguese,
        "russian" => LanguageWithStopwords::Russian,
        "spanish" => LanguageWithStopwords::Spanish,
        "swedish" => LanguageWithStopwords::Swedish,
        _ => return None,
    })
}

fn js_error(message: &str) -> JsValue {
    JsError::new(message).into()
}
