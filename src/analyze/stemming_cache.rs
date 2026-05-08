use std::num::NonZeroU8;

use ahash::AHashMap;

/// Keep a small cache of stemmed tokens to avoid repeated stemming of common tokens.
/// Can yield ~2x throughput improvement for stemmed analysis.
#[derive(Debug, Clone)]
pub struct StemmingCache {
    cache: AHashMap<CachedToken, StemmingCacheEntry>,
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
            cache: AHashMap::with_capacity(capacity),
        }
    }

    pub fn lookup(&self, key: &CachedToken) -> Option<&StemmingCacheEntry> {
        self.cache.get(key)
    }

    pub fn has_remaining_capacity(&self) -> bool {
        self.cache.len() < self.cache.capacity()
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
