"""Tests for the alyze_features Python bindings.

The expected values mirror the golden values in the `alyze-features` crate's own Rust tests
(`alyze-features/src/lib.rs`), so the bindings stay in lockstep with the underlying computation.
"""

import math

import alyze_features as af
import pytest


def approx(a, b):
    assert math.isclose(a, b, abs_tol=1e-9), f"expected {b}, got {a}"


def test_element_completeness():
    c = af.Analyzed("quick brown fox").element_completeness(
        af.Analyzed("the the quick red fox jumps")
    )
    approx(c.field_completeness, 2 / 6)
    approx(c.query_completeness, 2 / 3)
    approx(c.completeness, 0.5 * (2 / 6) + 0.5 * (2 / 3))


def test_element_similarity():
    s = af.Analyzed("quick brown fox").element_similarity(
        af.Analyzed("the quick red fox jumps")
    )
    proximity = 1.0 - (1.0 / 8.0) ** 2
    approx(s.proximity, proximity)
    approx(s.order, 1.0)
    approx(s.query_coverage, 2 / 3)
    approx(s.field_coverage, 2 / 5)
    approx(s.similarity, 0.35 * proximity + 0.15 * 1.0 + 0.30 * (2 / 3) + 0.20 * (2 / 5))


def test_text_similarity_matches_element_similarity():
    q = af.Analyzed("quick brown fox")
    d = af.Analyzed("the quick red fox jumps")
    ts = q.text_similarity(d)
    es = q.element_similarity(d)
    approx(ts.score, es.similarity)
    approx(ts.proximity, es.proximity)


def test_matches():
    approx(af.Analyzed("quick fox").matches(af.Analyzed("the quick red fox")), 1.0)
    approx(af.Analyzed("zebra").matches(af.Analyzed("the quick red fox")), 0.0)


def test_field_term_match():
    q = af.Analyzed("alpha beta")
    d = af.Analyzed("beta alpha beta gamma")
    t0 = q.field_term_match(d, 0)  # "alpha"
    approx(t0.first_position, 1.0)
    approx(t0.occurrences, 1.0)
    t1 = q.field_term_match(d, 1)  # "beta"
    approx(t1.first_position, 0.0)
    approx(t1.occurrences, 2.0)
    t2 = q.field_term_match(d, 2)  # out of range
    approx(t2.first_position, af.FIELD_TERM_MATCH_ABSENT_POSITION)
    approx(t2.occurrences, 0.0)


def test_field_match():
    m = af.Analyzed("quick brown fox").field_match(af.Analyzed("the quick red fox jumps"))
    approx(m.matches, 2.0)
    approx(m.segments, 1.0)
    approx(m.query_completeness, 2 / 3)
    approx(m.field_completeness, 2 / 5)
    approx(m.earliness, 1.0 - 1.0 / 5.0)
    approx(m.occurrence, 0.4)
    assert math.isclose(m.proximity, 0.71, abs_tol=1e-6)
    assert math.isclose(m.score, 0.364527230941695, abs_tol=1e-6)


def test_field_match_idf_weighted():
    q = af.Analyzed("quick brown fox")
    d = af.Analyzed("the quick red fox jumps")
    stats = {
        "quick": af.TermStats(document_frequency=1, document_count=1_000_000),
        "brown": af.TermStats(document_frequency=1000, document_count=1_000_000),
        "fox": af.TermStats(document_frequency=500_000, document_count=1_000_000),
    }
    m = q.field_match(d, stats)

    sig_quick = af.legacy_significance(1, 1_000_000)
    sig_fox = af.legacy_significance(500_000, 1_000_000)
    total_sig = sig_quick + af.legacy_significance(1000, 1_000_000) + sig_fox

    approx(m.weight, 2 / 3)
    approx(m.significance, (sig_quick + sig_fox) / total_sig)
    approx(m.weighted_occurrence, 0.2)


def test_legacy_significance():
    approx(af.legacy_significance(1, 1_000_000), 1.0)
    approx(af.legacy_significance(1000, 1_000_000), 0.75)
    approx(af.legacy_significance(5, 0), 0.5)


def test_query_term_count():
    approx(af.Analyzed("a b c").query_term_count(), 3.0)
    approx(af.Analyzed("a a b").query_term_count(), 3.0)


def test_term_stats_significance():
    approx(af.TermStats(document_frequency=1, document_count=1_000_000).significance(), 1.0)


def test_repr():
    r = repr(af.Analyzed("quick brown fox").element_similarity(af.Analyzed("quick fox")))
    assert r.startswith("ElementSimilarity(")
    assert "similarity=" in r


def test_analyzer_default_matches_shortcut():
    analyzer = af.Analyzer()
    q = analyzer.analyze("quick brown fox")
    d = analyzer.analyze("the quick red fox jumps")
    approx(q.element_similarity(d).similarity, af.Analyzed("quick brown fox").element_similarity(
        af.Analyzed("the quick red fox jumps")
    ).similarity)


def test_analyzer_stemming():
    # With stemming, "jumping" and "jumps" share a stem, so they match.
    analyzer = af.Analyzer(stemming=True, language="english")
    q = analyzer.analyze("jumping")
    d = analyzer.analyze("the fox jumps")
    approx(q.matches(d), 1.0)
    # Without stemming they don't.
    plain = af.Analyzer()
    approx(plain.analyze("jumping").matches(plain.analyze("the fox jumps")), 0.0)


def test_analyzer_case_sensitive():
    analyzer = af.Analyzer(case_sensitive=True)
    approx(analyzer.analyze("Fox").matches(analyzer.analyze("fox")), 0.0)
    approx(af.Analyzed("Fox").matches(af.Analyzed("fox")), 1.0)


def test_analyzer_validation():
    with pytest.raises(ValueError):
        af.Analyzer(stemming=True, case_sensitive=True)
    with pytest.raises(ValueError):
        af.Analyzer(stemming=True, language="klingon")
    with pytest.raises(ValueError):
        af.Analyzer(max_token_length=0)
