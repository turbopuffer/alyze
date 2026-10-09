# smartcn golden tests

Test data for the Rust port of Lucene's `smartcn` analyzer (`src/smartcn/`, `cjk` feature), the
segmenter behind Elasticsearch's `smartcn` analyzer and `smartcn_tokenizer`.

The port is checked against the reference Java implementation by golden files: `gen.sh` runs
`lucene-analysis-smartcn` over every input in `cases/` and records what it produced in `golden/`.
The golden files are committed, so the Rust tests (`src/smartcn/tests/`) need no JVM.

## Layout

| Path                        | What                                                              |
| --------------------------- | ----------------------------------------------------------------- |
| `java/SmartcnGolden.java`   | The generator: drives Lucene and dumps tokens/sentences/dict data |
| `gen.sh`                    | Compiles and runs it for every file below                         |
| `cases/upstream.txt`        | Inputs from Lucene's and Elasticsearch's own smartcn tests        |
| `cases/edge.txt`            | Hand-written edge cases (classification boundaries, chunking, …)  |
| `cases/fuzz.txt`            | Seeded random inputs, from `examples/smartcn_gen_fuzz.rs`         |
| `cases/wiki_zh.txt`         | ~512 KiB of Chinese Wikipedia, from `examples/smartcn_wiki_sample.rs` |
| `cases/dict_probes.txt`     | Words / word pairs to look up in the dictionaries                 |
| `golden/<case>.tokens`      | `HMMChineseTokenizer` output (ES `smartcn_tokenizer`)             |
| `golden/<case>.analyze`     | `SmartChineseAnalyzer` output (ES `smartcn`: + Porter + stop)     |
| `golden/<case>.sentences`   | JDK `BreakIterator` sentence boundaries the tokenizer splits on   |
| `golden/chartypes.txt`      | `Utility.getCharType` for every UTF-16 code unit                  |
| `golden/dict.txt`           | Dictionary checksums and probe frequencies                        |

## Formats

Case files hold one input per line. Inputs are escaped so that any text fits on a line: `\\`,
`\n`, `\r`, `\t`, and `\u{hex}` for other C0/C1 controls, DEL, U+2028 and U+2029. Everything else
(including all CJK text) is written as-is in UTF-8.

Token goldens have a `# <case index>` header per input followed by one line per token:

    <start byte>\t<end byte>\t<position increment>\t<escaped token text>

Offsets are UTF-8 byte offsets into the input (converted from Lucene's UTF-16 offsets). Position
increments are Lucene's: always 1 for the tokenizer stage, >1 in the analyzer stage after a
stop-filtered punctuation token.

Lucene reads its input in 1024-UTF-16-unit chunks; when a chunk ends inside a surrogate pair it
emits each half as a one-unit token. Those can't be written in UTF-8, so they appear as
`\u{d800}`..`\u{dfff}` escapes (the first half with the whole code point's byte range, the second
with an empty range). The Rust tests merge such a pair into one code-point token, which is what
the port emits. (Also: writing a lone surrogate through this JDK's UTF-8 writer hangs it, hence
the escaping.)

Sentence goldens have the same header, then one line of space-separated byte offsets of every
boundary, starting with 0 and ending with the input length.

`dict.txt` starts with implementation-independent summaries of the two dictionaries (entry counts
and FNV-1a 64 checksums over a canonical dump; see the generator's javadoc) and then one
`core\t<word>\t<freq>` or `bigram\t<word1>@<word2>\t<freq>` line per probe.

## Regenerating

Needs a JDK and the Lucene 10.4.0 jars `lucene-core`, `lucene-analysis-common` and
`lucene-analysis-smartcn` (from Maven Central). Defaults: Homebrew's `openjdk` and
`~/Src/smartcn/jars`; override with `JAVA_HOME` and `LUCENE_JARS`.

    cargo run -p alyze --example smartcn_gen_fuzz      # only if the generator changed
    cargo run -p alyze --example smartcn_wiki_sample   # only if the sample should change
    testdata/smartcn/gen.sh

## Large differential runs

The committed Wikipedia sample is small to keep the repo small. To run the port against a lot more
text (the parquet shard is ~120 MB; see the extractor's docs for the download):

    cargo run -p alyze --example smartcn_wiki_sample -- --bytes 50000000 --out /tmp/zh.txt
    testdata/smartcn/gen.sh tokens /tmp/zh.txt /tmp/zh.tokens
    SMARTCN_CASES=/tmp/zh.txt SMARTCN_TOKENS=/tmp/zh.tokens \
        cargo test --features cjk --release smartcn::tests::golden::tokens_large -- --ignored

Any file in the case-file format works, so the same applies to a bigger fuzz set
(`--example smartcn_gen_fuzz -- --cases 100000 --seed 7 --out /tmp/fuzz.txt`).
