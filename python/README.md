# alyze-features (Python)

Python bindings for the [`alyze-features`](../alyze-features) crate: text-match ranking
features (proximity, completeness, similarity, and the `fieldMatch` family) for second-stage
re-ranking. The features mirror the text-match rank features described in
[Vespa's documentation](https://docs.vespa.ai/en/reference/ranking/rank-features.html), computed
client-side.

These bindings are **not published to PyPI**. Install them straight from the git repository.

## Install

From a clone of the repo:

```bash
pip install ./python
```

Or directly from GitHub (builds the native extension on install, so a Rust toolchain is required):

```bash
pip install "git+https://github.com/turbopuffer/alyze.git#subdirectory=python"
```

For local development, [maturin](https://www.maturin.rs/) builds and installs into the active
virtualenv:

```bash
pip install maturin
maturin develop --release
```

## Usage

Analyze each piece of text **once** into an `Analyzed` object, then reuse it across feature calls.
The query is always the receiver; the document/field is the argument.

```python
import alyze_features as af

query = af.Analyzed("quick brown fox")
document = af.Analyzed("the quick red fox jumps")

# Structural similarity (proximity, order, coverage).
sim = query.element_similarity(document)
print(sim.similarity, sim.proximity, sim.order)

# Token-overlap completeness.
comp = query.element_completeness(document)
print(comp.completeness, comp.query_completeness, comp.field_completeness)

# Boolean match flag, and the full fieldMatch family.
print(query.matches(document))          # 1.0
print(query.field_match(document).score)
```

### Choosing analysis options

`Analyzed(text)` uses a default configuration. To control how text is tokenized, create an
`Analyzer` and reuse it for every field — using one analyzer for the query and the documents
guarantees they are tokenized identically, which the features rely on.

```python
import alyze_features as af

analyzer = af.Analyzer(
    case_sensitive=False,     # lowercase tokens (default)
    ascii_folding=False,      # fold "à" -> "a", etc.
    stemming=True,            # Snowball stemming for `language`
    remove_stopwords=False,   # drop stopwords for `language`
    language="english",       # used by stemming / stopword removal
    max_token_length=None,    # drop tokens longer than this many bytes (1-255), or None
)

query = analyzer.analyze("jumping foxes")
document = analyzer.analyze("the fox jumps")
print(query.matches(document))  # 1.0 — stemming unifies jumping/jumps, foxes/fox
```

`stemming` and `remove_stopwords` require `case_sensitive=False` and a `language` that supports the
feature; an invalid combination raises `ValueError`.

> **Note:** stopword removal and a `max_token_length` limit drop tokens while still advancing the
> position counter, so they introduce gaps in token positions. Leave them off to reproduce the
> `alyze-features` crate's reference semantics exactly.

### Feature methods on `Analyzed`

| Method | Returns |
| --- | --- |
| `element_completeness(document)` | `ElementCompleteness` |
| `element_similarity(document)` | `ElementSimilarity` |
| `text_similarity(document)` | `TextSimilarity` |
| `matches(document)` | `float` |
| `field_term_match(document, term_index)` | `FieldTermMatch` |
| `field_match(document, stats=None)` | `FieldMatch` |
| `query_term_count()` | `float` |

Every result object exposes its fields as plain attributes (e.g. `result.score`) and has a readable
`repr`.

### Corpus statistics for `field_match`

The significance/weight-dependent `fieldMatch` outputs need per-term corpus statistics. Pass a dict
mapping a query term to its `TermStats`; terms not in the dict use a neutral default.

```python
import alyze_features as af

query = af.Analyzed("quick brown fox")
document = af.Analyzed("the quick red fox jumps")

stats = {
    "quick": af.TermStats(document_frequency=1, document_count=1_000_000),
    "fox": af.TermStats(document_frequency=500_000, document_count=1_000_000),
}
m = query.field_match(document, stats)
print(m.significance, m.importance, m.weighted_occurrence)

# Term significance (normalized IDF in [0.5, 1]) is also available directly.
print(af.legacy_significance(1, 1_000_000))  # 1.0
```

## Notes

- `Analyzed(text)` analyzes with a default configuration (UAX#29 word segmentation, lowercased, no
  stemming or stopword removal), matching what `alyze-features` uses internally. Use `Analyzer` to
  change the options. Only this small option set is exposed, not the full `alyze` pipeline.
- Licensed under Apache-2.0 (derived from Vespa's Apache-2.0 reference implementation).
