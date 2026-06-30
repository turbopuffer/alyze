# alyze-features (Node.js / TypeScript)

Node.js bindings for the [`alyze-features`](../alyze-features) crate: text-match ranking features
(proximity, completeness, similarity, and the `fieldMatch` family) for second-stage re-ranking. The
features mirror the text-match rank features described in
[Vespa's documentation](https://docs.vespa.ai/en/reference/ranking/rank-features.html), computed
in-process via a native addon built with [napi-rs](https://napi.rs) v3. Ships with TypeScript types.

These bindings are **not published to npm**. Install them straight from the git repository.

## Install

Building the native addon requires a Rust toolchain. Clone the repo, then install the package by
path — the `prepare` script compiles the addon automatically:

```bash
npm install /path/to/alyze/node
```

For local development inside this directory:

```bash
npm install        # installs @napi-rs/cli and builds the addon (via `prepare`)
npm run build      # rebuild after Rust changes
npm test           # run the test suite
```

## Usage

Analyze each piece of text **once** into an `Analyzed` object, then reuse it across feature calls.
The query is always the receiver; the document/field is the argument.

```ts
import { Analyzed } from 'alyze-features'

const query = new Analyzed('quick brown fox')
const document = new Analyzed('the quick red fox jumps')

// Structural similarity (proximity, order, coverage).
const sim = query.elementSimilarity(document)
console.log(sim.similarity, sim.proximity, sim.order)

// Token-overlap completeness.
const comp = query.elementCompleteness(document)
console.log(comp.completeness, comp.queryCompleteness, comp.fieldCompleteness)

// Boolean match flag, and the full fieldMatch family.
console.log(query.matches(document)) // 1
console.log(query.fieldMatch(document).score)
```

### Choosing analysis options

`new Analyzed(text)` uses a default configuration. To control how text is tokenized, create an
`Analyzer` and reuse it for every field — using one analyzer for the query and the documents
guarantees they are tokenized identically, which the features rely on.

```ts
import { Analyzer } from 'alyze-features'

const analyzer = new Analyzer({
  caseSensitive: false, // lowercase tokens (default)
  asciiFolding: false, // fold "à" -> "a", etc.
  stemming: true, // Snowball stemming for `language`
  removeStopwords: false, // drop stopwords for `language`
  language: 'english', // used by stemming / stopword removal
  maxTokenLength: undefined, // drop tokens longer than this many bytes (1-255)
})

const query = analyzer.analyze('jumping foxes')
const document = analyzer.analyze('the fox jumps')
console.log(query.matches(document)) // 1 — stemming unifies jumping/jumps, foxes/fox
```

`stemming` and `removeStopwords` require `caseSensitive: false` and a `language` that supports the
feature; an invalid combination throws.

> **Note:** stopword removal and a `maxTokenLength` limit drop tokens while still advancing the
> position counter, so they introduce gaps in token positions. Leave them off to reproduce the
> `alyze-features` crate's reference semantics exactly.

### Feature methods on `Analyzed`

| Method | Returns |
| --- | --- |
| `elementCompleteness(document)` | `ElementCompleteness` |
| `elementSimilarity(document)` | `ElementSimilarity` |
| `textSimilarity(document)` | `TextSimilarity` |
| `matches(document)` | `number` |
| `fieldTermMatch(document, termIndex)` | `FieldTermMatch` |
| `fieldMatch(document, stats?)` | `FieldMatch` |
| `queryTermCount()` | `number` |

Every result is a plain object with `number` fields (e.g. `result.score`), fully typed.

### Corpus statistics for `fieldMatch`

The significance/weight-dependent `fieldMatch` outputs need per-term corpus statistics. Pass an
object mapping a query term to its `TermStats` (document frequency, corpus size, optional weight);
terms not in the object use a neutral default.

```ts
import { Analyzed, legacySignificance } from 'alyze-features'

const query = new Analyzed('quick brown fox')
const document = new Analyzed('the quick red fox jumps')

const m = query.fieldMatch(document, {
  quick: { documentFrequency: 1, documentCount: 1_000_000 },
  fox: { documentFrequency: 500_000, documentCount: 1_000_000 },
})
console.log(m.significance, m.importance, m.weightedOccurrence)

// Term significance (normalized IDF in [0.5, 1]) is also available directly.
console.log(legacySignificance(1, 1_000_000)) // 1
```

## Notes

- `new Analyzed(text)` analyzes with a default configuration (UAX#29 word segmentation, lowercased,
  no stemming or stopword removal), matching what `alyze-features` uses internally. Use `Analyzer`
  to change the options. Only this small option set is exposed, not the full `alyze` pipeline.
- Licensed under Apache-2.0 (derived from Vespa's Apache-2.0 reference implementation).
