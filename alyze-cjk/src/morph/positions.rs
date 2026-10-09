//! Lucene's `WrappedPositionArray`: a ring buffer of lattice positions covering the live window
//! of the search, from the last backtrace to the furthest position an arc reaches. Positions
//! behind a backtrace are recycled, so memory is bounded by that window (at most the
//! 1024-position forced backtrace gap plus the longest word), not by the input. Slots keep their
//! allocations (arc vectors) across inputs.

/// A position's contents; `clear` must leave it `is_clear`.
pub(crate) trait Slot: Default {
    fn clear(&mut self);
    fn is_clear(&self) -> bool;
}

pub(crate) struct Positions<P: Slot> {
    slots: Vec<P>,
    /// Slot the next new position goes to.
    next_write: usize,
    /// Next absolute position to allocate (one past the highest allocated).
    next_pos: usize,
    /// Number of live positions: `next_pos - count` is the oldest one still held.
    count: usize,
}

impl<P: Slot> Default for Positions<P> {
    fn default() -> Self {
        Positions {
            slots: Vec::new(),
            next_write: 0,
            next_pos: 0,
            count: 0,
        }
    }
}

impl<P: Slot> Positions<P> {
    /// The position `pos`, allocating every position up to it; it must not be behind the last
    /// `free_before`.
    pub fn get(&mut self, pos: usize) -> &mut P {
        while pos >= self.next_pos {
            if self.count == self.slots.len() {
                // Full: grow, rotating the live positions (oldest first) to the front.
                let old_len = self.slots.len();
                let mut grown: Vec<P> = Vec::with_capacity((old_len * 2).max(8));
                grown.extend(self.slots.drain(self.next_write..));
                grown.append(&mut self.slots);
                grown.resize_with(grown.capacity(), P::default);
                self.slots = grown;
                self.next_write = old_len;
            }
            if self.next_write == self.slots.len() {
                self.next_write = 0;
            }
            debug_assert!(self.slots[self.next_write].is_clear());
            self.next_write += 1;
            self.next_pos += 1;
            self.count += 1;
        }
        let index = self.index(pos);
        &mut self.slots[index]
    }

    pub fn at(&self, pos: usize) -> &P {
        &self.slots[self.index(pos)]
    }

    pub fn at_mut(&mut self, pos: usize) -> &mut P {
        let index = self.index(pos);
        &mut self.slots[index]
    }

    fn index(&self, pos: usize) -> usize {
        debug_assert!(
            pos < self.next_pos && pos >= self.next_pos - self.count,
            "position {pos} not live"
        );
        let behind = self.next_pos - pos;
        if self.next_write >= behind {
            self.next_write - behind
        } else {
            self.next_write + self.slots.len() - behind
        }
    }

    /// Lucene's `getNextPos`: one past the highest allocated position.
    pub fn next_pos(&self) -> usize {
        self.next_pos
    }

    /// Recycles every position before `pos`.
    pub fn free_before(&mut self, pos: usize) {
        let to_free = self.count - (self.next_pos - pos);
        let len = self.slots.len();
        let mut index = (self.next_write + len - self.count) % len;
        for _ in 0..to_free {
            self.slots[index].clear();
            index = (index + 1) % len;
        }
        self.count -= to_free;
    }

    pub fn reset(&mut self) {
        for slot in &mut self.slots {
            slot.clear();
        }
        self.next_write = 0;
        self.next_pos = 0;
        self.count = 0;
    }

    /// Number of position slots allocated, for tests.
    #[cfg(test)]
    pub fn slots(&self) -> usize {
        self.slots.len()
    }
}
