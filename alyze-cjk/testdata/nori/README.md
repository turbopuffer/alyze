# nori golden tests

Test data for the Rust port of Lucene's `nori` analyzer (`alyze-cjk/src/nori/`), the Korean
morphological analyzer behind Elasticsearch's `nori` analyzer, `nori_tokenizer`,
`nori_part_of_speech`, `nori_readingform` and `nori_number`.

The port is checked against the reference Java implementation by golden files: `gen.sh` runs
`lucene-analysis-nori` over every input in `cases/` in a number of configurations and records what
it produced in `golden/`. The golden files are committed, so the Rust tests (`src/nori/tests/`)
need no JVM.

## Layout

| Path                            | What                                                                 |
| ------------------------------- | -------------------------------------------------------------------- |
| `java/NoriGolden.java`          | The generator: drives Lucene and dumps tokens, tables, dictionaries  |
| `gen.sh`                        | Compiles and runs it for every file below                            |
| `cases/upstream.txt`            | Inputs from Lucene's and Elasticsearch's own nori tests              |
| `cases/edge.txt`                | Hand-written edge cases (see below)                                  |
| `cases/fuzz.txt`                | Seeded random inputs, from `../../examples/nori_gen_fuzz.rs`         |
| `cases/wiki_ko.txt`             | ~512 KiB of Korean Wikipedia, from `../../examples/nori_wiki_sample.rs` |
| `cases/dict_probes.txt`         | Dictionary lookups to record                                         |
| `userdict/*.txt`                | User dictionaries: Lucene's and ES's test ones, an `edge` one for the golden runs, and invalid ones |
| `golden/<case>.<config>.tokens` | `KoreanTokenizer` output (ES `nori_tokenizer`) in one configuration   |
| `golden/<case>.<config>.analyze`| `KoreanAnalyzer` output (ES `nori` analyzer)                         |
| `golden/<case>.<chain>.chain`   | Tokenizer + a custom filter chain, as ES users compose them          |
| `golden/chardef.txt`            | `CharacterDefinition` class of every UTF-16 code unit + class flags   |
| `golden/unicode.txt`            | JDK general category, script and `isDigit` of every code unit        |
| `golden/lowercase.txt`          | `Character.toLowerCase(int)` wherever it changes a code point        |
| `golden/dict.txt`               | Dictionary checksums, unknown-word entries, probe lookups            |
| `golden/userdict.<name>.txt`    | What `UserDictionary.open` builds from `userdict/<name>.txt`         |

## Configurations

Tokenizer configurations (`gen.sh`, `TokenizerConfig::named` in the tests); `default` is what
Elasticsearch uses out of the box:

| Name                   | decompound | punctuation | unknown unigrams | user dictionary     |
| ---------------------- | ---------- | ----------- | ---------------- | ------------------- |
| `default`              | discard    | discarded   | no               | none                |
| `mixed`                | mixed      | discarded   | no               | none                |
| `none`                 | none       | discarded   | no               | none                |
| `punct`                | discard    | kept        | no               | none                |
| `mixed_punct`          | mixed      | kept        | no               | none                |
| `unigrams`             | discard    | discarded   | yes              | none                |
| `userdict`             | discard    | discarded   | no               | `userdict/edge.txt` |
| `userdict_mixed_punct` | mixed      | kept        | no               | `userdict/edge.txt` |

Analyzer configurations: `default`, `mixed` (decompound mixed), `stoptags` (`NNP,NNG,NR,SP`),
`userdict` (`userdict/edge.txt`). Chains (on the `default` tokenizer unless noted): `reading`;
`number`; `number_punct` (punctuation kept, `SP` stopped, number: Lucene's own test setup);
`number_mixed_punct` (same with decompound mixed, so the number filter sees stacked tokens);
`pos_custom` (the tag list from Lucene's factory test); `graph` (decompound mixed + default stop
tags: the setup of ES's phrase-query test); `lowercase`.

The small case files (`upstream`, `edge`) run through every configuration; `fuzz` and `wiki_ko`
only through the ones that exercise distinct code paths, to keep the golden files a reasonable
size.

## Formats

Case files hold one input per line. Inputs are escaped so that any text fits on a line: `\\`,
`\n`, `\r`, `\t`, and `\u{hex}` for other C0/C1 controls, DEL, U+2028 and U+2029. Everything else
(including all Hangul) is written as-is in UTF-8.

Token goldens have a `# <case index>` header per input followed by one tab-separated line per
token:

    start byte, end byte, position increment, position length, token type (KNOWN/UNKNOWN/USER),
    POS type, left POS, right POS, reading (or -), morphemes (surface/TAG+surface/TAG, or -), text

Offsets are UTF-8 byte offsets into the input (converted from Lucene's UTF-16 offsets, counting a
surrogate pair's bytes on its high surrogate). Text and morpheme surfaces are escaped like inputs.

Lucene caps unknown words at 1024 UTF-16 code units, so a long run of supplementary characters is
cut inside a surrogate pair: the golden then has a token ending in a lone high surrogate and one
starting with the lone low surrogate (or, in unigram mode, a lone-low-surrogate token with an
empty byte range), escaped as `\u{d800}`..`\u{dfff}`. The Rust tests normalize those to what the
port emits (whole code points), see `golden_to_toks`.

`chardef.txt` starts with one `class\t<name>\t<invoke>\t<group>` line per character class, then
`<first hex>\t<last hex>\t<class>` runs. `unicode.txt` has `<first hex>\t<last hex>\t<Character
.getType>\t<UnicodeScript>\t<isDigit>` runs. `lowercase.txt` has `<hex>\t<hex>` pairs.

`dict.txt` starts with implementation-independent summaries (term and word counts and an FNV-1a
64 checksum over a canonical dump of the token-info dictionary; the unknown-word entries; the
connection-cost matrix dimensions and checksum; see the generator's javadoc for the exact bytes),
then the answers to the probes in `cases/dict_probes.txt`.

`userdict.<name>.txt` is `empty`, `error\t<message>`, or `entry\t<surface>\t<ord>\t<right id>\t
<segmentation>` lines in FST order.

## Edge cases

`cases/edge.txt` was written to hit every branch of the Java implementation: empty and
whitespace-only inputs; spaces of every kind (space separators are skipped and penalized, but tab,
LF and CR are `SPACE`-class characters that become `SP` tokens, while U+3000 and NBSP are the
reverse case); leading and trailing spaces; compounds, inflected and pre-analysed entries; Hanja
with readings and Hanja numerals; numbers with every separator and sign; conjoining and
compatibility jamo, NFD text, the interpunct U+318D and all the bracket/quote punctuation; Latin
with case and combining marks, Cyrillic, Greek, kana; emoji and other supplementary characters
(surrogate pairs are `DEFAULT`-class, script `UNKNOWN`, and never punctuation); symbols from each
`SYMBOL`/`NUMERIC` range; control and format characters; the first and last code point of every
`char.def` range and the one after; user-dictionary interactions; and long inputs: unknown runs
over 1024 code units (including one that puts the cut inside a surrogate pair), 1500 random
syllables with no spaces (forcing the 1024-position backtrace), and long realistic text.

## Regenerating

Needs a JDK (the goldens were made with JDK 27, Unicode 17, which matches the ICU data the port
uses and the JDK Elasticsearch bundles) and the Lucene 10.4.0 jars `lucene-core`,
`lucene-analysis-common` and `lucene-analysis-nori` (from Maven Central). Defaults: Homebrew's
`openjdk` and `~/Src/nori/jars`; override with `JAVA_HOME` and `LUCENE_JARS`.

    cargo run -p alyze-cjk --example nori_gen_fuzz      # only if the generator changed
    cargo run -p alyze-cjk --example nori_wiki_sample   # only if the sample should change
    alyze-cjk/testdata/nori/gen.sh

The port's dictionary blobs (`alyze-cjk/data/nori/`) are converted from a full dump of Lucene's
dictionaries, which is too big to commit:

    alyze-cjk/testdata/nori/gen.sh dump                  # writes ~/Src/nori/dump (NORI_DUMP_DIR)
    cargo run -p alyze-cjk --example nori_convert_dict -- ~/Src/nori/dump

## Large differential runs

The committed Wikipedia sample is small to keep the repo small. To run the port against a lot more
text (the parquet shard is ~177 MB; see the extractor's docs for the download):

    cargo run -p alyze-cjk --example nori_wiki_sample -- --bytes 50000000 --out /tmp/ko.txt
    alyze-cjk/testdata/nori/gen.sh tokens /tmp/ko.txt /tmp/ko.tokens
    NORI_CASES=/tmp/ko.txt NORI_TOKENS=/tmp/ko.tokens \
        cargo test -p alyze-cjk --release nori::tests::golden::tokens_large -- --ignored

Any file in the case-file format works, so the same applies to a bigger fuzz set
(`--example nori_gen_fuzz -- --cases 100000 --seed 7 --out /tmp/fuzz.txt`).
