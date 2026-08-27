---
type: guide
tags:
  - wiki
  - guide
  - how-to
title: Recover the raw source text from a normalized token
description: Using Token::byte_range and Token::input_index to get back the exact original substring for highlighting or offsets.
---
## Goal

Given a normalized `Token` (post-lowercasing/stemming/folding), get back the exact original substring it came from, e.g. to highlight the matched span in a search result UI, using the user's original casing and characters rather than the normalized form.

## Steps

1. **Keep the original input string around.** `Token::byte_range` indexes into whichever `&str` you passed to `Analyzer::analyze` (or the corresponding element of the iterator passed to `analyze_inputs`); you need that same string alive at the point you read the token.
2. **Slice it directly: `&input[token.byte_range.clone()]`.** This is always a valid UTF-8 boundary slice into the *original* text, regardless of what normalization did to `token.text`.
3. **For `analyze_inputs` over multiple strings, index by `token.input_index` first.** `byte_range` is relative to that specific input, not a global offset across all inputs; recover with `&inputs[token.input_index][token.byte_range.clone()]`.

## Relevant code

- [src/analyze/mod.rs](../../src/analyze/mod.rs): `Token` struct fields (`byte_range`, `input_index`) and the `tests` module's `byte_range_recovers_raw_*` / `input_index_and_byte_range_across_multiple_inputs` tests, which are effectively worked examples of this exact recipe.

## Gotchas

- **`byte_range` is not into `token.text`.** `token.text` may be shorter, longer, or contain entirely different bytes than the raw span (ASCII folding and stemming both shrink byte length; Unicode lowercasing can change which code point is used). Slicing `token.text` with `byte_range` will panic or silently return the wrong bytes.
- **The input string must still be alive and unchanged.** `Token`'s lifetime is tied to the `analyze`/`analyze_inputs` call; you can't stash a `byte_range` and read it back against the input later if the input has been dropped, moved, or mutated in the meantime.

## Related

- [Byte-range recovery](../concepts/byte-range-recovery.md)
- [Borrow until you can't](../concepts/borrow-until-you-cant.md)
- [Analyzer](../modules/analyzer.md)