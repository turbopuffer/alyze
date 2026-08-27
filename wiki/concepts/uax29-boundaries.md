---
type: concept
tags:
  - wiki
  - concept
  - unicode
title: "UAX #29 word/sentence boundaries"
description: The Unicode Text Segmentation standard that defines where one word or sentence ends and the next begins.
---
## Definition

[UAX #29](https://www.unicode.org/reports/tr29/) (Unicode Standard Annex #29, "Unicode Text Segmentation") is the specification for where word, sentence, and grapheme-cluster boundaries fall in Unicode text, e.g. why `"won't"` is one word, why `"example.com"` doesn't break at its period, why an emoji ZWJ sequence like 👨‍👩 stays together as one unit.

## Why it matters

A search engine's tokenizer and its query analyzer must segment text identically, or a document that should match a query silently won't (see [Performance philosophy](../architecture/performance-philosophy.md)). Implementing the actual UAX #29 rules, rather than a simpler approximation like split-on-whitespace, is what makes alyze's segmentation correct across contractions, abbreviations, Hebrew punctuation, CJK ideographs, regional-indicator flag sequences, and emoji, not just English prose.

## Where it lives

- [src/uax29/word/](../../src/uax29/word/mod.rs) and [src/uax29/sentence/](../../src/uax29/sentence/mod.rs) implement the word- and sentence-boundary rules as a [DFA state machine](./dfa-state-machine.md).
- [testdata/WordBreakTest.txt](../../testdata/WordBreakTest.txt) and [testdata/SentenceBreakTest.txt](../../testdata/SentenceBreakTest.txt) are the official Unicode conformance test suites; alyze's test suite runs all 1944 word-break cases and asserts zero failures.

## Related

- [DFA state machine](./dfa-state-machine.md)
- [Tokenizer](../modules/tokenizer.md)
- [Tokenize + analyze a string](../flows/tokenize-and-analyze.md)