# kuromoji golden tests

Test data for the Rust port of Lucene's `kuromoji` analyzer (`alyze-cjk/src/kuromoji/`), the
Japanese morphological analyzer behind Elasticsearch's `kuromoji` and `kuromoji_completion`
analyzers, `kuromoji_tokenizer`, the `kuromoji_iteration_mark` char filter and the
`kuromoji_baseform`, `kuromoji_part_of_speech`, `kuromoji_readingform`, `kuromoji_stemmer`,
`ja_stop`, `kuromoji_number`, `kuromoji_completion`, `hiragana_uppercase` and `katakana_uppercase`
filters (plus the `cjk_width` filter from analysis-common, which the recommended custom chain
uses).

The port is checked against the reference Java implementation by golden files: `gen.sh` runs
`lucene-analysis-kuromoji` over every input in `cases/` in a number of configurations and records
what it produced in `golden/`. The golden files are committed, so the Rust tests
(`src/kuromoji/tests/`) need no JVM.

## Layout

| Path                                | What                                                                   |
| ----------------------------------- | ---------------------------------------------------------------------- |
| `java/KuromojiGolden.java`          | The generator: drives Lucene and dumps tokens, tables, dictionaries    |
| `gen.sh`                            | Compiles and runs it for every file below                              |
| `cases/upstream.txt`                | Inputs from Lucene's and Elasticsearch's own kuromoji tests and docs   |
| `cases/edge.txt`                    | Hand-written edge cases (see below)                                    |
| `cases/long.txt`                    | The inputs of 1000+ characters from the two files above, run through fewer configurations |
| `cases/fuzz.txt`                    | Seeded random inputs, from `../../examples/kuromoji_gen_fuzz.rs`       |
| `cases/wiki_ja.txt`                 | ~512 KiB of Japanese Wikipedia, from `../../examples/kuromoji_wiki_sample.rs` |
| `cases/completion.txt`              | The subset of the above with bounded kana runs, from `tools/completion_cases.py` |
| `cases/dict_probes.txt`             | Dictionary lookups to record                                           |
| `cases/romaji.txt`                  | Katakana strings to romanize both ways                                 |
| `cases/numbers.txt`                 | Strings for the number filter's `normalizeNumber`                      |
| `userdict/*.txt`                    | User dictionaries: Lucene's, ES's and the docs' test ones, an `edge` one for the golden runs, and invalid ones |
| `stopwords/custom.txt`              | A custom stop-word list for the `stopwords` analyzer config and `stop_custom` chains |
| `golden/<case>.<config>.tokens`     | `JapaneseTokenizer` output (ES `kuromoji_tokenizer`) in one configuration |
| `golden/<case>.<config>.analyze`    | `JapaneseAnalyzer` output (ES `kuromoji` analyzer)                     |
| `golden/<case>.<config>.completion` | `JapaneseCompletionAnalyzer` output (ES `kuromoji_completion` analyzer) |
| `golden/<case>.<chain>.chain`       | (Char filter +) tokenizer + a custom filter chain, as ES users compose them |
| `golden/charfilter.<case>.<name>.txt` | A char filter's output text and offset map                           |
| `golden/romaji.txt`, `numbers.txt`  | Romanizations and number normalizations                                |
| `golden/chardef.txt`                | `CharacterDefinition` class of every UTF-16 code unit + class flags     |
| `golden/unicode.txt`                | JDK `Character.getType` of every code unit                             |
| `golden/lowercase.txt`              | `Character.toLowerCase(int)` wherever it changes a code point          |
| `golden/stoplists.txt`              | The jar's default stop tags and stop words                             |
| `golden/dict.txt`                   | Dictionary checksums, unknown-word entries, probe lookups              |
| `golden/userdict.<name>.txt`        | What `UserDictionary.open` builds from `userdict/<name>.txt`           |

## Configurations

Tokenizer configurations (`gen.sh`, `TokenizerConfig::named` in the tests); `default` is what
Elasticsearch's `kuromoji_tokenizer` uses out of the box:

| Name                      | mode     | punctuation | compound  | n-best                              | user dictionary     |
| ------------------------- | -------- | ----------- | --------- | ----------------------------------- | ------------------- |
| `default`                 | search   | discarded   | kept      | off                                 | none                |
| `normal`                  | normal   | discarded   | (n/a)     | off                                 | none                |
| `extended`                | extended | discarded   | kept      | off                                 | none                |
| `nocompound`              | search   | discarded   | discarded | off                                 | none                |
| `punct`                   | search   | kept        | kept      | off                                 | none                |
| `normal_punct`            | normal   | kept        | (n/a)     | off                                 | none                |
| `extended_punct`          | extended | kept        | kept      | off                                 | none                |
| `nbest`                   | search   | discarded   | kept      | cost 2000                           | none                |
| `normal_nbest`            | normal   | kept        | (n/a)     | cost 4000                           | none                |
| `nbest_examples`          | search   | discarded   | kept      | from `/鳩山積み-鳩山/鳩山積み-鳩/`  | none                |
| `userdict`                | search   | discarded   | kept      | off                                 | `userdict/edge.txt` |
| `userdict_normal`         | normal   | discarded   | (n/a)     | off                                 | `userdict/edge.txt` |
| `userdict_extended_punct` | extended | kept        | kept      | off                                 | `userdict/edge.txt` |

Note that Lucene's own `JapaneseAnalyzer` and its three-argument tokenizer constructor discard
compound tokens (`nocompound`), while Elasticsearch's `kuromoji_tokenizer` keeps them.

Analyzer configurations: `default`, `normal`, `extended`, `userdict` (`userdict/edge.txt`),
`stopwords` (`stopwords/custom.txt` with `stopwords_case: true`, replacing the default list).
Completion analyzer configurations: `index`, `query`, `userdict`, on `cases/completion.txt` only
(the completion filter emits every romaji keystroke variant of a reading, which is exponential
in the length of a kana run, so `tools/completion_cases.py` keeps the inputs whose kana runs are
at most 12 characters); the `completion_*` chains likewise. Chains (on the `default`
tokenizer unless noted): `baseform`; `pos` (default tags), `pos_docs` (the docs' two tags),
`pos_verb` (`動詞-自立`); `reading`, `romaji`, and both behind the `cjk_width` token filter
(`width_reading`, `width_romaji`); `stem` (minimum length 4), `stem6`; `stop` (default `ja_stop`),
`stop_custom` (`stopwords/custom.txt`, case-sensitive), `stop_custom_nocase`; `number` and
`number_nbest` (punctuation kept; the latter with n-best cost 2000 so the number filter sees
stacked tokens); `hiragana_upper`, `katakana_upper`; `completion_index`, `completion_query`
(`CJKWidthCharFilter` + normal-mode tokenizer without compounds + the completion filter, as the
completion analyzer builds them but without lowercasing); `itermark`, `itermark_kanji`,
`itermark_kana`, `itermark_none`, `itermark_punct` (the iteration-mark char filter in front of
the tokenizer); `width_char`, `width_char_punct` (`CJKWidthCharFilter` in front); `lowercase`;
`recommended` (the chain the Elasticsearch docs recommend: base form, part of speech, `cjk_width`,
`ja_stop`, stemmer, lowercase).

The small case files (`upstream`, `edge`) run through every configuration; `long` (their inputs of
1000+ characters: Lucene's curious strings and big document, the 1024-unit runs), `fuzz` and
`wiki_ja` only through the ones that exercise distinct code paths, to keep the golden files a
reasonable size (`gen.sh` has the list).

## Formats

Case files hold one input per line. Inputs are escaped so that any text fits on a line: `\\`,
`\n`, `\r`, `\t`, and `\u{hex}` for other C0/C1 controls, DEL, U+2028 and U+2029. Everything else
is written as-is in UTF-8.

Token goldens have a `# <case index>` header per input followed by one tab-separated line per
token:

    start byte, end byte, position increment, position length, token type (KNOWN/UNKNOWN/USER),
    part of speech, base form, reading, pronunciation, inflection type, inflection form, text

Offsets are UTF-8 byte offsets into the original input (converted from Lucene's UTF-16 offsets,
counting a surrogate pair's bytes on its high surrogate; after a char filter they are Lucene's
corrected offsets, so they refer to the original input while the text is the filtered text).
Strings are escaped like inputs; a null attribute is written as `\N`, which the escaper never
produces. The token type comes from the `ja.Token` behind the part-of-speech attribute, so it is
`\N` after the completion filter, which clears every attribute.

Lucene caps unknown words at 1024 UTF-16 code units, so a long run of supplementary characters is
cut inside a surrogate pair: the golden then has a token ending in a lone high surrogate and one
starting with the lone low surrogate (or, in extended mode, a lone-low-surrogate token with an
empty byte range), escaped as `\u{d800}`..`\u{dfff}`. The Rust tests normalize those to what the
port emits (whole code points), see `golden_to_toks`.

Goldens of stages that end with Lucene's `LowerCaseFilter` (the analyzers, the `lowercase` and
`recommended` chains) are compared against the port's pipeline *without* its lowercase filter,
through Java's simple lowercase mapping (`golden/lowercase.txt`); the port's own lowercasing is
checked separately (`golden::lowercase_is_the_only_difference`, `unicode::lowercase_matches_java
_except_expansions`). This keeps the one intended difference (alyze's full Unicode mapping) out
of the differential comparison without hiding anything else.

`charfilter.*.txt` has, per input, `text\t<filtered text>` and `map\t<fb>:<ob>...`: the original
byte offset (`correctOffset`) of every code-point boundary of the filtered text, including its
end. `romaji.txt` has, per input, `hepburn\t<ToStringUtil.getRomanization>` and
`keystrokes\t<n>\t<k>...` (`KatakanaRomanizer`, or `!` when the input isn't katakana plus ASCII
lowercase). `numbers.txt` has one normalized number per input line. `stoplists.txt` has `tag\t`
and `word\t` lines, sorted.

`chardef.txt` starts with one `class\t<name>\t<invoke>\t<group>` line per character class, then
`<first hex>\t<last hex>\t<class>` runs. `unicode.txt` has `<first hex>\t<last hex>\t<Character
.getType>` runs. `lowercase.txt` has `<hex>\t<hex>` pairs.

`dict.txt` starts with implementation-independent summaries (term and word counts and an FNV-1a
64 checksum over a canonical dump of the token-info dictionary; the unknown-word entries; the
connection-cost matrix dimensions and checksum; see the generator's javadoc for the exact bytes),
then the answers to the probes in `cases/dict_probes.txt`.

`userdict.<name>.txt` is `empty`, `error\t<exception>: <message>`, or `entry\t<key>\t<ord>\t<n>\t
<segment>...\t<reading>...\t<POS>` lines in term-index order. The rules go through what
Elasticsearch does to a `user_dictionary` file before handing it to Lucene (lines trimmed, blank
lines dropped, a repeated surface form dropped as with `lenient: true`), so the `dups` golden
shows the keep-first result; the strict duplicate error is checked natively.

## Edge cases

`cases/edge.txt` was written to hit every branch of the Java implementation: empty and
whitespace-only inputs and spaces of every kind (ASCII and ideographic spaces, tabs, line breaks,
NBSP, U+2028, zero-width and format characters); compounds at the search-mode penalty thresholds
(kanji runs of 2, 3 and more; other runs of 7, 8 and more) whose second-best segmentation is or
is not within the threshold, and the docs' and tests' decompounding examples; n-best inputs;
katakana with and without trailing prolonged sound marks at each length, small kana of every
kind including ㇷ゚; half-width katakana with voiced marks, orphan marks and double marks, and
fullwidth ASCII; iteration marks in every legal and illegal position (after punctuation, after 。
which flushes the filter, after a supplementary character, at the start, in runs, with voicing
changes); numbers with every separator, decimal point, sign, exponent kanji, width and stacked
tokens; unknown words of every character class with the first and last code point of every
`char.def` range and the one after each; supplementary characters (emoji, CJK extension B,
flags, ZWJ sequences, U+10FFFF) and noncharacters; every bracket, dash and quote; user-dictionary
interactions (exact matches, overlaps with compounds and system words, quoted keys, keys with
spaces and NBSP, half- and fullwidth keys, supplementary characters); and long inputs: unknown
runs over 1024 code units (including ones that put the cut inside a surrogate pair, with an odd
number of BMP characters in front), 1500 kana with no spaces (forcing the 1024-position
backtrace), the 1023 × あ + 手紙 shape from Lucene's tests, and long realistic text.

`cases/upstream.txt` holds every literal input of Lucene's kuromoji tests (including all 45
lines of `search-segmentation-tests.txt`, the five LUCENE-3897 "curious strings" and the
e-commerce document of `testBigDocument`, extracted from the Java sources by a script) and of
Elasticsearch's unit tests, REST tests and plugin docs. Two of the curious strings contain lone
surrogates in the Java source; they are replaced with U+FFFD, since the port's inputs are UTF-8.

## Regenerating

Needs a JDK (the goldens were made with JDK 27, Unicode 17, which matches the ICU data the port
uses and the JDK Elasticsearch bundles) and the Lucene 10.4.0 jars `lucene-core`,
`lucene-analysis-common` and `lucene-analysis-kuromoji` (from Maven Central; the 10.4.0 and
10.5.1 kuromoji jars carry byte-identical dictionaries and resources, and the module's sources
have only cosmetic changes up to Lucene main). Defaults: Homebrew's `openjdk` and
`~/Src/kuromoji/jars`; override with `JAVA_HOME` and `LUCENE_JARS`.

    cargo run -p alyze-cjk --example kuromoji_gen_fuzz      # only if the generator changed
    cargo run -p alyze-cjk --example kuromoji_wiki_sample   # only if the sample should change
    alyze-cjk/testdata/kuromoji/tools/completion_cases.py   # after any case file changed
    alyze-cjk/testdata/kuromoji/gen.sh

The port's dictionary blobs (`alyze-cjk/data/kuromoji/`) are converted from a full dump of
Lucene's dictionaries, which is too big to commit (and never from the mecab-ipadic CSVs: Lucene's
build order decides word order within a surface form, which decides lattice ties):

    alyze-cjk/testdata/kuromoji/gen.sh dump              # writes ~/Src/kuromoji/dump (KUROMOJI_DUMP_DIR)
    cargo run -p alyze-cjk --example kuromoji_convert_dict -- ~/Src/kuromoji/dump

## Large differential runs

The committed Wikipedia sample is small to keep the repo small. To run the port against a lot more
text (the parquet shard is ~176 MB; see the extractor's docs for the download):

    cargo run -p alyze-cjk --example kuromoji_wiki_sample -- --bytes 50000000 --out /tmp/ja.txt
    alyze-cjk/testdata/kuromoji/gen.sh tokens /tmp/ja.txt /tmp/ja.tokens
    KUROMOJI_CASES=/tmp/ja.txt KUROMOJI_TOKENS=/tmp/ja.tokens \
        cargo test -p alyze-cjk --release kuromoji::tests::golden::tokens_large -- --ignored

Any file in the case-file format works, so the same applies to a bigger fuzz set
(`--example kuromoji_gen_fuzz -- --cases 100000 --seed 7 --out /tmp/fuzz.txt`).

## Out of scope

Not covered, by decision: `ja_stop` with `remove_trailing: false` (Lucene's `SuggestStopFilter`
from the suggest module), the keyword-marker interaction of the base-form, stemmer and number
filters (`SetKeywordMarkerFilter`), Lucene's Graphviz lattice output, and Lucene's `bocchan.utf-8`
performance input (the Wikipedia sample serves for throughput).
