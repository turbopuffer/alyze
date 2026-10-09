//! The smartcn dictionaries, loaded from the blobs in `alyze-cjk`.
//!
//! Lucene ships them as Java-serialized primitive arrays (`coredict.mem`, `bigramdict.mem`); the
//! port converts those once into its own compact format (TODO: define it, and the conversion
//! tool) and parses that lazily on first use.
//!
//! Words are handled as UTF-16 code units throughout, like the Java implementation: every
//! dictionary word is in the BMP, and the bigram table only stores a 64-bit hash of
//! `word1 '@' word2` computed over code units, so the hash functions have to run on exactly the
//! same units to find anything.

/// The core word dictionary: every known word with its frequency, grouped by first character.
pub(crate) struct CoreDict;

/// The bigram dictionary: an open-addressing hash table from `hash(word1 '@' word2)` to the pair's
/// frequency. Only hashes are stored, so collisions in the Java implementation are part of the
/// behaviour to reproduce.
pub(crate) struct BigramDict;

static CORE: CoreDict = CoreDict;
static BIGRAM: BigramDict = BigramDict;

impl CoreDict {
    pub(crate) fn get() -> &'static CoreDict {
        &CORE
    }

    /// Frequency of `word`, or 0 if it isn't in the dictionary.
    pub(crate) fn frequency(&self, word: &[u16]) -> i32 {
        // Stub: the port has not been written yet.
        let _ = word;
        0
    }

    /// Visits every `(word, frequency)` entry, in no particular order.
    pub(crate) fn for_each_entry(&self, f: impl FnMut(&[u16], i32)) {
        // Stub: the port has not been written yet.
        let _ = f;
    }
}

impl BigramDict {
    pub(crate) fn get() -> &'static BigramDict {
        &BIGRAM
    }

    /// Frequency of the pair `word1 '@' word2` (passed already joined), or 0 if unknown.
    pub(crate) fn frequency(&self, pair: &[u16]) -> i32 {
        // Stub: the port has not been written yet.
        let _ = pair;
        0
    }

    /// Visits every `(hash, frequency)` entry, in no particular order.
    pub(crate) fn for_each_entry(&self, f: impl FnMut(u64, i32)) {
        // Stub: the port has not been written yet.
        let _ = f;
    }
}
