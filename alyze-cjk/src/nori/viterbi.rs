//! The lattice and Viterbi search: a port of Lucene's `morph.Viterbi` base class and nori's
//! `Viterbi` subclass (lattice construction, unknown-word grouping, backtrace with decompounding).
//!
//! The input is handled as UTF-16 code units, like Lucene, so that every dictionary lookup,
//! character-class decision and Unicode property check sees exactly what the Java code sees.
//! Lucene reads its input incrementally through a rolling buffer; since the whole input is in
//! memory here, positions are a plain array indexed by absolute code-unit position, but the
//! control flow of `forward()` (including when it returns to its caller, which resets per-call
//! state) is kept as in Lucene because it affects the output.

use super::char_def::{self, CharClass};
use super::dict::{ConnectionCosts, TokenInfoDict, UnknownDict};
use super::unicode::{self, GeneralCategory};
use super::{DecompoundMode, Morpheme, Options, TokenData, TokenKind, Tokens, pos, user_dict};

/// Lucene's cap on the length of an unknown word, in code units.
const MAX_UNKNOWN_WORD_LENGTH: usize = 1024;
/// Lucene's cap on how far the lattice may grow before a backtrace is forced.
const MAX_BACKTRACE_GAP: usize = 1024;
/// Lucene's `left-space-penalty-factor` for the parts of speech that shouldn't follow a space.
const SPACE_PENALTY: i32 = 3000;
/// How many code units' worth of per-input buffers a reused [`Tokens`] keeps between inputs.
const RETAINED_UNITS: usize = 64 * 1024;

/// One arc arriving at a position: the cheapest path ending with one word there.
#[derive(Clone, Copy, Debug)]
struct Arc {
    cost: i32,
    last_right_id: u16,
    /// Position the word's path came from (before any skipped space).
    back_pos: u32,
    /// Where the word's text starts (after the skipped space, if any).
    back_word_pos: u32,
    /// Index of the arc at `back_pos` the path continues from.
    back_index: u32,
    /// Word id: a dictionary word id, a user-dictionary ordinal, or `class << 8 | entry` for an
    /// unknown word.
    back_id: u32,
    back_type: TokenKind,
}

#[derive(Default)]
struct Position {
    arcs: Vec<Arc>,
}

/// Lucene's `WrappedPositionArray`: a ring buffer of positions covering the live window of the
/// lattice, from the last backtrace to the furthest position an arc reaches. Positions behind a
/// backtrace are recycled, so memory is bounded by that window (at most the 1024-position forced
/// backtrace gap plus the longest word), not by the input. Arc vectors keep their capacity.
#[derive(Default)]
pub(crate) struct Positions {
    slots: Vec<Position>,
    /// Slot the next new position goes to.
    next_write: usize,
    /// Next absolute position to allocate (one past the highest allocated).
    next_pos: usize,
    /// Number of live positions: `next_pos - count` is the oldest one still held.
    count: usize,
}

impl Positions {
    /// The position `pos`, allocating every position up to it; it must not be behind the last
    /// `free_before`.
    fn get(&mut self, pos: usize) -> &mut Position {
        while pos >= self.next_pos {
            if self.count == self.slots.len() {
                // Full: grow, rotating the live positions (oldest first) to the front.
                let old_len = self.slots.len();
                let mut grown: Vec<Position> = Vec::with_capacity((old_len * 2).max(8));
                grown.extend(self.slots.drain(self.next_write..));
                grown.append(&mut self.slots);
                grown.resize_with(grown.capacity(), Position::default);
                self.slots = grown;
                self.next_write = old_len;
            }
            if self.next_write == self.slots.len() {
                self.next_write = 0;
            }
            debug_assert!(self.slots[self.next_write].arcs.is_empty());
            self.next_write += 1;
            self.next_pos += 1;
            self.count += 1;
        }
        let index = self.index(pos);
        &mut self.slots[index]
    }

    fn at(&self, pos: usize) -> &Position {
        &self.slots[self.index(pos)]
    }

    fn at_mut(&mut self, pos: usize) -> &mut Position {
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
    fn next_pos(&self) -> usize {
        self.next_pos
    }

    /// Recycles every position before `pos`.
    fn free_before(&mut self, pos: usize) {
        let to_free = self.count - (self.next_pos - pos);
        let len = self.slots.len();
        let mut index = (self.next_write + len - self.count) % len;
        for _ in 0..to_free {
            self.slots[index].arcs.clear();
            index = (index + 1) % len;
        }
        self.count -= to_free;
    }

    fn reset(&mut self) {
        for slot in &mut self.slots {
            slot.arcs.clear();
        }
        self.next_write = 0;
        self.next_pos = 0;
        self.count = 0;
    }

    /// Number of position slots allocated, for tests.
    #[cfg(test)]
    pub(crate) fn slots(&self) -> usize {
        self.slots.len()
    }
}

/// Scratch space the search reuses across inputs (kept inside [`Tokens`]).
#[derive(Default)]
pub(crate) struct Scratch {
    units: Vec<u16>,
    byte_at: Vec<usize>,
    positions: Positions,
    pending: Vec<TokenData>,
    reading_buf: Vec<u16>,
}

impl Scratch {
    /// (position slots, code-unit buffer capacity), for tests of memory retention.
    #[cfg(test)]
    pub(crate) fn footprint(&self) -> (usize, usize) {
        (self.positions.slots(), self.units.capacity())
    }
}

pub(crate) struct Viterbi<'a> {
    input: &'a str,
    options: Options<'a>,
    /// The input as UTF-16 code units.
    units: Vec<u16>,
    /// Byte offset of every code-unit index (`units.len() + 1` entries). A low surrogate maps to
    /// the byte after its pair; token boundaries never fall there.
    byte_at: Vec<usize>,
    positions: Positions,
    dict: &'static TokenInfoDict,
    unk: &'static UnknownDict,
    costs: &'static ConnectionCosts,
    /// Next absolute position to process.
    pos: usize,
    /// Last absolute position backtraced from.
    last_backtrace_pos: usize,
    /// Set once the whole input has been consumed.
    end: bool,
    /// Tokens of one backtrace, in reverse order (Lucene's `pending` list).
    pending: Vec<TokenData>,
    reading_buf: Vec<u16>,
}

impl<'a> Viterbi<'a> {
    pub fn new(input: &'a str, options: Options<'a>, scratch: Scratch) -> Self {
        let Scratch {
            mut units,
            mut byte_at,
            positions,
            pending,
            reading_buf,
        } = scratch;
        units.clear();
        units.extend(input.encode_utf16());
        byte_at.clear();
        for (byte_offset, c) in input.char_indices() {
            byte_at.push(byte_offset);
            if c.len_utf16() == 2 {
                byte_at.push(byte_offset + c.len_utf8());
            }
        }
        byte_at.push(input.len());
        let mut viterbi = Viterbi {
            input,
            options,
            units,
            byte_at,
            positions,
            dict: TokenInfoDict::get(),
            unk: UnknownDict::get(),
            costs: ConnectionCosts::get(),
            pos: 0,
            last_backtrace_pos: 0,
            end: false,
            pending,
            reading_buf,
        };
        viterbi.reset_state();
        viterbi
    }

    /// Hands the scratch space back for the next input. The per-input buffers are sized to the
    /// input; after a large one they are trimmed so that a reused `Tokens` doesn't keep it.
    pub fn into_scratch(self) -> Scratch {
        let mut scratch = Scratch {
            units: self.units,
            byte_at: self.byte_at,
            positions: self.positions,
            pending: self.pending,
            reading_buf: self.reading_buf,
        };
        scratch.units.clear();
        scratch.units.shrink_to(RETAINED_UNITS);
        scratch.byte_at.clear();
        scratch.byte_at.shrink_to(RETAINED_UNITS);
        scratch.pending.clear();
        scratch.pending.shrink_to(RETAINED_UNITS / 16);
        scratch
    }

    fn reset_state(&mut self) {
        self.positions.reset();
        self.pos = 0;
        self.end = false;
        self.last_backtrace_pos = 0;
        self.pending.clear();
        // BOS.
        self.positions.get(0).arcs.push(Arc {
            cost: 0,
            last_right_id: 0,
            back_pos: 0,
            back_word_pos: 0,
            back_index: 0,
            back_id: 0,
            back_type: TokenKind::Known,
        });
    }

    /// Runs the whole input through the search, appending the tokens to `out`.
    pub fn run(&mut self, out: &mut Tokens) {
        while !self.end {
            self.forward(out);
        }
    }

    /// Lucene's `forward()`: extends the lattice until a backtrace produces tokens (or the input
    /// ends), then returns.
    fn forward(&mut self, out: &mut Tokens) {
        // Furthest position reached by a user word in this call; a user match that doesn't reach
        // further is not added (but still suppresses system-dictionary matches).
        let mut user_word_max_pos_ahead: isize = -1;
        let len = self.units.len();
        while self.pos < len {
            let pos = self.pos;
            self.positions.get(pos);
            let is_frontier = self.positions.next_pos() == pos + 1;
            let count = self.positions.at(pos).arcs.len();
            if count == 0 {
                // No arcs arrive here; move to the next position.
                self.pos += 1;
                continue;
            }

            if pos > self.last_backtrace_pos && count == 1 && is_frontier {
                // Only one node is alive at a frontier: the best path must come through it, so
                // the prefix of the best path can be committed.
                let before = out.len();
                self.backtrace(pos, 0, out);
                // Re-base the cost so it can't overflow.
                self.positions.at_mut(pos).arcs[0].cost = 0;
                if out.len() > before {
                    return;
                }
                // Only punctuation was produced; keep parsing.
            }

            if pos - self.last_backtrace_pos >= MAX_BACKTRACE_GAP {
                // Safety: too much buffered. Backtrace from the least-cost partial path (which can,
                // in general, be wrong) and prune all others.
                let mut least_cost = i32::MAX;
                let mut least_idx = usize::MAX;
                let mut least_pos = usize::MAX;
                for pos2 in pos..self.positions.next_pos() {
                    for (idx, arc) in self.positions.at(pos2).arcs.iter().enumerate() {
                        if arc.cost < least_cost {
                            least_cost = arc.cost;
                            least_idx = idx;
                            least_pos = pos2;
                        }
                    }
                }
                debug_assert!(
                    least_pos != usize::MAX,
                    "there is always at least one live path"
                );
                for pos2 in pos..self.positions.next_pos() {
                    let arcs = &mut self.positions.at_mut(pos2).arcs;
                    if pos2 != least_pos {
                        arcs.clear();
                    } else {
                        if least_idx != 0 {
                            arcs[0] = arcs[least_idx];
                        }
                        arcs.truncate(1);
                    }
                }
                let before = out.len();
                self.backtrace(least_pos, 0, out);
                self.positions.at_mut(least_pos).arcs[0].cost = 0;
                if self.pos != least_pos {
                    // Jumped into a future position.
                    debug_assert!(self.pos < least_pos);
                    self.pos = least_pos;
                }
                if out.len() > before {
                    return;
                }
                continue;
            }

            // A single space separator is skipped and attached as a prefix of the words that
            // follow it, which is how the space penalty is computed. (Lucene: `buffer.get(++pos)
            // == -1` resets `pos`, so a trailing space is processed as a character instead.)
            let mut word_pos = pos;
            if unicode::props(self.units[pos]).category == GeneralCategory::SpaceSeparator
                && pos + 1 < len
            {
                word_pos = pos + 1;
            }

            let mut any_matches = false;

            // First try the user dictionary: only the longest match counts.
            if let Some(user) = self.options.user_dictionary {
                let mut max_pos_ahead: isize = -1;
                let mut longest_ord = 0u64;
                user.terms()
                    .for_each_prefix(&self.units[word_pos..], |matched_len, ord| {
                        max_pos_ahead = (word_pos + matched_len - 1) as isize;
                        longest_ord = ord;
                        any_matches = true;
                    });
                if any_matches && max_pos_ahead > user_word_max_pos_ahead {
                    let ord = longest_ord as u32;
                    self.add(
                        user_dict::LEFT_ID,
                        user.right_id(ord),
                        user_dict::WORD_COST,
                        pos::Tag::NNG,
                        pos,
                        word_pos,
                        max_pos_ahead as usize + 1,
                        ord,
                        TokenKind::User,
                    );
                    user_word_max_pos_ahead = user_word_max_pos_ahead.max(max_pos_ahead);
                }
            }

            if !any_matches {
                // Next, the system dictionary: every word of every term starting here.
                let dict = self.dict;
                let units = std::mem::take(&mut self.units);
                dict.for_each_prefix(&units[word_pos..], |matched_len, ids| {
                    for id in ids {
                        let w = dict.word(id);
                        self.add(
                            w.left_id,
                            w.right_id,
                            i32::from(w.cost),
                            w.left_pos,
                            pos,
                            word_pos,
                            word_pos + matched_len,
                            id,
                            TokenKind::Known,
                        );
                        any_matches = true;
                    }
                });
                self.units = units;
            }

            self.process_unknown_word(any_matches, pos, word_pos);

            self.pos = word_pos + 1;
        }

        self.end = true;
        if self.pos > 0 {
            // EOS: pick the cheapest path including the cost of connecting to the end.
            let end_pos = self.pos;
            self.positions.get(end_pos);
            let mut least_cost = i32::MAX;
            let mut least_idx = usize::MAX;
            for (idx, arc) in self.positions.at(end_pos).arcs.iter().enumerate() {
                let cost = arc.cost + i32::from(self.costs.cost(arc.last_right_id, 0));
                if cost < least_cost {
                    least_cost = cost;
                    least_idx = idx;
                }
            }
            if least_idx != usize::MAX {
                self.backtrace(end_pos, least_idx, out);
            }
        }
    }

    /// Lucene's `processUnknownWord`: groups a run of characters that no dictionary covers (or
    /// whose class always invokes unknown processing) into one unknown word.
    fn process_unknown_word(&mut self, any_matches: bool, from_pos: usize, word_pos: usize) {
        let first = self.units[word_pos];
        let first_class = char_def::class(first);
        if any_matches && !char_def::invoke(first_class) {
            return;
        }
        let mut class = first_class;
        let mut length = 1usize;
        if char_def::group(first_class) {
            // Characters of the same script are considered part of the unknown word.
            let first_props = unicode::props(first);
            let mut script = first_props.script;
            let is_punct = unicode::is_punctuation(first, first_props.category);
            let is_digit = first_props.is_digit;
            let mut ahead = word_pos + 1;
            while length < MAX_UNKNOWN_WORD_LENGTH && ahead < self.units.len() {
                let ch = self.units[ahead];
                let p = unicode::props(ch);
                // Non-spacing marks inherit the script of their base character (UTR #24).
                let same_script = unicode::is_same_script(script, p.script)
                    || p.category == GeneralCategory::NonspacingMark;
                if same_script
                    && unicode::is_punctuation(ch, p.category) == is_punct
                    && p.is_digit == is_digit
                    && char_def::group(char_def::class(ch))
                {
                    length += 1;
                } else {
                    break;
                }
                // Once the run has a real script, use it (and that character's class).
                if unicode::is_common_or_inherited(script)
                    && !unicode::is_common_or_inherited(p.script)
                {
                    script = p.script;
                    class = char_def::class(ch);
                }
                ahead += 1;
            }
            // Port divergence: Lucene's cap can land inside a surrogate pair; keep it whole.
            let last = word_pos + length - 1;
            if length == MAX_UNKNOWN_WORD_LENGTH
                && (0xD800..0xDC00).contains(&self.units[last])
                && last + 1 < self.units.len()
                && (0xDC00..0xE000).contains(&self.units[last + 1])
            {
                length += 1;
            }
        }
        for (i, w) in self.unk.words(class).iter().enumerate() {
            self.add(
                w.left_id,
                w.right_id,
                i32::from(w.cost),
                w.left_pos,
                from_pos,
                word_pos,
                word_pos + length,
                (class as u32) << 8 | i as u32,
                TokenKind::Unknown,
            );
        }
    }

    /// Lucene's `add`: connects a word to the cheapest arc at `from_pos` and records the result
    /// at `end_pos`.
    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        left_id: u16,
        right_id: u16,
        word_cost: i32,
        left_pos: pos::Tag,
        from_pos: usize,
        word_pos: usize,
        end_pos: usize,
        word_id: u32,
        kind: TokenKind,
    ) {
        let num_spaces = word_pos - from_pos;
        let space_penalty = if num_spaces > 0 && has_space_penalty(left_pos) {
            SPACE_PENALTY
        } else {
            0
        };
        let mut least_cost = i32::MAX;
        let mut least_idx = usize::MAX;
        let from = self.positions.at(from_pos);
        debug_assert!(!from.arcs.is_empty());
        for (idx, arc) in from.arcs.iter().enumerate() {
            let cost =
                arc.cost + i32::from(self.costs.cost(arc.last_right_id, left_id)) + space_penalty;
            if cost < least_cost {
                least_cost = cost;
                least_idx = idx;
            }
        }
        least_cost += word_cost;
        self.positions.get(end_pos).arcs.push(Arc {
            cost: least_cost,
            last_right_id: right_id,
            back_pos: from_pos as u32,
            back_word_pos: word_pos as u32,
            back_index: least_idx as u32,
            back_id: word_id,
            back_type: kind,
        });
    }

    /// Lucene's `backtrace`: walks the best path back from `end_pos` to the last backtrace,
    /// producing tokens (decompounded according to the mode) in `out`.
    fn backtrace(&mut self, end_pos: usize, from_idx: usize, out: &mut Tokens) {
        if end_pos == self.last_backtrace_pos {
            return;
        }
        let discard_punctuation = self.options.discard_punctuation;
        let mode = self.options.decompound_mode;
        let mut pos = end_pos;
        let mut best_idx = from_idx;
        self.pending.clear();
        while pos > self.last_backtrace_pos {
            let arc = self.positions.at(pos).arcs[best_idx];
            let back_pos = arc.back_pos as usize;
            let back_word_pos = arc.back_word_pos as usize;
            let next_best_idx = arc.back_index as usize;
            debug_assert!(back_pos >= self.last_backtrace_pos);

            if self.options.output_unknown_unigrams && arc.back_type == TokenKind::Unknown {
                // One token per character (surrogate pairs together); tagged like an n-gram.
                let ngram = self.unk.words(CharClass::Ngram)[0];
                let mut i = pos;
                while i > back_word_pos {
                    let mut start = i - 1;
                    if start > back_word_pos && (0xDC00..0xE000).contains(&self.units[start]) {
                        start -= 1;
                    }
                    let text = out.push_text(self.slice(start, i));
                    self.pending.push(TokenData {
                        text,
                        byte_range: self.byte_at[start]..self.byte_at[i],
                        position_increment: 1,
                        position_length: 1,
                        kind: TokenKind::Unknown,
                        pos_type: pos::Type::Morpheme,
                        left_pos: ngram.left_pos,
                        right_pos: ngram.right_pos,
                        reading: None,
                        morphemes: 0..0,
                    });
                    i = start;
                }
            } else {
                self.emit_word(&arc, back_word_pos, pos, mode, discard_punctuation, out);
            }

            if !discard_punctuation && back_word_pos != back_pos {
                // The skipped space becomes a token of its own.
                let space = self.unk.words(CharClass::Space)[0];
                let text = out.push_text(self.slice(back_pos, back_word_pos));
                self.pending.push(TokenData {
                    text,
                    byte_range: self.byte_at[back_pos]..self.byte_at[back_word_pos],
                    position_increment: 1,
                    position_length: 1,
                    kind: TokenKind::Unknown,
                    pos_type: pos::Type::Morpheme,
                    left_pos: space.left_pos,
                    right_pos: space.right_pos,
                    reading: None,
                    morphemes: 0..0,
                });
            }

            pos = back_pos;
            best_idx = next_best_idx;
        }
        self.last_backtrace_pos = end_pos;
        self.positions.free_before(end_pos);
        for token in self.pending.drain(..).rev() {
            out.push(token);
        }
    }

    /// Emits one dictionary / user / unknown word spanning `[start, end)`, decompounded as the
    /// mode asks.
    fn emit_word(
        &mut self,
        arc: &Arc,
        start: usize,
        end: usize,
        mode: DecompoundMode,
        discard_punctuation: bool,
        out: &mut Tokens,
    ) {
        let (pos_type, left_pos, right_pos) = match arc.back_type {
            TokenKind::Known => {
                let w = self.dict.word(arc.back_id);
                (w.pos_type, w.left_pos, w.right_pos)
            }
            TokenKind::User => {
                let user = self
                    .options
                    .user_dictionary
                    .expect("user word without a dictionary");
                let pos_type = if user.segmentation(arc.back_id).is_empty() {
                    pos::Type::Morpheme
                } else {
                    pos::Type::Compound
                };
                (pos_type, pos::Tag::NNG, pos::Tag::NNG)
            }
            TokenKind::Unknown => {
                let class = CharClass::ALL[(arc.back_id >> 8) as usize];
                let w = self.unk.words(class)[(arc.back_id & 0xFF) as usize];
                (pos::Type::Morpheme, w.left_pos, w.right_pos)
            }
        };
        let byte_range = self.byte_at[start]..self.byte_at[end];

        if pos_type == pos::Type::Morpheme || mode == DecompoundMode::None {
            let first = self.units[start];
            if discard_punctuation && unicode::is_punctuation(first, unicode::props(first).category)
            {
                return;
            }
            let morphemes = if pos_type == pos::Type::Morpheme {
                0..0
            } else {
                self.push_morphemes(arc, start, end, out)
            };
            let text = out.push_text(self.slice(start, end));
            let reading = self.reading(arc, out);
            self.pending.push(TokenData {
                text,
                byte_range,
                position_increment: 1,
                position_length: 1,
                kind: arc.back_type,
                pos_type,
                left_pos,
                right_pos,
                reading,
                morphemes,
            });
            return;
        }

        let morphemes = self.push_morphemes(arc, start, end, out);
        if morphemes.is_empty() {
            // A decomposable entry without a decomposition: emitted whole.
            let text = out.push_text(self.slice(start, end));
            let reading = self.reading(arc, out);
            self.pending.push(TokenData {
                text,
                byte_range,
                position_increment: 1,
                position_length: 1,
                kind: arc.back_type,
                pos_type,
                left_pos,
                right_pos,
                reading,
                morphemes: 0..0,
            });
            return;
        }

        // Decompose: morphemes are emitted last to first (the pending list is reversed), each a
        // plain morpheme token of the same kind. Compounds slice the surface form; inflected and
        // pre-analysed entries give every morpheme the whole entry's range.
        let count = morphemes.len();
        let mut end_offset = end;
        for i in (0..count).rev() {
            let morpheme = out.morphemes[morphemes.start + i].clone();
            let morpheme_len = morpheme.text.encode_utf16().count();
            // (Lucene decrements `endOffset` for every type and lets it go negative for inflected
            // entries, whose morphemes don't tile the surface form; it only reads it for compounds.)
            let (m_start, m_end) = if pos_type == pos::Type::Compound {
                debug_assert!(end_offset >= morpheme_len);
                end_offset -= morpheme_len;
                (end_offset, end_offset + morpheme_len)
            } else {
                (start, end)
            };
            let text = out.push_text(&morpheme.text);
            let tag = morpheme.tag;
            self.pending.push(TokenData {
                text,
                byte_range: self.byte_at[m_start]..self.byte_at[m_end],
                position_increment: if i == 0 && mode == DecompoundMode::Mixed {
                    0
                } else {
                    1
                },
                position_length: 1,
                kind: arc.back_type,
                pos_type: pos::Type::Morpheme,
                left_pos: tag,
                right_pos: tag,
                reading: None,
                morphemes: 0..0,
            });
        }
        if mode == DecompoundMode::Mixed {
            let text = out.push_text(self.slice(start, end));
            let reading = self.reading(arc, out);
            self.pending.push(TokenData {
                text,
                byte_range,
                position_increment: 1,
                position_length: count.max(1) as u32,
                kind: arc.back_type,
                pos_type,
                left_pos,
                right_pos,
                reading,
                morphemes,
            });
        }
    }

    /// Appends the morphemes of the word to `out.morphemes`, returning their range (empty for a
    /// word without a decomposition).
    fn push_morphemes(
        &self,
        arc: &Arc,
        start: usize,
        end: usize,
        out: &mut Tokens,
    ) -> std::ops::Range<usize> {
        let from = out.morphemes.len();
        match arc.back_type {
            TokenKind::Known => {
                if let Some(morphemes) = self.dict.morphemes(arc.back_id) {
                    out.morphemes.extend(morphemes);
                }
            }
            TokenKind::User => {
                let user = self
                    .options
                    .user_dictionary
                    .expect("user word without a dictionary");
                let mut at = start;
                for &len in user.segmentation(arc.back_id) {
                    let part_end = (at + usize::from(len)).min(end);
                    out.morphemes.push(Morpheme {
                        tag: pos::Tag::NNG,
                        text: self.slice(at, part_end).to_owned(),
                    });
                    at = part_end;
                }
            }
            TokenKind::Unknown => {}
        }
        from..out.morphemes.len()
    }

    /// The reading of a known word, pushed into the text buffer.
    fn reading(&mut self, arc: &Arc, out: &mut Tokens) -> Option<std::ops::Range<usize>> {
        if arc.back_type != TokenKind::Known {
            return None;
        }
        if !self.dict.reading(arc.back_id, &mut self.reading_buf) {
            return None;
        }
        let reading = String::from_utf16(&self.reading_buf).expect("bad reading");
        Some(out.push_text(&reading))
    }

    /// The input text between two code-unit positions (never inside a surrogate pair).
    fn slice(&self, start: usize, end: usize) -> &'a str {
        &self.input[self.byte_at[start]..self.byte_at[end]]
    }
}

/// Lucene's `computeSpacePenalty`: the parts of speech penalized for following a space (endings,
/// particles, the positive designator and suffixes).
fn has_space_penalty(tag: pos::Tag) -> bool {
    use pos::Tag::*;
    matches!(
        tag,
        EP | EF
            | EC
            | ETN
            | ETM
            | JKS
            | JKC
            | JKG
            | JKO
            | JKB
            | JKV
            | JKQ
            | JX
            | JC
            | VCP
            | XSA
            | XSN
            | XSV
    )
}

/// Entry point used by [`super::tokenize`].
pub(crate) fn tokenize(input: &str, options: Options<'_>, out: &mut Tokens) {
    let scratch = std::mem::take(&mut out.scratch);
    let mut viterbi = Viterbi::new(input, options, scratch);
    viterbi.run(out);
    out.scratch = viterbi.into_scratch();
}
