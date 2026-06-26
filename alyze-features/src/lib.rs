//! Computed text-match features for passing into a second stage re-ranker.
//!
//! These features mirror the text-match rank features described at
//! <https://docs.vespa.ai/en/reference/ranking/rank-features.html>, computed client-side so the
//! same signals are available without the search backend. The implementations here are derived
//! from Vespa's (<https://github.com/vespa-engine/vespa>, Copyright Vespa.ai) reference
//! implementation, used under the Apache License 2.0. With gratitude to the Vespa team for their
//! excellent and carefully documented work.
//!
//! SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use alyze::analyze::{Analyzer, ReusableBuffer};

/// Analyzed text containing tokens and their positions, used for computing features on query and document fields.
pub struct Analyzed {
    tokens: BTreeMap<String, Vec<usize>>,
}

impl Analyzed {
    /// Analyzes the given text and returns an `Analyzed` struct containing the tokens and their positions.
    /// Used for computing features on query and document fields.
    pub fn from_text(analyzer: &Analyzer, buffer: &mut ReusableBuffer, text: &str) -> Self {
        let mut tokens = BTreeMap::<String, Vec<usize>>::new();
        analyzer.analyze(text, buffer, |t| {
            tokens
                .entry(t.text.to_string())
                .or_default()
                .push(t.position);
            true
        });
        Self { tokens }
    }

    /// Total number of tokens, counting repeats (the element length).
    fn total_num_tokens(&self) -> usize {
        self.tokens.values().map(Vec::len).sum()
    }

    /// The token occupying the given position, if any. Used to resolve a query term by its index
    /// (the `fieldTermMatch` term ordinal), which equals the token position here since each
    /// word-like token consumes one position in order.
    fn token_at_position(&self, position: usize) -> Option<&str> {
        self.tokens
            .iter()
            .find(|(_, positions)| positions.contains(&position))
            .map(|(term, _)| term.as_str())
    }

    /// The tokens as an ordered sequence (by position), with repeats — the field/query token list
    /// the match features are computed over. Assumes contiguous positions (no gaps), which holds
    /// for the analysis config used here (no stopword removal).
    fn ordered_tokens(&self) -> Vec<&str> {
        let mut by_position: Vec<(usize, &str)> = self
            .tokens
            .iter()
            .flat_map(|(term, positions)| positions.iter().map(move |&p| (p, term.as_str())))
            .collect();
        by_position.sort_unstable_by_key(|&(p, _)| p);
        by_position.into_iter().map(|(_, term)| term).collect()
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct ElementCompleteness {
    /// `0.5 * field_completeness + 0.5 * query_completeness`.
    pub completeness: f64,
    /// Fraction of the field's tokens that are matched query terms.
    pub field_completeness: f64,
    /// Fraction of the query terms present in the field.
    pub query_completeness: f64,
}

/// Default `fieldCompletenessImportance` for the `elementCompleteness` feature (the `fieldMatch`
/// family uses 0.05 instead).
const ELEMENT_COMPLETENESS_FIELD_IMPORTANCE: f64 = 0.5;

/// `elementCompleteness(field)` — how completely the query matches the field, as token overlap.
/// `completeness` blends `queryCompleteness` (the share of query terms found in the field) and
/// `fieldCompleteness` (the share of field tokens that are query terms).
///
/// Query terms are not deduplicated: a term occurring more than once in the query counts once per
/// occurrence, so the matched count sums the query multiplicity of each distinct query term that
/// appears in the field.
pub fn element_completeness(query: &Analyzed, document: &Analyzed) -> ElementCompleteness {
    let total_query_terms = query.total_num_tokens();
    let field_length = document.total_num_tokens();
    if total_query_terms == 0 || field_length == 0 {
        return ElementCompleteness::default();
    }
    // Number of query-term occurrences whose term appears in the field (each query-term entry
    // contributes, so duplicate query terms each count).
    let matched: usize = query
        .tokens
        .iter()
        .filter(|(term, _)| document.tokens.contains_key(term.as_str()))
        .map(|(_, query_positions)| query_positions.len())
        .sum();
    let query_completeness = matched as f64 / total_query_terms as f64;
    // `matches` is floored against the element length so field completeness never exceeds 1.
    let field_completeness = matched.min(field_length) as f64 / field_length as f64;
    let factor = ELEMENT_COMPLETENESS_FIELD_IMPORTANCE;
    ElementCompleteness {
        completeness: factor * field_completeness + (1.0 - factor) * query_completeness,
        field_completeness,
        query_completeness,
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct ElementSimilarity {
    /// `0.35*proximity + 0.15*order + 0.30*query_coverage + 0.20*field_coverage`.
    pub similarity: f64,
    /// How close together the matched terms sit in the field.
    pub proximity: f64,
    /// How often the matched terms appear in the same order as in the query.
    pub order: f64,
    /// Fraction of the query terms that matched.
    pub query_coverage: f64,
    /// Fraction of the field tokens that matched.
    pub field_coverage: f64,
}

/// `elementSimilarity(field)` — a measure of the structural similarity between the query and the
/// best matching field element, blending how closely (`proximity`) and in what order (`order`) the
/// matched query terms appear with how much of the query (`query_coverage`) and field
/// (`field_coverage`) they cover.
///
/// For a single-value (one element) field this is numerically identical to the `textSimilarity`
/// feature; see [`text_similarity`].
pub fn element_similarity(query: &Analyzed, document: &Analyzed) -> ElementSimilarity {
    let Some(s) = greedy_match(query, document) else {
        return ElementSimilarity::default();
    };
    ElementSimilarity {
        similarity: 0.35 * s.proximity
            + 0.15 * s.order
            + 0.30 * s.query_coverage
            + 0.20 * s.field_coverage,
        proximity: s.proximity,
        order: s.order,
        query_coverage: s.query_coverage,
        field_coverage: s.field_coverage,
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct TextSimilarity {
    /// `0.35*proximity + 0.15*order + 0.30*query_coverage + 0.20*field_coverage`.
    pub score: f64,
    /// How close together the matched terms sit in the field.
    pub proximity: f64,
    /// How often the matched terms appear in the same order as in the query.
    pub order: f64,
    /// Fraction of the query terms that matched.
    pub query_coverage: f64,
    /// Fraction of the field tokens that matched.
    pub field_coverage: f64,
}

/// `textSimilarity(field)` — how well the query matches the field, ignoring term weight,
/// connectedness and significance. Outputs `score` (a normalized blend of the others), `proximity`,
/// `order`, `query_coverage` and `field_coverage`.
///
/// Shares the same greedy match as [`element_similarity`]; the difference is only that
/// `elementSimilarity` multiplies by the element weight and aggregates (max) across elements, which
/// is a no-op for a single-value field.
pub fn text_similarity(query: &Analyzed, document: &Analyzed) -> TextSimilarity {
    let Some(s) = greedy_match(query, document) else {
        return TextSimilarity::default();
    };
    TextSimilarity {
        score: 0.35 * s.proximity
            + 0.15 * s.order
            + 0.30 * s.query_coverage
            + 0.20 * s.field_coverage,
        proximity: s.proximity,
        order: s.order,
        query_coverage: s.query_coverage,
        field_coverage: s.field_coverage,
    }
}

struct SimilarityScores {
    proximity: f64,
    order: f64,
    query_coverage: f64,
    field_coverage: f64,
}

/// The greedy single-pass match shared by the `elementSimilarity` and `textSimilarity` features.
///
/// Processes query-term occurrences in document-position order, matching each to its first
/// document occurrence strictly after the previously matched position. Each query-term occurrence
/// (the query is not deduplicated) and each distinct document position is consumed at most once.
/// Returns `None` when nothing matches (the field has no query terms).
fn greedy_match(query: &Analyzed, document: &Analyzed) -> Option<SimilarityScores> {
    // One entry per query-term occurrence that also occurs in the document. `idx` is the query
    // position, which recovers query order (query term indices determine orderness).
    struct Item<'a> {
        idx: usize,
        positions: &'a [usize],
        cursor: usize,
    }
    let mut items: Vec<Item> = Vec::new();
    for (term, query_positions) in &query.tokens {
        if let Some(document_positions) = document.tokens.get(term) {
            for &idx in query_positions {
                items.push(Item {
                    idx,
                    positions: document_positions,
                    cursor: 0,
                });
            }
        }
    }
    if items.is_empty() {
        return None;
    }
    let field_length = document.total_num_tokens();

    // Index of the queue front: the item whose current document position is smallest, ties broken
    // by smaller query index (the match queue's tie-break order).
    fn front(items: &[Item]) -> usize {
        let mut best = 0;
        for i in 1..items.len() {
            let (pi, pb) = (
                items[i].positions[items[i].cursor],
                items[best].positions[items[best].cursor],
            );
            if pi < pb || (pi == pb && items[i].idx < items[best].idx) {
                best = i;
            }
        }
        best
    }

    // The first (lowest-position) occurrence seeds the match and is consumed whole.
    let f = front(&items);
    let first = items.swap_remove(f);
    let mut last_pos = first.positions[first.cursor];
    let mut last_idx = first.idx;
    let mut matched = 1usize;
    let mut sum_proximity = 0.0;
    let mut num_in_order = 0usize;

    while !items.is_empty() {
        let f = front(&items);
        let pos = items[f].positions[items[f].cursor];
        if pos > last_pos {
            sum_proximity += proximity_score(pos - last_pos);
            if items[f].idx > last_idx {
                num_in_order += 1;
            }
            last_pos = pos;
            last_idx = items[f].idx;
            matched += 1;
            items.swap_remove(f); // consume this query term whole
        } else {
            // This occurrence is not strictly after the last match; skip to the term's next one.
            items[f].cursor += 1;
            if items[f].cursor >= items[f].positions.len() {
                items.swap_remove(f);
            }
        }
    }

    // matched can never exceed field_length (each match is at a distinct position), but it is
    // floored anyway.
    let matches = matched.min(field_length);
    let (proximity, order) = if matched < 2 {
        (proximity_score(field_length), matches as f64)
    } else {
        let pairs = (matched - 1) as f64;
        (sum_proximity / pairs, num_in_order as f64 / pairs)
    };
    let query_coverage = matched as f64 / query.total_num_tokens() as f64;
    let field_coverage = matches as f64 / field_length as f64;

    Some(SimilarityScores {
        proximity,
        order,
        query_coverage,
        field_coverage,
    })
}

/// `matches(field)` — `1.0` if any query term occurs in the field, else `0.0`. This is a boolean
/// field-match flag, not a count (the count is the separate `matchCount` feature).
pub fn matches(query: &Analyzed, document: &Analyzed) -> f64 {
    let any = query
        .tokens
        .keys()
        .any(|term| document.tokens.contains_key(term.as_str()));
    if any { 1.0 } else { 0.0 }
}

#[derive(Clone, Copy, Debug)]
pub struct FieldTermMatch {
    /// First position of the term in the field; the `1_000_000` sentinel if absent.
    pub first_position: f64,
    /// Number of occurrences of the term in the field.
    pub occurrences: f64,
}

/// Sentinel returned for `firstPosition`/`lastPosition` when the term is not present in the field.
pub const FIELD_TERM_MATCH_ABSENT_POSITION: f64 = 1_000_000.0;

impl Default for FieldTermMatch {
    fn default() -> Self {
        Self {
            first_position: FIELD_TERM_MATCH_ABSENT_POSITION,
            occurrences: 0.0,
        }
    }
}

/// `fieldTermMatch(field, term_index)` — the `firstPosition` and number of `occurrences` of a query
/// term in the field (the `weight`/`exactness` outputs are omitted; they need term weights / corpus
/// data).
///
/// `term_index` is the query term ordinal. An out-of-range index, or a term absent from the field,
/// returns the absent sentinel position and zero occurrences.
pub fn field_term_match(
    query: &Analyzed,
    document: &Analyzed,
    term_index: usize,
) -> FieldTermMatch {
    let Some(term) = query.token_at_position(term_index) else {
        return FieldTermMatch::default();
    };
    match document.tokens.get(term) {
        Some(positions) if !positions.is_empty() => FieldTermMatch {
            first_position: positions[0] as f64, // positions are stored ascending
            occurrences: positions.len() as f64,
        },
        _ => FieldTermMatch::default(),
    }
}

/// `queryTermCount` — the number of terms in the query (with repeats).
pub fn query_term_count(query: &Analyzed) -> f64 {
    query.total_num_tokens() as f64
}

/// Per-query-term corpus statistics needed for the significance/IDF- and weight-dependent
/// text-match features. Supply the raw document frequency and corpus size; the significance is
/// derived from them (see [`legacy_significance`]) so the IDF formula has a single source of truth.
#[derive(Clone, Copy, Debug)]
pub struct TermStats {
    /// Number of documents containing the term (`df`).
    pub document_frequency: u64,
    /// Total number of documents in the corpus (`N`).
    pub document_count: u64,
    /// Query term weight (weight percent). Defaults to 100.
    pub weight: i32,
}

impl Default for TermStats {
    fn default() -> Self {
        // df/count = 0 yields the neutral significance 0.5; the default weight percent is 100.
        Self {
            document_frequency: 0,
            document_count: 0,
            weight: 100,
        }
    }
}

impl TermStats {
    /// This term's significance (normalized IDF); see [`legacy_significance`].
    pub fn significance(&self) -> f64 {
        legacy_significance(self.document_frequency, self.document_count)
    }
}

/// Term significance: inverse document frequency, normalized to `[0.5, 1]`. This is the
/// `significance` the text-match features use, and it matches the search backend's definition so
/// scores stay identical across a migration.
///
/// `N` is a fixed reference corpus size (1,000,000), not the real corpus size. The actual
/// `document_count` is used only to form the term's document-frequency ratio `df / count`; that
/// ratio is then projected onto the reference corpus and scored against `ln(N)`. Hardcoding the
/// reference makes significance depend on the frequency *ratio* rather than absolute corpus size,
/// so a term keeps the same significance as the corpus grows, and it bounds the output: a term in
/// `1` of `N` documents (or rarer) saturates at `1.0`, a term in every document is `0.5`. The
/// backend hardcodes the same `N`, so we do too — using the real corpus size would diverge from it.
pub fn legacy_significance(document_frequency: u64, document_count: u64) -> f64 {
    const N: f64 = 1_000_000.0;
    if document_count == 0 {
        return 0.5; // corner case: no documents
    }
    // Project the term's document-frequency ratio onto the reference corpus, clamped to [1, N].
    let frequency = (document_frequency as f64 * N / document_count as f64).clamp(1.0, N);
    let logcount = N.ln();
    let idf = logcount - frequency.ln();
    let normalized_idf = idf / logcount; // [0, 1]
    0.5 + 0.5 * normalized_idf // [0.5, 1]
}

/// Outputs of the `fieldMatch(field)` feature family — a detailed set of signals describing how
/// well the query matched the field, derived from the best segmentation of the matched terms.
///
/// `score` is the base `fieldMatch(field)` value (an aggregate quality score in `[0, 1]`). The last
/// six outputs depend on per-term significance (IDF) and/or weight, supplied via [`TermStats`]; the
/// rest are purely positional. Term connectedness is taken as the default 0.1.
#[derive(Default, Clone, Copy, Debug)]
pub struct FieldMatch {
    /// Aggregate match quality in `[0, 1]` (the base `fieldMatch(field)` value).
    pub score: f64,
    /// Normalized proximity of the matched terms, weighted by term connectedness.
    pub proximity: f64,
    /// Like `proximity`, but not divided by the average connectedness.
    pub absolute_proximity: f64,
    /// Normalized proximity of the matched terms, ignoring connectedness.
    pub unweighted_proximity: f64,
    /// Total completeness, weighting field completeness less than query completeness.
    pub completeness: f64,
    /// Ratio of query terms that were matched in the field.
    pub query_completeness: f64,
    /// Ratio of field tokens that are matched query terms.
    pub field_completeness: f64,
    /// How well term order agreed within segments: `1 - out_of_order / pairs`.
    pub orderness: f64,
    /// Degree to which matched terms occur together in the same segment.
    pub relatedness: f64,
    /// How early in the field the first matched segment occurs.
    pub earliness: f64,
    /// Length of the longest matched, in-order, contiguous run of terms.
    pub longest_sequence: f64,
    /// `longest_sequence / matches`.
    pub longest_sequence_ratio: f64,
    /// Number of field segments needed to match the query as completely as possible.
    pub segments: f64,
    /// Summed distance between the start of each adjacent matched segment.
    pub segment_distance: f64,
    /// Closeness of the matched segments in the field.
    pub segment_proximity: f64,
    /// Number of position jumps (forward or backward) within segments.
    pub gaps: f64,
    /// Summed size of all gaps within segments.
    pub gap_length: f64,
    /// Number of field tokens preceding the first matched segment.
    pub head: f64,
    /// Number of field tokens following the last matched segment.
    pub tail: f64,
    /// Number of out-of-order token pairs within segments.
    pub out_of_order: f64,
    /// Number of query terms matched in the field.
    pub matches: f64,
    /// Normalized measure of how often the query terms occur in the field.
    pub occurrence: f64,
    /// Like `occurrence`, but independent of field length.
    pub absolute_occurrence: f64,
    // --- depend on TermStats (significance / weight) ---
    /// Normalized sum of the matched terms' significance (IDF).
    pub significance: f64,
    /// Normalized sum of the matched terms' weight.
    pub weight: f64,
    /// Average of `significance` and `weight`.
    pub importance: f64,
    /// `occurrence` weighted by term weight.
    pub weighted_occurrence: f64,
    /// `absolute_occurrence` weighted by term weight.
    pub weighted_absolute_occurrence: f64,
    /// `occurrence` weighted by term significance.
    pub significant_occurrence: f64,
}

/// Computes the `fieldMatch(field)` feature family for a query and document field.
///
/// Implements the segment-finding match algorithm (a dynamic program that picks the best
/// segmentation of the matched terms) using the standard default parameters and connectedness 0.1.
/// `stats` supplies per-query-term corpus statistics; pass `|_| TermStats::default()` if only the
/// purely positional outputs are needed (the significance/weight outputs will then use the neutral
/// defaults and should be ignored).
pub fn field_match(
    query: &Analyzed,
    document: &Analyzed,
    stats: impl Fn(&str) -> TermStats,
) -> FieldMatch {
    let query_terms = query.ordered_tokens();
    let field_terms = document.ordered_tokens();
    let mut weights = Vec::with_capacity(query_terms.len());
    let mut significances = Vec::with_capacity(query_terms.len());
    for &term in &query_terms {
        let s = stats(term);
        weights.push(s.weight);
        significances.push(s.significance());
    }
    field_match::compute(&query_terms, &field_terms, &weights, &significances)
}

/// The `fieldMatch` string-match metric computer: it finds the best segmentation of the matched
/// query terms in the field and derives the metric outputs from it.
mod field_match {
    use std::collections::{BTreeMap, BTreeSet};

    use super::FieldMatch;

    // Standard default parameters for the match metric computer.
    const PROXIMITY_LIMIT: i32 = 10;
    const MAX_ALTERNATIVE_SEGMENTATIONS: i32 = 1000;
    const MAX_OCCURRENCES: i32 = 100;
    const PROXIMITY_COMPLETENESS_IMPORTANCE: f64 = 0.9;
    const RELATEDNESS_IMPORTANCE: f64 = 0.9;
    const EARLINESS_IMPORTANCE: f64 = 0.05;
    const SEGMENT_PROXIMITY_IMPORTANCE: f64 = 0.05;
    const OCCURRENCE_IMPORTANCE: f64 = 0.05;
    const FIELD_COMPLETENESS_IMPORTANCE: f64 = 0.05;
    // Term connectedness is the default for all terms; with this value `proximity` reduces to
    // `unweightedProximity` and the weighted accumulation below is `pairProximity * 0.1`.
    const CONNECTEDNESS: f64 = 0.1;
    const PROXIMITY_TABLE: [f64; (PROXIMITY_LIMIT * 2 + 1) as usize] = [
        0.01, 0.02, 0.03, 0.04, 0.06, 0.08, 0.12, 0.17, 0.24, 0.33, 1.0, //
        0.71, 0.50, 0.35, 0.25, 0.18, 0.13, 0.09, 0.06, 0.04, 0.03,
    ];

    /// Maps a semantic distance back to a field index from a starting point `zero_j`. Depends only
    /// on the field length. Returns -1 for an undefined (-1) semantic distance.
    fn semantic_distance_to_field_index(
        semantic_distance: i32,
        zero_j: i32,
        field_len: i32,
    ) -> i32 {
        if semantic_distance == -1 {
            return -1;
        }
        let first_segment_length = PROXIMITY_LIMIT.min(field_len - zero_j);
        let second_segment_length = PROXIMITY_LIMIT.min(zero_j);
        if semantic_distance < first_segment_length {
            zero_j + semantic_distance
        } else if semantic_distance < first_segment_length + second_segment_length {
            zero_j - semantic_distance - 1 + first_segment_length
        } else if semantic_distance < field_len - zero_j + second_segment_length {
            zero_j + semantic_distance - second_segment_length
        } else {
            field_len - semantic_distance - 1
        }
    }

    /// The inverse of [`semantic_distance_to_field_index`].
    fn field_index_to_semantic_distance(j: i32, zero_j: i32, field_len: i32) -> i32 {
        if j == -1 {
            return -1;
        }
        let first_segment_length = PROXIMITY_LIMIT.min(field_len - zero_j);
        let second_segment_length = PROXIMITY_LIMIT.min(zero_j);
        if j >= zero_j {
            if (j - zero_j) < first_segment_length {
                j - zero_j
            } else {
                j - zero_j + second_segment_length
            }
        } else if (zero_j - j - 1) < second_segment_length {
            zero_j - j + first_segment_length - 1
        } else {
            (zero_j - j - 1) + field_len - zero_j
        }
    }

    /// Accumulated match metrics for one segmentation. Cloned as alternative segmentations are explored.
    #[derive(Clone)]
    struct Metrics {
        query_len: i32,
        field_len: i32,
        total_term_weight: f64,
        total_significance: f64,
        out_of_order: i32,
        segments: i32,
        gaps: i32,
        gap_length: i32,
        longest_sequence: i32,
        head: i32,
        tail: i32,
        matches: i32,
        proximity: f64,
        unweighted_proximity: f64,
        segment_distance: f64,
        pairs: i32,
        weight: f64,       // accumulated matched weight / total weight
        significance: f64, // accumulated matched significance / total significance
        occurrence: f64,
        absolute_occurrence: f64,
        weighted_occurrence: f64,
        weighted_absolute_occurrence: f64,
        significant_occurrence: f64,
        current_sequence: i32,
        segment_starts: Vec<i32>,
    }

    impl Metrics {
        fn new(
            query_len: i32,
            field_len: i32,
            total_term_weight: f64,
            total_significance: f64,
        ) -> Self {
            Self {
                query_len,
                field_len,
                total_term_weight,
                total_significance,
                out_of_order: 0,
                segments: 0,
                gaps: 0,
                gap_length: 0,
                longest_sequence: 1,
                head: -1,
                tail: -1,
                matches: 0,
                proximity: 0.0,
                unweighted_proximity: 0.0,
                segment_distance: 0.0,
                pairs: 0,
                weight: 0.0,
                significance: 0.0,
                occurrence: 0.0,
                absolute_occurrence: 0.0,
                weighted_occurrence: 0.0,
                weighted_absolute_occurrence: 0.0,
                significant_occurrence: 0.0,
                current_sequence: 0,
                segment_starts: Vec::new(),
            }
        }

        // --- derived metric outputs ---

        /// Exactness is the query-weighted average match exactness; with no stemming every match
        /// is exact, so this is 1 when anything matched and 0 otherwise.
        fn exactness(&self) -> f64 {
            if self.matches == 0 { 0.0 } else { 1.0 }
        }

        fn absolute_proximity(&self) -> f64 {
            if self.pairs < 1 {
                0.1
            } else {
                self.proximity / self.pairs as f64
            }
        }

        fn unweighted_proximity(&self) -> f64 {
            if self.pairs < 1 {
                1.0
            } else {
                self.unweighted_proximity / self.pairs as f64
            }
        }

        fn proximity(&self) -> f64 {
            // averageConnectedness with uniform default connectedness is just CONNECTEDNESS.
            self.absolute_proximity() / CONNECTEDNESS
        }

        fn query_completeness(&self) -> f64 {
            self.matches as f64 / self.query_len as f64
        }

        fn field_completeness(&self) -> f64 {
            self.matches as f64 / self.field_len as f64
        }

        fn completeness(&self) -> f64 {
            self.query_completeness() * (1.0 - FIELD_COMPLETENESS_IMPORTANCE)
                + FIELD_COMPLETENESS_IMPORTANCE * self.field_completeness()
        }

        fn orderness(&self) -> f64 {
            if self.pairs == 0 {
                1.0
            } else {
                1.0 - self.out_of_order as f64 / self.pairs as f64
            }
        }

        fn relatedness(&self) -> f64 {
            match self.matches {
                0 => 0.0,
                1 => 1.0,
                _ => 1.0 - (self.segments - 1) as f64 / (self.matches - 1) as f64,
            }
        }

        fn longest_sequence_ratio(&self) -> f64 {
            if self.matches == 0 {
                0.0
            } else {
                self.longest_sequence as f64 / self.matches as f64
            }
        }

        fn segment_proximity(&self) -> f64 {
            if self.matches == 0 {
                0.0
            } else {
                1.0 - self.segment_distance / self.field_len as f64
            }
        }

        fn earliness(&self) -> f64 {
            if self.matches == 0 {
                0.0
            } else if self.field_len == 1 {
                1.0
            } else {
                1.0 - self.head as f64 / (6.max(self.field_len) - 1) as f64
            }
        }

        fn importance(&self) -> f64 {
            (self.significance + self.weight) / 2.0
        }

        fn segmentation_score(&self) -> f64 {
            if self.segments == 0 {
                0.0
            } else {
                self.absolute_proximity() * self.exactness()
                    / (self.segments * self.segments) as f64
            }
        }

        fn matchscore(&self) -> f64 {
            let scaled_relatedness =
                1.0 - RELATEDNESS_IMPORTANCE + RELATEDNESS_IMPORTANCE * self.relatedness();
            (PROXIMITY_COMPLETENESS_IMPORTANCE
                * scaled_relatedness
                * self.proximity()
                * self.exactness()
                * self.completeness()
                * self.completeness()
                + EARLINESS_IMPORTANCE * self.earliness()
                + SEGMENT_PROXIMITY_IMPORTANCE * self.segment_proximity()
                + OCCURRENCE_IMPORTANCE * self.occurrence)
                / (PROXIMITY_COMPLETENESS_IMPORTANCE
                    + EARLINESS_IMPORTANCE
                    + SEGMENT_PROXIMITY_IMPORTANCE
                    + OCCURRENCE_IMPORTANCE)
        }

        // --- events emitted by the computer ---

        fn on_match(&mut self, weight: i32, significance: f64) {
            if self.matches >= self.field_len {
                return;
            }
            self.matches += 1;
            if self.total_term_weight > 0.0 {
                self.weight += weight as f64 / self.total_term_weight;
            }
            if self.total_significance > 0.0 {
                self.significance += significance / self.total_significance;
            }
        }

        fn on_sequence_start(&mut self, j: i32) {
            if self.head == -1 || j < self.head {
                self.head = j;
            }
            self.current_sequence = 1;
        }

        fn on_sequence_end(&mut self, j: i32) {
            let sequence_tail = self.field_len - j - 1;
            if self.tail == -1 || sequence_tail < self.tail {
                self.tail = sequence_tail;
            }
            if self.current_sequence > self.longest_sequence {
                self.longest_sequence = self.current_sequence;
            }
            self.current_sequence = 0;
        }

        fn on_pair(&mut self, j: i32, previous_j: i32) {
            let mut distance = j - previous_j - 1;
            if distance < 0 {
                distance += 1; // discontinuity where the two terms are in the same position
            }
            if distance.abs() > PROXIMITY_LIMIT {
                return; // contribution = 0
            }
            let pair_proximity = PROXIMITY_TABLE[(distance + PROXIMITY_LIMIT) as usize];
            self.unweighted_proximity += pair_proximity;
            // pow(pairProximity, connectedness/0.1) * max(0.1, connectedness)
            self.proximity += pair_proximity.powf(CONNECTEDNESS / 0.1) * CONNECTEDNESS.max(0.1);
            self.pairs += 1;
        }

        fn on_in_sequence(&mut self) {
            self.current_sequence += 1;
        }

        fn on_in_segment_gap(&mut self, j: i32, previous_j: i32) {
            self.gaps += 1;
            if j > previous_j {
                self.gap_length += (j - previous_j).abs() - 1; // may be 0 if the gap was in the query
            } else {
                self.out_of_order += 1;
                self.gap_length += (j - previous_j).abs();
            }
        }

        fn on_new_segment(&mut self, j: i32) {
            self.segments += 1;
            self.segment_starts.push(j);
        }

        fn on_complete(&mut self) {
            if self.segment_starts.len() <= 1 {
                self.segment_distance = 0.0;
            } else {
                self.segment_starts.sort_unstable();
                for w in self.segment_starts.windows(2) {
                    self.segment_distance += (w[1] - w[0] + 1) as f64;
                }
            }
            if self.head == -1 {
                self.head = 0;
            }
            if self.tail == -1 {
                self.tail = 0;
            }
        }
    }

    /// Best-known metrics up to a query position `i`, carried while exploring segmentations.
    #[derive(Clone)]
    struct SegmentStartPoint {
        i: i32,
        skip_i: i32,
        metrics: Metrics,
        previous_j: i32,
        semantic_distance_explored: i32,
        open: bool,
    }

    impl SegmentStartPoint {
        fn first(metrics: Metrics) -> Self {
            Self {
                i: 0,
                skip_i: 0,
                metrics,
                previous_j: 0,
                semantic_distance_explored: 0,
                open: true,
            }
        }

        fn at(i: i32, previous_j: i32, metrics: Metrics) -> Self {
            Self {
                i,
                skip_i: 0,
                metrics,
                previous_j,
                semantic_distance_explored: 0,
                open: true,
            }
        }

        fn start_i(&self) -> i32 {
            self.i + self.skip_i
        }

        fn explored_to(&mut self, j: i32, field_len: i32) {
            self.semantic_distance_explored =
                field_index_to_semantic_distance(j, self.previous_j, field_len) + 1;
        }

        fn offer_history(&mut self, offered_previous_j: i32, offered: Metrics) {
            if offered.segmentation_score() <= self.metrics.segmentation_score() {
                return; // reject
            }
            self.previous_j = offered_previous_j;
            self.metrics = offered;
        }
    }

    struct Computer<'a> {
        query: &'a [&'a str],
        field: &'a [&'a str],
        weights: &'a [i32],
        significances: &'a [f64],
        metrics: Metrics,
        segment_start_points: Vec<Option<SegmentStartPoint>>,
        alternative_segmentations_tried: i32,
    }

    impl<'a> Computer<'a> {
        fn field_len(&self) -> i32 {
            self.field.len() as i32
        }

        fn find_closest_in_field_by_semantic_distance(
            &self,
            i: i32,
            previous_j: i32,
            start_semantic_distance: i32,
        ) -> i32 {
            let term = self.query[i as usize];
            let field_len = self.field_len();
            for distance in start_semantic_distance..field_len {
                let j = semantic_distance_to_field_index(distance, previous_j, field_len);
                if j >= 0 && term == self.field[j as usize] {
                    return distance;
                }
            }
            -1
        }

        fn segment_start(&mut self, j: i32, previous_j: i32) {
            self.metrics.on_new_segment(j);
            if previous_j >= 0 {
                self.metrics.on_pair(j, previous_j);
            }
        }

        fn in_segment(&mut self, i: i32, j: i32, previous_j: i32, previous_i: i32) {
            self.metrics.on_pair(j, previous_j);
            if j == previous_j + 1 && i == previous_i + 1 {
                self.metrics.on_in_sequence();
            } else {
                self.metrics.on_in_segment_gap(j, previous_j);
            }
        }

        fn segment_end(&mut self, i: i32, j: i32) {
            let next = (i + 1) as usize;
            match self.segment_start_points[next].as_mut() {
                None => {
                    self.segment_start_points[next] =
                        Some(SegmentStartPoint::at(i + 1, j, self.metrics.clone()));
                }
                Some(start_of_next) => start_of_next.offer_history(j, self.metrics.clone()),
            }
        }

        fn find_alternative_segment_from(&mut self, sp: usize) -> bool {
            let mut semantic_distance_explored = self.segment_start_points[sp]
                .as_ref()
                .unwrap()
                .semantic_distance_explored;
            let mut previous_i: i32 = -1;
            let mut previous_j = self.segment_start_points[sp].as_ref().unwrap().previous_j;
            let mut has_open_sequence = false;
            let mut is_first = true;
            let start_i = self.segment_start_points[sp].as_ref().unwrap().start_i();
            let query_len = self.query.len() as i32;
            let field_len = self.field_len();

            let mut i = start_i;
            while i < query_len {
                let semantic_distance = self.find_closest_in_field_by_semantic_distance(
                    i,
                    previous_j,
                    semantic_distance_explored,
                );
                let j = semantic_distance_to_field_index(semantic_distance, previous_j, field_len);

                if j == -1 && semantic_distance_explored > 0 && is_first {
                    return false; // segment explored before, no more matches
                }
                if has_open_sequence && (j == -1 || j != previous_j + 1) {
                    self.metrics.on_sequence_end(previous_j);
                    has_open_sequence = false;
                }
                if is_first {
                    if j != -1 {
                        self.segment_start(j, -1);
                        self.segment_start_points[sp]
                            .as_mut()
                            .unwrap()
                            .explored_to(j, field_len);
                        is_first = false;
                    } else {
                        self.segment_start_points[sp].as_mut().unwrap().skip_i += 1;
                    }
                } else if (j - previous_j).abs() >= PROXIMITY_LIMIT {
                    self.segment_end(i - 1, previous_j);
                    return true;
                } else if j != -1 {
                    self.in_segment(i, j, previous_j, previous_i);
                }

                if j != -1 {
                    let (w, s) = (self.weights[i as usize], self.significances[i as usize]);
                    self.metrics.on_match(w, s);
                }
                if j != -1 && !has_open_sequence {
                    self.metrics.on_sequence_start(j);
                    has_open_sequence = true;
                }
                semantic_distance_explored = if j != -1 { 1 } else { 0 };
                if j >= 0 {
                    previous_i = i;
                    previous_j = j;
                }
                i += 1;
            }

            if has_open_sequence {
                self.metrics.on_sequence_end(previous_j);
            }
            if !is_first {
                self.segment_end(query_len - 1, previous_j);
                true
            } else {
                false
            }
        }

        fn find_open_segment(&mut self, start_i: i32) -> Option<usize> {
            for k in (start_i as usize)..self.segment_start_points.len() {
                let Some(sp) = self.segment_start_points[k].as_ref() else {
                    continue;
                };
                if !sp.open {
                    continue;
                }
                if sp.semantic_distance_explored == 0 {
                    return Some(k); // first attempt
                }
                if self.alternative_segmentations_tried >= MAX_ALTERNATIVE_SEGMENTATIONS {
                    continue;
                }
                self.alternative_segmentations_tried += 1;
                return Some(k);
            }
            None
        }

        fn find_last_start_point(&self) -> usize {
            (0..self.segment_start_points.len())
                .rev()
                .find(|&k| self.segment_start_points[k].is_some())
                .expect("at least the initial start point exists")
        }

        fn explore_segments(&mut self) {
            self.segment_start_points[0] = Some(SegmentStartPoint::first(self.metrics.clone()));
            let mut current = Some(0usize);
            while let Some(ci) = current {
                self.metrics = self.segment_start_points[ci]
                    .as_ref()
                    .unwrap()
                    .metrics
                    .clone();
                let found = self.find_alternative_segment_from(ci);
                if !found {
                    self.segment_start_points[ci].as_mut().unwrap().open = false;
                }
                let start_i = self.segment_start_points[ci].as_ref().unwrap().i;
                current = self.find_open_segment(start_i);
            }
            let last = self.find_last_start_point();
            self.metrics = self.segment_start_points[last]
                .as_ref()
                .unwrap()
                .metrics
                .clone();
        }
    }

    /// Counts query-term occurrences in the field for the occurrence-based outputs (`occurrence`,
    /// `absoluteOccurrence`, and their weighted/significant variants).
    ///
    /// The unique-term set is built only from query terms that actually occur in the field,
    /// deduplicated by value (i.e. the distinct query-term values present in the field). This drives
    /// both the `divider` and the `absoluteOccurrence` denominator.
    fn occurrence_counts(
        query: &[&str],
        field: &[&str],
        term_stats: &BTreeMap<&str, (i32, f64)>,
        metrics: &mut Metrics,
    ) {
        let unique: BTreeSet<&str> = query
            .iter()
            .copied()
            .filter(|t| field.contains(t))
            .collect();
        if unique.is_empty() {
            return;
        }
        let unique_count = unique.len() as i32;
        let divider = (field.len() as i32).min(MAX_OCCURRENCES * unique_count);
        if divider == 0 {
            return;
        }
        let max_occurrence = (field.len() as i32).min(MAX_OCCURRENCES) as f64;

        let mut occurrence = 0.0;
        let mut absolute_occurrence = 0.0;
        let mut weighted_absolute_occurrence = 0.0;
        let mut total_weight: i64 = 0;
        let mut total_weighted_occurrences = 0.0;
        let mut total_significant_occurrences = 0.0;
        let mut weighted: Vec<f64> = Vec::with_capacity(unique.len());
        let mut significant: Vec<f64> = Vec::with_capacity(unique.len());

        for &term in &unique {
            let (weight, significance) = term_stats[term];
            let weight = weight as f64;
            let mut term_occurrences = 0;
            for &field_term in field {
                if field_term == term {
                    term_occurrences += 1;
                    if term_occurrences == MAX_OCCURRENCES {
                        break;
                    }
                }
            }
            let occ = term_occurrences as f64;
            occurrence += occ / divider as f64;
            absolute_occurrence += occ / (MAX_OCCURRENCES * unique_count) as f64;
            weighted_absolute_occurrence += occ * weight / MAX_OCCURRENCES as f64;
            total_weight += weight as i64;
            total_weighted_occurrences += max_occurrence * weight / divider as f64;
            weighted.push(occ * weight / divider as f64);
            total_significant_occurrences += max_occurrence * significance / divider as f64;
            significant.push(occ * significance / divider as f64);
        }

        metrics.occurrence = occurrence;
        metrics.absolute_occurrence = absolute_occurrence;
        metrics.weighted_absolute_occurrence = weighted_absolute_occurrence
            / if total_weight > 0 {
                total_weight as f64
            } else {
                1.0
            };
        metrics.weighted_occurrence = if total_weighted_occurrences > 0.0 {
            weighted
                .iter()
                .map(|w| w / total_weighted_occurrences)
                .sum()
        } else {
            0.0
        };
        metrics.significant_occurrence = if total_significant_occurrences > 0.0 {
            significant
                .iter()
                .map(|s| s / total_significant_occurrences)
                .sum()
        } else {
            0.0
        };
    }

    pub(super) fn compute(
        query: &[&str],
        field: &[&str],
        weights: &[i32],
        significances: &[f64],
    ) -> FieldMatch {
        if query.is_empty() || field.is_empty() {
            return FieldMatch::default();
        }
        let total_term_weight: f64 = weights.iter().map(|&w| w as f64).sum();
        let total_significance: f64 = significances.iter().sum();

        let mut computer = Computer {
            query,
            field,
            weights,
            significances,
            metrics: Metrics::new(
                query.len() as i32,
                field.len() as i32,
                total_term_weight,
                total_significance,
            ),
            segment_start_points: vec![None; query.len() + 1],
            alternative_segmentations_tried: 0,
        };
        computer.explore_segments();
        let mut m = computer.metrics;

        // Per distinct query-term value: its (weight, significance). All occurrences of a value
        // share the same stats, so first-occurrence wins.
        let mut term_stats: BTreeMap<&str, (i32, f64)> = BTreeMap::new();
        for (idx, &term) in query.iter().enumerate() {
            term_stats
                .entry(term)
                .or_insert((weights[idx], significances[idx]));
        }
        occurrence_counts(query, field, &term_stats, &mut m);
        m.on_complete();

        FieldMatch {
            score: m.matchscore(),
            proximity: m.proximity(),
            absolute_proximity: m.absolute_proximity(),
            unweighted_proximity: m.unweighted_proximity(),
            completeness: m.completeness(),
            query_completeness: m.query_completeness(),
            field_completeness: m.field_completeness(),
            orderness: m.orderness(),
            relatedness: m.relatedness(),
            earliness: m.earliness(),
            longest_sequence: m.longest_sequence as f64,
            longest_sequence_ratio: m.longest_sequence_ratio(),
            segments: m.segments as f64,
            segment_distance: m.segment_distance,
            segment_proximity: m.segment_proximity(),
            gaps: m.gaps as f64,
            gap_length: m.gap_length as f64,
            head: m.head as f64,
            tail: m.tail as f64,
            out_of_order: m.out_of_order as f64,
            matches: m.matches as f64,
            occurrence: m.occurrence,
            absolute_occurrence: m.absolute_occurrence,
            significance: m.significance,
            weight: m.weight,
            importance: m.importance(),
            weighted_occurrence: m.weighted_occurrence,
            weighted_absolute_occurrence: m.weighted_absolute_occurrence,
            significant_occurrence: m.significant_occurrence,
        }
    }
}

/// Per-gap proximity score used by `elementSimilarity`/`textSimilarity`: `1` at distance 1,
/// decaying quadratically to `0` at distance 9 and beyond. `dist` is the gap between two adjacent
/// matched token positions (always `>= 1`).
fn proximity_score(dist: usize) -> f64 {
    if dist >= 9 {
        0.0
    } else {
        let d = (dist as f64 - 1.0) / 8.0;
        1.0 - d * d
    }
}

#[cfg(test)]
mod tests {
    use alyze::analyze::{AnalysisOptions, Analyzer, ReusableBuffer, TokenizerOptions};

    use crate::{
        Analyzed, FIELD_TERM_MATCH_ABSENT_POSITION, TermStats, element_completeness,
        element_similarity, field_match, field_term_match, legacy_significance, matches,
        query_term_count, text_similarity,
    };

    fn analyzed(text: &str) -> Analyzed {
        let analyzer = Analyzer::new(AnalysisOptions {
            tokenizer: TokenizerOptions::UAX29Word(Default::default()),
            maximum_token_length: None,
            case_sensitive: false,
            stopword_removal: None,
            stemming: None,
            ascii_folding: false,
        });
        Analyzed::from_text(&analyzer, &mut ReusableBuffer::new(), text)
    }

    fn approx_eq(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    #[test]
    fn test_element_completeness() {
        let c = element_completeness(
            &analyzed("quick brown fox"),
            &analyzed("the the quick red fox jumps"),
        );
        approx_eq(c.field_completeness, 2.0 / 6.0);
        approx_eq(c.query_completeness, 2.0 / 3.0);
        approx_eq(c.completeness, 0.5 * (2.0 / 6.0) + 0.5 * (2.0 / 3.0));
    }

    #[test]
    fn test_element_similarity() {
        let s = element_similarity(
            &analyzed("quick brown fox"),
            &analyzed("the quick red fox jumps"),
        );

        // "quick" and "fox" match at doc positions 1 and 3, a gap of 2 tokens:
        // 1 - ((2 - 1) / 8)^2.
        let proximity = 1.0 - (1.0 / 8.0_f64).powi(2);
        // the matches keep query order ("quick" before "fox"), so all pairs agree.
        let order = 1.0;
        let query_coverage = 2.0 / 3.0;
        let field_coverage = 2.0 / 5.0;

        approx_eq(s.proximity, proximity);
        approx_eq(s.order, order);
        approx_eq(s.query_coverage, query_coverage);
        approx_eq(s.field_coverage, field_coverage);
        approx_eq(
            s.similarity,
            0.35 * proximity + 0.15 * order + 0.30 * query_coverage + 0.20 * field_coverage,
        );
    }

    #[test]
    fn test_element_completeness_duplicate_query_terms() {
        // Duplicate query terms each count (the query is not deduplicated): "repeat"
        // occurs twice, so the matched count is 3 (repeat, repeat, term) over 6 field tokens.
        let c = element_completeness(
            &analyzed("repeat repeat term"),
            &analyzed("repeat term and repeat term again"),
        );
        approx_eq(c.query_completeness, 3.0 / 3.0);
        approx_eq(c.field_completeness, 3.0 / 6.0);
        approx_eq(c.completeness, 0.5 * (3.0 / 6.0) + 0.5 * 1.0);
    }

    #[test]
    fn test_text_similarity_matches_element_similarity() {
        let q = analyzed("quick brown fox");
        let d = analyzed("the quick red fox jumps");
        let ts = text_similarity(&q, &d);
        let es = element_similarity(&q, &d);
        approx_eq(ts.score, es.similarity);
        approx_eq(ts.proximity, es.proximity);
        approx_eq(ts.order, es.order);
        approx_eq(ts.query_coverage, es.query_coverage);
        approx_eq(ts.field_coverage, es.field_coverage);
    }

    #[test]
    fn test_matches() {
        approx_eq(
            matches(&analyzed("quick fox"), &analyzed("the quick red fox")),
            1.0,
        );
        approx_eq(
            matches(&analyzed("zebra"), &analyzed("the quick red fox")),
            0.0,
        );
    }

    #[test]
    fn test_field_term_match() {
        let q = analyzed("alpha beta");
        let d = analyzed("beta alpha beta gamma");
        // term 0 = "alpha": first at field position 1, occurs once.
        let t0 = field_term_match(&q, &d, 0);
        approx_eq(t0.first_position, 1.0);
        approx_eq(t0.occurrences, 1.0);
        // term 1 = "beta": first at position 0, occurs twice.
        let t1 = field_term_match(&q, &d, 1);
        approx_eq(t1.first_position, 0.0);
        approx_eq(t1.occurrences, 2.0);
        // out-of-range term index -> absent sentinel.
        let t2 = field_term_match(&q, &d, 2);
        approx_eq(t2.first_position, FIELD_TERM_MATCH_ABSENT_POSITION);
        approx_eq(t2.occurrences, 0.0);
    }

    #[test]
    fn test_field_match() {
        // Golden values confirmed against the live reference `fieldMatch(field)`. The positional
        // outputs are independent of term stats, so default (uniform) stats are fine here.
        let m = field_match(
            &analyzed("quick brown fox"),
            &analyzed("the quick red fox jumps"),
            |_| TermStats::default(),
        );
        // Integer / exact-rational metrics.
        approx_eq(m.matches, 2.0);
        approx_eq(m.segments, 1.0);
        approx_eq(m.gaps, 1.0);
        approx_eq(m.gap_length, 1.0);
        approx_eq(m.out_of_order, 0.0);
        approx_eq(m.longest_sequence, 1.0);
        approx_eq(m.head, 1.0);
        approx_eq(m.tail, 1.0);
        approx_eq(m.orderness, 1.0);
        approx_eq(m.relatedness, 1.0);
        approx_eq(m.longest_sequence_ratio, 0.5);
        approx_eq(m.query_completeness, 2.0 / 3.0);
        approx_eq(m.field_completeness, 2.0 / 5.0);
        approx_eq(m.completeness, (2.0 / 3.0) * 0.95 + 0.05 * (2.0 / 5.0));
        approx_eq(m.earliness, 1.0 - 1.0 / 5.0); // 1 - head/(max(6,5)-1)
        approx_eq(m.occurrence, 0.4);
        approx_eq(m.absolute_occurrence, 0.01);
        approx_eq(m.segment_distance, 0.0);
        approx_eq(m.segment_proximity, 1.0);
        // proximity: single pair one token apart -> proximityTable lookup = 0.71.
        assert!(
            (m.proximity - 0.71).abs() < 1e-6,
            "proximity {}",
            m.proximity
        );
        assert!(
            (m.absolute_proximity - 0.071).abs() < 1e-6,
            "absProx {}",
            m.absolute_proximity
        );
        // Aggregate score (the reference implementation returned 0.364527230941695).
        assert!(
            (m.score - 0.364527230941695).abs() < 1e-6,
            "score {}",
            m.score
        );
    }

    #[test]
    fn test_legacy_significance() {
        // df=1 in a 1M corpus is maximally significant; df=1000 lands at the [0.5,1] midpoint.
        approx_eq(legacy_significance(1, 1_000_000), 1.0);
        approx_eq(legacy_significance(1000, 1_000_000), 0.75);
        approx_eq(legacy_significance(5, 0), 0.5); // no documents -> neutral
    }

    #[test]
    fn test_field_match_idf_weighted() {
        let q = analyzed("quick brown fox");
        let d = analyzed("the quick red fox jumps");
        let stats = |t: &str| {
            let (document_frequency, document_count) = match t {
                "quick" => (1u64, 1_000_000u64),
                "brown" => (1000, 1_000_000),
                "fox" => (500_000, 1_000_000),
                _ => (0, 0),
            };
            TermStats {
                document_frequency,
                document_count,
                weight: 100,
            }
        };
        let m = field_match(&q, &d, stats);

        let sig_quick = legacy_significance(1, 1_000_000);
        let sig_fox = legacy_significance(500_000, 1_000_000);
        let total_sig = sig_quick + legacy_significance(1000, 1_000_000) + sig_fox;

        // Matched terms: "quick" and "fox" ("brown" is absent from the field).
        approx_eq(m.weight, 2.0 / 3.0); // uniform weights -> matched/total query terms
        approx_eq(m.significance, (sig_quick + sig_fox) / total_sig);
        approx_eq(
            m.importance,
            ((sig_quick + sig_fox) / total_sig + 2.0 / 3.0) / 2.0,
        );
        approx_eq(m.weighted_occurrence, 0.2);
        approx_eq(m.weighted_absolute_occurrence, 0.01);
        approx_eq(m.significant_occurrence, 0.2);
    }

    #[test]
    fn test_query_term_count() {
        approx_eq(query_term_count(&analyzed("a b c")), 3.0);
        approx_eq(query_term_count(&analyzed("a a b")), 3.0); // counts repeats
    }
}
