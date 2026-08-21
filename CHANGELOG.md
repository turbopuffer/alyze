# Changelog

## August 21, 2026

- `analyze`: replace `rust-stemmers` with [`frostem`], which tracks upstream Snowball. Stemming
  goes from 18 languages to 35: Armenian, Basque, Catalan, Czech, Esperanto, Estonian, Hindi,
  Indonesian, Irish, Lithuanian, Nepali, Persian, Polish, Serbian, Sesotho and Yiddish are new.
- **Breaking (stem output):** `rust-stemmers` last shipped in May 2021 and its algorithms had
  drifted from Snowball. Every existing language now matches upstream's reference vectors exactly,
  which changes stems for some inputs — Romanian (15.2% of Snowball's Romanian vocabulary),
  Turkish (9.6%), Italian (5.0%), French (4.2%), German (3.6%), Swedish (1.8%), Norwegian (0.7%),
  Finnish (0.2%), Russian (0.2%), English (0.1%), Danish (0.1%), Spanish (0.03%), Greek and
  Arabic (one word each). Hungarian, Portuguese and Tamil are unchanged. Existing indexes need a
  reindex for the affected languages.
- **Breaking (Dutch):** Snowball replaced its Dutch algorithm and renamed the old one
  `dutch_porter`. `StemmingLanguage::Dutch` now uses the current algorithm, which disagrees with
  the old one on 45.8% of Snowball's Dutch vocabulary. The previous behavior is available
  byte-for-byte as `StemmingLanguage::DutchPorter` (`"dutch_porter"` in the bindings).
- **Breaking (API):** `impl Into<rust_stemmers::Algorithm> for StemmingLanguage` is now
  `impl From<StemmingLanguage> for frostem::Algorithm`. Call sites using `.into()` are unaffected.

[`frostem`]: https://github.com/Xuanwo/frostem

## June 18, 2026

- `analyze`: add `Token::byte_range` and `Token::input_index` to recover a token's raw source substring

## May 8, 2026

- `analyze` module, support for stopword removal, stemming, ascii_folding, maximum_token_length, case sensitivity

## Apr 14, 2026

- Initial crate release
- UAX #29 compliant word and sentence tokenizer