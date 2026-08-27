---
title: Wiki Log
description: Append-only audit trail of wiki generation and refresh runs.
---
# Wiki Log

Append-only audit trail. Add one dated entry per generation or refresh run, recording the profile, the `source_commit` it was anchored to, and the coverage. The codebase-wiki skill describes the entry shape.

## 2026-07-18: generate

- Profile: internal/standard
- source_commit: 5a2e090
- Coverage: user-directed, data-structure-focused pass. Four major components, each folding its smaller components in: the DFA tokenizer, the analyzer filter pipeline (incl. stemming cache, stopwords, lowercasing tables), the alyze-features rank-features crate, and the wasm/node/python bindings. Architecture/flows/concepts/guides sections not yet written.
- Pages: [Overview](./OVERVIEW.md), [Tokenizer](./modules/tokenizer.md), [Analyzer](./modules/analyzer.md), [alyze-features](./modules/alyze-features.md), [Bindings](./modules/bindings.md)

## 2026-07-18: generate (expansion)

- Profile: internal/standard
- source_commit: 5a2e090
- Coverage: full pass. Added architecture (2 pages), flows (3), concepts (9), and guides (4) on top of the existing modules section. Research for the new sections was done via `tg` (turbogrep) semantic search over the whole repo, cross-checked against direct reads of the matched source files.
- Pages: [Workspace & crate boundaries](./architecture/workspace-boundaries.md), [Performance philosophy](./architecture/performance-philosophy.md), [Tokenize + analyze a string](./flows/tokenize-and-analyze.md), [Compute a rank feature](./flows/compute-rank-feature.md), [A binding call](./flows/binding-call.md), [UAX #29 boundaries](./concepts/uax29-boundaries.md), [DFA state machine](./concepts/dfa-state-machine.md), [TokenProperties bitmask](./concepts/token-properties-bitmask.md), [Borrow until you can't](./concepts/borrow-until-you-cant.md), [Stemming cache](./concepts/stemming-cache.md), [Perfect-hash stopwords](./concepts/perfect-hash-stopwords.md), [Byte-range recovery](./concepts/byte-range-recovery.md), [Token position monotonicity](./concepts/token-position-monotonicity.md), [Greedy positional matching](./concepts/greedy-positional-match.md), [Add a language](./guides/add-a-language.md), [Add a filter stage](./guides/add-a-filter-stage.md), [Reproduce benchmarks](./guides/reproduce-benchmarks.md), [Recover raw text](./guides/recover-raw-text-from-a-token.md)

## 2026-07-18: style pass

- Coverage: removed em dashes from every page's frontmatter (titles) and body text across all 24 wiki documents, replacing them with commas, colons, semicolons, or separate sentences depending on context. No content changes beyond punctuation.

## 2026-07-20: refresh (PR-history evidence pass)

- Profile: internal/standard
- source_commit: 1de437c (branch `at/filter-behavior-tests`; trunk had advanced to 5a2e090 + nightly-bench)
- Coverage: cross-checked contributor-facing pages against the full turbopuffer/alyze PR history (review comments, reviews, open PRs) rather than code alone. Handoff doc: marked items 3/5 in flight as upstream PRs #29/#30, rewrote item 11 as answered-in-history (Morgan parked npm/PyPI publishing on PR #25), added an open-PR overlap warning (#26 touches filters.rs; #27 will shift every cited line number), fixed one stale line ref (feature_result! macro, 358). Contributor guide: added per-reviewer lens breakdown (morgangallant, jpountz, pushrax, benesch, mattcuento, mlpuff), a "mine PR history for reviewer asks" first step, the tests/doc-comments zero-cost lane, the parked-decisions gotcha, and sharpened the perf framing: perf wins are welcome (#15, #17); the SIMDmaxx genre (complexity/memory for throughput) is what gets declined.
- Pages: [Work handoff: bindings cleanup tasks](./guides/bindings-work-handoff.md), [Become an effective contributor](./guides/become-an-effective-contributor.md)

## 2026-07-20: consolidate (contributor guide rewrite)

- Coverage: restructured [Become an effective contributor](./guides/become-an-effective-contributor.md) from the day's accreted patch-edits into one coherent piece. New order: the system-level resource rubric first (derived from alyze's read/write-path position in turbopuffer: latency wins nearly worthless, memory the most expensive spend, CPU reductions free money, determinism absolute), then process and reviewer lenses, merge/reject examples restated through the rubric (added #15 as a merged example), a new "three lanes that work" section (reviewer-requested work, tests/doc comments, #17-shaped perf), and a new step: ask "is this win interesting at the system level?" before "does it bench well?". Also restored the wiki's no-em-dash style, which the patch edits had violated.
- Pages: [Become an effective contributor](./guides/become-an-effective-contributor.md)

## 2026-07-20: refresh (item 8 verification)

- Profile: internal/standard
- source_commit: 96b3345 (branch `at/filter-behavior-tests`)
- Coverage: verified handoff item 8 (node vs python surface drift) against the actual binding sources. Diffed the public surfaces method-by-method; found no real drift — the apparent differences (`TermStats` plain-object vs `#[pyclass]` construction, `i64`-clamp vs `u64` integer types, `significance()` as a free function vs a method) are all pyo3-vs-napi idiom, not divergence. Rewrote item 8 from an open suspicion to a dated "checked, satisfied" verdict. Also added the workspace-boundary guardrail to item 13 (fix via separate CI jobs, not workspace membership).
- Pages: [Work handoff: bindings cleanup tasks](./guides/bindings-work-handoff.md)