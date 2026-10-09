#!/usr/bin/env bash
# Regenerates the nori golden files from the reference Java implementation.
#
#   ./gen.sh                                regenerate every file under golden/
#   ./gen.sh tokens IN OUT [key=value ...]  run one mode ad hoc (e.g. for a large differential run)
#   NORI_DUMP_DIR=... ./gen.sh dump         also write the full dictionary dump (uncommitted) for
#                                           the Rust dictionary converter
#
# Needs a JDK (default: Homebrew's openjdk, JDK 27 / Unicode 17) and the Lucene 10.4.0 jars
# (lucene-core, lucene-analysis-common, lucene-analysis-nori); see README.md.
set -euo pipefail
cd "$(dirname "$0")"

JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk}"
LUCENE_JARS="${LUCENE_JARS:-$HOME/Src/nori/jars}"
LUCENE_VERSION="${LUCENE_VERSION:-10.4.0}"
NORI_DUMP_DIR="${NORI_DUMP_DIR:-$HOME/Src/nori/dump}"
CP="$LUCENE_JARS/lucene-core-$LUCENE_VERSION.jar"
CP="$CP:$LUCENE_JARS/lucene-analysis-common-$LUCENE_VERSION.jar"
CP="$CP:$LUCENE_JARS/lucene-analysis-nori-$LUCENE_VERSION.jar"

mkdir -p build golden
"$JAVA_HOME/bin/javac" -d build -cp "$CP" java/NoriGolden.java

run() { "$JAVA_HOME/bin/java" -cp "build:$CP" NoriGolden "$@"; }

if [ $# -gt 0 ]; then
  if [ "$1" = dump ]; then
    mkdir -p "$NORI_DUMP_DIR"
    run dict cases/dict_probes.txt golden/dict.txt "dump=$NORI_DUMP_DIR"
    echo "dictionary dump written to $NORI_DUMP_DIR"
    exit 0
  fi
  run "$@"
  exit 0
fi

# Tokenizer configurations (see README.md). `default` is what Elasticsearch's nori_tokenizer and
# nori analyzer use out of the box.
tokens() {
  local case=$1 name=$2
  shift 2
  run tokens "cases/$case.txt" "golden/$case.$name.tokens" "$@"
}
# Every configuration on the small, systematic case files; on the big ones (fuzz, Wikipedia) only
# the configurations that exercise distinct code paths, to keep the golden files a reasonable size.
for case in upstream edge; do
  tokens "$case" default
  tokens "$case" mixed decompound=mixed
  tokens "$case" none decompound=none
  tokens "$case" punct punctuation=keep
  tokens "$case" mixed_punct decompound=mixed punctuation=keep
  tokens "$case" unigrams unigrams=true
  tokens "$case" userdict userdict=userdict/edge.txt
  tokens "$case" userdict_mixed_punct userdict=userdict/edge.txt decompound=mixed punctuation=keep
done
tokens fuzz default
tokens fuzz mixed_punct decompound=mixed punctuation=keep
tokens fuzz unigrams unigrams=true
tokens fuzz userdict_mixed_punct userdict=userdict/edge.txt decompound=mixed punctuation=keep
tokens wiki_ko default

# The `nori` analyzer: tokenizer (default) + part-of-speech stop + reading form + lowercase.
for case in upstream edge; do
  run analyze "cases/$case.txt" "golden/$case.default.analyze"
  run analyze "cases/$case.txt" "golden/$case.mixed.analyze" decompound=mixed
  run analyze "cases/$case.txt" "golden/$case.stoptags.analyze" stoptags=NNP,NNG,NR,SP
  run analyze "cases/$case.txt" "golden/$case.userdict.analyze" userdict=userdict/edge.txt
done
run analyze cases/fuzz.txt golden/fuzz.default.analyze
run analyze cases/fuzz.txt golden/fuzz.mixed.analyze decompound=mixed

# Custom filter chains, as Elasticsearch users compose them.
for case in upstream edge; do
  run chain "cases/$case.txt" "golden/$case.reading.chain" filters=reading
  run chain "cases/$case.txt" "golden/$case.number.chain" filters=number
  run chain "cases/$case.txt" "golden/$case.number_punct.chain" filters=pos:SP,number punctuation=keep
  run chain "cases/$case.txt" "golden/$case.number_mixed_punct.chain" filters=pos:SP,number punctuation=keep decompound=mixed
  run chain "cases/$case.txt" "golden/$case.pos_custom.chain" filters=pos:EP+EF+EC+ETN+ETM+JKS+JKC+JKG+JKO+JKB+JKV+JKQ+JX+JC
  run chain "cases/$case.txt" "golden/$case.graph.chain" filters=pos decompound=mixed
  run chain "cases/$case.txt" "golden/$case.lowercase.chain" filters=lowercase
done
run chain cases/fuzz.txt golden/fuzz.reading.chain filters=reading
run chain cases/fuzz.txt golden/fuzz.number_mixed_punct.chain filters=pos:SP,number punctuation=keep decompound=mixed
run chain cases/fuzz.txt golden/fuzz.graph.chain filters=pos decompound=mixed

run chardef golden/chardef.txt
run unicode golden/unicode.txt
run lowercase golden/lowercase.txt
run dict cases/dict_probes.txt golden/dict.txt
for rules in lucene es edge dups invalid empty; do
  run userdict "userdict/$rules.txt" "golden/userdict.$rules.txt"
done
echo "done: $(ls golden | wc -l | tr -d ' ') golden files, $(du -sh golden | cut -f1)"
