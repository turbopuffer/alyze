---
type: guide
tags:
  - wiki
  - guide
  - contributing
  - handoff
title: "Work handoff: bindings cleanup tasks"
description: "Task list for the v0 fallout in the node/python bindings: link, the suspicion, and the code to reference."
---
## Context

The Python and TypeScript bindings were merged as a deliberate v0 (reviewer on [PR #24](https://github.com/turbopuffer/alyze/pull/24): "looks good as a v0 ship and then let's fix forward whatever falls out"). This page lists the fallout we found, verified against the code on 2026-07-18; status re-checked against PR history on 2026-07-20. Each task: the link, why we think it's real, and the exact code to reference while doing it.

All PRs go to [turbopuffer/alyze](https://github.com/turbopuffer/alyze), reviewed by Morgan Gallant. Small separate PRs, each with `cargo fmt` + `cargo test` passing. See [Become an effective contributor](./become-an-effective-contributor.md).

Before picking an item, check open upstream PRs — they change what's worth doing and where the code will be:

- [PR #27](https://github.com/turbopuffer/alyze/pull/27) (Morgan, lazy tokenization + clippy) touches nearly every file this page cites (`src/analyze/*`, `src/uax29/*`, `wasm/src/lib.rs`) and adds a golden test suite. When it lands, every line reference below shifts; re-verify before use.
- [PR #26](https://github.com/turbopuffer/alyze/pull/26) (Jacse, perf) touches `src/analyze/filters.rs` and `mod.rs`, including a fast-path for the token-length check — overlaps the code items 5 covers.
- Items 3 and 5 are already in flight as [PR #29](https://github.com/turbopuffer/alyze/pull/29) and [PR #30](https://github.com/turbopuffer/alyze/pull/30); don't restart them.

## Trivial

### 1. `uv` in the Python README
- Link: [reviewer comment](https://github.com/turbopuffer/alyze/pull/24#discussion_r3505924684)
- Suspicion: reviewer mlpuff asked for `uv pip install maturin` instead of `pip install maturin`; never changed.
- Code: [python/README.md](../../python/README.md) line 29, the Install section's local-development block.

### 2. CHANGELOG backfill
- Link: [CHANGELOG.md](../../CHANGELOG.md)
- Suspicion: last entry is June 18, 2026. Missing since then: alyze-features crate ([#23](https://github.com/turbopuffer/alyze/pull/23)), Python SDK ([#24](https://github.com/turbopuffer/alyze/pull/24)), TypeScript SDK ([#25](https://github.com/turbopuffer/alyze/pull/25)), wasm `sentences()` ([#28](https://github.com/turbopuffer/alyze/pull/28)), HAS_ASCII_UPPER perf ([#17](https://github.com/turbopuffer/alyze/pull/17)).
- Code: [CHANGELOG.md](../../CHANGELOG.md) for the entry format (date heading, one bullet per change); `git log --oneline --since=2026-06-18` for what landed and when.

## Tests

### 3. Validation test the #19 reviewer asked for
- Link: [reviewer nit](https://github.com/turbopuffer/alyze/pull/19#discussion_r3425384227). The nit's "e.g." was open-ended; the full set is below.
- Suspicion: `build_options` has 5 validation cases; only the 2 stemming ones are tested. Untested: stopwords+case_sensitive, stopwords with an unsupported language, and max_token_length out of range (0 or >255).
- Status: in flight as [PR #29](https://github.com/turbopuffer/alyze/pull/29), awaiting review.
- Code: the test to extend is `test_analyzer_validation` in [python/tests/test_features.py](../../python/tests/test_features.py). All 5 checks and their exact error strings are in `build_options`, [python/src/lib.rs](../../python/src/lib.rs) lines 117-161. `stopword_language` is at line 199; Tamil/Arabic/Greek/Romanian/Turkish are absent from it, so any of those works as the unsupported-language case. The max_token_length bounds check is at lines 155-161; use 0 and 256 as the boundary inputs.

### 4. Stemming tests for the other 17 languages
- Suspicion: only English stemming is tested anywhere in the repo; the other 17 `StemmingLanguage` variants have zero coverage.
- Code: the enum is `StemmingLanguage` in [src/analyze/mod.rs](../../src/analyze/mod.rs) lines 65-84; its `Into<rust_stemmers::Algorithm>` mapping is right below at lines 86-108. The one existing English test to copy the shape from is `byte_range_recovers_raw_when_stemming_shrinks_bytes` at line ~539 of the same file. Python-side, the existing English-only test is `test_analyzer_stemming` in [python/tests/test_features.py](../../python/tests/test_features.py) lines 127-148. Expected stems come from Snowball's published test vectors (snowballstem.org, per-language `voc.txt`/`output.txt`).

### 5. `max_token_length` and `ascii_folding` tests
- Suspicion: `maximum_token_length` has zero tests (no test ever sets it); `ascii_folding` has one case (café).
- Status: in flight as [PR #30](https://github.com/turbopuffer/alyze/pull/30) (branch `at/filter-behavior-tests`), awaiting review. Watch [PR #26](https://github.com/turbopuffer/alyze/pull/26), which adds a fast-path to the same token-length check.
- Code: the two filter functions are `within_token_length_limit` ([src/analyze/filters.rs](../../src/analyze/filters.rs) line 42) and `ascii_fold` / `fold_non_ascii_char` (lines 116 and 132 of the same file). The option fields are on `AnalysisOptions` in [src/analyze/mod.rs](../../src/analyze/mod.rs) lines 14-23. The one existing folding test to extend is `byte_range_recovers_raw_when_ascii_folding_shrinks_bytes` at line ~527. For the position-gap assertion, copy the shape of `byte_range_correct_after_filtering` at line ~576.

### 6. Non-Latin input through the bindings
- Suspicion: the Rust tokenizer is tested on Hebrew/CJK/Thai, but no binding test ever sends non-Latin text across the FFI boundary.
- Code: pull test inputs from the tokenizer's own test module in [src/uax29/word/mod.rs](../../src/uax29/word/mod.rs) (Hebrew gershayim cases ~line 370, CJK `中` ~line 490, Thai `ก` ~line 498). Drive them through `Analyzer.analyze` in [python/src/lib.rs](../../python/src/lib.rs) line 107 and the equivalent in [node/src/lib.rs](../../node/src/lib.rs) line 105.

### 7. Error-message tests
- Suspicion: error text is meant to match the turbopuffer API's wording but nothing pins it.
- Code: every message string lives in `build_options`: [python/src/lib.rs](../../python/src/lib.rs) lines ~115-170 and [node/src/lib.rs](../../node/src/lib.rs) lines 115-176. Assert the exact strings from both.

## API polish

### 8. node vs python surface drift
- Checked 2026-07-20 — no real drift. Diffed the surfaces method-by-method; what looks like divergence is just pyo3 vs napi idiom, not a mismatch:
  - `TermStats` construction: python is a `#[pyclass]` with a `#[new]`; node is a `#[napi(object)]` plain object literal. Each is the only correct idiom for its platform, same capability.
  - Integer types: node takes `i64` and clamps `.max(0) as u64` (JS has no u64); python takes `u64` directly. Same values reach the core; both default `weight` to 100.
  - `significance()`: a method on python's `TermStats`, a free `legacySignificance(df, N)` on node (a plain object can't carry methods). Only genuine surface difference, and a defensible idiom choice — not a bug.
- Verdict: satisfied. Nothing to fix unless a future feature is added to one binding and not the other.
- Code (for re-checks): the full public surfaces are the `#[napi]` items in [node/src/lib.rs](../../node/src/lib.rs) and the `#[pyclass]`/`#[pymethods]` items in [python/src/lib.rs](../../python/src/lib.rs).

### 9. `.pyi` type stubs
- Suspicion: none exist; IDE autocomplete on the compiled module is poor.
- Code: everything to stub is in [python/src/lib.rs](../../python/src/lib.rs): `Analyzer` (line 61), `Analyzed` (line 225), `TermStats` (line 312), and the macro that generates the result classes (`feature_result!`, line 358, one class per feature result). Ship the stub via `[tool.maturin]` in [python/pyproject.toml](../../python/pyproject.toml).

### 10. Do the README examples run?
- Suspicion: nobody has likely executed them since merge.
- Code: the snippets in [python/README.md](../../python/README.md) and [node/README.md](../../node/README.md), checked against the actual method names and signatures in [python/src/lib.rs](../../python/src/lib.rs) and [node/src/lib.rs](../../node/src/lib.rs) (e.g. `field_match(document, stats)`, `legacy_significance`, result-object field names like `importance` / `weighted_occurrence` at python/src/lib.rs ~line 441).

## Structural (ask Morgan before building)

### 11. Publish to PyPI/npm?
- Answered in history — it's a decision, not backlog. On [PR #25](https://github.com/turbopuffer/alyze/pull/25), RogutKuba asked why not publish to npm; Morgan: "probably best to have this unpublished for the time being" (unsure of npm + napi build best practices). Don't build release CI unless he reopens it.
- Code (only if reopened): [python/pyproject.toml](../../python/pyproject.toml) and [node/package.json](../../node/package.json) are already publish-shaped; the work would be CI release jobs in [.github/workflows/](https://github.com/turbopuffer/alyze/tree/trunk/.github/workflows).

### 12. Sentence boundaries for node/python
- Suspicion: wasm exposes sentence segmentation ([#28](https://github.com/turbopuffer/alyze/pull/28)); node/python have no way to get it.
- Code: the function to mirror is `sentences()` in [wasm/src/lib.rs](../../wasm/src/lib.rs) (its output type `SentenceRange` is at line 110); it wraps `uax29::sentence::tokenize` in [src/uax29/sentence/mod.rs](../../src/uax29/sentence/mod.rs).

### 13. CI never runs the binding test suites
- Suspicion: verified. CI runs `cargo fmt` + `cargo test --workspace --exclude alyze-wasm`; node/python aren't workspace members, so they're never compiled or tested in CI.
- Code: the workflow is [.github/workflows/ci.yml](../../.github/workflows/ci.yml). The suites that exist but never run: [python/tests/test_features.py](../../python/tests/test_features.py) (runner: `maturin develop && pytest`) and [node/__test__/features.test.mjs](../../node/__test__/features.test.mjs) (runner: `npm ci && npm test`, per the `test` script in [node/package.json](../../node/package.json)). Do this one first; nothing else on this page gates without it.
- Shape of the fix: separate CI jobs that build each binding with its own toolchain (`maturin develop && pytest`, `npm ci && npm test`). The bindings stay out of the root workspace by design — each has its own one-member `[workspace]` so `cargo test --workspace` never links PyO3/napi (see the comment in [python/Cargo.toml](../../python/Cargo.toml) / [node/Cargo.toml](../../node/Cargo.toml)).

## Related

- [Become an effective contributor](./become-an-effective-contributor.md)
- [Bindings](../modules/bindings.md)
- [A binding call, host language to Rust and back](../flows/binding-call.md)