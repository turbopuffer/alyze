//! Katakana romanization: Lucene's `ToStringUtil.getRomanization` (the reading-form filter's
//! `use_romaji`) and `KatakanaRomanizer` (the completion filter's keystroke variants).

use std::collections::HashMap;
use std::sync::OnceLock;

/// Modified Hepburn romanization of a katakana reading, as `ToStringUtil.getRomanization` does
/// it, with its three-character lookahead, macrons for ウ-lengthened vowels (トウキョウ → tōkyō),
/// ー dropped, ン as m before labials and n' before vowels and y, and a small ッ doubling the
/// following k/s/t/p. Characters it doesn't know pass through.
pub fn hepburn(reading: &str) -> String {
    let s: Vec<char> = reading.chars().collect();
    let mut out = String::with_capacity(reading.len());
    let len = s.len();
    let mut i = 0;
    while i < len {
        let ch = s[i];
        let ch2 = if i + 1 < len { s[i + 1] } else { '\0' };
        let ch3 = if i + 2 < len { s[i + 2] } else { '\0' };
        // Returns how many extra characters a rule consumed.
        let extra = romanize_one(ch, ch2, ch3, &mut out);
        i += 1 + extra;
    }
    out
}

/// The Java `switch`: appends the romanization of `ch` (with lookahead `ch2`, `ch3`) and returns
/// how many lookahead characters it consumed.
fn romanize_one(ch: char, ch2: char, ch3: char, out: &mut String) -> usize {
    // A consonant with the five y-contractions and the ェ variant (キ, シ, チ, ニ, ヒ, ミ, リ, ギ,
    // ジ, ヂ, ビ, ピ): `yo`/`yu` with a following ウ take a macron.
    let palatal = |out: &mut String, base: &str, y: &str| -> usize {
        match (ch2, ch3) {
            ('ョ', 'ウ') => {
                out.push_str(y);
                out.push('ō');
                2
            }
            ('ュ', 'ウ') => {
                out.push_str(y);
                out.push('ū');
                2
            }
            ('ャ', _) => {
                out.push_str(y);
                out.push('a');
                1
            }
            ('ョ', _) => {
                out.push_str(y);
                out.push('o');
                1
            }
            ('ュ', _) => {
                out.push_str(y);
                out.push('u');
                1
            }
            ('ェ', _) => {
                out.push_str(y);
                out.push('e');
                1
            }
            _ => {
                out.push_str(base);
                0
            }
        }
    };
    // An o-row kana whose following ウ lengthens it.
    let long_o = |out: &mut String, short: &str, long: &str| -> usize {
        if ch2 == 'ウ' {
            out.push_str(long);
            1
        } else {
            out.push_str(short);
            0
        }
    };
    match ch {
        'ッ' => {
            match ch2 {
                'カ' | 'キ' | 'ク' | 'ケ' | 'コ' => out.push('k'),
                'サ' | 'シ' | 'ス' | 'セ' | 'ソ' => out.push('s'),
                'タ' | 'チ' | 'ツ' | 'テ' | 'ト' => out.push('t'),
                'パ' | 'ピ' | 'プ' | 'ペ' | 'ポ' => out.push('p'),
                _ => {}
            }
            0
        }
        'ア' => {
            out.push('a');
            0
        }
        'イ' => match ch2 {
            'ィ' => {
                out.push_str("yi");
                1
            }
            'ェ' => {
                out.push_str("ye");
                1
            }
            _ => {
                out.push('i');
                0
            }
        },
        'ウ' => match ch2 {
            'ァ' => {
                out.push_str("wa");
                1
            }
            'ィ' => {
                out.push_str("wi");
                1
            }
            'ゥ' => {
                out.push_str("wu");
                1
            }
            'ェ' => {
                out.push_str("we");
                1
            }
            'ォ' => {
                out.push_str("wo");
                1
            }
            'ュ' => {
                out.push_str("wyu");
                1
            }
            _ => {
                out.push('u');
                0
            }
        },
        'エ' => {
            out.push('e');
            0
        }
        'オ' => long_o(out, "o", "ō"),
        'カ' => {
            out.push_str("ka");
            0
        }
        'キ' => palatal(out, "ki", "ky"),
        'ク' => match ch2 {
            'ァ' | 'ヮ' => {
                out.push_str("kwa");
                1
            }
            'ィ' => {
                out.push_str("kwi");
                1
            }
            'ェ' => {
                out.push_str("kwe");
                1
            }
            'ォ' => {
                out.push_str("kwo");
                1
            }
            _ => {
                out.push_str("ku");
                0
            }
        },
        'ケ' => {
            out.push_str("ke");
            0
        }
        'コ' => long_o(out, "ko", "kō"),
        'サ' => {
            out.push_str("sa");
            0
        }
        'シ' => palatal(out, "shi", "sh"),
        'ス' => {
            if ch2 == 'ィ' {
                out.push_str("si");
                1
            } else {
                out.push_str("su");
                0
            }
        }
        'セ' => {
            out.push_str("se");
            0
        }
        'ソ' => long_o(out, "so", "sō"),
        'タ' => {
            out.push_str("ta");
            0
        }
        'チ' => palatal(out, "chi", "ch"),
        'ツ' => match ch2 {
            'ァ' => {
                out.push_str("tsa");
                1
            }
            'ィ' => {
                out.push_str("tsi");
                1
            }
            'ェ' => {
                out.push_str("tse");
                1
            }
            'ォ' => {
                out.push_str("tso");
                1
            }
            'ュ' => {
                out.push_str("tsyu");
                1
            }
            _ => {
                out.push_str("tsu");
                0
            }
        },
        'テ' => match ch2 {
            'ィ' => {
                out.push_str("ti");
                1
            }
            'ゥ' => {
                out.push_str("tu");
                1
            }
            'ュ' => {
                out.push_str("tyu");
                1
            }
            _ => {
                out.push_str("te");
                0
            }
        },
        'ト' => match ch2 {
            'ウ' => {
                out.push_str("tō");
                1
            }
            'ゥ' => {
                out.push_str("tu");
                1
            }
            _ => {
                out.push_str("to");
                0
            }
        },
        'ナ' => {
            out.push_str("na");
            0
        }
        'ニ' => palatal(out, "ni", "ny"),
        'ヌ' => {
            out.push_str("nu");
            0
        }
        'ネ' => {
            out.push_str("ne");
            0
        }
        'ノ' => long_o(out, "no", "nō"),
        'ハ' => {
            out.push_str("ha");
            0
        }
        'ヒ' => palatal(out, "hi", "hy"),
        'フ' => match (ch2, ch3) {
            ('ャ', _) => {
                out.push_str("fya");
                1
            }
            ('ュ', _) => {
                out.push_str("fyu");
                1
            }
            ('ィ', 'ェ') => {
                out.push_str("fye");
                2
            }
            ('ョ', _) => {
                out.push_str("fyo");
                1
            }
            ('ァ', _) => {
                out.push_str("fa");
                1
            }
            ('ィ', _) => {
                out.push_str("fi");
                1
            }
            ('ェ', _) => {
                out.push_str("fe");
                1
            }
            ('ォ', _) => {
                out.push_str("fo");
                1
            }
            _ => {
                out.push_str("fu");
                0
            }
        },
        'ヘ' => {
            out.push_str("he");
            0
        }
        'ホ' => match ch2 {
            'ウ' => {
                out.push_str("hō");
                1
            }
            'ゥ' => {
                out.push_str("hu");
                1
            }
            _ => {
                out.push_str("ho");
                0
            }
        },
        'マ' => {
            out.push_str("ma");
            0
        }
        'ミ' => palatal(out, "mi", "my"),
        'ム' => {
            out.push_str("mu");
            0
        }
        'メ' => {
            out.push_str("me");
            0
        }
        'モ' => long_o(out, "mo", "mō"),
        'ヤ' => {
            out.push_str("ya");
            0
        }
        'ユ' => {
            out.push_str("yu");
            0
        }
        'ヨ' => long_o(out, "yo", "yō"),
        'ラ' => {
            if ch2 == '゜' {
                out.push_str("la");
                1
            } else {
                out.push_str("ra");
                0
            }
        }
        'リ' => {
            if ch2 == '゜' {
                out.push_str("li");
                1
            } else {
                palatal(out, "ri", "ry")
            }
        }
        'ル' => {
            if ch2 == '゜' {
                out.push_str("lu");
                1
            } else {
                out.push_str("ru");
                0
            }
        }
        'レ' => {
            if ch2 == '゜' {
                out.push_str("le");
                1
            } else {
                out.push_str("re");
                0
            }
        }
        'ロ' => match ch2 {
            'ウ' => {
                out.push_str("rō");
                1
            }
            '゜' => {
                out.push_str("lo");
                1
            }
            _ => {
                out.push_str("ro");
                0
            }
        },
        'ワ' => {
            out.push_str("wa");
            0
        }
        'ヰ' => {
            out.push('i');
            0
        }
        'ヱ' => {
            out.push('e');
            0
        }
        'ヲ' => {
            out.push('o');
            0
        }
        'ン' => {
            match ch2 {
                'バ' | 'ビ' | 'ブ' | 'ベ' | 'ボ' | 'パ' | 'ピ' | 'プ' | 'ペ' | 'ポ' | 'マ'
                | 'ミ' | 'ム' | 'メ' | 'モ' => out.push('m'),
                'ヤ' | 'ユ' | 'ヨ' | 'ア' | 'イ' | 'ウ' | 'エ' | 'オ' => out.push_str("n'"),
                _ => out.push('n'),
            }
            0
        }
        'ガ' => {
            out.push_str("ga");
            0
        }
        'ギ' => palatal(out, "gi", "gy"),
        'グ' => match ch2 {
            'ァ' | 'ヮ' => {
                out.push_str("gwa");
                1
            }
            'ィ' => {
                out.push_str("gwi");
                1
            }
            'ェ' => {
                out.push_str("gwe");
                1
            }
            'ォ' => {
                out.push_str("gwo");
                1
            }
            _ => {
                out.push_str("gu");
                0
            }
        },
        'ゲ' => {
            out.push_str("ge");
            0
        }
        'ゴ' => long_o(out, "go", "gō"),
        'ザ' => {
            out.push_str("za");
            0
        }
        'ジ' | 'ヂ' => palatal(out, "ji", "j"),
        'ズ' => {
            if ch2 == 'ィ' {
                out.push_str("zi");
                1
            } else {
                out.push_str("zu");
                0
            }
        }
        'ゼ' => {
            out.push_str("ze");
            0
        }
        'ゾ' => long_o(out, "zo", "zō"),
        'ダ' => {
            out.push_str("da");
            0
        }
        'ヅ' => {
            out.push_str("zu");
            0
        }
        'デ' => match ch2 {
            'ィ' => {
                out.push_str("di");
                1
            }
            'ュ' => {
                out.push_str("dyu");
                1
            }
            _ => {
                out.push_str("de");
                0
            }
        },
        'ド' => match ch2 {
            'ウ' => {
                out.push_str("dō");
                1
            }
            'ゥ' => {
                out.push_str("du");
                1
            }
            _ => {
                out.push_str("do");
                0
            }
        },
        'バ' => {
            out.push_str("ba");
            0
        }
        'ビ' => palatal(out, "bi", "by"),
        'ブ' => {
            out.push_str("bu");
            0
        }
        'ベ' => {
            out.push_str("be");
            0
        }
        'ボ' => long_o(out, "bo", "bō"),
        'パ' => {
            out.push_str("pa");
            0
        }
        'ピ' => palatal(out, "pi", "py"),
        'プ' => {
            out.push_str("pu");
            0
        }
        'ペ' => {
            out.push_str("pe");
            0
        }
        'ポ' => long_o(out, "po", "pō"),
        'ヷ' => {
            out.push_str("va");
            0
        }
        'ヸ' => {
            out.push_str("vi");
            0
        }
        'ヹ' => {
            out.push_str("ve");
            0
        }
        'ヺ' => {
            out.push_str("vo");
            0
        }
        'ヴ' => {
            if ch2 == 'ィ' && ch3 == 'ェ' {
                out.push_str("vye");
                2
            } else {
                out.push('v');
                0
            }
        }
        'ァ' => {
            out.push('a');
            0
        }
        'ィ' => {
            out.push('i');
            0
        }
        'ゥ' => {
            out.push('u');
            0
        }
        'ェ' => {
            out.push('e');
            0
        }
        'ォ' => {
            out.push('o');
            0
        }
        'ヮ' => {
            out.push_str("wa");
            0
        }
        'ャ' => {
            out.push_str("ya");
            0
        }
        'ュ' => {
            out.push_str("yu");
            0
        }
        'ョ' => {
            out.push_str("yo");
            0
        }
        'ー' => 0,
        other => {
            out.push(other);
            0
        }
    }
}

// ------------------------------------------------------------------------------------------------
// KatakanaRomanizer

/// Lucene's `romaji_map.txt`: katakana keystroke → romaji spellings, in file order.
static ROMAJI_MAP: &str = include_str!("../../data/kuromoji/romaji_map.txt");

struct KeystrokeMap {
    /// Keystroke (UTF-16) → its romaji spellings, in file order (a repeated keystroke keeps the
    /// last line, like `HashMap.put`).
    map: HashMap<Vec<u16>, Vec<String>>,
    max_len: usize,
}

fn keystroke_map() -> &'static KeystrokeMap {
    static MAP: OnceLock<KeystrokeMap> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map: HashMap<Vec<u16>, Vec<String>> = HashMap::new();
        for line in ROMAJI_MAP.lines() {
            if line.starts_with('#') {
                continue;
            }
            let cols: Vec<&str> = line.trim().split(',').collect();
            if cols.len() < 2 {
                continue;
            }
            map.insert(
                cols[0].encode_utf16().collect(),
                cols[1..].iter().map(|s| s.to_string()).collect(),
            );
        }
        let max_len = map.keys().map(Vec::len).max().unwrap_or(0);
        KeystrokeMap { map, max_len }
    })
}

/// Every romaji keystroke sequence that types `katakana`, as `KatakanaRomanizer.romanize` lists
/// them (シ → si, shi; ン → n, nn; longest keystroke match first; characters it doesn't know are
/// appended as-is once no more match). `katakana` must consist of katakana (U+30A0..U+30FF) and
/// ASCII lowercase letters only.
pub fn keystrokes(katakana: &str) -> Vec<String> {
    let map = keystroke_map();
    let input: Vec<u16> = katakana.encode_utf16().collect();
    let mut pending: Vec<String> = Vec::new();
    let mut pos = 0usize;
    while pos < input.len() {
        // Greedily take the longest matching keystroke.
        let mut matched: Option<(usize, &Vec<String>)> = None;
        for len in (1..=(input.len() - pos).min(map.max_len)).rev() {
            if let Some(candidates) = map.map.get(&input[pos..pos + len]) {
                matched = Some((len, candidates));
                break;
            }
        }
        let Some((len, candidates)) = matched else {
            break;
        };
        if pending.is_empty() {
            pending = candidates.clone();
        } else if candidates.len() == 1 {
            for p in &mut pending {
                p.push_str(&candidates[0]);
            }
        } else {
            let mut outputs = Vec::with_capacity(pending.len() * candidates.len());
            for c in candidates {
                for p in &pending {
                    outputs.push(format!("{p}{c}"));
                }
            }
            pending = outputs;
        }
        pos += len;
    }
    if pos < input.len() {
        let rest = String::from_utf16_lossy(&input[pos..]);
        for p in &mut pending {
            p.push_str(&rest);
        }
    }
    pending
}

/// `CharSequenceUtils`, for the completion filter.
pub(crate) mod chars {
    pub fn is_lowercase_alphabets(s: &str) -> bool {
        s.encode_utf16()
            .all(|u| (0x61..=0x7A).contains(&u) || (0xFF41..=0xFF5A).contains(&u))
    }

    pub fn is_kana(s: &str) -> bool {
        s.encode_utf16()
            .all(|u| (0x3040..=0x309F).contains(&u) || (0x30A0..=0x30FF).contains(&u))
    }

    pub fn is_katakana_or_hw_alphabets(s: &str) -> bool {
        s.encode_utf16()
            .all(|u| (0x30A0..=0x30FF).contains(&u) || (0x61..=0x7A).contains(&u))
    }

    /// Hiragana ぁ..ゖ and ゝゞ shifted to katakana.
    pub fn to_katakana(s: &str) -> String {
        let units: Vec<u16> = s
            .encode_utf16()
            .map(|u| {
                if (0x3041..=0x3096).contains(&u) || u == 0x309D || u == 0x309E {
                    u + 0x60
                } else {
                    u
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    }
}
