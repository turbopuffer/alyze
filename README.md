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
| word break               | 529 MiB/s  |
| word break + `word_like` | 491 MiB/s  |
| sentence break           | 468 MiB/s  |

**Analysis** (`benches/wikipedia.rs`, `analysis` group) — each row adds one stage to the pipeline,
so the deltas approximate each filter's marginal cost:

| Pipeline                                              | Throughput |
| ----------------------------------------------------- | ---------- |
| tokenize only (case sensitive)                        | 402 MiB/s  |
| + lowercase                                           | 375 MiB/s  |
| + stopword removal (English)                          | 316 MiB/s  |
| + stemming (English)                                  | 139 MiB/s  |
| full (max length + stopwords + stemming + ASCII fold) | 134 MiB/s  |

Reproduce with `cargo bench --bench wikipedia` (first run downloads the Wikipedia dataset into
`.cache/`).

### CJK analyzers

Enable `smartcn` (Chinese), `nori` (Korean), or `kuromoji` (Japanese) independently, or
use `cjk` to enable all three. All are disabled by default.

```toml
[dependencies]
alyze = { version = "0.2.0", features = ["nori"] }
```

Enabled analyzers are available under `alyze::cjk`, for example `alyze::cjk::nori`.
Only enabled analyzers and their dictionaries are compiled. Published packages download
and verify dictionaries at build time; see [offline build instructions](src/cjk/README.md#dictionary-builds).

Use the common analyzer API with any enabled CJK pipeline:

```rust
use alyze::analyze::{Analyzer, ReusableBuffer};

let analyzer = Analyzer::nori(Default::default());
let mut buffer = ReusableBuffer::new();
analyzer.analyze("한국어를 분석합니다", &mut buffer, |token| {
    println!("{}: {}", token.position, token.text);
    true
});
```

Use `Analyzer::smartcn`, `Analyzer::nori`, or `Analyzer::kuromoji` with the corresponding
options from `alyze::cjk`. Reuse the buffer across calls; keep any borrowed user dictionaries
or stop lists alive for as long as the analyzer.

### Text-match features

The companion [`alyze-features`](alyze-features/) crate computes query/document text-match signals
(e.g. `fieldMatch`, `elementCompleteness`, `elementSimilarity`, `textSimilarity`) for use in a
second-stage re-ranker, mirroring
[Vespa's rank features](https://docs.vespa.ai/en/reference/ranking/rank-features.html) so the same
signals are available client-side. It's a separate, Apache-2.0 crate (see below) and lives in this
workspace.

### License

`alyze` contains MIT-licensed code and Apache-2.0-licensed CJK implementations derived from
[Apache Lucene](https://lucene.apache.org/). See [LICENSE](LICENSE),
[LICENSE-APACHE](LICENSE-APACHE), [NOTICE](NOTICE), and the
[CJK documentation](src/cjk/README.md) for dictionary attribution and licensing.

The separate `alyze-features` crate is Apache-2.0; its features derive from
[Vespa](https://github.com/vespa-engine/vespa), Copyright Vespa.ai.
