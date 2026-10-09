//! Token filters, applied in place to a [`Tokens`] buffer: Elasticsearch's `kuromoji_baseform`,
//! `kuromoji_part_of_speech`, `ja_stop`, `kuromoji_stemmer`, `kuromoji_readingform`,
//! `kuromoji_number`, `hiragana_uppercase`, `katakana_uppercase`, `kuromoji_completion`, plus
//! the `cjk_width` token filter and the lowercasing both analyzers end with. They compose in any
//! order, like Elasticsearch filter chains.

use std::collections::HashSet;

use super::Tokens;

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
        let _ = (words, ignore_case);
        todo!()
    }

    /// [`DEFAULT_STOP_WORDS`], case-insensitive like Lucene's default set.
    pub fn japanese() -> StopWords {
        StopWords::new(DEFAULT_STOP_WORDS.iter().copied(), true)
    }

    pub fn contains(&self, word: &str) -> bool {
        let _ = word;
        todo!()
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

/// Replaces the text of inflected verbs and adjectives with their base form (Lucene's
/// `JapaneseBaseFormFilter`).
pub fn base_form(tokens: &mut Tokens) {
    let _ = tokens;
    todo!()
}

/// Removes tokens whose part of speech is in `stop_tags` (Lucene's
/// `JapanesePartOfSpeechStopFilter`). Like Lucene's filtering filters, the removed tokens'
/// positions are kept as gaps: the next surviving token's `position_increment` grows by the
/// increments of the removed ones.
pub fn part_of_speech_stop(tokens: &mut Tokens, stop_tags: &StopTags) {
    let _ = (tokens, stop_tags);
    todo!()
}

/// Removes tokens whose text is a stop word (Lucene's `StopFilter`, Elasticsearch's `ja_stop`),
/// keeping positions as gaps like [`part_of_speech_stop`].
pub fn stop(tokens: &mut Tokens, stop_words: &StopWords) {
    let _ = (tokens, stop_words);
    todo!()
}

/// Drops a trailing prolonged sound mark (ー) from katakana tokens of at least `minimum_length`
/// code units (Lucene's `JapaneseKatakanaStemFilter`, Elasticsearch's `kuromoji_stemmer`). Only
/// fullwidth katakana (U+30A0..U+30FF) counts; `minimum_length` must be at least 1.
pub fn katakana_stem(tokens: &mut Tokens, minimum_length: usize) {
    let _ = (tokens, minimum_length);
    todo!()
}

/// Replaces each token's text with its reading (Lucene's `JapaneseReadingFormFilter`,
/// Elasticsearch's `kuromoji_readingform`): a token without one that contains hiragana gets the
/// text with hiragana shifted to katakana, other unknown words keep their text. With
/// `use_romaji`, the reading (or, failing that, the text) is romanized with [`super::romaji::hepburn`].
pub fn reading_form(tokens: &mut Tokens, use_romaji: bool) {
    let _ = (tokens, use_romaji);
    todo!()
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
    let _ = tokens;
    todo!()
}

/// Lucene's `JapaneseNumberFilter.normalizeNumber`: the plain decimal value of a Japanese number,
/// or the input unchanged when it doesn't parse.
pub(crate) fn normalize_number(number: &str) -> String {
    let _ = number;
    todo!()
}

/// Replaces small hiragana (ぁぃぅぇぉっゃゅょゎゕゖ) with the full-size letters (Lucene's
/// `JapaneseHiraganaUppercaseFilter`).
pub fn hiragana_uppercase(tokens: &mut Tokens) {
    let _ = tokens;
    todo!()
}

/// Replaces small katakana (ァィゥェォヵㇰヶㇱㇲッㇳㇴㇵㇶㇷㇸㇹㇺャュョㇻㇼㇽㇾㇿヮ, and ㇷ゚ → プ) with
/// the full-size letters (Lucene's `JapaneseKatakanaUppercaseFilter`).
pub fn katakana_uppercase(tokens: &mut Tokens) {
    let _ = tokens;
    todo!()
}

/// Lucene's `CJKWidthFilter` (Elasticsearch's `cjk_width`): the same folding as
/// [`super::char_filter::cjk_width`], on token text (offsets are unchanged).
pub fn cjk_width(tokens: &mut Tokens) {
    let _ = tokens;
    todo!()
}

/// Lowercases every token's text, with alyze's pinned Unicode lowercase mapping (Lucene's
/// `LowerCaseFilter` uses Java's simple mapping; see the module docs for the difference).
pub fn lowercase(tokens: &mut Tokens) {
    let _ = tokens;
    todo!()
}

/// The mapping [`lowercase`] applies, on one string.
pub(crate) fn lowercase_text(text: &str) -> String {
    let _ = text;
    todo!()
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
    let _ = (tokens, mode);
    todo!()
}
