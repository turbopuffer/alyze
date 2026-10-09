#!/usr/bin/env bash
# Regenerates the smartcn golden files from the reference Java implementation.
#
#   ./gen.sh                      regenerate every file under golden/
#   ./gen.sh tokens IN OUT        run one mode ad hoc (e.g. for a large differential run)
#
# Needs a JDK (default: Homebrew's openjdk) and the Lucene 10.4.0 jars (lucene-core,
# lucene-analysis-common, lucene-analysis-smartcn); see README.md.
set -euo pipefail
cd "$(dirname "$0")"

JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk}"
LUCENE_JARS="${LUCENE_JARS:-$HOME/Src/smartcn/jars}"
LUCENE_VERSION="${LUCENE_VERSION:-10.4.0}"
CP="$LUCENE_JARS/lucene-core-$LUCENE_VERSION.jar"
CP="$CP:$LUCENE_JARS/lucene-analysis-common-$LUCENE_VERSION.jar"
CP="$CP:$LUCENE_JARS/lucene-analysis-smartcn-$LUCENE_VERSION.jar"

mkdir -p build golden
"$JAVA_HOME/bin/javac" -d build -cp "$CP" java/SmartcnGolden.java

run() { "$JAVA_HOME/bin/java" -cp "build:$CP" SmartcnGolden "$@"; }

if [ $# -gt 0 ]; then
  run "$@"
  exit 0
fi

for case in upstream edge fuzz wiki_zh; do
  run tokens    "cases/$case.txt" "golden/$case.tokens"
  run sentences "cases/$case.txt" "golden/$case.sentences"
done
# The analyzer stage adds only stemming + stop filtering on top of the tokenizer, which the smaller
# sets cover; the Wikipedia sample is kept to the tokenizer stage to keep the repo small.
for case in upstream edge fuzz; do
  run analyze "cases/$case.txt" "golden/$case.analyze"
done
run chartypes golden/chartypes.txt
run dict cases/dict_probes.txt golden/dict.txt
echo "done: $(ls golden | wc -l | tr -d ' ') golden files, $(du -sh golden | cut -f1)"
