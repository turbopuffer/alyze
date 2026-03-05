# alyze

A high-performance tokenization and analysis implementation for full-text search. Provides a
[UAX #29](https://www.unicode.org/reports/tr29/) compliant tokenizer, implemented with a hand-rolled
deterministic finite automaton (DFA). On my laptop (M3 Pro), can tokenize 64MiB of English Wikipedia
in ~172ms, or roughly ~372 MiB/s. Non-ASCII text will be slower.

This crate is currently in alpha, but we have ambitions to expand the scope of this crate to encompass
a full suite of analysis tools, including stemming, stopword removal, case folding, etc. During alpha
development, backwards compatibility is not guaranteed, but we'll do our best to minimize breaking changes.