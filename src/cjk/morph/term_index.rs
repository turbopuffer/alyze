//! The term index: an `fst::Map` keyed by UTF-16BE surface forms (so byte order equals code-unit
//! order, which is the order Lucene's FSTs and user dictionaries use), used by the system and
//! user dictionaries of both analyzers.

use fst::raw::{Fst, Node, Output};

pub(crate) struct TermIndex<D: AsRef<[u8]>> {
    fst: Fst<D>,
}

impl<D: AsRef<[u8]>> TermIndex<D> {
    pub fn new(fst: Fst<D>) -> Self {
        TermIndex { fst }
    }

    /// Calls `f(length, ordinal)` for every term that is a prefix of `text`, shortest first.
    /// Returns whether any term matched. Walks two FST transitions per code unit.
    pub fn for_each_prefix(&self, text: &[u16], mut f: impl FnMut(usize, u64)) -> bool {
        let fst = &self.fst;
        let mut node: Node<'_> = fst.root();
        let mut out = Output::zero();
        let mut any = false;
        for (i, unit) in text.iter().enumerate() {
            for byte in unit.to_be_bytes() {
                let Some(idx) = node.find_input(byte) else {
                    return any;
                };
                let t = node.transition(idx);
                out = out.cat(t.out);
                node = fst.node(t.addr);
            }
            if node.is_final() {
                any = true;
                f(i + 1, out.cat(node.final_output()).value());
            }
        }
        any
    }

    /// The ordinal of `text` if it is a term.
    #[cfg(test)]
    pub fn lookup(&self, text: &[u16]) -> Option<u64> {
        let mut found = None;
        self.for_each_prefix(text, |len, ord| {
            if len == text.len() {
                found = Some(ord);
            }
        });
        found
    }

    /// Every (term, ordinal) in order.
    #[cfg(test)]
    pub fn for_each_term(&self, mut f: impl FnMut(&[u16], u64)) {
        use fst::Streamer;
        let mut stream = self.fst.stream();
        let mut units = Vec::new();
        while let Some((key, out)) = stream.next() {
            units.clear();
            units.extend(key.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])));
            f(&units, out.value());
        }
    }
}

/// Builds a term index from `(UTF-16 key, ordinal)` pairs that are sorted by key and unique.
pub(crate) fn build(entries: impl IntoIterator<Item = (Vec<u16>, u64)>) -> TermIndex<Vec<u8>> {
    let mut builder = fst::MapBuilder::memory();
    for (key, ord) in entries {
        let bytes: Vec<u8> = key.iter().flat_map(|u| u.to_be_bytes()).collect();
        builder
            .insert(bytes, ord)
            .expect("entries are sorted and unique");
    }
    let fst = Fst::new(builder.into_inner().expect("in-memory FST")).expect("freshly built FST");
    TermIndex::new(fst)
}
