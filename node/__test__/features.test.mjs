// Tests for the alyze-features Node bindings.
//
// The expected values mirror the golden values in the `alyze-features` crate's own Rust tests
// (`alyze-features/src/lib.rs`), so the bindings stay in lockstep with the underlying computation.

import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  Analyzed,
  Analyzer,
  legacySignificance,
  FIELD_TERM_MATCH_ABSENT_POSITION,
} from '../index.js'

const approx = (a, b) => assert.ok(Math.abs(a - b) < 1e-9, `expected ${b}, got ${a}`)

test('elementCompleteness', () => {
  const c = new Analyzed('quick brown fox').elementCompleteness(
    new Analyzed('the the quick red fox jumps'),
  )
  approx(c.fieldCompleteness, 2 / 6)
  approx(c.queryCompleteness, 2 / 3)
  approx(c.completeness, 0.5 * (2 / 6) + 0.5 * (2 / 3))
})

test('elementSimilarity', () => {
  const s = new Analyzed('quick brown fox').elementSimilarity(
    new Analyzed('the quick red fox jumps'),
  )
  const proximity = 1.0 - (1.0 / 8.0) ** 2
  approx(s.proximity, proximity)
  approx(s.order, 1.0)
  approx(s.queryCoverage, 2 / 3)
  approx(s.fieldCoverage, 2 / 5)
  approx(s.similarity, 0.35 * proximity + 0.15 * 1.0 + 0.3 * (2 / 3) + 0.2 * (2 / 5))
})

test('textSimilarity matches elementSimilarity', () => {
  const q = new Analyzed('quick brown fox')
  const d = new Analyzed('the quick red fox jumps')
  approx(q.textSimilarity(d).score, q.elementSimilarity(d).similarity)
  approx(q.textSimilarity(d).proximity, q.elementSimilarity(d).proximity)
})

test('matches', () => {
  approx(new Analyzed('quick fox').matches(new Analyzed('the quick red fox')), 1.0)
  approx(new Analyzed('zebra').matches(new Analyzed('the quick red fox')), 0.0)
})

test('fieldTermMatch', () => {
  const q = new Analyzed('alpha beta')
  const d = new Analyzed('beta alpha beta gamma')
  const t0 = q.fieldTermMatch(d, 0) // "alpha"
  approx(t0.firstPosition, 1.0)
  approx(t0.occurrences, 1.0)
  const t1 = q.fieldTermMatch(d, 1) // "beta"
  approx(t1.firstPosition, 0.0)
  approx(t1.occurrences, 2.0)
  const t2 = q.fieldTermMatch(d, 2) // out of range
  approx(t2.firstPosition, FIELD_TERM_MATCH_ABSENT_POSITION)
  approx(t2.occurrences, 0.0)
})

test('fieldMatch', () => {
  const m = new Analyzed('quick brown fox').fieldMatch(new Analyzed('the quick red fox jumps'))
  approx(m.matches, 2.0)
  approx(m.segments, 1.0)
  approx(m.queryCompleteness, 2 / 3)
  approx(m.fieldCompleteness, 2 / 5)
  approx(m.earliness, 1.0 - 1.0 / 5.0)
  approx(m.occurrence, 0.4)
  assert.ok(Math.abs(m.proximity - 0.71) < 1e-6)
  assert.ok(Math.abs(m.score - 0.364527230941695) < 1e-6)
})

test('fieldMatch with IDF/weight stats', () => {
  const q = new Analyzed('quick brown fox')
  const d = new Analyzed('the quick red fox jumps')
  const stats = {
    quick: { documentFrequency: 1, documentCount: 1_000_000 },
    brown: { documentFrequency: 1000, documentCount: 1_000_000 },
    fox: { documentFrequency: 500_000, documentCount: 1_000_000 },
  }
  const m = q.fieldMatch(d, stats)

  const sigQuick = legacySignificance(1, 1_000_000)
  const sigFox = legacySignificance(500_000, 1_000_000)
  const totalSig = sigQuick + legacySignificance(1000, 1_000_000) + sigFox

  approx(m.weight, 2 / 3)
  approx(m.significance, (sigQuick + sigFox) / totalSig)
  approx(m.weightedOccurrence, 0.2)
})

test('legacySignificance', () => {
  approx(legacySignificance(1, 1_000_000), 1.0)
  approx(legacySignificance(1000, 1_000_000), 0.75)
  approx(legacySignificance(5, 0), 0.5)
})

test('queryTermCount', () => {
  approx(new Analyzed('a b c').queryTermCount(), 3.0)
  approx(new Analyzed('a a b').queryTermCount(), 3.0)
})

test('Analyzer default matches the shortcut', () => {
  const analyzer = new Analyzer()
  const q = analyzer.analyze('quick brown fox')
  const d = analyzer.analyze('the quick red fox jumps')
  approx(
    q.elementSimilarity(d).similarity,
    new Analyzed('quick brown fox').elementSimilarity(new Analyzed('the quick red fox jumps'))
      .similarity,
  )
})

test('Analyzer stemming', () => {
  const analyzer = new Analyzer({ stemming: true, language: 'english' })
  approx(analyzer.analyze('jumping').matches(analyzer.analyze('the fox jumps')), 1.0)
  const plain = new Analyzer()
  approx(plain.analyze('jumping').matches(plain.analyze('the fox jumps')), 0.0)
})

test('Analyzer case sensitivity', () => {
  const analyzer = new Analyzer({ caseSensitive: true })
  approx(analyzer.analyze('Fox').matches(analyzer.analyze('fox')), 0.0)
  approx(new Analyzed('Fox').matches(new Analyzed('fox')), 1.0)
})

test('Analyzer validation throws', () => {
  assert.throws(() => new Analyzer({ stemming: true, caseSensitive: true }))
  assert.throws(() => new Analyzer({ stemming: true, language: 'klingon' }))
  assert.throws(() => new Analyzer({ maxTokenLength: 0 }))
})
