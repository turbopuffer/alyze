---
type: guide
tags:
  - wiki
  - guide
  - how-to
title: Add a filter stage to the analysis pipeline
description: Where a new normalization stage plugs into AnalysisOptions and Analyzer::analyze_inputs, and the invariants it must respect.
---
## Goal

Add a new normalization stage to the [Analyzer](../modules/analyzer.md)'s filter pipeline (today: length limit → lowercase → stopword removal → stemming → ASCII folding), in the right place and without breaking the pipeline's existing guarantees.

## Steps

1. **Add a field to `AnalysisOptions`.** In [src/analyze/mod.rs](../../src/analyze/mod.rs), following the existing convention: the struct's doc comment states fields are "ordered in the sequence they are applied in"; insert the new field where the new stage should run, not just at the end.
2. **Extend `AnalysisOptions::valid()` if the new stage has preconditions.** Stemming and stopword removal both require `case_sensitive: false`; if the new stage has an analogous constraint, encode it here so an invalid combination panics at `Analyzer::new` time, not partway through analysis.
3. **Add the filter call in `Analyzer::analyze_inputs`'s breakpoint closure**, in the position matching where you inserted the option field. Follow the existing gating pattern: check `TokenProperties` (or the token's current content) first, and only do the actual work, transitioning `InputRefOrBuffered` to `Buffered`, if the stage would actually change something. See [Borrow until you can't](../concepts/borrow-until-you-cant.md).
4. **Decide whether the stage can drop tokens.** If so, call `return true;` from the closure (skip, like stopword removal and the length filter do), but only *after* `next_position` has already been incremented, so [token position monotonicity](../concepts/token-position-monotonicity.md) still holds.
5. **Add a benchmark row.** [benches/wikipedia.rs](../../benches/wikipedia.rs)'s `analysis_benchmark` adds one pipeline stage per row specifically so the throughput delta between rows approximates each stage's marginal cost; add a new `configs` entry with the new stage enabled to measure it the same way.
6. **Add tests mirroring the existing ones** in `src/analyze/mod.rs`'s `#[cfg(test)] mod tests`. In particular, if the new stage can change a token's byte length, add a `byte_range_recovers_raw_when_<stage>_*` case (see [Byte-range recovery](../concepts/byte-range-recovery.md)).

## Relevant code

- [src/analyze/mod.rs](../../src/analyze/mod.rs): `AnalysisOptions`, `Analyzer::analyze_inputs`, `InputRefOrBuffered`
- [src/analyze/filters.rs](../../src/analyze/filters.rs): where the existing simple filter functions (length limit, ASCII folding) live; a good template for a stateless new stage
- [benches/wikipedia.rs](../../benches/wikipedia.rs): `analysis_benchmark`'s `configs` array

## Gotchas

- **Field order in `AnalysisOptions` is documentation, not enforcement.** Nothing stops the driver loop in `analyze_inputs` from applying stages out of the order the struct declares them in. Keep them in sync manually; a mismatch is a silent correctness bug (e.g. stemming before lowercasing would feed the stemmer non-normalized case).
- **Every fast path needs an actual proof of identity, not just a plausible-looking heuristic.** The crate has already hit this once: skipping stemming based on token shape looks safe but isn't (Finnish stems `"100"`). See [Performance philosophy](../architecture/performance-philosophy.md) before adding a new "skip when it looks unnecessary" branch.
- **A new stage that changes byte length must not break `byte_range` recovery.** `byte_range` always indexes the *original* input, never the normalized `text`; a new stage doesn't change this contract, but it's easy to accidentally couple the two if you're not careful with the `InputRefOrBuffered` transitions.
- **This binding surface is duplicated three times downstream.** If the new stage should be configurable from JS/Node/Python, it also needs an option field and validation added to all three [bindings](../modules/bindings.md)' `build_options` functions; see [A binding call, host language to Rust and back](../flows/binding-call.md).

## Related

- [Analyzer](../modules/analyzer.md)
- [Borrow until you can't](../concepts/borrow-until-you-cant.md)
- [Token position monotonicity](../concepts/token-position-monotonicity.md)
- [Performance philosophy](../architecture/performance-philosophy.md)
- [Tokenize + analyze a string](../flows/tokenize-and-analyze.md)