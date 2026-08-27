---
type: concept
tags:
  - wiki
  - concept
  - analyzer
title: Token position monotonicity
description: Why every word-like token consumes a position even when a filter drops it, and why that matters for phrase distance.
---
## Definition

Every *word-like* token consumes the next position in a monotonically increasing counter, even if a later filter stage (length limit, stopword removal) goes on to drop that token entirely and it's never emitted to the caller.

## Why it matters

Phrase and proximity queries ("quick fox" as an exact phrase, or a proximity boost for nearby terms) depend on token positions reflecting the *original* text layout. If a dropped stopword didn't consume a position, `"the quick fox"` and `"quick fox"` would produce identical position sequences for `quick` and `fox` after stopword removal: silently collapsing a real difference in the source text. By advancing `next_position` before any filter gets a chance to drop the token, positions stay faithful to the original word count regardless of what stopword removal or length limits later filter out. This does mean position sequences can have gaps, which is intentional, not a bug. For example, tokenizing `the Quick fox` with English stopword removal:

| word | position assigned | emitted? |
|-------|-------|-------|
| the | 0 | no (stopword) |
| Quick | 1 | yes |
| fox | 2 | yes |

`Quick` keeps position 1, not 0: the gap left by the dropped `the` is preserved rather than closed up.

Across multiple inputs (`Analyzer::analyze_inputs`), positions are threaded monotonically across *all* of them, so a phrase match doesn't spuriously span two unrelated fields at the same low position numbers.

## Where it lives

- [src/analyze/mod.rs](../../src/analyze/mod.rs): `next_position` in `Analyzer::analyze_inputs`, advanced immediately after the tokenizer confirms `props.is_word_like()`, before the length/stopword/stemming filters run.
- Test: `byte_range_correct_after_filtering` and `input_index_and_byte_range_across_multiple_inputs` in the same file.

## Related

- [Analyzer](../modules/analyzer.md)
- [Tokenize + analyze a string](../flows/tokenize-and-analyze.md)
- [Greedy positional matching](./greedy-positional-match.md)