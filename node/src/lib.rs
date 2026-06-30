//! Node.js bindings for [`alyze_features`], built with napi-rs.
//!
//! The crate computes text-match ranking features (proximity, completeness, similarity, the
//! `fieldMatch` family, …) for second-stage re-ranking. The JS surface mirrors that: analyze a
//! piece of text once into an [`Analyzed`] object, then call feature methods on it, passing the
//! other field as the argument. The query is always the receiver:
//!
//! ```js
//! const { Analyzed } = require('alyze-features')
//!
//! const query = new Analyzed('quick brown fox')
//! const document = new Analyzed('the quick red fox jumps')
//!
//! query.elementSimilarity(document).similarity
//! query.fieldMatch(document).score
//! ```
//!
//! Only the feature layer is exposed; the underlying `alyze` analysis pipeline is an internal
//! detail. `new Analyzed(text)` uses a default configuration; use [`Analyzer`] to choose options.
//!
//! SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Mutex;

use alyze::analyze::{
    AnalysisOptions, Analyzer as AlyzeAnalyzer, LanguageWithStopwords, ReusableBuffer,
    StemmingLanguage, StopwordRemoval, TokenizerOptions,
};
// `::` disambiguates the crate from the items defined in this module.
use ::alyze_features as features;
use napi::bindgen_prelude::*;
use napi_derive::napi;

/// The analysis configuration used by the no-argument `new Analyzed(text)`: UAX#29 word
/// segmentation, lowercased, no stemming, stopword removal, or length limit. Matches the
/// configuration the `alyze-features` crate analyzes with, so default results match the Rust crate.
fn default_options() -> AnalysisOptions {
    AnalysisOptions {
        tokenizer: TokenizerOptions::UAX29Word(Default::default()),
        maximum_token_length: None,
        case_sensitive: false,
        stopword_removal: None,
        stemming: None,
        ascii_folding: false,
    }
}

/// Options for [`Analyzer`]. All fields are optional; omitted fields use their default.
#[napi(object)]
pub struct AnalyzerOptions {
    /// When false (the default), tokens are lowercased.
    pub case_sensitive: Option<bool>,
    /// Fold non-ASCII characters to an ASCII equivalent where one exists (e.g. `à` -> `a`).
    pub ascii_folding: Option<bool>,
    /// Apply Snowball stemming for `language`. Requires `caseSensitive: false`.
    pub stemming: Option<bool>,
    /// Drop stopwords for `language`. Requires `caseSensitive: false`.
    pub remove_stopwords: Option<bool>,
    /// Language used for stemming and stopword removal. Defaults to `"english"`.
    pub language: Option<String>,
    /// Drop tokens longer than this many bytes (1-255). Omit to keep all tokens.
    pub max_token_length: Option<u32>,
}

/// An analyzer with a fixed configuration, used to turn text into [`Analyzed`] objects.
///
/// Create one with the options you want, then call `analyze(text)` for each field. Using the same
/// analyzer for the query and the documents guarantees they are tokenized identically, which the
/// features rely on. For the default configuration you can skip this and use `new Analyzed(text)`.
///
/// Note: stopword removal and a `maxTokenLength` limit drop tokens while still advancing the
/// position counter, so they introduce gaps in token positions. Leave them off to reproduce the
/// `alyze-features` crate's reference semantics exactly.
#[napi]
pub struct Analyzer {
    analyzer: AlyzeAnalyzer,
    // Reused across `analyze` calls to avoid re-allocating per call; guarded for thread-safety.
    buffer: Mutex<ReusableBuffer>,
}

#[napi]
impl Analyzer {
    /// Build an analyzer. `stemming` and `removeStopwords` both require `caseSensitive: false` and a
    /// `language` that supports the feature; an invalid combination throws.
    #[napi(constructor)]
    pub fn new(options: Option<AnalyzerOptions>) -> Result<Self> {
        let options = options.unwrap_or(AnalyzerOptions {
            case_sensitive: None,
            ascii_folding: None,
            stemming: None,
            remove_stopwords: None,
            language: None,
            max_token_length: None,
        });
        let analysis_options = build_options(&options)?;
        Ok(Self {
            analyzer: AlyzeAnalyzer::new(analysis_options),
            buffer: Mutex::new(ReusableBuffer::new()),
        })
    }

    /// Analyze `text` into a reusable [`Analyzed`] object.
    #[napi]
    pub fn analyze(&self, text: String) -> Analyzed {
        let mut buffer = self.buffer.lock().expect("analyzer buffer mutex poisoned");
        Analyzed {
            inner: features::Analyzed::from_text(&self.analyzer, &mut buffer, &text),
        }
    }
}

/// Translate the user-facing options into validated [`AnalysisOptions`], applying the same
/// validation rules as the turbopuffer API.
fn build_options(options: &AnalyzerOptions) -> Result<AnalysisOptions> {
    let case_sensitive = options.case_sensitive.unwrap_or(false);
    let stemming_on = options.stemming.unwrap_or(false);
    let remove_stopwords = options.remove_stopwords.unwrap_or(false);
    let language = options.language.as_deref().unwrap_or("english");

    if stemming_on && case_sensitive {
        return Err(Error::new(
            Status::InvalidArg,
            "stemming is not supported when case sensitivity is enabled",
        ));
    }
    if remove_stopwords && case_sensitive {
        return Err(Error::new(
            Status::InvalidArg,
            "stopword removal is not supported when case sensitivity is enabled",
        ));
    }

    let stemming = if stemming_on {
        Some(stemming_language(language).ok_or_else(|| {
            Error::new(
                Status::InvalidArg,
                format!("stemming is not supported for language: {language}"),
            )
        })?)
    } else {
        None
    };

    let stopword_removal = if remove_stopwords {
        let language = stopword_language(language).ok_or_else(|| {
            Error::new(
                Status::InvalidArg,
                format!("stopword removal is not supported for language: {language}"),
            )
        })?;
        Some(StopwordRemoval::ForLanguage(language))
    } else {
        None
    };

    let max_token_length = match options.max_token_length {
        Some(n) if n == 0 || n > 255 => {
            return Err(Error::new(
                Status::InvalidArg,
                "maxTokenLength must be between 1 and 255",
            ));
        }
        Some(n) => Some(n as usize),
        None => None,
    };

    Ok(AnalysisOptions {
        tokenizer: TokenizerOptions::UAX29Word(Default::default()),
        maximum_token_length: max_token_length,
        case_sensitive,
        stopword_removal,
        stemming,
        ascii_folding: options.ascii_folding.unwrap_or(false),
    })
}

/// Maps a language name to its stemming algorithm, if supported.
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

/// Maps a language name to its stopword list, if one exists.
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

/// Analyzed text — a piece of text tokenized once and reused across feature calls.
///
/// Construct it with `new Analyzed(text)` for the default configuration, or with
/// `new Analyzer(options).analyze(text)` to choose the analysis options. Then call the feature
/// methods, passing the field you want to compare against. For the asymmetric features (everything
/// except `matches`) the receiver is the query and the argument is the document/field.
#[napi]
pub struct Analyzed {
    inner: features::Analyzed,
}

#[napi]
impl Analyzed {
    /// Analyze `text` with the default configuration. Use `new Analyzer(options).analyze(text)` to
    /// choose the analysis options.
    #[napi(constructor)]
    pub fn new(text: String) -> Self {
        let analyzer = AlyzeAnalyzer::new(default_options());
        let mut buffer = ReusableBuffer::new();
        Self {
            inner: features::Analyzed::from_text(&analyzer, &mut buffer, &text),
        }
    }

    /// `elementCompleteness(field)` — how completely the query (`this`) matches the `document`,
    /// measured as token overlap.
    #[napi]
    pub fn element_completeness(&self, document: &Analyzed) -> ElementCompleteness {
        features::element_completeness(&self.inner, &document.inner).into()
    }

    /// `elementSimilarity(field)` — structural similarity (proximity, order, coverage) between the
    /// query (`this`) and the `document`.
    #[napi]
    pub fn element_similarity(&self, document: &Analyzed) -> ElementSimilarity {
        features::element_similarity(&self.inner, &document.inner).into()
    }

    /// `textSimilarity(field)` — how well the query (`this`) matches the `document`, ignoring term
    /// weight and significance. Numerically equal to `elementSimilarity` for single-element fields.
    #[napi]
    pub fn text_similarity(&self, document: &Analyzed) -> TextSimilarity {
        features::text_similarity(&self.inner, &document.inner).into()
    }

    /// `matches(field)` — `1.0` if any query term occurs in the `document`, else `0.0`.
    #[napi]
    pub fn matches(&self, document: &Analyzed) -> f64 {
        features::matches(&self.inner, &document.inner)
    }

    /// `fieldTermMatch(field, termIndex)` — the first position and occurrence count of the query
    /// term at `termIndex` within the `document`. An out-of-range index, or a term absent from the
    /// document, returns the absent sentinel position and zero occurrences.
    #[napi]
    pub fn field_term_match(&self, document: &Analyzed, term_index: u32) -> FieldTermMatch {
        features::field_term_match(&self.inner, &document.inner, term_index as usize).into()
    }

    /// `queryTermCount` — the number of terms in this text (with repeats).
    #[napi]
    pub fn query_term_count(&self) -> f64 {
        features::query_term_count(&self.inner)
    }

    /// `fieldMatch(field)` — the full `fieldMatch` feature family for the query (`this`) against the
    /// `document`.
    ///
    /// `stats` optionally maps a query term to its corpus [`TermStats`]; terms not in the map use a
    /// neutral default. Omit it to compute only the purely positional outputs (the
    /// significance/weight-dependent outputs then use the neutral defaults and should be ignored).
    #[napi]
    pub fn field_match(
        &self,
        document: &Analyzed,
        stats: Option<HashMap<String, TermStats>>,
    ) -> FieldMatch {
        let lookup = |term: &str| {
            stats
                .as_ref()
                .and_then(|s| s.get(term))
                .map(TermStats::to_rust)
                .unwrap_or_default()
        };
        features::field_match(&self.inner, &document.inner, lookup).into()
    }
}

/// Per-query-term corpus statistics for the significance/weight-dependent `fieldMatch` outputs.
/// Supply the raw document frequency and corpus size; the significance (normalized IDF) is derived
/// from them — see [`legacy_significance`].
#[napi(object)]
pub struct TermStats {
    /// Number of documents containing the term (`df`).
    pub document_frequency: i64,
    /// Total number of documents in the corpus (`N`).
    pub document_count: i64,
    /// Query term weight (weight percent). Defaults to 100 when omitted.
    pub weight: Option<i32>,
}

impl TermStats {
    fn to_rust(&self) -> features::TermStats {
        features::TermStats {
            document_frequency: self.document_frequency.max(0) as u64,
            document_count: self.document_count.max(0) as u64,
            weight: self.weight.unwrap_or(100),
        }
    }
}

/// Defines a plain-object result type mirroring an `alyze-features` output struct (all fields
/// `f64`), with a `From` conversion. `#[napi(object)]` maps it to a plain JS object.
macro_rules! feature_object {
    ($name:ident from $src:path { $($field:ident),+ $(,)? }) => {
        #[napi(object)]
        pub struct $name {
            $(pub $field: f64,)+
        }

        impl From<$src> for $name {
            fn from(v: $src) -> Self {
                Self { $($field: v.$field,)+ }
            }
        }
    };
}

feature_object!(ElementCompleteness from features::ElementCompleteness {
    completeness,
    field_completeness,
    query_completeness,
});

feature_object!(ElementSimilarity from features::ElementSimilarity {
    similarity,
    proximity,
    order,
    query_coverage,
    field_coverage,
});

feature_object!(TextSimilarity from features::TextSimilarity {
    score,
    proximity,
    order,
    query_coverage,
    field_coverage,
});

feature_object!(FieldTermMatch from features::FieldTermMatch {
    first_position,
    occurrences,
});

feature_object!(FieldMatch from features::FieldMatch {
    score,
    proximity,
    absolute_proximity,
    unweighted_proximity,
    completeness,
    query_completeness,
    field_completeness,
    orderness,
    relatedness,
    earliness,
    longest_sequence,
    longest_sequence_ratio,
    segments,
    segment_distance,
    segment_proximity,
    gaps,
    gap_length,
    head,
    tail,
    out_of_order,
    matches,
    occurrence,
    absolute_occurrence,
    significance,
    weight,
    importance,
    weighted_occurrence,
    weighted_absolute_occurrence,
    significant_occurrence,
});

/// Term significance: inverse document frequency normalized to `[0.5, 1]`, matching the search
/// backend's definition. `documentFrequency` is the number of documents containing the term;
/// `documentCount` is the corpus size.
#[napi]
pub fn legacy_significance(document_frequency: i64, document_count: i64) -> f64 {
    features::legacy_significance(document_frequency.max(0) as u64, document_count.max(0) as u64)
}

/// Sentinel returned for `firstPosition` when a term is absent from the field.
#[napi]
pub const FIELD_TERM_MATCH_ABSENT_POSITION: f64 = features::FIELD_TERM_MATCH_ABSENT_POSITION;
