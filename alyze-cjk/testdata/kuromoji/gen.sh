#!/usr/bin/env bash
# Regenerates the kuromoji golden files from the reference Java implementation.
#
#   ./gen.sh                                regenerate every file under golden/
#   ./gen.sh tokens IN OUT [key=value ...]  run one mode ad hoc (e.g. for a large differential run)
#   KUROMOJI_DUMP_DIR=... ./gen.sh dump     also write the full dictionary dump (uncommitted) for
#                                           the Rust dictionary converter
#
# Needs a JDK (default: Homebrew's openjdk, JDK 27 / Unicode 17) and the Lucene 10.4.0 jars
# (lucene-core, lucene-analysis-common, lucene-analysis-kuromoji); see README.md.
set -euo pipefail
cd "$(dirname "$0")"

JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk}"
LUCENE_JARS="${LUCENE_JARS:-$HOME/Src/kuromoji/jars}"
LUCENE_VERSION="${LUCENE_VERSION:-10.4.0}"
KUROMOJI_DUMP_DIR="${KUROMOJI_DUMP_DIR:-$HOME/Src/kuromoji/dump}"
CP="$LUCENE_JARS/lucene-core-$LUCENE_VERSION.jar"
CP="$CP:$LUCENE_JARS/lucene-analysis-common-$LUCENE_VERSION.jar"
CP="$CP:$LUCENE_JARS/lucene-analysis-kuromoji-$LUCENE_VERSION.jar"

mkdir -p build golden
"$JAVA_HOME/bin/javac" -d build -cp "$CP" java/KuromojiGolden.java

run() { "$JAVA_HOME/bin/java" -cp "build:$CP" KuromojiGolden "$@"; }

if [ $# -gt 0 ]; then
  if [ "$1" = dump ]; then
    mkdir -p "$KUROMOJI_DUMP_DIR"
    run dict cases/dict_probes.txt golden/dict.txt "dump=$KUROMOJI_DUMP_DIR"
    echo "dictionary dump written to $KUROMOJI_DUMP_DIR"
    exit 0
  fi
  run "$@"
  exit 0
fi

# Tokenizer configurations (see README.md). `default` is what Elasticsearch's kuromoji_tokenizer
# uses out of the box (search mode, punctuation discarded, compound tokens kept, no n-best).
tokens() {
  local case=$1 name=$2
  shift 2
  run tokens "cases/$case.txt" "golden/$case.$name.tokens" "$@"
}
# Every configuration on the small, systematic case files; on the big ones (fuzz, Wikipedia) only
# the configurations that exercise distinct code paths, to keep the golden files a reasonable size.
for case in upstream edge; do
  tokens "$case" default
  tokens "$case" normal mode=normal
  tokens "$case" extended mode=extended
  tokens "$case" nocompound compound=discard
  tokens "$case" punct punctuation=keep
  tokens "$case" normal_punct mode=normal punctuation=keep
  tokens "$case" extended_punct mode=extended punctuation=keep
  tokens "$case" nbest nbest=2000
  tokens "$case" normal_nbest mode=normal punctuation=keep nbest=4000
  tokens "$case" nbest_examples 'nbest_examples=/鳩山積み-鳩山/鳩山積み-鳩/'
  tokens "$case" userdict userdict=userdict/edge.txt
  tokens "$case" userdict_normal userdict=userdict/edge.txt mode=normal
  tokens "$case" userdict_extended_punct userdict=userdict/edge.txt mode=extended punctuation=keep
done
for case in long fuzz; do
  tokens "$case" default
  tokens "$case" extended_punct mode=extended punctuation=keep
  tokens "$case" nbest nbest=2000
  tokens "$case" userdict userdict=userdict/edge.txt
done
tokens wiki_ja default

# The `kuromoji` analyzer: width char filter + tokenizer (punctuation and compounds discarded) +
# base form + part-of-speech stop + stop words + katakana stem + lowercase.
for case in upstream edge; do
  run analyze "cases/$case.txt" "golden/$case.default.analyze"
  run analyze "cases/$case.txt" "golden/$case.normal.analyze" mode=normal
  run analyze "cases/$case.txt" "golden/$case.extended.analyze" mode=extended
  run analyze "cases/$case.txt" "golden/$case.userdict.analyze" userdict=userdict/edge.txt
  run analyze "cases/$case.txt" "golden/$case.stopwords.analyze" stopwords=stopwords/custom.txt stopwords_case=true
done
run analyze cases/long.txt golden/long.default.analyze
run analyze cases/fuzz.txt golden/fuzz.default.analyze

# The `kuromoji_completion` analyzer: width char filter + tokenizer (normal mode) + completion +
# lowercase. Only on cases/completion.txt (see tools/completion_cases.py): the completion filter
# emits every romaji keystroke variant of a reading, exponential in the length of a kana run.
run completion cases/completion.txt golden/completion.index.completion completion=index
run completion cases/completion.txt golden/completion.query.completion completion=query
run completion cases/completion.txt golden/completion.userdict.completion completion=index userdict=userdict/edge.txt

# Custom filter chains, as Elasticsearch users compose them (see `chain` in the Rust tests).
chain() {
  local case=$1 name=$2
  shift 2
  run chain "cases/$case.txt" "golden/$case.$name.chain" "$@"
}
for case in upstream edge; do
  chain "$case" baseform filters=baseform
  chain "$case" pos filters=pos
  chain "$case" pos_docs filters=pos:助詞-格助詞-一般+助詞-終助詞
  chain "$case" pos_verb filters=pos:動詞-自立
  chain "$case" reading filters=reading
  chain "$case" romaji filters=romaji
  chain "$case" width_reading filters=width,reading
  chain "$case" width_romaji filters=width,romaji
  chain "$case" stem filters=stem:4
  chain "$case" stem6 filters=stem:6
  chain "$case" stop filters=stop
  chain "$case" stop_custom filters=stop:stopwords/custom.txt
  chain "$case" stop_custom_nocase filters=stop:stopwords/custom.txt:nocase
  chain "$case" number filters=number punctuation=keep
  chain "$case" number_nbest filters=number punctuation=keep nbest=2000
  chain "$case" hiragana_upper filters=hiragana_upper
  chain "$case" katakana_upper filters=katakana_upper
  chain "$case" itermark_kanji filters= charfilter=itermark:kanji=1,kana=0
  chain "$case" itermark_kana filters= charfilter=itermark:kanji=0,kana=1
  chain "$case" itermark_punct filters= charfilter=itermark punctuation=keep
  chain "$case" width_char_punct filters= charfilter=width punctuation=keep
  chain "$case" lowercase filters=lowercase
  chain "$case" recommended filters=baseform,pos,width,stop,stem:4,lowercase
done
chain long number_nbest filters=number punctuation=keep nbest=2000
chain long recommended filters=baseform,pos,width,stop,stem:4,lowercase
chain fuzz number_nbest filters=number punctuation=keep nbest=2000
chain fuzz itermark_punct filters= charfilter=itermark punctuation=keep
chain fuzz recommended filters=baseform,pos,width,stop,stem:4,lowercase

# Char filters on their own: the filtered text and the offset map.
for case in upstream edge; do
  run charfilter "cases/$case.txt" "golden/charfilter.$case.itermark.txt" charfilter=itermark
  run charfilter "cases/$case.txt" "golden/charfilter.$case.itermark_kanji.txt" charfilter=itermark:kanji=1,kana=0
  run charfilter "cases/$case.txt" "golden/charfilter.$case.itermark_kana.txt" charfilter=itermark:kanji=0,kana=1
  run charfilter "cases/$case.txt" "golden/charfilter.$case.width.txt" charfilter=width
done
for case in long fuzz; do
  run charfilter "cases/$case.txt" "golden/charfilter.$case.itermark.txt" charfilter=itermark
  run charfilter "cases/$case.txt" "golden/charfilter.$case.width.txt" charfilter=width
done

run romaji cases/romaji.txt golden/romaji.txt
run number cases/numbers.txt golden/numbers.txt
run chardef golden/chardef.txt
run unicode golden/unicode.txt
run lowercase golden/lowercase.txt
run stoplists golden/stoplists.txt
run dict cases/dict_probes.txt golden/dict.txt
for rules in lucene es docs edge dups empty invalid_surface invalid_count malformed_quotes malformed_fields whitespace; do
  run userdict "userdict/$rules.txt" "golden/userdict.$rules.txt"
done
echo "done: $(ls golden | wc -l | tr -d ' ') golden files, $(du -sh golden | cut -f1)"
