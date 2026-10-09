//! Generates the seeded random inputs in `testdata/smartcn/cases/fuzz.txt` for the smartcn
//! differential tests. Deterministic: the same seed always produces the same file, so the file is
//! committed and this only needs rerunning when the generator changes.
//!
//!     cargo run -p alyze-cjk --example smartcn_gen_fuzz [-- --cases N --seed S --out PATH]
//!
//! After regenerating, rerun `testdata/smartcn/gen.sh` to refresh the golden files.
//!
//! The inputs mix runs drawn from classes chosen to hit every branch of the Java implementation:
//! dictionary words and random hanzi (including ones outside the U+4E00..U+9FA5 range smartcn
//! calls HANZI), ASCII and fullwidth letters/digits, every delimiter range, whitespace of every
//! kind the tokenizer treats specially, other scripts (which it emits char by char), surrogate
//! pairs, combining/format characters, the exact code points at each classification boundary, and
//! long inputs that cross the 1024-char read buffer with and without newlines in them.

use std::fmt::Write as _;

fn main() {
    let mut cases = 2000usize;
    let mut seed = 0x5eed_5ca7_c0de_2026u64;
    let mut out = format!(
        "{}/testdata/smartcn/cases/fuzz.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next().expect("missing value");
        match arg.as_str() {
            "--cases" => cases = value.parse().unwrap(),
            "--seed" => seed = value.parse().unwrap(),
            "--out" => out = value,
            _ => panic!("unknown argument {arg}"),
        }
    }

    let mut rng = Rng(seed);
    let mut file = String::new();
    for _ in 0..cases {
        let input = generate(&mut rng);
        escape_line(&input, &mut file);
        file.push('\n');
    }
    std::fs::write(&out, &file).unwrap();
    eprintln!("wrote {cases} cases ({} bytes) to {out}", file.len());
}

/// splitmix64; tiny and good enough for test-input generation.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn range(&mut self, lo: usize, hi_inclusive: usize) -> usize {
        lo + self.below(hi_inclusive - lo + 1)
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    fn char_in(&mut self, lo: u32, hi_inclusive: u32) -> char {
        loop {
            let cp = lo + self.below((hi_inclusive - lo + 1) as usize) as u32;
            if let Some(c) = char::from_u32(cp) {
                return c;
            }
        }
    }
}

const WORDS: &[&str] = &[
    "我",
    "你",
    "他",
    "的",
    "了",
    "和",
    "是",
    "在",
    "有",
    "不",
    "这",
    "那",
    "就",
    "也",
    "都",
    "购买",
    "道具",
    "服装",
    "中国",
    "人民",
    "共和国",
    "北京",
    "上海",
    "大学",
    "学生",
    "经济",
    "发展",
    "社会",
    "主义",
    "历史",
    "文化",
    "科学",
    "技术",
    "公司",
    "市场",
    "政府",
    "国家",
    "世界",
    "时间",
    "问题",
    "工作",
    "生活",
    "研究",
    "系统",
    "信息",
    "数据",
    "网络",
    "计算机",
    "软件",
    "用户",
    "服务",
    "产品",
    "中华人民共和国",
    "马克思列宁主义",
    "南京市长江大桥",
    "结婚的和尚未结婚的",
    "乒乓球拍卖完了",
    "我们中出了一个叛徒",
    "北京大学生前来应聘",
    "研究生命起源",
    "长春市长春药店",
    "他说的确实在理",
    "优素福·拉扎·吉拉尼",
    "一",
    "二",
    "三",
    "十",
    "百",
    "千",
    "万",
    "年",
    "月",
    "日",
    "人",
    "大",
    "小",
    "上",
    "下",
    "中",
    "国",
    "家",
    "学",
    "生",
    "子",
    "们",
    "来",
    "去",
    "说",
    "看",
    "想",
    "要",
];

const ENGLISH: &[&str] = &[
    "the",
    "a",
    "an",
    "of",
    "and",
    "to",
    "in",
    "is",
    "was",
    "were",
    "be",
    "it",
    "that",
    "this",
    "with",
    "for",
    "as",
    "on",
    "at",
    "by",
    "from",
    "he",
    "she",
    "they",
    "we",
    "you",
    "I",
    "Mr",
    "Dr",
    "Smith",
    "John",
    "London",
    "test",
    "tests",
    "testing",
    "tested",
    "running",
    "runs",
    "happily",
    "generously",
    "conditional",
    "relational",
    "hopping",
    "hoped",
    "sky",
    "skies",
    "cats",
    "ponies",
    "caresses",
    "agreed",
    "feed",
    "plastered",
    "bled",
    "motoring",
    "sing",
    "U.S.A.",
    "e.g.",
    "i.e.",
    "etc.",
    "3.14",
    "1,000",
    "100%",
    "C++",
    "e-mail",
    "iPhone",
    "Title:San",
    "ＡＢＣ",
    "Ｔｅｓｔｓ",
    "１２３４",
];

const ASCII_PUNCT: &[char] = &[
    '.', ',', '!', '?', ';', ':', '\'', '"', '(', ')', '[', ']', '{', '}', '<', '>', '-', '_', '=',
    '+', '*', '/', '\\', '|', '@', '#', '$', '%', '^', '&', '~', '`',
];

const CJK_PUNCT: &[char] = &[
    '。', '，', '、', '；', '：', '？', '！', '「', '」', '『', '』', '（', '）', '《', '》', '【',
    '】', '“', '”', '‘', '’', '…', '—', '·', '～', '－', '．', '￥', '〔', '〕', '〈', '〉', '＠',
    '＃',
];

const WHITESPACE: &[&str] = &[
    " ",
    " ",
    " ",
    "  ",
    "\t",
    "\n",
    "\n",
    "\r\n",
    "\r",
    "　",
    "　",
    "\u{2028}",
    "\u{2029}",
    "\u{85}",
    "\u{0C}",
    "\u{0B}",
    "\u{A0}",
    "\u{3000}\u{3000}",
];

/// Exact boundary code points of every range in `Utility.getCharType`, plus neighbours.
const BOUNDARY: &[char] = &[
    '\u{4DFF}',
    '\u{4E00}',
    '\u{9FA5}',
    '\u{9FA6}',
    '\u{9FFF}',
    '\u{3400}',
    '\u{4DBF}',
    '\u{0020}',
    '\u{0021}',
    '\u{002F}',
    '\u{0030}',
    '\u{0039}',
    '\u{003A}',
    '\u{0040}',
    '\u{0041}',
    '\u{005A}',
    '\u{005B}',
    '\u{0060}',
    '\u{0061}',
    '\u{007A}',
    '\u{007B}',
    '\u{007E}',
    '\u{007F}',
    '\u{0080}',
    '\u{00A0}',
    '\u{00BB}',
    '\u{00BC}',
    '\u{00FF}',
    '\u{0100}',
    '\u{200F}',
    '\u{2010}',
    '\u{2642}',
    '\u{2643}',
    '\u{2FFF}',
    '\u{3000}',
    '\u{3001}',
    '\u{301E}',
    '\u{301F}',
    '\u{3020}',
    '\u{FE2F}',
    '\u{FE30}',
    '\u{FF0F}',
    '\u{FF10}',
    '\u{FF19}',
    '\u{FF1A}',
    '\u{FF20}',
    '\u{FF21}',
    '\u{FF3A}',
    '\u{FF3B}',
    '\u{FF40}',
    '\u{FF41}',
    '\u{FF5A}',
    '\u{FF5B}',
    '\u{FF63}',
    '\u{FF64}',
    '\u{FFEF}',
    '\u{FFFD}',
    '\u{FFFF}',
    '\u{E000}',
    '\u{D7FF}',
    '\u{10000}',
    '\u{1F600}',
    '\u{2A6DF}',
    '\u{20000}',
    '\u{E0001}',
    '\u{10FFFF}',
    '\u{0000}',
    '\u{0001}',
    '\u{001F}',
];

const MARKS_AND_FORMAT: &[&str] = &[
    "\u{301}",
    "\u{300}",
    "\u{20DD}",
    "\u{200B}",
    "\u{200C}",
    "\u{200D}",
    "\u{FEFF}",
    "\u{2060}",
    "\u{00AD}",
    "\u{0BBE}",
    "\u{1F3FB}",
    "\u{E0067}",
];

const EMOJI: &[&str] = &[
    "😀",
    "😀😀",
    "🇨🇳",
    "👨‍👩‍👧‍👦",
    "👍🏽",
    "❤️",
    "🎉",
    "𠀀",
    "𠀁",
    "𫟼",
    "𜬻",
    "🀄",
];

fn other_script(rng: &mut Rng, s: &mut String) {
    let len = rng.range(1, 6);
    let (lo, hi) = *rng.pick(&[
        (0x0410, 0x044F), // Cyrillic upper + lower
        (0x0391, 0x03C9), // Greek
        (0x0621, 0x064A), // Arabic
        (0x05D0, 0x05EA), // Hebrew
        (0xAC00, 0xD7A3), // Hangul syllables
        (0x3041, 0x3096), // Hiragana
        (0x30A1, 0x30FA), // Katakana
        (0x0E01, 0x0E3A), // Thai
        (0x0905, 0x0939), // Devanagari
        (0x00C0, 0x00FF), // Latin-1 letters
        (0x0100, 0x017F), // Latin Extended-A
        (0x1E00, 0x1EFF), // Latin Extended Additional
    ]);
    for _ in 0..len {
        s.push(rng.char_in(lo, hi));
    }
    if (lo, hi) == (0x0905, 0x0939) && rng.chance(50) {
        s.push(*rng.pick(&['।', '॥']));
    }
}

fn english_sentence(rng: &mut Rng, s: &mut String) {
    let words = rng.range(2, 9);
    for i in 0..words {
        if i > 0 {
            s.push(' ');
        }
        let w = *rng.pick(ENGLISH);
        if i == 0 && rng.chance(70) {
            let mut chars = w.chars();
            if let Some(first) = chars.next() {
                s.extend(first.to_uppercase());
                s.push_str(chars.as_str());
            }
        } else if rng.chance(10) {
            s.push_str(&w.to_uppercase());
        } else {
            s.push_str(w);
        }
    }
    s.push_str(rng.pick(&[".", ".", ".", "?", "!", "...", ".\"", ".)", "。", ""]));
    s.push_str(rng.pick(&[" ", " ", "  ", "\n", "", "\t"]));
}

fn chinese_sentence(rng: &mut Rng, s: &mut String) {
    let words = rng.range(1, 12);
    for _ in 0..words {
        s.push_str(rng.pick(WORDS));
        if rng.chance(8) {
            s.push(*rng.pick(&['，', '、', ',', ' ']));
        }
    }
    s.push_str(rng.pick(&["。", "。", "！", "？", "……", "", "\n", "。\n"]));
}

fn run(rng: &mut Rng, s: &mut String, allow_whitespace: bool) {
    match rng.below(100) {
        0..=27 => s.push_str(rng.pick(WORDS)),
        28..=37 => {
            for _ in 0..rng.range(1, 4) {
                s.push(rng.char_in(0x4E00, 0x9FA5));
            }
        }
        38..=40 => {
            let (lo, hi) = *rng.pick(&[(0x9FA6, 0x9FFF), (0x3400, 0x4DBF), (0x20000, 0x2A6DF)]);
            s.push(rng.char_in(lo, hi));
        }
        41..=49 => {
            for _ in 0..rng.range(1, 10) {
                let c = rng.char_in(b'a' as u32, b'z' as u32);
                s.push(if rng.chance(25) {
                    c.to_ascii_uppercase()
                } else {
                    c
                });
            }
        }
        50..=54 => {
            for _ in 0..rng.range(1, 8) {
                s.push(rng.char_in(b'0' as u32, b'9' as u32));
            }
        }
        55..=57 => {
            for _ in 0..rng.range(1, 5) {
                let (lo, hi) = *rng.pick(&[(0xFF21, 0xFF3A), (0xFF41, 0xFF5A), (0xFF10, 0xFF19)]);
                s.push(rng.char_in(lo, hi));
            }
        }
        58..=64 => s.push(*rng.pick(ASCII_PUNCT)),
        65..=71 => s.push(*rng.pick(CJK_PUNCT)),
        72..=80 => {
            if allow_whitespace {
                s.push_str(rng.pick(WHITESPACE));
            } else {
                s.push_str(rng.pick(WORDS));
            }
        }
        81..=85 => other_script(rng, s),
        86..=89 => {
            let (lo, hi) = *rng.pick(&[
                (0x00A1, 0x00FF),
                (0x2010, 0x2642),
                (0x2643, 0x2FFF),
                (0x3001, 0x303F),
                (0xFE30, 0xFF65),
                (0xFF66, 0xFFEF),
            ]);
            s.push(rng.char_in(lo, hi));
        }
        90..=92 => s.push_str(rng.pick(EMOJI)),
        93..=95 => s.push_str(rng.pick(MARKS_AND_FORMAT)),
        96..=97 => s.push(*rng.pick(BOUNDARY)),
        _ => {
            if allow_whitespace {
                english_sentence(rng, s);
            } else {
                chinese_sentence(rng, s);
            }
        }
    }
}

fn generate(rng: &mut Rng) -> String {
    let mut s = String::new();
    match rng.below(100) {
        // Short random mixes.
        0..=59 => {
            for _ in 0..rng.range(1, 30) {
                run(rng, &mut s, true);
            }
        }
        // Sentence-like text, for the sentence splitter.
        60..=74 => {
            for _ in 0..rng.range(1, 8) {
                if rng.chance(50) {
                    chinese_sentence(rng, &mut s);
                } else {
                    english_sentence(rng, &mut s);
                }
            }
        }
        // Medium.
        75..=89 => {
            for _ in 0..rng.range(30, 150) {
                run(rng, &mut s, true);
            }
        }
        // Long, crossing the 1024-char buffer, with whitespace/newlines present.
        90..=94 => {
            while s.chars().count() < rng.range(1000, 2600) {
                run(rng, &mut s, true);
            }
        }
        // Long with no whitespace at all, so the buffer is cut at an arbitrary char.
        _ => {
            let target = rng.range(1025, 2300);
            while s.chars().count() < target {
                run(rng, &mut s, false);
            }
        }
    }
    s
}

/// Same escaping as the Java side: backslash, CR, LF, TAB, other C0/C1 controls, DEL, LS, PS.
fn escape_line(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20
                || ((c as u32) >= 0x7F && (c as u32) < 0xA0)
                || c == '\u{2028}'
                || c == '\u{2029}' =>
            {
                write!(out, "\\u{{{:x}}}", c as u32).unwrap();
            }
            c => out.push(c),
        }
    }
}
