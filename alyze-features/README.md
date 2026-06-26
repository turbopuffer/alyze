# alyze-features

Client-side text-match ranking features for a second-stage re-ranker, mirroring
[Vespa's rank features](https://docs.vespa.ai/en/reference/ranking/rank-features.html) so the same
signals can be computed without the search backend. Built on [`alyze`](../) for tokenization/analysis.

Provides (all keyed to their Vespa feature names):

- `field_match` — the `fieldMatch` family (segment-based match metrics, ~28 outputs)
- `element_completeness`, `element_similarity`, `text_similarity`
- `matches`, `field_term_match`, `query_term_count`
- `legacy_significance` / `TermStats` — IDF significance from raw `(document_frequency,
  document_count)`, for the significance/weight-weighted outputs

## License

Apache-2.0. Unlike the MIT-licensed `alyze` crate, the implementations here are derived from
[Vespa](https://github.com/vespa-engine/vespa)'s reference implementation (Copyright Vespa.ai),
used under the Apache License 2.0 — hence this crate is kept separate and Apache-2.0 licensed. See
`LICENSE` and the header of `src/lib.rs`. With gratitude to the Vespa team.

This crate is not published.
