# Changelog

## 0.2.0 — October 20, 2026

- Add SmartCN (Chinese), Nori (Korean), and Kuromoji (Japanese) analyzers under `alyze::cjk`.
  Enable them individually with `smartcn`, `nori`, or `kuromoji`, or enable all three with `cjk`.
  All are disabled by default.
- Add `TokenizerOptions::Whitespace`, which splits on Unicode whitespace and preserves punctuation.
  It supports case normalization, ASCII folding, and token length limits; stemming and stopword
  removal are unsupported.
- `Analyzer` now has a lifetime parameter for borrowed CJK dictionaries and stop lists.
  Existing callers storing an analyzer without borrowed configuration can use `Analyzer<'static>`.
- Add `Token::position_length` to represent compound tokens spanning multiple positions.
  Overlapping tokens can share a position.
- Require Rust 1.88 or later.
- Download enabled CJK dictionaries at build time from versioned GitHub Release assets and verify
  their SHA-256 checksums. Dictionaries are embedded in the executable; runtime needs no network
  access or dictionary files. For offline builds, set `ALYZE_CJK_DICTIONARIES` to a directory
  containing the release archives. See [dictionary build instructions](src/cjk/README.md#dictionary-builds).

## June 18, 2026

- `analyze`: add `Token::byte_range` and `Token::input_index` to recover a token's raw source substring

## May 8, 2026

- `analyze` module, support for stopword removal, stemming, ascii_folding, maximum_token_length, case sensitivity

## Apr 14, 2026

- Initial crate release
- UAX #29 compliant word and sentence tokenizer
