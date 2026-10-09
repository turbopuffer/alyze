//! The lattice and Viterbi search: a port of Lucene's `morph.Viterbi` / `morph.ViterbiNBest` as
//! specialised by `ja.ViterbiNBest` (search-mode penalties, second-best backtrace under
//! compounds, extended-mode unigrams, n-best output) and of the position bookkeeping in
//! `JapaneseTokenizer.incrementToken`.
//!
//! The input is handled as UTF-16 code units, like Lucene, so that every dictionary lookup,
//! character-class decision and Unicode property check sees exactly what the Java code sees.
//! Lucene reads its input incrementally through a rolling buffer; the whole input is in memory
//! here, but positions still live in a ring buffer over the live window (`morph::positions`), and
//! the control flow of `forward()` (including when it returns to its caller, which resets
//! per-call state) is kept as in Lucene because it affects the output.

use super::char_def::{self, CharClass};
use super::dict::{ConnectionCosts, TokenInfoDict, UnknownDict};
use super::{Mode, Options, TokenData, TokenKind, Tokens, unicode, user_dict};
use crate::morph::positions::Slot;

/// Lucene's cap on the length of an unknown word, in code units.
const MAX_UNKNOWN_WORD_LENGTH: usize = 1024;
/// Lucene's cap on how far the lattice may grow before a backtrace is forced.
const MAX_BACKTRACE_GAP: usize = 1024;
/// How many code units' worth of per-input buffers a reused [`Tokens`] keeps between inputs.
const RETAINED_UNITS: usize = 64 * 1024;

const SEARCH_MODE_KANJI_LENGTH: usize = 2;
const SEARCH_MODE_OTHER_LENGTH: usize = 7;
const SEARCH_MODE_KANJI_PENALTY: i32 = 3000;
const SEARCH_MODE_OTHER_PENALTY: i32 = 1700;

/// The word id of Lucene's BOS arc (`-1`).
const BOS_ID: u32 = u32::MAX;
/// The unknown-dictionary word id Lucene gives extended-mode unigrams (`CharacterDefinition.NGRAM`,
/// which is also the id of the first unknown entry: the NGRAM class's).
const NGRAM_ID: u32 = 0;

/// One arc arriving at a position: the cheapest path ending with one word there.
#[derive(Clone, Copy, Debug)]
struct Arc {
    cost: i32,
    last_right_id: u16,
    /// Position the word starts at.
    back_pos: u32,
    /// Index of the arc at `back_pos` the path continues from.
    back_index: u32,
    /// Word id: a dictionary word id, a user-dictionary ordinal, or `class << 8 | entry` for an
    /// unknown word ([`unknown_id`]).
    back_id: u32,
    back_type: TokenKind,
}

/// A forward pointer, used only while rescoring under a compound (`pruneAndRescore`).
#[derive(Clone, Copy, Debug)]
struct ForwardArc {
    pos: u32,
    id: u32,
    kind: TokenKind,
}

#[derive(Default)]
struct Position {
    arcs: Vec<Arc>,
    forward: Vec<ForwardArc>,
}

impl Slot for Position {
    fn clear(&mut self) {
        self.arcs.clear();
        debug_assert!(self.forward.is_empty());
    }

    fn is_clear(&self) -> bool {
        self.arcs.is_empty()
    }
}

type Positions = crate::morph::positions::Positions<Position>;

fn unknown_id(class: CharClass, entry: usize) -> u32 {
    (class as u32) << 8 | entry as u32
}

/// A token found by a backtrace, before its attributes are materialized (Lucene's `ja.Token`).
#[derive(Clone, Copy, Debug)]
struct Pending {
    /// Absolute code-unit range.
    start: u32,
    end: u32,
    word_id: u32,
    /// For a user word, which segment of the rule (its reading).
    segment: u16,
    kind: TokenKind,
    position_length: u32,
}

/// Scratch space the search reuses across inputs (kept inside [`Tokens`]).
#[derive(Default)]
pub(crate) struct Scratch {
    units: Vec<u16>,
    byte_at: Vec<usize>,
    positions: Positions,
    pending: Vec<Pending>,
    lattice: Lattice,
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
    search_mode: bool,
    extended_mode: bool,
    output_compounds: bool,
    output_nbest: bool,
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
    pending: Vec<Pending>,
    lattice: Lattice,
    /// `JapaneseTokenizer.lastTokenPos`: start of the last token served, for position increments.
    last_token_pos: isize,
    /// `calcNBestCost` support: the probed span, the smallest delta found, and the root base of
    /// the last lattice probed.
    probe: Option<(usize, usize)>,
    probe_delta: i32,
    probed_root_base: isize,
}

impl<'a> Viterbi<'a> {
    pub fn new(input: &'a str, options: Options<'a>, scratch: Scratch) -> Self {
        let Scratch {
            mut units,
            mut byte_at,
            positions,
            pending,
            lattice,
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
        let (search_mode, extended_mode, output_compounds) = match options.mode {
            Mode::Search => (true, false, !options.discard_compound_token),
            Mode::Extended => (true, true, !options.discard_compound_token),
            Mode::Normal => (false, false, false),
        };
        let mut viterbi = Viterbi {
            input,
            options,
            search_mode,
            extended_mode,
            output_compounds,
            output_nbest: options.nbest_cost > 0,
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
            lattice,
            last_token_pos: -1,
            probe: None,
            probe_delta: i32::MAX,
            probed_root_base: -1,
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
            lattice: self.lattice,
        };
        scratch.units.clear();
        scratch.units.shrink_to(RETAINED_UNITS);
        scratch.byte_at.clear();
        scratch.byte_at.shrink_to(RETAINED_UNITS);
        scratch.pending.clear();
        scratch.pending.shrink_to(RETAINED_UNITS / 16);
        scratch.lattice.shrink();
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
            back_index: 0,
            back_id: BOS_ID,
            back_type: TokenKind::Known,
        });
    }

    /// Runs the whole input through the search, appending the tokens to `out`.
    pub fn run(&mut self, out: &mut Tokens) {
        while !self.end {
            self.forward(out);
        }
    }

    /// `JapaneseTokenizer.probeDelta` with the tokenizer at n-best cost 1: the smallest extra cost
    /// of a lattice node spanning `[start, end)` over the lattices built for `input`, or `None`
    /// when no lattice is ever probed.
    pub fn probe(&mut self, start: usize, end: usize, out: &mut Tokens) -> Option<i32> {
        self.probe = Some((start, end));
        self.run(out);
        (self.probe_delta != i32::MAX).then_some(self.probe_delta)
    }

    /// Lucene's `forward()`: extends the lattice until a backtrace produces tokens (or the input
    /// ends), then returns.
    fn forward(&mut self, out: &mut Tokens) {
        // Index of the last character of the unknown word found at the previous position.
        let mut unknown_word_end_index: isize = -1;
        // Furthest position reached by a user word in this call (kept for parity with Lucene; it
        // only decides whether a user match extends the record, since every match is added).
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
                if self.output_nbest {
                    self.backtrace_nbest(pos, false);
                }
                self.backtrace(pos, 0);
                if self.output_nbest {
                    self.fixup_pending_list();
                }
                // Re-base the cost so it can't overflow.
                self.positions.at_mut(pos).arcs[0].cost = 0;
                if !self.pending.is_empty() {
                    self.serve(out);
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
                if self.output_nbest {
                    self.backtrace_nbest(least_pos, false);
                }
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
                self.backtrace(least_pos, 0);
                if self.output_nbest {
                    self.fixup_pending_list();
                }
                self.positions.at_mut(least_pos).arcs[0].cost = 0;
                if self.pos != least_pos {
                    // Jumped into a future position.
                    debug_assert!(self.pos < least_pos);
                    self.pos = least_pos;
                }
                if !self.pending.is_empty() {
                    self.serve(out);
                    return;
                }
                continue;
            }

            let mut any_matches = false;

            // First try the user dictionary: every match is added (Lucene's
            // `outputLongestUserEntryOnly` is off for Japanese).
            if let Some(user) = self.options.user_dictionary {
                let mut max_pos_ahead: isize = -1;
                let units = std::mem::take(&mut self.units);
                user.terms()
                    .for_each_prefix(&units[pos..], |matched_len, ord| {
                        max_pos_ahead = (pos + matched_len - 1) as isize;
                        any_matches = true;
                        self.add(
                            user_dict::LEFT_ID,
                            user_dict::RIGHT_ID,
                            user_dict::WORD_COST,
                            pos,
                            pos + matched_len,
                            ord as u32,
                            TokenKind::User,
                            false,
                        );
                    });
                self.units = units;
                if any_matches && max_pos_ahead > user_word_max_pos_ahead {
                    user_word_max_pos_ahead = max_pos_ahead;
                }
            }

            if !any_matches {
                // Next, the system dictionary: every word of every term starting here.
                let dict = self.dict;
                let units = std::mem::take(&mut self.units);
                dict.for_each_prefix(&units[pos..], |matched_len, ids| {
                    for id in ids {
                        let w = dict.word(id);
                        self.add(
                            w.left_id,
                            w.right_id,
                            i32::from(w.cost),
                            pos,
                            pos + matched_len,
                            id,
                            TokenKind::Known,
                            false,
                        );
                        any_matches = true;
                    }
                });
                self.units = units;
            }

            // `shouldSkipProcessUnknownWord`: in normal mode, a position inside the unknown word
            // found at the previous position is skipped.
            if self.search_mode || unknown_word_end_index <= pos as isize {
                let unknown_word_length = self.process_unknown_word(any_matches, pos);
                unknown_word_end_index = (pos + unknown_word_length) as isize;
            }

            self.pos += 1;
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
            if self.output_nbest {
                self.backtrace_nbest(end_pos, true);
            }
            if least_idx != usize::MAX {
                self.backtrace(end_pos, least_idx);
            }
            if self.output_nbest {
                self.fixup_pending_list();
            }
            self.serve(out);
        }
    }

    /// Lucene's `ja.ViterbiNBest.processUnknownWord`: groups a run of characters of the same
    /// class (and punctuation-ness) that no dictionary covers, or whose class always invokes
    /// unknown processing, into one unknown word per unknown-dictionary entry of the class.
    /// Returns the word length (0 when nothing was added).
    fn process_unknown_word(&mut self, any_matches: bool, pos: usize) -> usize {
        let first = self.units[pos];
        let class = char_def::class(first);
        if any_matches && !char_def::invoke(class) {
            return 0;
        }
        let is_punct = is_punctuation(first);
        let mut length = 1usize;
        if char_def::group(class) {
            let mut ahead = pos + 1;
            while length < MAX_UNKNOWN_WORD_LENGTH && ahead < self.units.len() {
                let ch = self.units[ahead];
                if char_def::class(ch) == class && is_punctuation(ch) == is_punct {
                    length += 1;
                } else {
                    break;
                }
                ahead += 1;
            }
            // Port divergence: Lucene's cap can land inside a surrogate pair; keep it whole.
            let last = pos + length - 1;
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
                pos,
                pos + length,
                unknown_id(class, i),
                TokenKind::Unknown,
                false,
            );
        }
        length
    }

    /// The connection ids and cost of a word, by type (Lucene's `getDict(type)` lookups).
    fn word_info(&self, kind: TokenKind, id: u32) -> (u16, u16, i32) {
        match kind {
            TokenKind::Known => {
                let w = self.dict.word(id);
                (w.left_id, w.right_id, i32::from(w.cost))
            }
            TokenKind::Unknown => {
                let w = self.unk.words(CharClass::ALL[(id >> 8) as usize])[(id & 0xFF) as usize];
                (w.left_id, w.right_id, i32::from(w.cost))
            }
            TokenKind::User => (
                user_dict::LEFT_ID,
                user_dict::RIGHT_ID,
                user_dict::WORD_COST,
            ),
        }
    }

    /// Lucene's `add`: connects a word to the cheapest arc at `from_pos` and records the result
    /// at `end_pos`. `add_penalty` applies the search-mode penalty (only while rescoring).
    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        left_id: u16,
        right_id: u16,
        word_cost: i32,
        from_pos: usize,
        end_pos: usize,
        word_id: u32,
        kind: TokenKind,
        add_penalty: bool,
    ) {
        let mut least_cost = i32::MAX;
        let mut least_idx = usize::MAX;
        let from = self.positions.at(from_pos);
        debug_assert!(!from.arcs.is_empty());
        for (idx, arc) in from.arcs.iter().enumerate() {
            let cost = arc.cost + i32::from(self.costs.cost(arc.last_right_id, left_id));
            if cost < least_cost {
                least_cost = cost;
                least_idx = idx;
            }
        }
        least_cost += word_cost;
        if add_penalty && kind != TokenKind::User {
            least_cost += self.compute_penalty(from_pos, end_pos - from_pos);
        }
        self.positions.get(end_pos).arcs.push(Arc {
            cost: least_cost,
            last_right_id: right_id,
            back_pos: from_pos as u32,
            back_index: least_idx as u32,
            back_id: word_id,
            back_type: kind,
        });
    }

    /// Lucene's `computePenalty`: search mode penalizes long words, so that compounds decompose
    /// (kanji runs over 2, anything else over 7 code units).
    fn compute_penalty(&self, pos: usize, length: usize) -> i32 {
        if length > SEARCH_MODE_KANJI_LENGTH {
            let all_kanji = self.units[pos..pos + length]
                .iter()
                .all(|&u| char_def::is_kanji(char_def::class(u)));
            if all_kanji {
                return (length - SEARCH_MODE_KANJI_LENGTH) as i32 * SEARCH_MODE_KANJI_PENALTY;
            } else if length > SEARCH_MODE_OTHER_LENGTH {
                return (length - SEARCH_MODE_OTHER_LENGTH) as i32 * SEARCH_MODE_OTHER_PENALTY;
            }
        }
        0
    }

    /// Lucene's `ja.ViterbiNBest.backtrace`: walks the best path back from `end_pos` to the last
    /// backtrace, producing tokens in `pending` (last first). In search mode a word that incurs a
    /// penalty is a compound: if, with the word pruned from the lattice, a second-best path
    /// through its span costs no more than the penalty, that path is followed instead and the
    /// compound is emitted alongside it, spanning its parts' positions.
    fn backtrace(&mut self, end_pos: usize, from_idx: usize) {
        if end_pos == self.last_backtrace_pos {
            return;
        }
        let discard_punctuation = self.options.discard_punctuation;
        let mut pos = end_pos;
        let mut best_idx = from_idx;
        let mut alt_token: Option<Pending> = None;
        // The left id of the token after the one being processed (we walk backwards).
        let mut last_left_word_id: i32 = -1;
        let mut back_count = 0u32;

        while pos > self.last_backtrace_pos {
            let arc = self.positions.at(pos).arcs[best_idx];
            let mut back_pos = arc.back_pos as usize;
            debug_assert!(back_pos >= self.last_backtrace_pos);
            let mut length = pos - back_pos;
            let mut back_type = arc.back_type;
            let mut back_id = arc.back_id;
            let mut next_best_idx = arc.back_index as usize;

            if self.search_mode && alt_token.is_none() && back_type != TokenKind::User {
                let penalty = self.compute_penalty(back_pos, length);
                if penalty > 0 {
                    // The penalty bounds the extra cost a second-best segmentation may have.
                    let mut max_cost = arc.cost + penalty;
                    if last_left_word_id != -1 {
                        let right_id = self.word_info(back_type, back_id).1;
                        max_cost += i32::from(self.costs.cost(right_id, last_left_word_id as u16));
                    }
                    // Prune all too-long tokens from the span and rescore it.
                    self.prune_and_rescore(back_pos, pos, arc.back_index as usize);
                    // Find the second-best arc arriving here.
                    let mut least_cost = i32::MAX;
                    let mut least_idx = usize::MAX;
                    for (idx, a) in self.positions.at(pos).arcs.iter().enumerate() {
                        let mut cost = a.cost;
                        if last_left_word_id != -1 {
                            let right_id = self.word_info(a.back_type, a.back_id).1;
                            cost += i32::from(self.costs.cost(right_id, last_left_word_id as u16));
                        }
                        if cost < least_cost {
                            least_cost = cost;
                            least_idx = idx;
                        }
                    }
                    if least_idx != usize::MAX
                        && least_cost <= max_cost
                        && self.positions.at(pos).arcs[least_idx].back_pos as usize != back_pos
                    {
                        // Keep the compound to emit when the alternate path joins back.
                        alt_token = Some(Pending {
                            start: back_pos as u32,
                            end: pos as u32,
                            word_id: back_id,
                            segment: 0,
                            kind: back_type,
                            position_length: 1,
                        });
                        // Redirect the backtrace to the second-best path.
                        best_idx = least_idx;
                        let a = self.positions.at(pos).arcs[best_idx];
                        next_best_idx = a.back_index as usize;
                        back_pos = a.back_pos as usize;
                        length = pos - back_pos;
                        back_type = a.back_type;
                        back_id = a.back_id;
                        back_count = 0;
                    }
                    // Else no second-best path; only the compound token is output.
                }
            }

            if let Some(mut alt) = alt_token {
                if alt.start as usize >= back_pos {
                    if self.output_compounds {
                        debug_assert_eq!(alt.start as usize, back_pos);
                        if back_count > 0 {
                            back_count += 1;
                            alt.position_length = back_count;
                            self.pending.push(alt);
                        } else {
                            // The alternate path was all punctuation: the compound is dropped.
                            debug_assert!(discard_punctuation);
                        }
                    }
                    alt_token = None;
                }
            }

            if back_type == TokenKind::User {
                // Expand the rule into its segments (added in order, then reversed, since the
                // pending list is served backwards).
                let user = self
                    .options
                    .user_dictionary
                    .expect("user word without a dictionary");
                let entry = user.entry(back_id);
                let first = self.pending.len();
                let mut current = back_pos;
                for (j, &len) in entry.segments.iter().enumerate() {
                    let len = usize::from(len);
                    self.pending.push(Pending {
                        start: current as u32,
                        end: (current + len) as u32,
                        word_id: back_id,
                        segment: j as u16,
                        kind: TokenKind::User,
                        position_length: 1,
                    });
                    current += len;
                }
                self.pending[first..].reverse();
                back_count += entry.segments.len() as u32;
            } else if self.extended_mode && back_type == TokenKind::Unknown {
                // Unknown words become unigrams (surrogate pairs together), tagged as n-grams.
                let mut unigram_count = 0u32;
                let mut i = length;
                while i > 0 {
                    i -= 1;
                    let mut char_len = 1usize;
                    if i > 0 && (0xDC00..0xE000).contains(&self.units[back_pos + i]) {
                        i -= 1;
                        char_len = 2;
                    }
                    if !discard_punctuation || !is_punctuation(self.units[back_pos + i]) {
                        let start = back_pos + i;
                        self.pending.push(Pending {
                            start: start as u32,
                            end: (start + char_len) as u32,
                            word_id: NGRAM_ID,
                            segment: 0,
                            kind: TokenKind::Unknown,
                            position_length: 1,
                        });
                        unigram_count += 1;
                    }
                }
                back_count += unigram_count;
            } else if !discard_punctuation || length == 0 || !is_punctuation(self.units[back_pos]) {
                self.pending.push(Pending {
                    start: back_pos as u32,
                    end: pos as u32,
                    word_id: back_id,
                    segment: 0,
                    kind: back_type,
                    position_length: 1,
                });
                back_count += 1;
            }

            last_left_word_id = i32::from(self.word_info(back_type, back_id).0);
            pos = back_pos;
            best_idx = next_best_idx;
        }

        self.last_backtrace_pos = end_pos;
        self.positions.free_before(end_pos);
    }

    /// Lucene's `pruneAndRescore`: drops every arc in `(start_pos, end_pos]` that reaches back
    /// before `start_pos` (the compound and anything else crossing its start), then rebuilds the
    /// arcs inside the span from forward pointers, with the search-mode penalty applied to every
    /// word, so that a second-best segmentation of the compound's span can be found.
    fn prune_and_rescore(&mut self, start_pos: usize, end_pos: usize, best_start_idx: usize) {
        // First pass: walk backwards, building forward arcs and pruning inadmissible ones.
        for pos in ((start_pos + 1)..=end_pos).rev() {
            let arcs = std::mem::take(&mut self.positions.at_mut(pos).arcs);
            for arc in &arcs {
                let back_pos = arc.back_pos as usize;
                if back_pos >= start_pos {
                    self.positions.at_mut(back_pos).forward.push(ForwardArc {
                        pos: pos as u32,
                        id: arc.back_id,
                        kind: arc.back_type,
                    });
                }
            }
            // The position's arcs are cleared (and the vector's capacity kept).
            let mut arcs = arcs;
            arcs.clear();
            self.positions.at_mut(pos).arcs = arcs;
        }

        // Second pass: walk forward, rescoring.
        for pos in start_pos..end_pos {
            let forward = std::mem::take(&mut self.positions.at_mut(pos).forward);
            if self.positions.at(pos).arcs.is_empty() {
                // No arcs arrive here any more.
                self.positions.at_mut(pos).forward = forward;
                self.positions.at_mut(pos).forward.clear();
                continue;
            }
            if pos == start_pos {
                // On the initial position only the best path counts, so the sub-segmentation is
                // in the context of what the compound had matched.
                let best = self.positions.at(pos).arcs[best_start_idx];
                let right_id = if start_pos == 0 {
                    0
                } else {
                    self.word_info(best.back_type, best.back_id).1
                };
                let path_cost = best.cost;
                for f in &forward {
                    let (left_id, word_right_id, word_cost) = self.word_info(f.kind, f.id);
                    let to_pos = f.pos as usize;
                    let new_cost = path_cost
                        + word_cost
                        + i32::from(self.costs.cost(right_id, left_id))
                        + self.compute_penalty(pos, to_pos - pos);
                    self.positions.get(to_pos).arcs.push(Arc {
                        cost: new_cost,
                        last_right_id: word_right_id,
                        back_pos: pos as u32,
                        back_index: best_start_idx as u32,
                        back_id: f.id,
                        back_type: f.kind,
                    });
                }
            } else {
                // On other positions, the best over every arriving arc.
                for f in &forward {
                    let (left_id, right_id, word_cost) = self.word_info(f.kind, f.id);
                    self.add(
                        left_id,
                        right_id,
                        word_cost,
                        pos,
                        f.pos as usize,
                        f.id,
                        f.kind,
                        true,
                    );
                }
            }
            let mut forward = forward;
            forward.clear();
            self.positions.at_mut(pos).forward = forward;
        }
    }

    // --------------------------------------------------------------------------------------------
    // n-best

    /// Lucene's `morph.ViterbiNBest.backtraceNBest`: builds a lattice over the window since the
    /// last backtrace and registers every node of the best path and of each n-best path whose
    /// cost is within the n-best cost of the best one.
    fn backtrace_nbest(&mut self, end_pos: usize, use_eos: bool) {
        let mut lattice = std::mem::take(&mut self.lattice);
        lattice.setup(self, self.last_backtrace_pos, end_pos, use_eos);
        lattice.mark_unreachable();
        lattice.calc_left_cost(self.costs);
        lattice.calc_right_cost(self.costs);
        let best_cost = lattice.best_cost();
        let mut nodes = Vec::new();
        lattice.best_path_node_list(&mut nodes);
        for &node in &nodes {
            self.register_node(&lattice, node);
        }
        let mut n = 2;
        loop {
            lattice.n_best_node_list(n, &mut nodes);
            if nodes.is_empty() {
                break;
            }
            let cost = lattice.cost(nodes[0]);
            if best_cost.wrapping_add(self.options.nbest_cost) < cost {
                break;
            }
            for &node in &nodes {
                self.register_node(&lattice, node);
            }
            n += 1;
        }
        self.lattice = lattice;
    }

    /// Lucene's `ja.ViterbiNBest.registerNode`: adds an n-best lattice node to the pending list
    /// (a user word as the whole entry plus each proper segment).
    fn register_node(&mut self, lattice: &Lattice, node: usize) {
        let base = lattice.root_base;
        let left = base + lattice.left[node] as usize;
        let right = base + lattice.right[node] as usize;
        let kind = lattice.dic_type[node];
        let word_id = lattice.word_id[node];
        if self.options.discard_punctuation && is_punctuation(self.units[left]) {
            return;
        }
        if kind == TokenKind::User {
            let user = self
                .options
                .user_dictionary
                .expect("user word without a dictionary");
            let entry = user.entry(word_id);
            self.pending.push(Pending {
                start: left as u32,
                end: right as u32,
                word_id,
                segment: 0,
                kind,
                position_length: 1,
            });
            let mut current = 0usize;
            for (j, &len) in entry.segments.iter().enumerate() {
                let len = usize::from(len);
                if len < right - left {
                    let start = left + current;
                    self.pending.push(Pending {
                        start: start as u32,
                        end: (start + len) as u32,
                        word_id,
                        segment: j as u16,
                        kind,
                        position_length: 1,
                    });
                }
                current += len;
            }
        } else {
            self.pending.push(Pending {
                start: left as u32,
                end: right as u32,
                word_id,
                segment: 0,
                kind,
                position_length: 1,
            });
        }
    }

    /// Lucene's `fixupPendingList`: with n-best output the pending list holds the best path's
    /// tokens and the n-best nodes, possibly overlapping; sort them, drop duplicates (a user
    /// token wins over a known or unknown one of the same span), compute position lengths from
    /// the distinct token edges, and reverse into serving order.
    fn fixup_pending_list(&mut self) {
        self.pending.sort_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then((a.end - a.start).cmp(&(b.end - b.start)))
                .then((b.kind as u8).cmp(&(a.kind as u8)))
        });
        self.pending
            .dedup_by(|b, a| a.start == b.start && a.end == b.end);
        let mut edges: Vec<u32> = Vec::with_capacity(self.pending.len() * 2);
        for p in &self.pending {
            edges.push(p.start);
            edges.push(p.end);
        }
        edges.sort_unstable();
        edges.dedup();
        for p in &mut self.pending {
            let index = |o: u32| edges.binary_search(&o).unwrap() as u32;
            p.position_length = index(p.end) - index(p.start);
        }
        self.pending.reverse();
    }

    // --------------------------------------------------------------------------------------------
    // Serving tokens

    /// `JapaneseTokenizer.incrementToken` for every pending token, last first.
    fn serve(&mut self, out: &mut Tokens) {
        if let Some((start, end)) = self.probe {
            // `probeDelta`: probe each newly built lattice once, when its tokens are served.
            if self.output_nbest && self.lattice.root_base as isize != self.probed_root_base {
                self.probed_root_base = self.lattice.root_base as isize;
                self.probe_delta = self.probe_delta.min(self.lattice.probe_delta(start, end));
            }
        }
        let pending = std::mem::take(&mut self.pending);
        for token in pending.iter().rev() {
            self.materialize(token, out);
        }
        let mut pending = pending;
        pending.clear();
        self.pending = pending;
    }

    fn materialize(&mut self, token: &Pending, out: &mut Tokens) {
        let start = token.start as usize;
        let end = token.end as usize;
        let (position_increment, position_length) = if token.start as isize == self.last_token_pos {
            (0, token.position_length)
        } else if self.output_nbest {
            (1, token.position_length)
        } else {
            (1, 1)
        };
        self.last_token_pos = token.start as isize;
        let text = out.push_text(&self.input[self.byte_at[start]..self.byte_at[end]]);
        let surface = &self.units[start..end];
        let mut data = TokenData {
            text,
            byte_range: self.byte_at[start]..self.byte_at[end],
            position_increment,
            position_length,
            kind: token.kind,
            part_of_speech: 0..0,
            base_form: None,
            reading: None,
            pronunciation: None,
            inflection_type: None,
            inflection_form: None,
        };
        match token.kind {
            TokenKind::Known => {
                let id = token.word_id;
                data.part_of_speech = out.push_text(self.dict.part_of_speech(id));
                data.base_form = self.dict.base_form(id, surface).map(|s| out.push_text(&s));
                data.reading = Some(out.push_text(&self.dict.reading(id, surface)));
                data.pronunciation = Some(out.push_text(&self.dict.pronunciation(id, surface)));
                data.inflection_type = self.dict.inflection_type(id).map(|s| out.push_text(s));
                data.inflection_form = self.dict.inflection_form(id).map(|s| out.push_text(s));
            }
            TokenKind::Unknown => {
                let id = token.word_id;
                let w = self.unk.words(CharClass::ALL[(id >> 8) as usize])[(id & 0xFF) as usize];
                data.part_of_speech = out.push_text(w.part_of_speech);
            }
            TokenKind::User => {
                let user = self
                    .options
                    .user_dictionary
                    .expect("user word without a dictionary");
                let entry = user.entry(token.word_id);
                data.part_of_speech = out.push_text(&entry.part_of_speech);
                data.reading = Some(out.push_text(&entry.readings[token.segment as usize]));
            }
        }
        out.push(data);
    }
}

#[inline]
fn is_punctuation(unit: u16) -> bool {
    unicode::is_punctuation(unicode::category(unit))
}

// ------------------------------------------------------------------------------------------------
// Lattice (n-best)

/// Lucene's `ViterbiNBest.Lattice`: the words of one backtrace window as nodes chained by start
/// (`lroot`) and end (`rroot`) offset, with forward and backward Viterbi costs so that the cost
/// of the best path through any node is known.
#[derive(Default)]
struct Lattice {
    root_base: usize,
    root_size: usize,
    lroot: Vec<i32>,
    rroot: Vec<i32>,
    use_eos: bool,
    dic_type: Vec<TokenKind>,
    word_id: Vec<u32>,
    /// -1 excluded, 0 unused, 1 best path, n: n-th best path.
    mark: Vec<i32>,
    left_id: Vec<u16>,
    right_id: Vec<u16>,
    word_cost: Vec<i32>,
    left_cost: Vec<i32>,
    right_cost: Vec<i32>,
    left_node: Vec<i32>,
    right_node: Vec<i32>,
    /// Start / end offsets relative to `root_base` (-1 for BOS / EOS).
    left: Vec<i32>,
    right: Vec<i32>,
    left_chain: Vec<i32>,
    right_chain: Vec<i32>,
}

impl Lattice {
    fn shrink(&mut self) {
        if self.dic_type.capacity() > 4096 {
            *self = Lattice::default();
        }
    }

    fn clear_nodes(&mut self) {
        self.dic_type.clear();
        self.word_id.clear();
        self.mark.clear();
        self.left_id.clear();
        self.right_id.clear();
        self.word_cost.clear();
        self.left_cost.clear();
        self.right_cost.clear();
        self.left_node.clear();
        self.right_node.clear();
        self.left.clear();
        self.right.clear();
        self.left_chain.clear();
        self.right_chain.clear();
    }

    fn node_count(&self) -> usize {
        self.dic_type.len()
    }

    fn add_node(
        &mut self,
        viterbi: &Viterbi<'_>,
        kind: TokenKind,
        word_id: u32,
        left: i32,
        right: i32,
    ) -> usize {
        let node = self.node_count();
        self.dic_type.push(kind);
        self.word_id.push(word_id);
        self.mark.push(0);
        if word_id == BOS_ID {
            self.word_cost.push(0);
            self.left_id.push(0);
            self.right_id.push(0);
        } else {
            let (left_id, right_id, cost) = viterbi.word_info(kind, word_id);
            self.word_cost.push(cost);
            self.left_id.push(left_id);
            self.right_id.push(right_id);
        }
        self.left_cost.push(0);
        self.right_cost.push(0);
        self.left_node.push(-1);
        self.right_node.push(-1);
        self.left.push(left);
        self.right.push(right);
        if left >= 0 {
            self.left_chain.push(self.lroot[left as usize]);
            self.lroot[left as usize] = node as i32;
        } else {
            self.left_chain.push(-1);
        }
        if right >= 0 {
            self.right_chain.push(self.rroot[right as usize]);
            self.rroot[right as usize] = node as i32;
        } else {
            self.right_chain.push(-1);
        }
        node
    }

    fn setup(
        &mut self,
        viterbi: &Viterbi<'_>,
        prev_offset: usize,
        end_offset: usize,
        use_eos: bool,
    ) {
        debug_assert_eq!(viterbi.positions.at(prev_offset).arcs.len(), 1);
        self.use_eos = use_eos;
        self.root_base = prev_offset;
        self.root_size = end_offset - prev_offset + 1;
        self.lroot.clear();
        self.lroot.resize(self.root_size, -1);
        self.rroot.clear();
        self.rroot.resize(self.root_size, -1);
        self.clear_nodes();
        // BOS (node 0): the word the window continues from.
        let first = viterbi.positions.at(prev_offset).arcs[0];
        let bos = self.add_node(viterbi, first.back_type, first.back_id, -1, 0);
        debug_assert_eq!(bos, 0);
        // EOS (node 1).
        let eos = self.add_node(
            viterbi,
            TokenKind::Known,
            BOS_ID,
            (end_offset - prev_offset) as i32,
            -1,
        );
        debug_assert_eq!(eos, 1);
        for offset in ((prev_offset + 1)..=end_offset).rev() {
            let right = offset - prev_offset;
            // Only nodes something continues from (ends where a node starts) are connected.
            if self.lroot[right] >= 0 {
                for i in 0..viterbi.positions.at(offset).arcs.len() {
                    let arc = viterbi.positions.at(offset).arcs[i];
                    self.add_node(
                        viterbi,
                        arc.back_type,
                        arc.back_id,
                        (arc.back_pos as usize - prev_offset) as i32,
                        right as i32,
                    );
                }
            }
        }
    }

    /// Marks nodes that start where nothing ends as unreachable.
    fn mark_unreachable(&mut self) {
        for index in 1..self.root_size.saturating_sub(1) {
            if self.rroot[index] < 0 {
                let mut node = self.lroot[index];
                while node >= 0 {
                    self.mark[node as usize] = -1;
                    node = self.left_chain[node as usize];
                }
            }
        }
    }

    fn connection_cost(&self, costs: &ConnectionCosts, left: usize, right: usize) -> i32 {
        let left_id = self.left_id[right];
        if left_id == 0 && !self.use_eos {
            0
        } else {
            i32::from(costs.cost(self.right_id[left], left_id))
        }
    }

    fn calc_left_cost(&mut self, costs: &ConnectionCosts) {
        for index in 0..self.root_size {
            let mut node = self.lroot[index];
            while node >= 0 {
                let n = node as usize;
                if self.mark[n] >= 0 {
                    let mut least_node = -1i32;
                    let mut least_cost = i32::MAX;
                    let mut left_node = self.rroot[index];
                    while left_node >= 0 {
                        let l = left_node as usize;
                        if self.mark[l] >= 0 {
                            let cost = self.left_cost[l]
                                + self.word_cost[l]
                                + self.connection_cost(costs, l, n);
                            if cost < least_cost {
                                least_cost = cost;
                                least_node = left_node;
                            }
                        }
                        left_node = self.right_chain[l];
                    }
                    debug_assert!(least_node >= 0);
                    self.left_node[n] = least_node;
                    self.left_cost[n] = least_cost;
                }
                node = self.left_chain[n];
            }
        }
    }

    fn calc_right_cost(&mut self, costs: &ConnectionCosts) {
        for index in (0..self.root_size).rev() {
            let mut node = self.rroot[index];
            while node >= 0 {
                let n = node as usize;
                if self.mark[n] >= 0 {
                    let mut least_node = -1i32;
                    let mut least_cost = i32::MAX;
                    let mut right_node = self.lroot[index];
                    while right_node >= 0 {
                        let r = right_node as usize;
                        if self.mark[r] >= 0 {
                            let cost = self.right_cost[r]
                                + self.word_cost[r]
                                + self.connection_cost(costs, n, r);
                            if cost < least_cost {
                                least_cost = cost;
                                least_node = right_node;
                            }
                        }
                        right_node = self.left_chain[r];
                    }
                    debug_assert!(least_node >= 0);
                    self.right_node[n] = least_node;
                    self.right_cost[n] = least_cost;
                }
                node = self.right_chain[n];
            }
        }
    }

    /// Marks every node with the same span as `reference`.
    fn mark_same_span_node(&mut self, reference: usize, value: i32) {
        let left = self.left[reference];
        let right = self.right[reference];
        let mut node = self.lroot[left as usize];
        while node >= 0 {
            let n = node as usize;
            if self.right[n] == right {
                self.mark[n] = value;
            }
            node = self.left_chain[n];
        }
    }

    fn best_path_node_list(&mut self, list: &mut Vec<usize>) {
        list.clear();
        let mut node = self.right_node[0];
        while node != 1 {
            let n = node as usize;
            list.push(n);
            self.mark_same_span_node(n, 1);
            node = self.right_node[n];
        }
    }

    fn cost(&self, node: usize) -> i32 {
        self.left_cost[node] + self.word_cost[node] + self.right_cost[node]
    }

    /// The unmarked nodes of least path cost (ties with distinct spans all count), marked `n`.
    fn n_best_node_list(&mut self, n: i32, list: &mut Vec<usize>) {
        list.clear();
        let mut least_cost = i32::MAX;
        let mut least_left = -1;
        let mut least_right = -1;
        for node in 2..self.node_count() {
            if self.mark[node] == 0 {
                let cost = self.cost(node);
                if cost < least_cost {
                    least_cost = cost;
                    least_left = self.left[node];
                    least_right = self.right[node];
                    list.clear();
                    list.push(node);
                } else if cost == least_cost
                    && (self.left[node] != least_left || self.right[node] != least_right)
                {
                    list.push(node);
                }
            }
        }
        for &node in list.iter() {
            self.mark_same_span_node(node, n);
        }
    }

    fn best_cost(&self) -> i32 {
        self.left_cost[1]
    }

    /// Lucene's `probeDelta`: how much more than the best path the cheapest path through a node
    /// spanning `[start, end)` costs (`i32::MAX` when the span is outside the window; a huge
    /// value when it is inside but no node spans it, as in Lucene).
    fn probe_delta(&self, start: usize, end: usize) -> i32 {
        let Some(left) = start.checked_sub(self.root_base) else {
            return i32::MAX;
        };
        let Some(right) = end.checked_sub(self.root_base) else {
            return i32::MAX;
        };
        if self.root_size < right {
            return i32::MAX;
        }
        let mut probed_cost = i32::MAX;
        if left < self.root_size {
            let mut node = self.lroot[left];
            while node >= 0 {
                let n = node as usize;
                if self.right[n] == right as i32 {
                    probed_cost = probed_cost.min(self.cost(n));
                }
                node = self.left_chain[n];
            }
        }
        probed_cost.wrapping_sub(self.best_cost())
    }
}

/// Entry point used by [`super::tokenize`].
pub(crate) fn tokenize(input: &str, options: Options<'_>, out: &mut Tokens) {
    let scratch = std::mem::take(&mut out.scratch);
    let mut viterbi = Viterbi::new(input, options, scratch);
    viterbi.run(out);
    out.scratch = viterbi.into_scratch();
}

/// `JapaneseTokenizer.probeDelta` for [`super::calc_nbest_cost`]: `-1` when `word` doesn't occur
/// in `text` or no lattice spans it, else the n-best cost needed for it to appear.
pub(crate) fn probe_delta(text: &str, word: &str, options: Options<'_>) -> i32 {
    let Some(start_bytes) = text.find(word) else {
        return -1;
    };
    let start = text[..start_bytes].encode_utf16().count();
    let end = start + word.encode_utf16().count();
    let options = Options {
        nbest_cost: 1,
        ..options
    };
    let mut out = Tokens::new();
    let mut viterbi = Viterbi::new(text, options, Scratch::default());
    viterbi.probe(start, end, &mut out).unwrap_or(-1)
}
