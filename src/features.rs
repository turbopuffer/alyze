//! Computed features for passing into a second stage re-ranker.

use std::collections::BTreeMap;

use crate::analyze::{Analyzer, ReusableBuffer};

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

    /// Number of distinct tokens, ignoring repeats.
    fn num_unique_tokens(&self) -> usize {
        self.tokens.len()
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct ElementCompleteness {
    pub completeness: f64, // `0.5 * field_completeness + 0.5 * query_completeness`
    pub field_completeness: f64, // fraction of the document's tokens that are matched query terms
    pub query_completeness: f64, // fraction of the query's unique terms present in the document
}

/// Computes the element completeness feature (Vespa) for a query and document field, e.g. a
/// measure of how well the document field matches the query in terms of token overlap.
pub fn element_completeness(query: &Analyzed, document: &Analyzed) -> ElementCompleteness {
    if query.tokens.is_empty() || document.tokens.is_empty() {
        return ElementCompleteness::default();
    }
    let overlapping_tokens = query
        .tokens
        .keys()
        .filter(|term| document.tokens.contains_key(term.as_str()))
        .count();
    let query_completeness = overlapping_tokens as f64 / query.num_unique_tokens() as f64;
    let field_completeness = overlapping_tokens as f64 / document.total_num_tokens() as f64;
    ElementCompleteness {
        completeness: 0.5 * field_completeness + 0.5 * query_completeness,
        field_completeness,
        query_completeness,
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct ElementSimilarity {
    pub similarity: f64, // `0.35*proximity + 0.15*order + 0.30*query_coverage + 0.20*field_coverage`
    pub proximity: f64,  // how close together matched terms sit in the document
    pub order: f64,      // how often matched terms appear in the same order as in the query
    pub query_coverage: f64, // fraction of the distinct query terms that matched
    pub field_coverage: f64, // fraction of matched terms over document length
}

/// Computes the element similarity feature (Vespa) for a query and document field, blending how
/// closely and in what order the matched query terms appear with how much of the query and document
/// they cover.
pub fn element_similarity(query: &Analyzed, document: &Analyzed) -> ElementSimilarity {
    if query.tokens.is_empty() {
        return ElementSimilarity::default();
    }

    // For each matched term, its (document position, query position). The query
    // position recovers query order, which the alphabetical keys throw away.
    let mut matched: Vec<(usize, usize)> = query
        .tokens
        .iter()
        .filter_map(|(term, query_positions)| {
            let document_position = document.tokens.get(term)?[0];
            Some((document_position, query_positions[0]))
        })
        .collect();
    if matched.is_empty() {
        return ElementSimilarity::default();
    }
    matched.sort_unstable();

    let num_matched = matched.len();
    // Floor the length at the match count so coverage never exceeds 1.
    let num_tokens = document.total_num_tokens().max(num_matched);

    let mut sum_proximity = 0.0;
    let mut num_in_order = 0;
    for pair in matched.windows(2) {
        let (prev_document, prev_query) = pair[0];
        let (document, query) = pair[1];
        sum_proximity += proximity_score(document - prev_document);
        if query > prev_query {
            num_in_order += 1;
        }
    }

    let (proximity, order) = if num_matched < 2 {
        // No pair to score: fall back to the proximity of the whole element.
        (proximity_score(num_tokens), num_matched as f64)
    } else {
        let num_pairs = (num_matched - 1) as f64;
        (sum_proximity / num_pairs, num_in_order as f64 / num_pairs)
    };
    let query_coverage = num_matched as f64 / query.num_unique_tokens() as f64;
    let field_coverage = num_matched as f64 / num_tokens as f64;

    ElementSimilarity {
        similarity: 0.35 * proximity + 0.15 * order + 0.30 * query_coverage + 0.20 * field_coverage,
        proximity,
        order,
        query_coverage,
        field_coverage,
    }
}

/// Vespa's per-gap proximity score: `1` at distance 1, decaying quadratically to
/// `0` at distance 9 and beyond. `dist` is the gap between two adjacent matched
/// token positions (always `>= 1`).
fn proximity_score(dist: usize) -> f64 {
    if dist > 8 {
        0.0
    } else {
        let d = (dist as f64 - 1.0) / 8.0;
        1.0 - d * d
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        analyze::{AnalysisOptions, Analyzer, ReusableBuffer, TokenizerOptions},
        features::{Analyzed, element_completeness, element_similarity},
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
}
