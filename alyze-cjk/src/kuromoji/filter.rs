//! Token filters, applied in place to a [`Tokens`] buffer: Elasticsearch's `kuromoji_baseform`,
//! `kuromoji_part_of_speech`, `ja_stop`, `kuromoji_stemmer`, `kuromoji_readingform`,
//! `kuromoji_number`, `hiragana_uppercase`, `katakana_uppercase`, `kuromoji_completion`, plus
//! the `cjk_width` token filter and the lowercasing both analyzers end with. They compose in any
//! order, like Elasticsearch filter chains.

use std::collections::HashSet;

use super::{TokenData, Tokens, char_filter, romaji};
use crate::morph::lowercase;
use crate::morph::number::{self, Numerals};

/// Lucene's default `stoptags.txt`: parts of speech the `kuromoji` analyzer and
/// `kuromoji_part_of_speech` drop by default.
pub static DEFAULT_STOP_TAGS: &[&str] = &[
    "接続詞",
    "助詞",
    "助詞-格助詞",
    "助詞-格助詞-一般",
    "助詞-格助詞-引用",
    "助詞-格助詞-連語",
    "助詞-接続助詞",
    "助詞-係助詞",
    "助詞-副助詞",
    "助詞-間投助詞",
    "助詞-並立助詞",
    "助詞-終助詞",
    "助詞-副助詞／並立助詞／終助詞",
    "助詞-連体化",
    "助詞-副詞化",
    "助詞-特殊",
    "助動詞",
    "記号",
    "記号-一般",
    "記号-読点",
    "記号-句点",
    "記号-空白",
    "記号-括弧開",
    "記号-括弧閉",
    "その他-間投",
    "フィラー",
    "非言語音",
];

/// Lucene's default `stopwords.txt`: the `_japanese_` stop words of the `kuromoji` analyzer and
/// `ja_stop`.
pub static DEFAULT_STOP_WORDS: &[&str] = &[
    "の",
    "に",
    "は",
    "を",
    "た",
    "が",
    "で",
    "て",
    "と",
    "し",
    "れ",
    "さ",
    "ある",
    "いる",
    "も",
    "する",
    "から",
    "な",
    "こと",
    "として",
    "い",
    "や",
    "れる",
    "など",
    "なっ",
    "ない",
    "この",
    "ため",
    "その",
    "あっ",
    "よう",
    "また",
    "もの",
    "という",
    "あり",
    "まで",
    "られ",
    "なる",
    "へ",
    "か",
    "だ",
    "これ",
    "によって",
    "により",
    "おり",
    "より",
    "による",
    "ず",
    "なり",
    "られる",
    "において",
    "ば",
    "なかっ",
    "なく",
    "しかし",
    "について",
    "せ",
    "だっ",
    "その後",
    "できる",
    "それ",
    "う",
    "ので",
    "なお",
    "のみ",
    "でき",
    "き",
    "つ",
    "における",
    "および",
    "いう",
    "さらに",
    "でも",
    "ら",
    "たり",
    "その他",
    "に関する",
    "たち",
    "ます",
    "ん",
    "なら",
    "に対して",
    "特に",
    "せる",
    "及び",
    "これら",
    "とき",
    "では",
    "にて",
    "ほか",
    "ながら",
    "うち",
    "そして",
    "とともに",
    "ただし",
    "かつて",
    "それぞれ",
    "または",
    "お",
    "ほど",
    "ものの",
    "に対する",
    "ほとんど",
    "と共に",
    "といった",
    "です",
    "とも",
    "ところ",
    "ここ",
];

/// A set of parts of speech to drop. Matching is on the whole hyphen-joined tag, as in Lucene:
/// `助詞` does not match `助詞-格助詞-一般`, which is why the default list spells out every level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopTags {
    tags: HashSet<String>,
}

impl StopTags {
    pub fn new<S: Into<String>>(tags: impl IntoIterator<Item = S>) -> StopTags {
        StopTags {
            tags: tags.into_iter().map(Into::into).collect(),
        }
    }

    /// [`DEFAULT_STOP_TAGS`].
    pub fn defaults() -> StopTags {
        StopTags::new(DEFAULT_STOP_TAGS.iter().copied())
    }

    pub fn contains(&self, part_of_speech: &str) -> bool {
        self.tags.contains(part_of_speech)
    }

    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }
}

impl Default for StopTags {
    fn default() -> Self {
        StopTags::defaults()
    }
}

/// A set of stop words (Lucene's `CharArraySet`), optionally case-insensitive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopWords {
    words: HashSet<String>,
    ignore_case: bool,
}

impl StopWords {
    /// With `ignore_case`, words are compared lowercased (alyze's mapping; Lucene lowercases per
    /// code point with Java's simple mapping).
    pub fn new<S: Into<String>>(
        words: impl IntoIterator<Item = S>,
        ignore_case: bool,
    ) -> StopWords {
        let words = words
            .into_iter()
            .map(|w| {
                let w: String = w.into();
                if ignore_case {
                    lowercase::lowercase_text(&w)
                } else {
                    w
                }
            })
            .collect();
        StopWords { words, ignore_case }
    }

    /// [`DEFAULT_STOP_WORDS`], case-insensitive like Lucene's default set.
    pub fn japanese() -> StopWords {
        StopWords::new(DEFAULT_STOP_WORDS.iter().copied(), true)
    }

    pub fn contains(&self, word: &str) -> bool {
        if self.ignore_case && !lowercase::is_lowercase_ascii(word) {
            self.words.contains(&lowercase::lowercase_text(word))
        } else {
            self.words.contains(word)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}

impl Default for StopWords {
    fn default() -> Self {
        StopWords::japanese()
    }
}

/// Replaces a token's text, reusing the buffer when it is unchanged.
fn set_text(tokens: &mut Tokens, index: usize, text: &str) {
    if &tokens.text[tokens.items[index].text.clone()] != text {
        let range = tokens.push_text(text);
        tokens.items[index].text = range;
    }
}

/// Replaces the text of inflected verbs and adjectives with their base form (Lucene's
/// `JapaneseBaseFormFilter`).
pub fn base_form(tokens: &mut Tokens) {
    for i in 0..tokens.items.len() {
        if let Some(base) = tokens.items[i].base_form.clone() {
            tokens.items[i].text = base;
        }
    }
}

/// Removes tokens for which `drop(text buffer, token)` says so, keeping the removed tokens'
/// positions as gaps like Lucene's `FilteringTokenFilter`: the next surviving token's
/// `position_increment` grows by the increments of the removed ones.
fn filtering(tokens: &mut Tokens, mut drop: impl FnMut(&str, &TokenData) -> bool) {
    let text = std::mem::take(&mut tokens.text);
    let mut skipped_increment = 0u32;
    tokens.items.retain_mut(|token| {
        if drop(&text, token) {
            skipped_increment += token.position_increment;
            false
        } else {
            token.position_increment += skipped_increment;
            skipped_increment = 0;
            true
        }
    });
    tokens.text = text;
}

/// Removes tokens whose part of speech is in `stop_tags` (Lucene's
/// `JapanesePartOfSpeechStopFilter`). Like Lucene's filtering filters, the removed tokens'
/// positions are kept as gaps: the next surviving token's `position_increment` grows by the
/// increments of the removed ones. A token without a part of speech (after the completion
/// filter) is kept, as Lucene keeps a null one.
pub fn part_of_speech_stop(tokens: &mut Tokens, stop_tags: &StopTags) {
    filtering(tokens, |text, token| {
        !token.part_of_speech.is_empty() && stop_tags.contains(&text[token.part_of_speech.clone()])
    });
}

/// Removes tokens whose text is a stop word (Lucene's `StopFilter`, Elasticsearch's `ja_stop`),
/// keeping positions as gaps like [`part_of_speech_stop`].
pub fn stop(tokens: &mut Tokens, stop_words: &StopWords) {
    filtering(tokens, |text, token| {
        stop_words.contains(&text[token.text.clone()])
    });
}

/// Drops a trailing prolonged sound mark (ー) from katakana tokens of at least `minimum_length`
/// code units (Lucene's `JapaneseKatakanaStemFilter`, Elasticsearch's `kuromoji_stemmer`). Only
/// fullwidth katakana (U+30A0..U+30FF) counts; `minimum_length` must be at least 1.
pub fn katakana_stem(tokens: &mut Tokens, minimum_length: usize) {
    assert!(minimum_length >= 1, "minimumLength must be >=1");
    for i in 0..tokens.items.len() {
        let text = &tokens.text[tokens.items[i].text.clone()];
        let units: Vec<u16> = text.encode_utf16().collect();
        if units.len() < minimum_length
            || !units.iter().all(|u| (0x30A0..=0x30FF).contains(u))
            || *units.last().unwrap() != 0x30FC
        {
            continue;
        }
        let stemmed = text[..text.len() - 'ー'.len_utf8()].to_owned();
        set_text(tokens, i, &stemmed);
    }
}

/// Replaces each token's text with its reading (Lucene's `JapaneseReadingFormFilter`,
/// Elasticsearch's `kuromoji_readingform`): a token without one that contains hiragana gets the
/// text with hiragana shifted to katakana, other unknown words keep their text. With
/// `use_romaji`, the reading (or, failing that, the text) is romanized with [`super::romaji::hepburn`].
pub fn reading_form(tokens: &mut Tokens, use_romaji: bool) {
    for i in 0..tokens.items.len() {
        let text = &tokens.text[tokens.items[i].text.clone()];
        let mut reading: Option<String> = tokens.items[i]
            .reading
            .clone()
            .map(|r| tokens.text[r].to_owned());
        if reading.is_none() && text.encode_utf16().any(is_hiragana) {
            // An OOV term with hiragana: its katakana form serves as the reading.
            let units: Vec<u16> = text
                .encode_utf16()
                .map(|u| if is_hiragana(u) { u + 0x60 } else { u })
                .collect();
            reading = Some(String::from_utf16_lossy(&units));
        }
        let new_text = if use_romaji {
            match &reading {
                None => romaji::hepburn(text),
                Some(r) => romaji::hepburn(r),
            }
        } else {
            match reading {
                None => continue,
                Some(r) => r,
            }
        };
        set_text(tokens, i, &new_text);
    }
}

fn is_hiragana(u: u16) -> bool {
    (0x3041..=0x3096).contains(&u)
}

/// Japanese numerals, for the shared number parser.
struct Kansuji;

impl Numerals for Kansuji {
    fn digit(c: char) -> Option<u8> {
        Some(match c {
            '〇' => 0,
            '一' => 1,
            '二' => 2,
            '三' => 3,
            '四' => 4,
            '五' => 5,
            '六' => 6,
            '七' => 7,
            '八' => 8,
            '九' => 9,
            _ => return None,
        })
    }

    fn exponent(c: char) -> u32 {
        match c {
            '十' => 1,
            '百' => 2,
            '千' => 3,
            '万' => 4,
            '億' => 8,
            '兆' => 12,
            '京' => 16,
            '垓' => 20,
            _ => 0,
        }
    }
}

/// Normalizes Japanese numbers (Lucene's `JapaneseNumberFilter`): consecutive tokens made of
/// Arabic or kanji numerals, powers of ten (十, 百, 千, 万, 億, 兆, 京, 垓) and decimal points /
/// thousands separators are merged into one token holding the plain decimal value (十万二千五百 →
/// 102500, ３．２千 → 3200). The merged token spans the merged tokens' offsets. Punctuation must
/// not have been discarded for separators to be seen.
///
/// A literal port of Lucene's stateful filter, quirks included: the merged token takes every
/// attribute but text and offsets from the token that ended the run, a stacked token (position
/// increment 0) makes the filter emit it and the tokens under it unchanged, and the numeral
/// buffer is only cleared after a successful merge.
pub fn number(tokens: &mut Tokens) {
    let input = std::mem::take(&mut tokens.items);
    let mut filter = NumberFilter {
        input: &input,
        next: 0,
        state: None,
        numeral: String::new(),
        fall_through_tokens: 0,
        exhausted: false,
    };
    let mut output = Vec::with_capacity(input.len());
    while let Some(token) = filter.increment_token(tokens) {
        output.push(token);
    }
    tokens.items = output;
}

struct NumberFilter<'a> {
    input: &'a [TokenData],
    next: usize,
    /// A token read past the end of a run, to emit on the next call.
    state: Option<TokenData>,
    numeral: String,
    fall_through_tokens: u32,
    exhausted: bool,
}

impl NumberFilter<'_> {
    /// Lucene's `input.incrementToken()`: the next token's attributes, or `None` at the end.
    fn next_input(&mut self) -> Option<TokenData> {
        let token = self.input.get(self.next)?.clone();
        self.next += 1;
        Some(token)
    }

    fn increment_token(&mut self, tokens: &mut Tokens) -> Option<TokenData> {
        // Emit a previously captured token we read past earlier.
        if let Some(state) = self.state.take() {
            return Some(state);
        }
        if self.exhausted {
            return None;
        }
        let Some(mut current) = self.next_input() else {
            self.exhausted = true;
            return None;
        };
        if self.fall_through_tokens > 0 {
            self.fall_through_tokens -= 1;
            return Some(current);
        }
        if current.position_increment == 0 {
            self.fall_through_tokens = current.position_length.saturating_sub(1);
            return Some(current);
        }

        let mut more_tokens = true;
        let mut composed = false;
        let mut start_offset = 0;
        let mut end_offset = 0;
        let pre_composition = current.clone();
        let mut term = tokens.text[current.text.clone()].to_owned();
        let mut numeral_term = number::is_numeral::<Kansuji>(&term);
        while more_tokens && numeral_term {
            if !composed {
                start_offset = current.byte_range.start;
                composed = true;
            }
            end_offset = current.byte_range.end;
            match self.next_input() {
                Some(next) => current = next,
                None => {
                    more_tokens = false;
                    self.exhausted = true;
                }
            }
            if current.position_increment == 0 {
                // A stacked token: capture it (and let the tokens under it through), and emit the
                // run's first token as it was.
                self.fall_through_tokens = current.position_length.saturating_sub(1);
                self.state = Some(current);
                return more_tokens.then_some(pre_composition);
            }
            self.numeral.push_str(&term);
            if more_tokens {
                term = tokens.text[current.text.clone()].to_owned();
                numeral_term =
                    number::is_numeral::<Kansuji>(&term) || number::is_numeral_punctuation(&term);
            }
        }
        if composed {
            if more_tokens {
                // Read past the numerals: emit this token on the next call.
                self.state = Some(current.clone());
            }
            let normalized = normalize_number(&self.numeral);
            let mut merged = current;
            merged.text = tokens.push_text(&normalized);
            merged.byte_range = start_offset..end_offset;
            self.numeral.clear();
            return Some(merged);
        }
        more_tokens.then_some(current)
    }
}

/// Lucene's `JapaneseNumberFilter.normalizeNumber`: the plain decimal value of a Japanese number,
/// or the input unchanged when it doesn't parse.
pub(crate) fn normalize_number(number: &str) -> String {
    number::normalize_number::<Kansuji>(number)
}

/// Rewrites each token's text through a per-code-unit mapping that never changes the length.
fn map_units(tokens: &mut Tokens, map: impl Fn(u16) -> u16) {
    for i in 0..tokens.items.len() {
        let text = &tokens.text[tokens.items[i].text.clone()];
        let units: Vec<u16> = text.encode_utf16().map(&map).collect();
        let mapped = String::from_utf16_lossy(&units);
        set_text(tokens, i, &mapped);
    }
}

/// Replaces small hiragana (ぁぃぅぇぉっゃゅょゎゕゖ) with the full-size letters (Lucene's
/// `JapaneseHiraganaUppercaseFilter`).
pub fn hiragana_uppercase(tokens: &mut Tokens) {
    map_units(tokens, |u| match u {
        0x3041 => 0x3042, // ぁ → あ
        0x3043 => 0x3044, // ぃ → い
        0x3045 => 0x3046, // ぅ → う
        0x3047 => 0x3048, // ぇ → え
        0x3049 => 0x304A, // ぉ → お
        0x3063 => 0x3064, // っ → つ
        0x3083 => 0x3084, // ゃ → や
        0x3085 => 0x3086, // ゅ → ゆ
        0x3087 => 0x3088, // ょ → よ
        0x308E => 0x308F, // ゎ → わ
        0x3095 => 0x304B, // ゕ → か
        0x3096 => 0x3051, // ゖ → け
        other => other,
    });
}

fn katakana_uppercase_unit(u: u16) -> u16 {
    match u {
        0x30A1 => 0x30A2, // ァ → ア
        0x30A3 => 0x30A4, // ィ → イ
        0x30A5 => 0x30A6, // ゥ → ウ
        0x30A7 => 0x30A8, // ェ → エ
        0x30A9 => 0x30AA, // ォ → オ
        0x30F5 => 0x30AB, // ヵ → カ
        0x31F0 => 0x30AF, // ㇰ → ク
        0x30F6 => 0x30B1, // ヶ → ケ
        0x31F1 => 0x30B7, // ㇱ → シ
        0x31F2 => 0x30B9, // ㇲ → ス
        0x30C3 => 0x30C4, // ッ → ツ
        0x31F3 => 0x30C8, // ㇳ → ト
        0x31F4 => 0x30CC, // ㇴ → ヌ
        0x31F5 => 0x30CF, // ㇵ → ハ
        0x31F6 => 0x30D2, // ㇶ → ヒ
        0x31F7 => 0x30D5, // ㇷ → フ
        0x31F8 => 0x30D8, // ㇸ → ヘ
        0x31F9 => 0x30DB, // ㇹ → ホ
        0x31FA => 0x30E0, // ㇺ → ム
        0x30E3 => 0x30E4, // ャ → ヤ
        0x30E5 => 0x30E6, // ュ → ユ
        0x30E7 => 0x30E8, // ョ → ヨ
        0x31FB => 0x30E9, // ㇻ → ラ
        0x31FC => 0x30EA, // ㇼ → リ
        0x31FD => 0x30EB, // ㇽ → ル
        0x31FE => 0x30EC, // ㇾ → レ
        0x31FF => 0x30ED, // ㇿ → ロ
        0x30EE => 0x30EF, // ヮ → ワ
        other => other,
    }
}

/// Replaces small katakana (ァィゥェォヵㇰヶㇱㇲッㇳㇴㇵㇶㇷㇸㇹㇺャュョㇻㇼㇽㇾㇿヮ, and ㇷ゚ → プ) with
/// the full-size letters (Lucene's `JapaneseKatakanaUppercaseFilter`).
pub fn katakana_uppercase(tokens: &mut Tokens) {
    for i in 0..tokens.items.len() {
        let text = &tokens.text[tokens.items[i].text.clone()];
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut out: Vec<u16> = Vec::with_capacity(units.len());
        let mut from = 0;
        while from < units.len() {
            let u = units[from];
            if u == 0x31F7 && from + 1 < units.len() && units[from + 1] == 0x309A {
                // ㇷ゚ → プ
                out.push(0x30D7);
                from += 2;
            } else {
                out.push(katakana_uppercase_unit(u));
                from += 1;
            }
        }
        let mapped = String::from_utf16_lossy(&out);
        set_text(tokens, i, &mapped);
    }
}

/// Lucene's `CJKWidthFilter` (Elasticsearch's `cjk_width`): the same folding as
/// [`super::char_filter::cjk_width`], on token text (offsets are unchanged).
pub fn cjk_width(tokens: &mut Tokens) {
    for i in 0..tokens.items.len() {
        let text = &tokens.text[tokens.items[i].text.clone()];
        let mut units: Vec<u16> = text.encode_utf16().collect();
        let mut j = 0;
        while j < units.len() {
            let ch = units[j];
            if (0xFF01..=0xFF5E).contains(&ch) {
                units[j] = ch - 0xFEE0;
            } else if (0xFF65..=0xFF9F).contains(&ch) {
                let combined = if (ch == 0xFF9E || ch == 0xFF9F) && j > 0 {
                    let prev = units[j - 1];
                    let c = char_filter::combine_voice_mark(prev, ch);
                    if c != prev {
                        units[j - 1] = c;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if combined {
                    units.remove(j);
                    continue;
                }
                units[j] = char_filter::KANA_NORM[(ch - 0xFF65) as usize];
            }
            j += 1;
        }
        let mapped = String::from_utf16_lossy(&units);
        set_text(tokens, i, &mapped);
    }
}

/// Lowercases every token's text, with alyze's pinned Unicode lowercase mapping (Lucene's
/// `LowerCaseFilter` uses Java's simple mapping; see the module docs for the difference).
pub fn lowercase(tokens: &mut Tokens) {
    let mut lowered = String::new();
    for i in 0..tokens.items.len() {
        let text = &tokens.text[tokens.items[i].text.clone()];
        if lowercase::is_lowercase_ascii(text) {
            continue;
        }
        lowered.clear();
        lowercase::lowercase_into(text, &mut lowered);
        if lowered != text {
            let range = tokens.push_text(&lowered);
            tokens.items[i].text = range;
        }
    }
}

/// The mapping [`lowercase`] applies, on one string.
pub(crate) fn lowercase_text(text: &str) -> String {
    lowercase::lowercase_text(text)
}

/// How the completion filter joins tokens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompletionMode {
    /// Each token on its own (for indexing).
    #[default]
    Index,
    /// Input-method aware: consecutive all-kana tokens are joined, and a trailing run of ASCII
    /// lowercase letters (a half-typed romaji syllable) is appended to the token before it.
    Query,
}

/// Lucene's `JapaneseCompletionFilter` (Elasticsearch's `kuromoji_completion`): after each token
/// (position increment 1) emits every romaji keystroke sequence of its reading
/// ([`super::romaji::keystrokes`]) as tokens at the same position and offsets (increment 0). A
/// token without a reading uses its text when that is all kana. Emitted tokens carry no part of
/// speech, reading or other attributes, as Lucene clears them.
pub fn completion(tokens: &mut Tokens, mode: CompletionMode) {
    use romaji::chars;
    let input = std::mem::take(&mut tokens.items);
    let mut output: Vec<TokenData> = Vec::with_capacity(input.len());
    // The pending token: surface, reading, offsets, and the input token it started from.
    let mut pending: Option<(String, String, usize, usize, TokenData)> = None;
    let mut generate = |pdg: &(String, String, usize, usize, TokenData),
                        out: &mut Vec<TokenData>,
                        tokens: &mut Tokens| {
        let (surface, reading, start, end, template) = pdg;
        let mut first = template.clone();
        first.text = tokens.push_text(surface);
        first.byte_range = *start..*end;
        first.position_increment = 1;
        first.position_length = 1;
        first.part_of_speech = 0..0;
        first.base_form = None;
        first.reading = None;
        first.pronunciation = None;
        first.inflection_type = None;
        first.inflection_form = None;
        out.push(first.clone());
        // Readings that can't be romanized are skipped.
        if reading.is_empty() || !chars::is_katakana_or_hw_alphabets(reading) {
            return;
        }
        for r in romaji::keystrokes(reading) {
            let mut t = first.clone();
            t.text = tokens.push_text(&r);
            t.position_increment = 0;
            out.push(t);
        }
    };
    for token in input {
        let surface = tokens.text[token.text.clone()].to_owned();
        // Lucene's CharsRefBuilder appends "null" for a missing reading.
        let reading = match &token.reading {
            Some(r) => tokens.text[r.clone()].to_owned(),
            None if chars::is_kana(&surface) => chars::to_katakana(&surface),
            None => "null".to_owned(),
        };
        let (start, end) = (token.byte_range.start, token.byte_range.end);
        match pending.take() {
            Some(mut pdg) => {
                if mode == CompletionMode::Query
                    && !chars::is_lowercase_alphabets(&pdg.0)
                    && chars::is_lowercase_alphabets(&surface)
                {
                    // A word split mid-IME composition: join the half-typed syllable back on,
                    // using the surface in place of its reading.
                    pdg.0.push_str(&surface);
                    pdg.1.push_str(&surface);
                    pdg.3 = end;
                    generate(&pdg, &mut output, tokens);
                } else if mode == CompletionMode::Query
                    && chars::is_kana(&pdg.0)
                    && chars::is_kana(&surface)
                {
                    pdg.0.push_str(&surface);
                    pdg.1.push_str(&reading);
                    pdg.3 = end;
                    pending = Some(pdg);
                } else {
                    generate(&pdg, &mut output, tokens);
                    pending = Some((surface, reading, start, end, token));
                }
            }
            None => pending = Some((surface, reading, start, end, token)),
        }
    }
    if let Some(pdg) = pending {
        generate(&pdg, &mut output, tokens);
    }
    tokens.items = output;
}
