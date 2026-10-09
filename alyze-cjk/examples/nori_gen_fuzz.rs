//! Generates the seeded random inputs in `testdata/nori/cases/fuzz.txt` for the nori differential
//! tests. Deterministic: the same seed always produces the same file, so the file is committed and
//! this only needs rerunning when the generator changes.
//!
//!     cargo run -p alyze-cjk --example nori_gen_fuzz [-- --cases N --seed S --out PATH]
//!
//! After regenerating, rerun `testdata/nori/gen.sh` to refresh the golden files.
//!
//! The inputs mix runs drawn from classes chosen to hit every branch of the Java implementation:
//! dictionary words (nouns, verb and adjective stems with endings, particles, adverbs, numerals,
//! compounds, inflected and pre-analysed entries, Hanja with readings), random Hangul syllables and
//! jamo, every character class of mecab-ko-dic's `char.def` with the code points at each class
//! boundary, whitespace of every kind the tokenizer treats differently (space-separator skipping
//! and penalty versus the SPACE class), Latin with case and combining marks, other scripts, digits
//! with decimal points and thousands separators, fullwidth forms, symbols, format characters,
//! emoji and other supplementary characters, and long runs that cross the 1024-unit unknown-word
//! cap and the 1024-position forced backtrace.

use std::fmt::Write as _;

fn main() {
    let mut cases = 1000usize;
    let mut seed = 0x5eed_c0de_0000_4b4fu64; // "KO"
    let mut out = format!(
        "{}/testdata/nori/cases/fuzz.txt",
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

// Vocabulary sampled from mecab-ko-dic (lowest-cost entries of the main part-of-speech files),
// plus the words Lucene's and Elasticsearch's tests use.
const NOUNS: &[&str] = &[
    "하나하나",
    "집들이",
    "다음",
    "하나님",
    "이후",
    "말씀",
    "이야기",
    "정도",
    "과정",
    "아저씨",
    "여기저기",
    "안내서",
    "연기자",
    "평등",
    "과거",
    "후보",
    "시간",
    "옛날",
    "이번",
    "학년도",
    "분야",
    "얘기",
    "대통령",
    "가족",
    "나중",
    "한국어",
    "전화",
    "이해",
    "합의",
    "인간",
    "교수",
    "가운데",
    "오늘날",
    "지난해",
    "자본주의",
    "남자",
    "선생",
    "신화",
    "저작",
    "해제",
    "뿌리",
    "나무",
    "화학",
    "것",
    "사이즈",
    "인치",
    "모니터",
    "언어",
    "프로그래밍",
    "날씨",
    "세기",
    "청사",
    "정부",
    "용",
    "표",
    "개",
    "원",
    "와인",
    "구입",
    "초밥",
    "가격",
    "자본금",
    "오늘",
    "어제",
    "한국",
    "중국",
    "일본",
    "평창",
    "동계",
    "올림픽",
    "대회",
    "도로",
    "지반",
    "수자원",
    "건설",
    "환경",
    "건축",
    "화재",
    "설비",
    "연구",
    "나라",
    "학교",
    "집",
    "밥",
    "물",
    "바람",
    "꽃",
    "열매",
    "사람",
    "세상",
    "마음",
    "생각",
    "문제",
    "사회",
    "경제",
    "문화",
    "역사",
    "기술",
    "정보",
    "데이터",
    "검색",
    "형태소",
    "분석기",
    "루씬",
    "엘라스틱서치",
    "서울",
    "부산",
    "대전",
    "광주",
    "강남구",
    "테헤란로",
];
const PROPER: &[&str] = &[
    "게르만",
    "한글",
    "동대문",
    "삼국지",
    "러시아어",
    "훈민정음",
    "한국사",
    "그리스어",
    "명심보감",
    "가나안족",
    "세종",
    "대한민국",
    "갠지스강",
    "가락지나물",
    "보험계약대출이율",
    "가늠표",
    "정부세종청사",
    "월드와이드웹",
    "가나아트센터",
    "가교협",
    "은점표범나비",
    "참다랭이",
];
const VERB_STEMS: &[&str] = &[
    "빛나",
    "나아가",
    "나오",
    "만나",
    "만들",
    "들어가",
    "나가",
    "위하",
    "닿",
    "지나가",
    "올라가",
    "넣",
    "있",
    "보이",
    "모르",
    "읽",
    "들어오",
    "지내",
    "주고받",
    "벗어나",
    "가지",
    "되",
    "나타나",
    "깨어나",
    "다가오",
    "돌아가",
    "일어나",
    "받",
    "올리",
    "먹",
    "그러",
    "하",
    "가",
    "오",
    "보",
    "살",
    "죽",
    "쓰",
    "알",
    "웃",
    "울",
];
const ADJ_STEMS: &[&str] = &[
    "같",
    "좋",
    "없",
    "새롭",
    "맛있",
    "힘들",
    "짧",
    "지나치",
    "편하",
    "재밌",
    "다르",
    "괜찮",
    "친하",
    "수많",
    "엄청나",
    "귀하",
    "어이없",
    "싫",
    "넓",
    "어렵",
    "깊",
    "귀찮",
    "늦",
    "짙",
    "아프",
    "거칠",
    "아쉽",
    "낡",
    "그렇",
    "비싸",
    "심하",
    "바쁘",
    "급하",
    "과하",
    "나쁘",
    "대단하",
];
const ENDINGS: &[&str] = &[
    "지만",
    "면서",
    "다가",
    "게",
    "어서",
    "아서",
    "는지",
    "은데",
    "도록",
    "다시피",
    "라고",
    "다고",
    "더라도",
    "듯이",
    "아도",
    "고",
    "거나",
    "어도",
    "더니",
    "습니다",
    "는구나",
    "어요",
    "는가",
    "거든요",
    "는데요",
    "더라구요",
    "에요",
    "야",
    "구나",
    "는다",
    "지요",
    "어",
    "습니까",
    "네",
    "잖아",
    "라는",
    "는다는",
    "다는",
    "던",
    "은",
    "는",
    "을",
    "ㄹ",
    "ㄴ",
    "기",
    "음",
    "았",
    "었",
    "겠",
    "시",
    "으시",
    "었었",
    "다",
    "니다",
    "ㅂ니다",
    "ㅂ니까",
    "세요",
    "셨",
    "아",
    "여",
    "아요",
    "았다",
    "었다",
    "ㄴ다",
    "는다고",
    "자",
    "니",
    "냐",
    "지",
    "죠",
];
const PARTICLES: &[&str] = &[
    "가",
    "이",
    "을",
    "를",
    "은",
    "는",
    "의",
    "에",
    "에서",
    "에게",
    "께",
    "께서",
    "로",
    "으로",
    "와",
    "과",
    "도",
    "만",
    "까지",
    "부터",
    "처럼",
    "보다",
    "조차",
    "마저",
    "이나",
    "나",
    "라도",
    "이라도",
    "밖에",
    "뿐",
    "하고",
    "랑",
    "이랑",
    "한테",
    "더러",
    "대로",
    "마다",
    "요",
    "이다",
    "입니다",
    "이에요",
    "예요",
];
const ADVERBS: &[&str] = &[
    "되게",
    "아직",
    "거의",
    "실제로",
    "먼저",
    "이렇게",
    "제대로",
    "같이",
    "언제나",
    "참으로",
    "그렇게",
    "전혀",
    "얼마나",
    "적어도",
    "모두",
    "다시",
    "사실",
    "그대로",
    "때때로",
    "너무나",
    "매우",
    "지금",
    "게다가",
    "절대로",
    "아무리",
    "이제",
    "없이",
    "많이",
    "그냥",
    "그리고",
    "그러나",
    "하지만",
    "그래서",
    "또한",
    "즉",
    "및",
];
const DETERMINERS: &[&str] = &[
    "갖은",
    "무슨",
    "이런",
    "어떤",
    "모든",
    "그런",
    "다른",
    "어느",
    "몇몇",
    "한두",
    "한",
    "두",
    "세",
    "네",
    "첫",
    "온갖",
    "그",
    "이",
    "저",
    "아무런",
    "여섯",
];
const PRONOUNS: &[&str] = &[
    "이거",
    "그것",
    "자기",
    "거기",
    "그거",
    "여기",
    "여러분",
    "우리",
    "어디",
    "이것",
    "무엇",
    "뭐",
    "저희",
    "누구",
    "당신",
    "그녀",
    "너",
    "나",
    "저",
    "그분",
];
const NUMERALS: &[&str] = &[
    "하나",
    "둘",
    "셋",
    "넷",
    "다섯",
    "여섯",
    "일곱",
    "여덟",
    "아홉",
    "열",
    "스물",
    "서른",
    "마흔",
    "백",
    "천",
    "만",
    "억",
    "조",
    "경",
    "해",
    "일",
    "이",
    "삼",
    "사",
    "오",
    "육",
    "칠",
    "팔",
    "구",
    "영",
    "십",
    "둘째",
    "첫째",
    "셋째",
    "수십",
    "수백",
    "백만",
    "천만",
    "일천",
    "십만이천오백",
    "육백이만오천일",
    "해경조억만천백십일",
    "삼천2백２십삼",
];
const INFLECTED: &[&str] = &[
    "감싸여",
    "걸어왔",
    "그르친다면",
    "끌어당길",
    "내온다",
    "다녔으므로",
    "돌아가신",
    "들립니다",
    "라더니",
    "모아요",
    "반해서",
    "부끄러운가",
    "아까와서",
    "얼정거렸",
    "우스운",
    "준대면은",
    "차롄데요",
    "터져나와",
    "할려구",
    "회산데",
    "고양이로소이다",
    "안녕하세요",
    "입니다",
    "됐다",
    "했어요",
    "갔었었다",
];
const HANJA: &[&str] = &[
    "鄕歌",
    "車丞相",
    "喜悲哀歡",
    "五朔居廬",
    "伯固",
    "冥闇",
    "反汗",
    "坑骨格構造",
    "宇宙核戰爭",
    "師椽",
    "愛好",
    "東峯",
    "汗顔",
    "無毁無譽",
    "積重",
    "菜羹",
    "詳計",
    "長途",
    "老將軍",
    "漢字",
    "韓國",
    "日本",
    "中國",
    "東京",
    "一",
    "二",
    "三",
    "四",
    "五",
    "六",
    "七",
    "八",
    "九",
    "十",
    "百",
    "千",
    "萬",
    "億",
    "兆",
    "三國志",
    "丞相",
    "㢿弓",
];
const LATIN: &[&str] = &[
    "the",
    "and",
    "of",
    "to",
    "in",
    "is",
    "Lucene",
    "Elasticsearch",
    "nori",
    "Korean",
    "analyzer",
    "c++",
    "C++",
    "c++world",
    "JavaScript",
    "iPhone",
    "Pro",
    "Hello",
    "World",
    "HELLO",
    "straße",
    "İstanbul",
    "ǅemal",
    "naïve",
    "café",
    "ka̠k̚t͡ɕ͈a̠k̚",
    "Ба̀лтичко",
    "мо̑ре",
    "εἰμί",
    "ΣΊΣΥΦΟΣ",
    "Привет",
    "мир",
    "Ελληνικά",
    "ＡＢＣ",
    "ａｂｃ",
    "Ｔｅｓｔ",
    "x",
    "A4",
    "G",
    "2PM",
    "e\u{301}",
    "a\u{301}b",
];
const KANA: &[&str] = &[
    "こんにちは",
    "カタカナ",
    "ｶﾀｶﾅ",
    "ひらがな",
    "ー",
    "カー",
    "ｶﾞ",
    "タワー",
    "テキスト",
    "の",
    "です",
];
const JAMO: &[&str] = &[
    "ㄱ",
    "ㄴ",
    "ㄷ",
    "ㅋ",
    "ㅎ",
    "ㅠ",
    "ㅏ",
    "ㅑ",
    "ㅋㅋ",
    "ㅋㅋㅋ",
    "ㅎㅎ",
    "ㅠㅠ",
    "ㆍ",
    "\u{1100}",
    "\u{1161}",
    "\u{11a8}",
    "\u{11ab}",
    "\u{1100}\u{1161}",
    "\u{d7b0}",
    "\u{a960}",
];
const SYMBOLS: &[&str] = &[
    "★", "☆", "→", "∑", "©", "™", "№", "℃", "㎞", "㈜", "㉠", "∞", "≠", "±", "~", "`", "^", "|",
    "\\", "@", "#", "&", "*", "_", "=", "<", ">", "§", "¶", "¿", "¡", "«", "»", "‰", "′", "※",
    "〒", "〓", "＃", "＆", "＠", "￦", "￥", "$", "€", "₩", "%", "①", "②", "Ⅻ", "½", "²", "³",
    "\u{fffd}", "\u{fffe}", "\u{3007}", "\u{3005}",
];
const PUNCT: &[&str] = &[
    ".", ",", "!", "?", ";", ":", "…", "·", "/", "-", "(", ")", "[", "]", "{", "}", "\"", "'", "“",
    "”", "‘", "’", "「", "」", "『", "』", "〈", "〉", "《", "》", "。", "、", "！", "？", "．",
    "，", "：", "；", "｡", "｢", "｣", "･", "ㆍ", "...", "!!!", "???", "!?", "^^", "~~",
];
const WHITESPACE: &[&str] = &[
    " ", " ", " ", " ", " ", "  ", "   ", "        ", "\t", "\n", "\r\n", "\r", "\u{3000}",
    "\u{a0}", "\u{2003}", "\u{2009}", "\u{200b}", "\u{b}", "\u{c}", "\u{85}", "\u{2028}",
    "\u{2029}",
];
const FORMAT_AND_MARKS: &[&str] = &[
    "\u{200d}", "\u{200c}", "\u{200e}", "\u{feff}", "\u{ad}", "\u{2060}", "\u{202a}", "\u{202c}",
    "\u{301}", "\u{302}", "\u{20d0}", "\u{302f}", "\u{3029}", "\u{3039}", "\u{0}", "\u{1}",
    "\u{7f}",
];
const EMOJI: &[&str] = &[
    "😀",
    "😂",
    "👍",
    "👍🏽",
    "🇰🇷",
    "👨\u{200d}👩\u{200d}👧",
    "🀄",
    "𝐀",
    "𝐁",
    "𠀀",
    "𠀁",
    "😀😀",
    "🎉",
    "❤",
    "✅",
    "\u{1f1f0}",
];

/// First and last code point of every `char.def` range, and the code point just after each.
const BOUNDARY: &[u32] = &[
    0x20, 0x21, 0x2f, 0x30, 0x39, 0x3a, 0x40, 0x41, 0x5a, 0x5b, 0x60, 0x61, 0x7a, 0x7b, 0x7e, 0x7f,
    0xa0, 0xa1, 0xbf, 0xc0, 0xff, 0x100, 0x17f, 0x180, 0x236, 0x237, 0x374, 0x3fb, 0x3fc, 0x400,
    0x4f9, 0x4fa, 0x500, 0x50f, 0x510, 0x1100, 0x11ff, 0x1200, 0x1e00, 0x1ef9, 0x1efa, 0x2000,
    0x206f, 0x2070, 0x209f, 0x20a0, 0x20cf, 0x20d0, 0x20ff, 0x2100, 0x214f, 0x2150, 0x218f, 0x2190,
    0x21ff, 0x2200, 0x22ff, 0x2300, 0x23ff, 0x2400, 0x2460, 0x24ff, 0x2500, 0x2501, 0x257f, 0x2580,
    0x259f, 0x25a0, 0x25ff, 0x2600, 0x26fe, 0x26ff, 0x2700, 0x27bf, 0x27c0, 0x27ef, 0x27f0, 0x27ff,
    0x2800, 0x28ff, 0x2900, 0x297f, 0x2980, 0x2a00, 0x2aff, 0x2b00, 0x2bff, 0x2c00, 0x2e80, 0x2ef3,
    0x2ef4, 0x2f00, 0x2fd5, 0x2fd6, 0x3000, 0x3005, 0x3006, 0x3007, 0x3008, 0x303f, 0x3040, 0x3041,
    0x309f, 0x30a0, 0x30a1, 0x30fc, 0x30ff, 0x3100, 0x3130, 0x318f, 0x3190, 0x31f0, 0x31ff, 0x3200,
    0x32fe, 0x32ff, 0x3300, 0x33ff, 0x3400, 0x4db5, 0x4db6, 0x4e00, 0x9fa5, 0x9fa6, 0x9fff, 0xa000,
    0xac00, 0xd7a3, 0xd7a4, 0xe000, 0xf8ff, 0xf900, 0xfa2d, 0xfa2e, 0xfa30, 0xfa6a, 0xfa6b, 0xfe30,
    0xfe4f, 0xfe50, 0xfe6b, 0xfe6c, 0xff01, 0xff0f, 0xff10, 0xff19, 0xff1a, 0xff1f, 0xff20, 0xff21,
    0xff3a, 0xff3b, 0xff40, 0xff41, 0xff5a, 0xff5b, 0xff65, 0xff66, 0xff9d, 0xff9e, 0xff9f, 0xffa0,
    0xffe0, 0xffef, 0xfff0, 0xfffd,
];

fn hangul_word(rng: &mut Rng, s: &mut String) {
    match rng.below(14) {
        0..=3 => s.push_str(rng.pick(NOUNS)),
        4 => s.push_str(rng.pick(PROPER)),
        5 | 6 => {
            s.push_str(rng.pick(VERB_STEMS));
            s.push_str(rng.pick(ENDINGS));
        }
        7 => {
            s.push_str(rng.pick(ADJ_STEMS));
            s.push_str(rng.pick(ENDINGS));
        }
        8 => s.push_str(rng.pick(ADVERBS)),
        9 => s.push_str(rng.pick(DETERMINERS)),
        10 => s.push_str(rng.pick(PRONOUNS)),
        11 => s.push_str(rng.pick(NUMERALS)),
        12 => s.push_str(rng.pick(INFLECTED)),
        _ => {
            // random syllables: mostly unknown to the dictionary
            for _ in 0..rng.range(1, 5) {
                s.push(rng.char_in(0xAC00, 0xD7A3));
            }
        }
    }
    if rng.chance(45) {
        s.push_str(rng.pick(PARTICLES));
    }
}

fn number(rng: &mut Rng, s: &mut String) {
    let fullwidth = rng.chance(20);
    let digit = |rng: &mut Rng| {
        let d = rng.below(10) as u32;
        char::from_u32(if fullwidth {
            0xFF10 + d
        } else {
            '0' as u32 + d
        })
        .unwrap()
    };
    if rng.chance(10) {
        s.push(*rng.pick(&['-', '+', '－', '＋']));
    }
    for i in 0..rng.range(1, 7) {
        if i > 0 && rng.chance(15) {
            s.push(*rng.pick(&['.', ',', '．', '，']));
        }
        s.push(digit(rng));
    }
    if rng.chance(25) {
        s.push_str(rng.pick(&[
            "만",
            "천",
            "백",
            "억",
            "원",
            "개",
            "년",
            "월",
            "일",
            "시",
            "분",
            "%",
            "℃",
            "㎞",
            "인치",
            "사이즈",
        ]));
    }
}

fn run(rng: &mut Rng, s: &mut String) {
    match rng.below(100) {
        0..=39 => hangul_word(rng, s),
        40..=47 => number(rng, s),
        48..=53 => s.push_str(rng.pick(LATIN)),
        54..=58 => s.push_str(rng.pick(HANJA)),
        59..=61 => s.push_str(rng.pick(KANA)),
        62..=65 => s.push_str(rng.pick(JAMO)),
        66..=71 => s.push_str(rng.pick(PUNCT)),
        72..=76 => s.push_str(rng.pick(SYMBOLS)),
        77..=79 => s.push_str(rng.pick(EMOJI)),
        80..=82 => s.push_str(rng.pick(FORMAT_AND_MARKS)),
        83..=85 => s.push(char::from_u32(*rng.pick(BOUNDARY)).unwrap()),
        86..=88 => {
            // a random code point from one char.def class range
            let (lo, hi) = *rng.pick(&[
                (0x4E00, 0x9FA5),
                (0x3400, 0x4DB5),
                (0xF900, 0xFA2D),
                (0x2E80, 0x2EF3),
                (0x2F00, 0x2FD5),
                (0x3041, 0x309F),
                (0x30A1, 0x30FF),
                (0xFF66, 0xFF9F),
                (0x1100, 0x11FF),
                (0x3130, 0x318F),
                (0x0400, 0x04F9),
                (0x0374, 0x03FB),
                (0x00C0, 0x024F),
                (0x1E00, 0x1EF9),
                (0x2000, 0x206F),
                (0x2100, 0x218F),
                (0x2190, 0x2BFF),
                (0x3000, 0x303F),
                (0x3200, 0x33FF),
                (0xFE30, 0xFE6B),
                (0xFF01, 0xFF65),
                (0xFFE0, 0xFFEF),
                (0x0590, 0x05FF),
                (0x0600, 0x06FF),
                (0x0E00, 0x0E7F),
                (0x0900, 0x097F),
                (0xE000, 0xF8FF),
                (0xA960, 0xA97F),
                (0xD7B0, 0xD7FF),
                (0x1F300, 0x1F64F),
                (0x20000, 0x2A6DF),
                (0x1D400, 0x1D7FF),
                (0x10000, 0x100FF),
            ]);
            for _ in 0..rng.range(1, 3) {
                s.push(rng.char_in(lo, hi));
            }
        }
        89..=91 => {
            // decomposed Hangul (conjoining jamo), as NFD text would be
            for _ in 0..rng.range(1, 3) {
                s.push(rng.char_in(0x1100, 0x1112));
                s.push(rng.char_in(0x1161, 0x1175));
                if rng.chance(50) {
                    s.push(rng.char_in(0x11A8, 0x11C2));
                }
            }
        }
        92..=94 => {
            // Latin with combining marks / mixed case
            for _ in 0..rng.range(1, 6) {
                let upper = rng.chance(30);
                s.push(rng.char_in(
                    if upper { 'A' as u32 } else { 'a' as u32 },
                    if upper { 'Z' as u32 } else { 'z' as u32 },
                ));
                if rng.chance(20) {
                    s.push(rng.char_in(0x300, 0x36F));
                }
            }
        }
        95..=96 => {
            // a repeated character (grouping, or not, by class)
            let c = *rng.pick(&[
                'ㅋ', 'ㅎ', 'ㅠ', '!', '.', '?', '~', '★', 'a', '1', '가', '😀', '一', 'ー', '-',
                '…',
            ]);
            for _ in 0..rng.range(2, 12) {
                s.push(c);
            }
        }
        _ => {
            // an arbitrary BMP or supplementary code point
            if rng.chance(70) {
                s.push(rng.char_in(0x20, 0xFFFD));
            } else {
                s.push(rng.char_in(0x10000, 0x10FFFF));
            }
        }
    }
}

fn generate(rng: &mut Rng) -> String {
    let mut s = String::new();
    // Length classes: mostly short, some medium, a few long enough to cross the 1024 limits.
    // (The golden files grow with the input, so long inputs are rare; `cases/edge.txt` has the
    // systematic ones.)
    let target = match rng.below(100) {
        0..=4 => 0,
        5..=59 => rng.range(1, 40),
        60..=89 => rng.range(40, 200),
        90..=97 => rng.range(200, 800),
        _ => rng.range(800, 2500),
    };
    if target == 0 {
        if rng.chance(50) {
            s.push_str(rng.pick(WHITESPACE));
        }
        return s;
    }
    let space_style = rng.below(10); // 0: no spaces at all, 1: spaces everywhere, else: mixed
    while s.chars().count() < target {
        run(rng, &mut s);
        let want_space = match space_style {
            0 => false,
            1 => true,
            _ => rng.chance(55),
        };
        if want_space {
            s.push_str(rng.pick(WHITESPACE));
        }
    }
    if rng.chance(10) {
        s.push_str(rng.pick(WHITESPACE));
    }
    if rng.chance(10) {
        s.insert_str(0, rng.pick(WHITESPACE));
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
