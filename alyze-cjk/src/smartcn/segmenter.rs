//! Segments one sentence into words: a port of Lucene's `HHMMSegmenter`, `SegGraph`,
//! `BiSegGraph` and `SegTokenFilter`.
//!
//! A lattice of candidate tokens is built over the sentence (every dictionary word starting at
//! each Chinese character, runs of letters, runs of digits, single punctuation and other
//! characters), every token is linked to every token that can follow it with a weight derived
//! from the pair's bigram frequency, and the lightest path from the sentence-begin to the
//! sentence-end sentinel is the segmentation.

use std::ops::Range;

use super::TokenKind;
use super::char_type::{CharType, char_type};
use super::dict::{BigramDict, CoreDict};

/// Maximum bigram frequency, used by the smoothing in [`edge_weight`] and as the frequency of
/// punctuation tokens.
const MAX_FREQUENCY: i32 = 2079997 + 80000;

/// Dictionary identities of the non-Chinese token kinds, as the bigram data was trained with.
const STRING_IDENTITY: [u16; 4] = [0x672A, 0x0023, 0x0023, 0x4E32]; // 未##串
const NUMBER_IDENTITY: [u16; 4] = [0x672A, 0x0023, 0x0023, 0x6570]; // 未##数
const SENTENCE_BEGIN_IDENTITY: [u16; 4] = [0x59CB, 0x0023, 0x0023, 0x59CB]; // 始##始
const SENTENCE_END_IDENTITY: [u16; 4] = [0x672B, 0x0023, 0x0023, 0x672B]; // 末##末
const PAIR_SEPARATOR: u16 = b'@' as u16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WordType {
    SentenceBegin,
    SentenceEnd,
    ChineseWord,
    String,
    Number,
    Delimiter,
    FullwidthString,
    FullwidthNumber,
}

struct LatticeToken {
    /// What the token is to the dictionaries: the word's code units, or an identity constant.
    identity: Vec<u16>,
    /// Index into [`Lattice::slots`]: 0 for the begin sentinel, `byte start + 1` otherwise.
    slot: usize,
    /// Byte end in the sentence; 0 for the begin sentinel.
    end: usize,
    word_type: WordType,
    frequency: i32,
}

struct Edge {
    from: usize,
    weight: f64,
}

#[derive(Clone, Copy)]
struct PathNode {
    weight: f64,
    previous: usize,
}

pub(crate) struct Segmenter {
    core: &'static CoreDict,
    bigram: &'static BigramDict,
    tokens: Vec<LatticeToken>,
    /// Token indices grouped by start: slot 0 is the begin sentinel, slot `b + 1` the tokens
    /// starting at byte `b`, and the last slot the end sentinel.
    slots: Vec<Vec<usize>>,
    incoming: Vec<Vec<Edge>>,
    path: Vec<PathNode>,
    pair: Vec<u16>,
}

impl Segmenter {
    pub(crate) fn new() -> Self {
        Segmenter {
            core: CoreDict::get(),
            bigram: BigramDict::get(),
            tokens: Vec::new(),
            slots: Vec::new(),
            incoming: Vec::new(),
            path: Vec::new(),
            pair: Vec::new(),
        }
    }

    /// Segments `sentence`, calling `emit` with each word's normalized text (valid only during
    /// the call), byte range within the sentence and kind. Returns false if `emit` did.
    pub(crate) fn segment(
        &mut self,
        sentence: &str,
        scratch: &mut String,
        mut emit: impl FnMut(&str, Range<usize>, TokenKind) -> bool,
    ) -> bool {
        self.build_lattice(sentence);
        self.build_edges();
        let path = self.lightest_path();
        let words = &path[1..path.len() - 1];
        for &index in words {
            let token = &self.tokens[index];
            let byte_range = token.slot - 1..token.end;
            scratch.clear();
            match token.word_type {
                WordType::ChineseWord => {
                    scratch.extend(
                        char::decode_utf16(token.identity.iter().copied()).map(|c| c.unwrap()),
                    );
                }
                WordType::String | WordType::Number => {
                    for c in sentence[byte_range.clone()].chars() {
                        scratch.push(c.to_ascii_lowercase());
                    }
                }
                WordType::FullwidthString | WordType::FullwidthNumber => {
                    for c in sentence[byte_range.clone()].chars() {
                        scratch.push(halfwidth(c).to_ascii_lowercase());
                    }
                }
                WordType::Delimiter => scratch.push(','),
                WordType::SentenceBegin | WordType::SentenceEnd => unreachable!(),
            }
            let kind = match token.word_type {
                WordType::Delimiter => TokenKind::Punctuation,
                _ => TokenKind::Word,
            };
            if !emit(scratch, byte_range, kind) {
                return false;
            }
        }
        true
    }

    fn build_lattice(&mut self, sentence: &str) {
        self.tokens.clear();
        for slot in &mut self.slots {
            slot.clear();
        }
        if self.slots.len() < sentence.len() + 2 {
            self.slots.resize_with(sentence.len() + 2, Vec::new);
        }

        self.add_token(LatticeToken {
            identity: SENTENCE_BEGIN_IDENTITY.to_vec(),
            slot: 0,
            end: 0,
            word_type: WordType::SentenceBegin,
            frequency: self.core.frequency(&SENTENCE_BEGIN_IDENTITY),
        });

        let chars: Vec<(usize, char, CharType)> = sentence
            .char_indices()
            .map(|(at, c)| (at, c, char_type(c as u32)))
            .collect();
        let end_of = |index: usize| chars.get(index).map_or(sentence.len(), |&(at, _, _)| at);

        let mut word = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let (start, c, kind) = chars[i];
            match kind {
                CharType::SpaceLike => i += 1,
                CharType::Surrogate => {
                    self.add_token(LatticeToken {
                        identity: c.encode_utf16(&mut [0; 2]).to_vec(),
                        slot: start + 1,
                        end: end_of(i + 1),
                        word_type: WordType::ChineseWord,
                        frequency: 0,
                    });
                    i += 1;
                }
                CharType::Hanzi => {
                    let head = c as u16;
                    word.clear();
                    word.push(head);
                    self.add_token(LatticeToken {
                        identity: word.clone(),
                        slot: start + 1,
                        end: end_of(i + 1),
                        word_type: WordType::ChineseWord,
                        frequency: self.core.frequency(&word),
                    });
                    let row = self.core.row(head).unwrap_or(&[]);
                    let mut found = (!row.is_empty()).then_some(0);
                    let mut j = i + 1;
                    while let Some(entry_index) = found {
                        let entry = &row[entry_index];
                        if word.len() > 1 && *entry.suffix == word[1..] {
                            self.add_token(LatticeToken {
                                identity: word.clone(),
                                slot: start + 1,
                                end: end_of(j),
                                word_type: WordType::ChineseWord,
                                frequency: entry.freq,
                            });
                        }
                        while j < chars.len() && chars[j].2 == CharType::SpaceLike {
                            j += 1;
                        }
                        if j < chars.len() && chars[j].2 == CharType::Hanzi {
                            word.push(chars[j].1 as u16);
                            found = CoreDict::first_with_prefix(row, &word[1..], entry_index);
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    i += 1;
                }
                CharType::Letter | CharType::FullwidthLetter => {
                    let mut has_fullwidth = kind == CharType::FullwidthLetter;
                    let mut j = i + 1;
                    while j < chars.len()
                        && matches!(chars[j].2, CharType::Letter | CharType::FullwidthLetter)
                    {
                        has_fullwidth |= chars[j].2 == CharType::FullwidthLetter;
                        j += 1;
                    }
                    self.add_token(LatticeToken {
                        identity: STRING_IDENTITY.to_vec(),
                        slot: start + 1,
                        end: end_of(j),
                        word_type: if has_fullwidth {
                            WordType::FullwidthString
                        } else {
                            WordType::String
                        },
                        frequency: self.core.frequency(&STRING_IDENTITY),
                    });
                    i = j;
                }
                CharType::Digit | CharType::FullwidthDigit => {
                    let mut has_fullwidth = kind == CharType::FullwidthDigit;
                    let mut j = i + 1;
                    while j < chars.len()
                        && matches!(chars[j].2, CharType::Digit | CharType::FullwidthDigit)
                    {
                        has_fullwidth |= chars[j].2 == CharType::FullwidthDigit;
                        j += 1;
                    }
                    self.add_token(LatticeToken {
                        identity: NUMBER_IDENTITY.to_vec(),
                        slot: start + 1,
                        end: end_of(j),
                        word_type: if has_fullwidth {
                            WordType::FullwidthNumber
                        } else {
                            WordType::Number
                        },
                        frequency: self.core.frequency(&NUMBER_IDENTITY),
                    });
                    i = j;
                }
                CharType::Delimiter => {
                    self.add_token(LatticeToken {
                        identity: vec![c as u16],
                        slot: start + 1,
                        end: end_of(i + 1),
                        word_type: WordType::Delimiter,
                        frequency: MAX_FREQUENCY,
                    });
                    i += 1;
                }
                CharType::Other => {
                    self.add_token(LatticeToken {
                        identity: STRING_IDENTITY.to_vec(),
                        slot: start + 1,
                        end: end_of(i + 1),
                        word_type: WordType::String,
                        frequency: self.core.frequency(&STRING_IDENTITY),
                    });
                    i += 1;
                }
            }
        }

        self.add_token(LatticeToken {
            identity: SENTENCE_END_IDENTITY.to_vec(),
            slot: sentence.len() + 1,
            end: sentence.len() + 1,
            word_type: WordType::SentenceEnd,
            frequency: self.core.frequency(&SENTENCE_END_IDENTITY),
        });
    }

    fn add_token(&mut self, token: LatticeToken) {
        self.slots[token.slot].push(self.tokens.len());
        self.tokens.push(token);
    }

    /// Links every token to the tokens in the first occupied slot at or after its end.
    fn build_edges(&mut self) {
        for edges in &mut self.incoming {
            edges.clear();
        }
        if self.incoming.len() < self.tokens.len() {
            self.incoming.resize_with(self.tokens.len(), Vec::new);
        }
        let end_sentinel = self.tokens.len() - 1;
        for from in 0..end_sentinel {
            let token = &self.tokens[from];
            let Some(next_slot) = self.slots[token.end + 1..]
                .iter()
                .find(|slot| !slot.is_empty())
            else {
                break;
            };
            for &to in next_slot {
                self.pair.clear();
                self.pair.extend_from_slice(&token.identity);
                self.pair.push(PAIR_SEPARATOR);
                self.pair.extend_from_slice(&self.tokens[to].identity);
                let weight = edge_weight(token.frequency, self.bigram.frequency(&self.pair));
                self.incoming[to].push(Edge { from, weight });
            }
        }
    }

    /// Dynamic programming over the lattice in token order; ties keep the earliest edge. Returns
    /// the token indices on the lightest path, sentinels included.
    fn lightest_path(&mut self) -> Vec<usize> {
        self.path.clear();
        self.path.push(PathNode {
            weight: 0.0,
            previous: 0,
        });
        for to in 1..self.tokens.len() {
            let mut best = PathNode {
                weight: f64::MAX,
                previous: 0,
            };
            for edge in &self.incoming[to] {
                let weight = self.path[edge.from].weight + edge.weight;
                if weight < best.weight {
                    best = PathNode {
                        weight,
                        previous: edge.from,
                    };
                }
            }
            self.path.push(best);
        }
        let mut path = Vec::new();
        let mut current = self.tokens.len() - 1;
        path.push(current);
        while current != 0 {
            current = self.path[current].previous;
            path.push(current);
        }
        path.reverse();
        path
    }
}

/// Negative log of the smoothed probability of `pair_frequency` following a word of
/// `word_frequency`, in the same floating-point operation order as Lucene.
fn edge_weight(word_frequency: i32, pair_frequency: i32) -> f64 {
    let smooth = 0.1;
    let tiny = 1.0 / f64::from(MAX_FREQUENCY);
    let word_frequency = f64::from(word_frequency);
    let pair_frequency = f64::from(pair_frequency);
    -(smooth * (1.0 + word_frequency) / f64::from(MAX_FREQUENCY)
        + (1.0 - smooth) * ((1.0 - tiny) * pair_frequency / (1.0 + word_frequency) + tiny))
        .ln()
}

/// Maps a fullwidth letter or digit to its ASCII form, as Lucene does for runs containing any.
fn halfwidth(c: char) -> char {
    if c as u32 >= 0xFF10 {
        char::from_u32(c as u32 - 0xFEE0).unwrap()
    } else {
        c
    }
}
