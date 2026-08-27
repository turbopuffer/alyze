---
type: guide
tags:
  - wiki
  - guide
  - how-to
  - benchmarks
title: Reproduce the throughput benchmarks
description: Running cargo bench --bench wikipedia and reading the tokenization/analysis results in the README.
---
## Goal

Reproduce the throughput numbers quoted in the root [README.md](../../README.md) (word break, sentence break, and the per-stage analysis pipeline breakdown) on your own machine.

## Steps

1. **Run the bench.** `cargo bench --bench wikipedia` from the repo root. The first run downloads 64 MiB worth of English Wikipedia article text (from the `wikimedia/wikipedia` Hugging Face dataset, as parquet shards) into `.cache/wikipedia/` and caches it there for every subsequent run.
2. **Read the two benchmark groups.** The `wikipedia` group measures raw tokenization: `word break`, `word break + word_like` (same tokenization, but also reading the `TokenProperties::is_word_like()` bit; measures that bit's marginal cost), and `sentence break`. The `analysis` group measures the full [Analyzer](../modules/analyzer.md) pipeline, adding one filter stage per row (`tokenize only (case sensitive)` → `+ lowercase` → `+ stopwords` → `+ stemming` → `full pipeline`) so the throughput delta between adjacent rows approximates that stage's marginal cost.
3. **Compare against the README's numbers**, which were captured on an M5 Pro with `sample_size(16)` (Criterion reports the median of 16 samples). Expect different absolute numbers on different hardware, but the *relative* deltas between pipeline stages (e.g. stemming being by far the most expensive stage) should hold.

## Relevant code

- [benches/wikipedia.rs](../../benches/wikipedia.rs): both benchmark functions, the parquet-loading/caching helpers, and the criterion group registration
- [README.md](../../README.md): the published throughput tables these benchmarks reproduce
- [Cargo.toml](../../Cargo.toml): `[[bench]] name = "wikipedia"`, `harness = false` (Criterion supplies its own harness)

## Gotchas

- **First run downloads real data over the network.** 41 parquet shard URLs against Hugging Face, stopping once 64 MiB of text is accumulated; subsequent runs reuse `.cache/wikipedia/` and don't re-download.
- **`benches/` and `testdata/` are excluded from the published crate** (`exclude = ["testdata/", "benches/"]` in [Cargo.toml](../../Cargo.toml)). You need the full git checkout, not just the crates.io package, to run these.
- **Every config asserts `options.valid()` before benchmarking.** If you add a new pipeline stage (see [Add a filter stage](./add-a-filter-stage.md)) with its own validity constraint, a benchmark config that violates it fails fast with a panic naming the bad config, not a silently wrong number.

## Related

- [Analyzer](../modules/analyzer.md)
- [Performance philosophy](../architecture/performance-philosophy.md)
- [Add a filter stage to the analysis pipeline](./add-a-filter-stage.md)