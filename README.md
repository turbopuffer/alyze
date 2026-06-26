# alyze

A high-performance tokenization and analysis implementation for full-text search. Provides a
[UAX #29](https://www.unicode.org/reports/tr29/) compliant tokenizer, implemented with a hand-rolled
deterministic finite automaton (DFA). Includes a complete analyzer implementation, with support for
lowercasing, ASCII case folding, stemming & stopword removal.

Currently in production at [turbopuffer](https://turbopuffer.com) powering the `word_v4` tokenizer.

### Benchmarks

Throughput over 64 MiB of English Wikipedia article text (`cargo bench`), running on an M5 Pro.
Numbers are the median of 16 samples.

**Tokenization** (`benches/wikipedia.rs`, `wikipedia` group):

| Benchmark                | Throughput |
| ------------------------ | ---------- |
| word break               | 508 MiB/s  |
| word break + `word_like` | 490 MiB/s  |
| sentence break           | 465 MiB/s  |

**Analysis** (`benches/wikipedia.rs`, `analysis` group) — each row adds one stage to the pipeline,
so the deltas approximate each filter's marginal cost:

| Pipeline                                              | Throughput |
| ----------------------------------------------------- | ---------- |
| tokenize only (case sensitive)                        | 415 MiB/s  |
| + lowercase                                           | 324 MiB/s  |
| + stopword removal (English)                          | 283 MiB/s  |
| + stemming (English)                                  | 132 MiB/s  |
| full (max length + stopwords + stemming + ASCII fold) | 126 MiB/s  |

Reproduce with `cargo bench --bench wikipedia` (first run downloads the Wikipedia dataset into
`.cache/`).

### Text-match features

The companion [`alyze-features`](alyze-features/) crate computes query/document text-match signals
(e.g. `fieldMatch`, `elementCompleteness`, `elementSimilarity`, `textSimilarity`) for use in a
second-stage re-ranker, mirroring
[Vespa's rank features](https://docs.vespa.ai/en/reference/ranking/rank-features.html) so the same
signals are available client-side. It's a separate, Apache-2.0 crate (see below) and lives in this
workspace.

### License

`alyze` is MIT licensed. The separate `alyze-features` crate is Apache-2.0 (its features are derived
from [Vespa](https://github.com/vespa-engine/vespa), Copyright Vespa.ai) — see
[`alyze-features/`](alyze-features/) for details. With gratitude to the Vespa team for their
excellent, well-documented work.
