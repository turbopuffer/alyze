//! Katakana romanization: Lucene's `ToStringUtil.getRomanization` (the reading-form filter's
//! `use_romaji`) and `KatakanaRomanizer` (the completion filter's keystroke variants).

/// Modified Hepburn romanization of a katakana reading, as `ToStringUtil.getRomanization` does
/// it (ー lengthens nothing: マージャン → majan; ン before a vowel or y gets an apostrophe: コンヤ
/// → kon'ya; a small ッ doubles the next consonant). Characters it doesn't know pass through.
pub fn hepburn(reading: &str) -> String {
    let _ = reading;
    todo!()
}

/// Every romaji keystroke sequence that types `katakana`, as `KatakanaRomanizer.romanize` lists
/// them (シ → si, shi; ン → n, nn; longest keystroke match first; characters it doesn't know are
/// appended as-is once no more match). `katakana` must consist of katakana (U+30A0..U+30FF) and
/// ASCII lowercase letters only.
pub fn keystrokes(katakana: &str) -> Vec<String> {
    let _ = katakana;
    todo!()
}
