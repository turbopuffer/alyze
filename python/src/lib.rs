//! Python bindings for [`alyze_features`].
//!
//! The crate computes text-match ranking features (proximity, completeness, similarity, the
//! `fieldMatch` family, …) for second-stage re-ranking. The Python surface mirrors that: analyze a
//! piece of text once into an [`Analyzed`] object, then call feature methods on it, passing the
//! other field as the argument. The query is always the receiver:
//!
//! ```python
//! import alyze_features as af
//!
//! query = af.Analyzed("quick brown fox")
//! document = af.Analyzed("the quick red fox jumps")
//!
//! query.element_similarity(document).similarity
//! query.field_match(document).score
//! ```
//!
//! Only the feature layer is exposed; the underlying `alyze` analysis pipeline is an internal
//! detail. Text is analyzed with the same fixed configuration the `alyze-features` crate uses for
//! its own computations (UAX#29 word segmentation, lowercased, no stemming or stopword removal) so
//! that token positions stay contiguous, which the features assume.
//!
//! SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Mutex;

use alyze::analyze::{
    AnalysisOptions, Analyzer as AlyzeAnalyzer, LanguageWithStopwords, ReusableBuffer,
    StemmingLanguage, StopwordRemoval, TokenizerOptions,
};
// `::` disambiguates the crate from the `alyze_features` PyO3 module function below.
use ::alyze_features as features;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// The analysis configuration used by the no-argument `Analyzed(text)` shortcut: UAX#29 word
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

/// An analyzer with a fixed configuration, used to turn text into [`Analyzed`] objects.
///
/// Create one with the options you want, then call `analyze(text)` for each field. Using the same
/// analyzer for the query and the documents guarantees they are tokenized identically, which the
/// features rely on. For the default configuration you can skip this and use `Analyzed(text)`.
///
/// Note: stopword removal and a `max_token_length` limit drop tokens while still advancing the
/// position counter, so they introduce gaps in token positions. Leave them off to reproduce the
/// `alyze-features` crate's reference semantics exactly.
#[pyclass(frozen)]
pub struct Analyzer {
    analyzer: AlyzeAnalyzer,
    // Reused across `analyze` calls to avoid re-allocating per call; guarded for thread-safety.
    buffer: Mutex<ReusableBuffer>,
}

#[pymethods]
impl Analyzer {
    /// Build an analyzer.
    ///
    /// `stemming` and `remove_stopwords` both require `case_sensitive=False` and a `language` that
    /// supports the feature; an invalid combination raises `ValueError`. `max_token_length` (1–255)
    /// drops longer tokens; `None` (the default) keeps all tokens.
    #[new]
    #[pyo3(signature = (
        *,
        case_sensitive=false,
        ascii_folding=false,
        stemming=false,
        remove_stopwords=false,
        language="english".to_owned(),
        max_token_length=None,
    ))]
    fn new(
        case_sensitive: bool,
        ascii_folding: bool,
        stemming: bool,
        remove_stopwords: bool,
        language: String,
        max_token_length: Option<usize>,
    ) -> PyResult<Self> {
        let options = build_options(
            case_sensitive,
            ascii_folding,
            stemming,
            remove_stopwords,
            &language,
            max_token_length,
        )?;
        Ok(Self {
            analyzer: AlyzeAnalyzer::new(options),
            buffer: Mutex::new(ReusableBuffer::new()),
        })
    }

    /// Analyze `text` into a reusable [`Analyzed`] object.
    fn analyze(&self, text: &str) -> Analyzed {
        let mut buffer = self.buffer.lock().expect("analyzer buffer mutex poisoned");
        Analyzed {
            inner: features::Analyzed::from_text(&self.analyzer, &mut buffer, text),
        }
    }
}

/// Translate the user-facing options into validated [`AnalysisOptions`], applying the same
/// validation rules as the turbopuffer API (mirrors the `wasm` bindings' `build_options`).
fn build_options(
    case_sensitive: bool,
    ascii_folding: bool,
    stemming: bool,
    remove_stopwords: bool,
    language: &str,
    max_token_length: Option<usize>,
) -> PyResult<AnalysisOptions> {
    if stemming && case_sensitive {
        return Err(PyValueError::new_err(
            "stemming is not supported when case sensitivity is enabled",
        ));
    }
    if remove_stopwords && case_sensitive {
        return Err(PyValueError::new_err(
            "stopword removal is not supported when case sensitivity is enabled",
        ));
    }

    let stemming = if stemming {
        Some(stemming_language(language).ok_or_else(|| {
            PyValueError::new_err(format!("stemming is not supported for language: {language}"))
        })?)
    } else {
        None
    };

    let stopword_removal = if remove_stopwords {
        let language = stopword_language(language).ok_or_else(|| {
            PyValueError::new_err(format!(
                "stopword removal is not supported for language: {language}"
            ))
        })?;
        Some(StopwordRemoval::ForLanguage(language))
    } else {
        None
    };

    if let Some(max_token_length) = max_token_length
        && (max_token_length == 0 || max_token_length > 255)
    {
        return Err(PyValueError::new_err(
            "max_token_length must be between 1 and 255",
        ));
    }

    Ok(AnalysisOptions {
        tokenizer: TokenizerOptions::UAX29Word(Default::default()),
        maximum_token_length: max_token_length,
        case_sensitive,
        stopword_removal,
        stemming,
        ascii_folding,
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
/// Construct it with `Analyzed(text)` for the default configuration, or with
/// `Analyzer(...).analyze(text)` to choose the analysis options. Then call the feature methods,
/// passing the field you want to compare against. For the asymmetric features (everything except
/// `matches`) the receiver is the query and the argument is the document/field.
#[pyclass(frozen)]
pub struct Analyzed {
    inner: features::Analyzed,
}

#[pymethods]
impl Analyzed {
    /// Analyze `text` with the default configuration. Use `Analyzer(...).analyze(text)` to choose
    /// the analysis options.
    #[new]
    fn new(text: &str) -> Self {
        let analyzer = AlyzeAnalyzer::new(default_options());
        let mut buffer = ReusableBuffer::new();
        Self {
            inner: features::Analyzed::from_text(&analyzer, &mut buffer, text),
        }
    }

    /// `elementCompleteness(field)` — how completely the query (`self`) matches the `document`,
    /// measured as token overlap.
    fn element_completeness(&self, document: &Analyzed) -> ElementCompleteness {
        features::element_completeness(&self.inner, &document.inner).into()
    }

    /// `elementSimilarity(field)` — structural similarity (proximity, order, coverage) between the
    /// query (`self`) and the `document`.
    fn element_similarity(&self, document: &Analyzed) -> ElementSimilarity {
        features::element_similarity(&self.inner, &document.inner).into()
    }

    /// `textSimilarity(field)` — how well the query (`self`) matches the `document`, ignoring term
    /// weight and significance. Numerically equal to `element_similarity` for single-element fields.
    fn text_similarity(&self, document: &Analyzed) -> TextSimilarity {
        features::text_similarity(&self.inner, &document.inner).into()
    }

    /// `matches(field)` — `1.0` if any query term occurs in the `document`, else `0.0`.
    fn matches(&self, document: &Analyzed) -> f64 {
        features::matches(&self.inner, &document.inner)
    }

    /// `fieldTermMatch(field, term_index)` — the first position and occurrence count of the query
    /// term at `term_index` within the `document`. An out-of-range index, or a term absent from the
    /// document, returns the absent sentinel position and zero occurrences.
    fn field_term_match(&self, document: &Analyzed, term_index: usize) -> FieldTermMatch {
        features::field_term_match(&self.inner, &document.inner, term_index).into()
    }

    /// `queryTermCount` — the number of terms in this text (with repeats).
    fn query_term_count(&self) -> f64 {
        features::query_term_count(&self.inner)
    }

    /// `fieldMatch(field)` — the full `fieldMatch` feature family for the query (`self`) against the
    /// `document`.
    ///
    /// `stats` optionally maps a query term to its corpus [`TermStats`] (document frequency, corpus
    /// size, weight); terms not present in the map use the neutral default. Pass `None` to compute
    /// only the purely positional outputs (the significance/weight-dependent outputs then use the
    /// neutral defaults and should be ignored).
    #[pyo3(signature = (document, stats=None))]
    fn field_match(
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

    fn __repr__(&self) -> String {
        format!("Analyzed(query_term_count={})", self.query_term_count())
    }
}

/// Per-query-term corpus statistics for the significance/weight-dependent `fieldMatch` outputs.
///
/// Supply the raw document frequency and corpus size; the significance (normalized IDF) is derived
/// from them via [`legacy_significance`].
// `from_py_object` so `TermStats` can be passed by value in the `field_match` stats map.
#[pyclass(frozen, get_all, from_py_object)]
#[derive(Clone)]
pub struct TermStats {
    /// Number of documents containing the term (`df`).
    pub document_frequency: u64,
    /// Total number of documents in the corpus (`N`).
    pub document_count: u64,
    /// Query term weight (weight percent). Defaults to 100.
    pub weight: i32,
}

#[pymethods]
impl TermStats {
    #[new]
    #[pyo3(signature = (document_frequency=0, document_count=0, weight=100))]
    fn new(document_frequency: u64, document_count: u64, weight: i32) -> Self {
        Self {
            document_frequency,
            document_count,
            weight,
        }
    }

    /// This term's significance (normalized IDF in `[0.5, 1]`); see [`legacy_significance`].
    fn significance(&self) -> f64 {
        self.to_rust().significance()
    }

    fn __repr__(&self) -> String {
        format!(
            "TermStats(document_frequency={}, document_count={}, weight={})",
            self.document_frequency, self.document_count, self.weight
        )
    }
}

impl TermStats {
    fn to_rust(&self) -> features::TermStats {
        features::TermStats {
            document_frequency: self.document_frequency,
            document_count: self.document_count,
            weight: self.weight,
        }
    }
}

/// Defines a read-only result `#[pyclass]` mirroring an `alyze-features` output struct (all fields
/// `f64`), with a `From` conversion and a `__repr__` listing every field.
macro_rules! feature_result {
    ($name:ident from $src:path { $($field:ident),+ $(,)? }) => {
        // Output-only, never extracted from Python, so opt out of the `FromPyObject` derive.
        #[pyclass(frozen, get_all, skip_from_py_object)]
        #[derive(Clone)]
        pub struct $name {
            $(pub $field: f64,)+
        }

        impl From<$src> for $name {
            fn from(v: $src) -> Self {
                Self { $($field: v.$field,)+ }
            }
        }

        #[pymethods]
        impl $name {
            fn __repr__(&self) -> String {
                let fields = [$((stringify!($field), self.$field)),+];
                let body = fields
                    .iter()
                    .map(|(name, value)| format!("{name}={value}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", stringify!($name), body)
            }
        }
    };
}

feature_result!(ElementCompleteness from features::ElementCompleteness {
    completeness,
    field_completeness,
    query_completeness,
});

feature_result!(ElementSimilarity from features::ElementSimilarity {
    similarity,
    proximity,
    order,
    query_coverage,
    field_coverage,
});

feature_result!(TextSimilarity from features::TextSimilarity {
    score,
    proximity,
    order,
    query_coverage,
    field_coverage,
});

feature_result!(FieldTermMatch from features::FieldTermMatch {
    first_position,
    occurrences,
});

feature_result!(FieldMatch from features::FieldMatch {
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
/// backend's definition. `document_frequency` is the number of documents containing the term;
/// `document_count` is the corpus size.
#[pyfunction]
fn legacy_significance(document_frequency: u64, document_count: u64) -> f64 {
    features::legacy_significance(document_frequency, document_count)
}

/// Sentinel returned for `first_position` when a term is absent from the field.
const FIELD_TERM_MATCH_ABSENT_POSITION: f64 = features::FIELD_TERM_MATCH_ABSENT_POSITION;

#[pymodule]
fn alyze_features(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Analyzer>()?;
    m.add_class::<Analyzed>()?;
    m.add_class::<TermStats>()?;
    m.add_class::<ElementCompleteness>()?;
    m.add_class::<ElementSimilarity>()?;
    m.add_class::<TextSimilarity>()?;
    m.add_class::<FieldTermMatch>()?;
    m.add_class::<FieldMatch>()?;
    m.add_function(wrap_pyfunction!(legacy_significance, m)?)?;
    m.add(
        "FIELD_TERM_MATCH_ABSENT_POSITION",
        FIELD_TERM_MATCH_ABSENT_POSITION,
    )?;
    Ok(())
}
