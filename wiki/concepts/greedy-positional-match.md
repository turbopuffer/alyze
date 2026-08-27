---
type: concept
tags:
  - wiki
  - concept
  - alyze-features
title: Greedy positional matching
description: The single-pass algorithm alyze-features uses to align query terms to document positions for the similarity features.
---
## Definition

Lines up query words with document words: walk through the document's occurrences of the query terms in position order, and each time, grab whichever not-yet-used query-term occurrence can attach there first. When two occurrences tie on document position, the one earlier in the query wins.

That's alyze-features's private `greedy_match`, a single-pass, greedy algorithm private to [alyze-features](../modules/alyze-features.md). It produces `proximity`, `order`, `query_coverage`, `field_coverage` (bundled as `SimilarityScores`), the shared building blocks behind `elementSimilarity` and `textSimilarity`. `fieldMatch` is a separate feature with its own segment-based matching algorithm; it does not reuse `greedy_match`.

### Worked example

Query: `cat the mat` (query positions 0, 1, 2). Document: `the cat sat on the mat`, so `the` occurs at document positions 0 and 4, `cat` at 1, `mat` at 5.

| step | candidates (term@position) | picked | why |
|---|---|---|---|
| 1 | the@0, cat@1, mat@5 | the@0 | smallest position, seeds the match |
| 2 | cat@1, mat@5 | cat@1 | smallest position left, and 1 is after 0 |
| 3 | mat@5 | mat@5 | only one left, and 5 is after 1 |

Three matches over two consecutive pairs: `the` (query index 1) to `cat` (query index 0) is out of order, since the query index went down. `cat` to `mat` (query index 2) is in order, since it went up. So `order` comes out to 1/2. Note that `the`'s second occurrence, at document position 4, is never even looked at: once `the` is picked at step 1, that query-term occurrence is fully consumed and dropped from consideration for good.

If a term's next candidate position isn't strictly after the last match (for example, a repeated query word whose only usable document occurrence was already claimed by an earlier match), it produces no match: the algorithm just advances to that occurrence's next candidate position, or drops it once it runs out of positions to try.

## Why it matters

Finding the mathematically optimal alignment between query terms and document positions costs more than a single greedy pass, and Vespa's own reference implementation, which these features are deliberately mirroring, uses this same greedy strategy. Matching that choice, not just the output formulas, is what keeps alyze's client-side scores consistent with a Vespa-style backend's scores for the same input. The algorithm makes one pass with no backtracking: each query-term occurrence is either matched or dropped, and never revisited once that happens.

## Where it lives

- [alyze-features/src/lib.rs](../../alyze-features/src/lib.rs): the private `greedy_match` function and its `SimilarityScores` result, called from both `element_similarity` and `text_similarity`.

## Related

- [alyze-features](../modules/alyze-features.md)
- [Compute a rank feature](../flows/compute-rank-feature.md)
- [Token position monotonicity](./token-position-monotonicity.md)