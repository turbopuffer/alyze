use std::num::NonZeroU8;

use ahash::AHashMap;

/// Keep a small cache of stemmed tokens to avoid repeated stemming of common tokens.
/// Can yield ~2x throughput improvement for stemmed analysis.
///
/// A new cache has no stemming algorithm, so its first stemming analysis discards any
/// manually inserted entries. Cloning or moving an analyzer-warmed cache preserves its
/// algorithm; subsequent analyses retain entries until the stemming language changes.
#[derive(Debug, Clone)]
pub struct StemmingCache {
    cache: AHashMap<CachedToken, StemmingCacheEntry>,
    initial_capacity: usize,
    algorithm: Option<rust_stemmers::Algorithm>,
}

#[derive(Debug, Clone)]
pub enum StemmingCacheEntry {
    Unchanged,
    Stemmed(CachedToken),
}

impl StemmingCacheEntry {
    pub fn new_unchanged() -> Self {
        Self::Unchanged
    }

    pub fn new_stemmed(stemming_result: CachedToken) -> Self {
        Self::Stemmed(stemming_result)
    }
}

// We'll use a short token type to avoid caching larger tokens, which are
// less likely to benefit from caching (rarer).
pub type CachedToken = ShortToken<10>;

impl StemmingCache {
    pub fn new_with_capacity(capacity: usize) -> Self {
        Self {
            cache: AHashMap::new(),
            initial_capacity: capacity,
            algorithm: None,
        }
    }

    /// Cached results are only valid for the algorithm that produced them.
    pub(super) fn set_algorithm(&mut self, algorithm: rust_stemmers::Algorithm) {
        if self.algorithm != Some(algorithm) {
            self.cache.clear(); // Retain the allocation for the next language.
            self.algorithm = Some(algorithm);
        }
    }

    pub fn lookup(&self, key: &CachedToken) -> Option<&StemmingCacheEntry> {
        self.cache.get(key)
    }

    pub fn has_remaining_capacity(&self) -> bool {
        self.cache.len() < self.cache.capacity().max(self.initial_capacity)
    }

    pub fn insert_no_clobber_assume_capacity(
        &mut self,
        key: CachedToken,
        entry: StemmingCacheEntry,
    ) {
        debug_assert!(
            self.has_remaining_capacity(),
            "should only insert when there is remaining capacity"
        );
        if self.cache.capacity() == 0 {
            self.cache.reserve(self.initial_capacity);
        }
        let clobbered = self.cache.insert(key, entry);
        debug_assert!(
            clobbered.is_none(),
            "cache should not evict existing entries"
        );
    }
}

// Non-empty, short token with a maximum size (N).
// Avoids heap allocs and minimizes memory footprint.
#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub struct ShortToken<const N: usize> {
    valid_length: NonZeroU8,
    buffer: [u8; N],
}

impl<const N: usize> ShortToken<N> {
    const _ASSERT: () = assert!(N <= u8::MAX as usize, "N must be <= 255");

    pub fn new_from_str(s: &str) -> Option<Self> {
        let bytes = s.as_bytes();
        if bytes.len() == 0 || bytes.len() > N {
            return None;
        }
        let mut buffer = [0; N];
        buffer[..bytes.len()].copy_from_slice(bytes);
        Some(Self {
            valid_length: NonZeroU8::new(bytes.len() as u8).unwrap(),
            buffer,
        })
    }

    pub fn as_str(&self) -> &str {
        let valid_length = self.valid_length.get() as usize;

        // SAFETY: Buffer is always initialized with valid UTF-8 from the constructor,
        // and valid_length is guaranteed to be <= the length of the buffer.
        unsafe { std::str::from_utf8_unchecked(&self.buffer[..valid_length]) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::{AnalysisOptions, Analyzer, ReusableBuffer, StemmingLanguage};

    #[test]
    fn stemming_cache_first_analysis_discards_unscoped_entries() {
        let mut buffer = ReusableBuffer::new();
        *buffer.stemming_cache() = StemmingCache::new_with_capacity(8);
        let key = CachedToken::new_from_str("running").unwrap();
        let stem = CachedToken::new_from_str("run").unwrap();
        buffer
            .stemming_cache()
            .insert_no_clobber_assume_capacity(key, StemmingCacheEntry::Stemmed(stem));
        let analyzer = Analyzer::new(AnalysisOptions {
            stemming: Some(StemmingLanguage::English),
            ..crate::analyze::tests::opts()
        });
        analyzer.analyze("cats", &mut buffer, |_| true);
        assert!(buffer.stemming_cache().lookup(&key).is_none());

        buffer
            .stemming_cache()
            .insert_no_clobber_assume_capacity(key, StemmingCacheEntry::Stemmed(stem));
        analyzer.analyze("dogs", &mut buffer, |_| true);
        assert!(matches!(
            buffer.stemming_cache().lookup(&key),
            Some(StemmingCacheEntry::Stemmed(value)) if value.as_str() == "run"
        ));
    }

    #[test]
    fn stemming_cache_keeps_capacity_and_same_language_entries() {
        let mut buffer = ReusableBuffer::new();
        *buffer.stemming_cache() = StemmingCache::new_with_capacity(1);
        let mut options = AnalysisOptions {
            stemming: Some(StemmingLanguage::English),
            ..crate::analyze::tests::opts()
        };
        let english = Analyzer::new(options);
        english.analyze("manger", &mut buffer, |_| true);
        let capacity = buffer.stemming_cache().cache.capacity();
        assert!(capacity > 0);
        for i in 1..capacity {
            english.analyze(&i.to_string(), &mut buffer, |_| true);
        }
        assert!(!buffer.stemming_cache().has_remaining_capacity());
        buffer.reset_keep_stemming_cache();
        english.analyze("running", &mut buffer, |token| {
            assert_eq!(token.text, "run");
            true
        });
        let key = CachedToken::new_from_str("manger").unwrap();
        assert!(matches!(
            buffer.stemming_cache().lookup(&key),
            Some(StemmingCacheEntry::Unchanged)
        ));
        assert_eq!(buffer.stemming_cache().cache.capacity(), capacity);
        assert!(!buffer.stemming_cache().has_remaining_capacity());

        for (language, expected) in [
            (StemmingLanguage::French, "mang"),
            (StemmingLanguage::English, "manger"),
        ] {
            options.stemming = Some(language);
            Analyzer::new(options).analyze("manger", &mut buffer, |token| {
                assert_eq!(token.text, expected);
                true
            });
            assert_eq!(buffer.stemming_cache().cache.capacity(), capacity);
            assert_eq!(buffer.stemming_cache().cache.len(), 1);
        }
    }
}
