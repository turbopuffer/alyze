#!/usr/bin/env python3
"""Writes cases/completion.txt: the inputs of cases/upstream.txt, cases/edge.txt and
cases/fuzz.txt that the completion filter can handle in bounded time.

Lucene's JapaneseCompletionFilter emits every romaji keystroke variant of a token's reading
(KatakanaRomanizer: シ → si/shi, ン → n/nn, ...), which is exponential in the reading's length,
and an unknown kana run becomes one token with its surface as reading. So inputs are kept only
when every run of kana (hiragana, katakana, half-width katakana, prolonged sound mark) is at most
MAX_KANA_RUN characters and the whole input is at most MAX_LEN characters, and at most MAX_FUZZ fuzz inputs are taken. Deterministic; rerun
after the case files change, then gen.sh.
"""
import re, sys, os
MAX_KANA_RUN = 12
MAX_LEN = 300
MAX_FUZZ = 300
KANA = re.compile(r'[ぁ-ゟ゠-ヿㇰ-ㇿｦ-ﾟ]+')
here = os.path.dirname(os.path.abspath(__file__))
cases = os.path.join(here, '..', 'cases')
out = []
seen = set()
for name in ['upstream', 'edge', 'fuzz']:
    taken = 0
    for line in open(os.path.join(cases, name + '.txt'), encoding='utf-8').read().split('\n')[:-1]:
        if line in seen or (name == 'fuzz' and taken >= MAX_FUZZ):
            continue
        # The file is escaped; escapes never contain kana, so the check works on the raw line.
        if len(line) > MAX_LEN or any(len(m.group()) > MAX_KANA_RUN for m in KANA.finditer(line)):
            continue
        seen.add(line)
        out.append(line)
        taken += 1
open(os.path.join(cases, 'completion.txt'), 'w', encoding='utf-8').write('\n'.join(out) + '\n')
print(len(out), 'cases', file=sys.stderr)
