//! Pieces shared by the MeCab-style analyzers (`nori`, `kuromoji`): the counterpart of Lucene's
//! `org.apache.lucene.analysis.morph` package plus the data-structure and Unicode helpers both
//! ports need. Each analyzer keeps its own Viterbi search (Lucene's `forward()` is shared by
//! inheritance there, but the two subclasses differ in enough hooks that one copy each, kept
//! line-for-line with Lucene, is clearer than a generic driver).

pub(crate) mod char_table;
pub(crate) mod java;
pub(crate) mod lowercase;
pub(crate) mod number;
pub(crate) mod positions;
pub(crate) mod term_index;
pub(crate) mod unicode;
