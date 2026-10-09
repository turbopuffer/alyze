//! The lattice and Viterbi search: a port of Lucene's `morph.Viterbi` / `morph.ViterbiNBest` as
//! specialised by `ja.ViterbiNBest` (search-mode penalties, second-best backtrace under
//! compounds, extended-mode unigrams, n-best output).

use super::{Options, Tokens};

/// The tokenizer's working memory, retained in a [`Tokens`] buffer across calls.
#[derive(Default)]
pub(crate) struct Scratch {}

impl Scratch {
    /// (position slots, code-unit buffer capacity).
    #[cfg(test)]
    pub(crate) fn footprint(&self) -> (usize, usize) {
        todo!()
    }
}

pub(crate) fn tokenize(input: &str, options: Options<'_>, out: &mut Tokens) {
    let _ = (input, options, out);
    todo!()
}
