//! Generates the seeded random inputs in `testdata/kuromoji/cases/fuzz.txt` for the kuromoji
//! differential tests. Deterministic: the same seed always produces the same file, so the file is
//! committed and this only needs rerunning when the generator changes.
//!
//!     cargo run -p alyze-cjk --example kuromoji_gen_fuzz [-- --cases N --seed S --out PATH]
//!
//! After regenerating, rerun `testdata/kuromoji/gen.sh` to refresh the golden files.
//!
//! The inputs mix runs drawn from classes chosen to hit every branch of the Java implementation:
//! mecab-ipadic vocabulary (nouns, proper nouns and place names, long kanji compounds that search
//! mode decompounds and 2-kanji words it must not, inflected verbs and adjectives, auxiliaries,
//! particles, adverbs, pronouns, conjunctions, interjections, fillers), long katakana compounds
//! that trip the search-mode "other" penalty, katakana ending in the prolonged sound mark for the
//! stemmer, small kana, half-width katakana with voiced marks for the width filter, full-width
//! ASCII, hiragana-only runs, Arabic and kanji numerals with separators, signs, years and counters
//! for the number filter, iteration marks in legal and illegal positions, every character class
//! of mecab-ipadic's `char.def` with the code points at each class boundary, punctuation and
//! whitespace of every kind, format and combining characters, other scripts, emoji and other
//! supplementary characters, and long runs that cross the 1024-unit unknown-word cap (once inside
//! a surrogate pair) and the 1024-position forced backtrace.

use std::fmt::Write as _;

fn main() {
    let mut cases = 500usize;
    let mut seed = 0x5eed_c0de_0000_4a41u64; // "JA"
    let mut out = format!(
        "{}/testdata/kuromoji/cases/fuzz.txt",
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

// Vocabulary sampled from mecab-ipadic 2.7.0 (surface forms verified against the CSVs), plus the
// words Lucene's and Elasticsearch's tests use. Inflected forms are sequences of dictionary
// morphemes (stem + auxiliaries), not single entries.
const NOUNS: &[&str] = &[
    "東京",
    "日本",
    "学校",
    "会社",
    "電車",
    "新聞",
    "仕事",
    "時間",
    "問題",
    "世界",
    "言葉",
    "文化",
    "経済",
    "社会",
    "歴史",
    "技術",
    "情報",
    "検索",
    "寿司",
    "手紙",
    "友達",
    "先生",
    "学生",
    "今日",
    "明日",
    "昨日",
    "今年",
    "来年",
    "天気",
    "料理",
    "音楽",
    "映画",
    "写真",
    "部屋",
    "図書館",
    "病院",
    "銀行",
    "空港",
    "大学",
    "辞書",
    "鉛筆",
    "机",
    "椅子",
    "自転車",
    "名前",
    "家族",
    "子供",
    "兄弟",
    "両親",
    "人間",
    "動物",
    "植物",
    "野菜",
    "果物",
    "魚",
    "肉",
    "米",
    "茶",
    "牛乳",
    "砂糖",
    "塩",
    "朝",
    "昼",
    "夜",
    "春",
    "夏",
    "秋",
    "冬",
    "山",
    "川",
    "海",
    "空",
    "花",
    "犬",
    "猫",
    "人",
    "国",
    "家",
    "道",
    "駅",
    "店",
    "町",
    "市",
    "県",
    "車",
    "本",
    "水",
    "木",
    "火",
    "土",
    "金",
    "雨",
    "雪",
    "風",
    "光",
    "声",
    "心",
    "目",
    "手",
    "足",
    "頭",
    "顔",
    "漢字",
    "放課後",
    "海賊版",
    "降誕祭",
    "本支社",
    "唯我独尊",
    "新幹線",
    "情報処理",
    "勉強",
    "出口",
    "入口",
    "学問",
];
const PROPER: &[&str] = &[
    "大阪",
    "京都",
    "奈良",
    "名古屋",
    "横浜",
    "北海道",
    "富士山",
    "新宿",
    "渋谷",
    "多摩川",
    "千葉",
    "埼玉",
    "神戸",
    "福岡",
    "沖縄",
    "太郎",
    "花子",
    "山田",
    "田中",
    "鈴木",
    "佐々木",
    "代々木",
    "徳川家康",
    "豊臣秀吉",
    "織田信長",
    "夏目漱石",
    "坂本龍馬",
    "いすゞ",
    "アメリカ",
    "イギリス",
    "フランス",
    "パナソニック",
    "東京大学",
    "京都大学",
    "大阪大学",
    "日本経済新聞",
];
/// Kanji compounds of 3 to 8 characters: search mode penalises them and decompounds.
const COMPOUNDS: &[&str] = &[
    "関西国際空港",
    "奈良先端科学技術大学院大学",
    "東京都知事選挙",
    "自然言語処理",
    "機械学習",
    "高速道路",
    "国民健康保険",
    "国際連合",
    "携帯電話",
    "東京都",
    "十重二十重",
    "中部日本放送",
    "武蔵野短期大学",
    "甲南女子大学",
    "鹿児島国際観光",
    "口永良部島",
    "情報処理技術者",
    "日本経済新聞社",
    "東京国際空港",
    "東京大学",
];
const VERBS: &[&str] = &[
    "食べた",
    "食べたい",
    "食べる",
    "食べます",
    "食べない",
    "食べられる",
    "食べなかった",
    "行きます",
    "行こう",
    "行った",
    "行かない",
    "行きたい",
    "走らなければならない",
    "走る",
    "走った",
    "見る",
    "見た",
    "見ます",
    "来る",
    "来た",
    "来ない",
    "する",
    "した",
    "します",
    "しよう",
    "しない",
    "読んだ",
    "読む",
    "書いた",
    "書きます",
    "話す",
    "話した",
    "飲む",
    "飲んだ",
    "泳ぐ",
    "思う",
    "思った",
    "言う",
    "言った",
    "聞く",
    "聞いた",
    "作る",
    "作った",
    "待つ",
    "待った",
    "死ぬ",
    "遊ぶ",
    "買う",
    "買った",
    "出かける",
    "帰ります",
    "帰ってきた",
    "勉強する",
    "勉強しました",
    "働いている",
    "住んでいます",
    "分かりません",
    "知っていますか",
    "できる",
    "できません",
    "ある",
    "あった",
    "いる",
    "いた",
];
const ADJECTIVES: &[&str] = &[
    "美しかった",
    "おいしくない",
    "美しい",
    "高い",
    "安い",
    "大きい",
    "小さい",
    "新しい",
    "古い",
    "良い",
    "悪い",
    "暑い",
    "寒い",
    "楽しい",
    "難しい",
    "易しい",
    "速かった",
    "嬉しくて",
    "静かだ",
    "綺麗な",
    "元気です",
    "便利で",
    "有名な",
    "面白い",
    "面白かった",
    "暖かい",
    "冷たい",
    "忙しい",
    "忙しかった",
    "高くない",
    "大きくて",
    "大きな",
    "小さな",
];
const AUXILIARIES: &[&str] = &[
    "です",
    "ます",
    "でした",
    "ました",
    "だ",
    "だった",
    "でしょう",
    "ません",
    "ない",
    "たい",
    "らしい",
    "そうだ",
    "ようだ",
    "ございます",
    "なければ",
    "べき",
    "まい",
    "ぬ",
    "た",
    "て",
];
const PARTICLES: &[&str] = &[
    "は",
    "が",
    "を",
    "に",
    "の",
    "で",
    "と",
    "から",
    "まで",
    "より",
    "へ",
    "も",
    "や",
    "か",
    "ね",
    "よ",
    "ば",
    "ても",
    "でも",
    "には",
    "では",
    "とは",
    "とか",
    "など",
    "だけ",
    "しか",
    "ほど",
    "くらい",
    "って",
    "について",
    "によって",
    "として",
];
const ADVERBS: &[&str] = &[
    "とても",
    "非常に",
    "もっと",
    "ずっと",
    "いつも",
    "時々",
    "ゆっくり",
    "すぐ",
    "また",
    "まだ",
    "もう",
    "たぶん",
    "きっと",
    "全然",
    "少し",
    "たくさん",
    "本当に",
    "特に",
    "必ず",
    "大変",
    "かなり",
    "ほとんど",
];
const PRONOUNS: &[&str] = &[
    "私",
    "僕",
    "あなた",
    "彼",
    "彼女",
    "我々",
    "これ",
    "それ",
    "あれ",
    "どれ",
    "ここ",
    "そこ",
    "あそこ",
    "どこ",
    "誰",
    "何",
    "自分",
    "こちら",
    "私たち",
];
const ADNOMINALS: &[&str] = &[
    "この",
    "その",
    "あの",
    "どの",
    "こんな",
    "そんな",
    "あらゆる",
    "いわゆる",
];
const CONJUNCTIONS: &[&str] = &[
    "そして",
    "しかし",
    "また",
    "でも",
    "だから",
    "それで",
    "ところで",
    "つまり",
    "または",
    "および",
    "それから",
    "たとえば",
    "ただし",
];
const INTERJECTIONS: &[&str] = &[
    "はい",
    "いいえ",
    "ええ",
    "あら",
    "おお",
    "おはよう",
    "こんにちは",
    "ありがとう",
    "すみません",
    "さようなら",
    "どうぞ",
    "ただいま",
    "なるほど",
    "いただきます",
    "やれやれ",
];
const FILLERS: &[&str] = &[
    "あの",
    "えーと",
    "えっと",
    "うーん",
    "まあ",
    "ええと",
    "あのー",
    "えー",
    "なんか",
    "そうですね",
];
/// Katakana of 7+ characters: search mode penalises them with the "other" penalty.
const KATAKANA_LONG: &[&str] = &[
    "シニアソフトウェアエンジニア",
    "システムアドミニストレーター",
    "ソフトウェアエンジニア",
    "コンピューターサイエンス",
    "インターナショナルスクール",
    "アプリケーションプログラミングインターフェース",
    "ストリートチルドレン",
    "アマチュアカメラマン",
    "サンシャインシティプリンスホテル",
    "インフォームド・コンセント",
    "プレアデスセンター",
    "エレクトロニクス",
];
const KATAKANA_SHORT: &[&str] = &[
    "コーヒー",
    "テレビ",
    "パソコン",
    "ビール",
    "サッカー",
    "バス",
    "タクシー",
    "ホテル",
    "レストラン",
    "カタカナ",
    "ヴィッツ",
    "ソフトウェア",
    "エンジニア",
    "システム",
    "アプリケーション",
    "プログラミング",
    "インターフェース",
    "データベース",
    "ゼロ",
    "ヌ",
];
/// End in the prolonged sound mark: stemmed when 4+ characters and all katakana.
const KATAKANA_STEM: &[&str] = &[
    "サーバー",
    "コピー",
    "パーティー",
    "コンピューター",
    "エレベーター",
    "ユーザー",
    "センター",
    "メンバー",
    "スーパー",
    "パーサー",
    "ルーター",
    "プリンター",
    "ファー",
    "カー",
    "キー",
    "ー",
    "ーー",
    "ーーー",
    "サーバーー",
    "ｻｰﾊﾞｰ",
    "サーバｰ",
];
const SMALL_KANA: &[&str] = &[
    "ッ",
    "ャ",
    "ュ",
    "ョ",
    "ァ",
    "ィ",
    "ゥ",
    "ェ",
    "ォ",
    "ヮ",
    "ヵ",
    "ヶ",
    "ぁ",
    "ぃ",
    "ぅ",
    "ぇ",
    "ぉ",
    "っ",
    "ゃ",
    "ゅ",
    "ょ",
    "ゎ",
    "ゕ",
    "ゖ",
    "ㇷ゚",
    "ㇰ",
    "ㇱ",
    "ㇻ",
    "ㇿ",
    "ヷ",
    "ヸ",
    "ヹ",
    "ヺ",
    "ゔ",
    "ゐ",
    "ゑ",
    "ゟ",
    "ヿ",
    "゛",
    "゜",
    "\u{3099}",
    "\u{309a}",
    "ッッッ",
    "ャャ",
    "ぁぁぁ",
];
/// Half-width katakana, with voiced marks in combinable and orphan positions.
const HALFWIDTH: &[&str] = &[
    "ﾊﾟﾅｿﾆｯｸ",
    "ｳﾞｨｯﾂ",
    "ﾞ",
    "ﾟ",
    "ｶﾀｶﾅ",
    "ｱｲｳｴｵ",
    "ｰ",
    "･",
    "ｦ",
    "ﾝ",
    "ﾞﾞ",
    "ｶﾞﾞ",
    "ｳﾟ",
    "ｱﾞ",
    "ﾜﾞ",
    "ｺﾝﾋﾟｭｰﾀｰ",
    "ｻｰﾊﾞｰ",
    "ﾃﾞｰﾀ",
    "ｬ",
    "ｭ",
    "ｮ",
    "ｯ",
    "ﾞﾟ",
    "ｶﾞ",
    "ｷﾞ",
    "ﾊﾞ",
    "ﾊﾟ",
    "ﾎﾟ",
    "ﾋﾟ",
    "ﾍﾟ",
    "ﾌﾟ",
    "ﾌﾞ",
    "ﾝﾞ",
    "ｰﾞ",
    "ｦﾞ",
    "aﾞ",
    "1ﾟ",
    "ヽﾞ",
    "カﾞ",
    "ﾞ漢",
    "ﾟ ",
    "ｶﾞｷﾞｸﾞｹﾞｺﾞ",
    "ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟ",
    "ﾟﾟﾟ",
];
const FULLWIDTH: &[&str] = &[
    "Ｔｅｓｔ",
    "１２３４",
    "Ｃｕｌｔｕｒｅ ｏｆ Ｊａｐａｎ",
    "ＡＢＣ",
    "ａｂｃ",
    "！",
    "？",
    "（）",
    "＃",
    "＠",
    "～",
    "￥",
    "＄",
    "％",
    "＆",
    "＊",
    "＋",
    "－",
    "＝",
    "［］",
    "｛｝",
    "＜＞",
    "／",
    "＼",
    "＾",
    "＿",
    "｀",
    "｜",
    "Ｌｕｃｅｎｅ",
    "Ｋｕｒｏｍｏｊｉ",
    "Ｈｅｌｌｏ　Ｗｏｒｌｄ",
    "ＡＢＣ１２３",
    "ａＢｃ",
    "３．１４",
    "１，０００",
    "ｘ＝ｙ",
];
const HIRAGANA_PHRASES: &[&str] = &[
    "ありがとうございます",
    "よろしくおねがいします",
    "そうですね",
    "なんだかんだ",
    "こんにちはせかい",
    "わたしはがくせいです",
    "きょうはいいてんきですね",
    "たべものがおいしい",
    "ぐるぐる",
    "ふわふわ",
    "どきどき",
    "こころ",
    "ひらがな",
    "あいうえお",
    "かきくけこ",
    "さしすせそ",
    "ん",
    "っ",
    "ゔぁ",
    "をを",
];
/// Iteration marks in legal and illegal positions.
const ITERATION: &[&str] = &[
    "時々",
    "馬鹿々々しい",
    "ところゞゝゝ",
    "みすゞ",
    "々",
    "ゝ",
    "ゞ",
    "ヽ",
    "ヾ",
    "々々々々",
    "ゝゝゝゝ",
    "ヽヽヽ",
    "ゞゞ",
    "佐々木",
    "代々木",
    "人々",
    "日々",
    "我々",
    "山々",
    "数々",
    "いすゞ",
    "こゝろ",
    "学問のすゝめ",
    "ますゝ",
    "ばゝ",
    "はゞ",
    "カヽ",
    "ガヽ",
    "ハヾ",
    "〻",
    "。ゝ",
    "。々",
    "。ヾ",
    "😀ヾ",
    "😀々",
    "👍ゝ",
    "ーゝ",
    "々時",
    "ゝと",
    "時々々",
    "時々ゝ",
    "々々々々々々々々々々々々",
    "ゝゞゝゞゝゞ",
    "ヽヾヽヾ",
    "家々々々",
    "ところゞゝゝゞ",
    "民主々々義",
    "各々",
    "屡々",
    "偶々",
    "其々",
    "ひゞ",
    "ぶゝ",
    "んゝ",
    "ぁゝ",
    "ゝゝ。ゝゝ",
    "アヽ",
    "ヴヽ",
    "ワヾ",
    "ンヾ",
    "あゞあゝ",
    "々 々",
    "々\u{3000}々",
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
    "kuromoji",
    "Japanese",
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
    "Tōkyō",
    "Ōsaka",
    "x",
    "A4",
    "G",
    "2PM",
    "e\u{301}",
    "a\u{301}b",
    "ABC123",
    "abc_def",
    "foo-bar",
    "user@example.com",
    "http://example.com/path?q=1",
    "C:\\Windows",
];
const OTHER_SCRIPTS: &[&str] = &[
    "Ба̀лтичко",
    "мо̑ре",
    "εἰμί",
    "ΣΊΣΥΦΟΣ",
    "Привет",
    "мир",
    "Ελληνικά",
    "Москва",
    "Ωμέγα",
    "αβγ",
    "ΑΒΓ",
    "АБВ",
    "абв",
    "한국어",
    "안녕하세요",
    "서울",
    "김치",
    "ㅋㅋ",
    "\u{1100}\u{1161}",
    "가",
    "กขค",
    "สวัสดี",
    "العربية",
    "שלום",
    "नमस्ते",
    "ไทย",
    "Ⴀⴀ",
    "ܐܒܓ",
    "Ꙁꙁ",
];
const SYMBOLS: &[&str] = &[
    "★", "☆", "→", "∑", "©", "™", "№", "℃", "㎞", "㈱", "㊙", "∞", "≠", "±", "§", "¶", "¿", "¡",
    "«", "»", "‰", "′", "※", "〒", "〓", "￥", "€", "¥", "$", "£", "①", "②", "Ⅻ", "ⅻ", "½", "²",
    "³", "○", "●", "◎", "△", "▲", "□", "■", "◇", "◆", "♪", "♥", "☎", "〆", "〇", "〃", "〠", "⌘",
    "⁰", "⁹", "₀", "⑩", "⑳", "㉑", "㋐", "㍻", "㍼", "㍽", "㍾", "㌔", "㌢", "㎝", "㎏", "℡", "℻",
    "⇒", "⇔", "∀", "∂", "∃", "∇", "∈", "∋", "√", "∝", "∠", "⊥", "⌒", "≒", "≡", "∬", "\u{fffd}",
    "\u{fffe}", "\u{fdd0}", "\u{fff0}", "\u{feff}", "\u{e000}", "\u{f8ff}",
];
const PUNCT: &[&str] = &[
    "。",
    "、",
    "！",
    "？",
    "「",
    "」",
    "『",
    "』",
    "（",
    "）",
    "【",
    "】",
    "・",
    "…",
    "ー",
    "〜",
    "～",
    "〔",
    "〕",
    "《",
    "》",
    "〈",
    "〉",
    "―",
    "‐",
    "−",
    "‥",
    "：",
    "；",
    "，",
    "．",
    "゛",
    "゜",
    "・・・",
    "。。。",
    "ーー",
    "〜〜",
    "「」",
    "『』",
    "（）",
    "【】",
    ".",
    ",",
    "!",
    "?",
    ";",
    ":",
    "'",
    "\"",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    "/",
    "-",
    "_",
    "+",
    "=",
    "<",
    ">",
    "@",
    "#",
    "$",
    "%",
    "^",
    "&",
    "*",
    "|",
    "\\",
    "`",
    "~",
    "...",
    "!!!",
    "???",
    "!?",
    "^^",
    "~~",
    "--",
    "\u{2018}",
    "\u{2019}",
    "\u{201c}",
    "\u{201d}",
    "｡",
    "｢",
    "｣",
    "､",
    "･",
];
const WHITESPACE: &[&str] = &[
    " ", " ", " ", " ", " ", "  ", "   ", "        ", "\t", "\n", "\r\n", "\r", "\u{3000}",
    "\u{3000}", "\u{a0}", "\u{2003}", "\u{2009}", "\u{200b}", "\u{b}", "\u{c}", "\u{85}",
    "\u{2028}", "\u{2029}", "\u{1680}",
];
const FORMAT_AND_MARKS: &[&str] = &[
    "\u{200d}",
    "\u{200c}",
    "\u{200b}",
    "\u{200e}",
    "\u{feff}",
    "\u{ad}",
    "\u{2060}",
    "\u{202a}",
    "\u{202c}",
    "\u{2062}",
    "\u{301}",
    "\u{302}",
    "\u{308}",
    "\u{20d0}",
    "\u{3099}",
    "\u{309a}",
    "\u{302f}",
    "\u{3029}",
    "\u{3039}",
    "\u{0}",
    "\u{1}",
    "\u{7f}",
    "\u{85}",
    "\u{9f}",
    "\u{d0}",
    "\u{fe0f}",
    "\u{e0100}",
];
const EMOJI: &[&str] = &[
    "😀",
    "😂",
    "👍",
    "👍🏽",
    "🇯🇵",
    "👨\u{200d}👩\u{200d}👧",
    "🀄",
    "𝐀",
    "𝐁",
    "𠀀",
    "𠀁",
    "𡈽",
    "😀😀",
    "🎉",
    "❤",
    "✅",
    "🍣",
    "🗾",
    "🎌",
    "\u{1f1ef}",
    "\u{fdd0}😀",
    "\u{fdd0}😀😀😀",
    "🗻🍜🍱",
    "\u{10000}",
    "\u{10ffff}",
    "\u{e0001}",
];

/// First and last code point of every `char.def` range, and the code point just after each.
const BOUNDARY: &[u32] = &[
    0x9, 0xa, 0xb, 0xc, 0x20, 0x21, 0x2f, 0x30, 0x39, 0x3a, 0x40, 0x41, 0x5a, 0x5b, 0x60, 0x61,
    0x7a, 0x7b, 0x7e, 0x7f, 0xa1, 0xbf, 0xc0, 0xd0, 0xd1, 0xff, 0x100, 0x17f, 0x180, 0x236, 0x237,
    0x374, 0x3fb, 0x3fc, 0x400, 0x4f9, 0x4fa, 0x500, 0x50f, 0x510, 0x1e00, 0x1ef9, 0x1efa, 0x2000,
    0x206f, 0x2070, 0x209f, 0x20a0, 0x20cf, 0x20d0, 0x20ff, 0x2100, 0x214b, 0x214c, 0x214f, 0x2150,
    0x218f, 0x2190, 0x21ff, 0x2200, 0x22ff, 0x2300, 0x23ff, 0x2400, 0x2460, 0x24ff, 0x2500, 0x2501,
    0x257f, 0x2580, 0x259f, 0x25a0, 0x25ff, 0x2600, 0x26fe, 0x26ff, 0x2700, 0x27bf, 0x27c0, 0x27ef,
    0x27f0, 0x27ff, 0x2800, 0x28ff, 0x2900, 0x297f, 0x2980, 0x2a00, 0x2aff, 0x2b00, 0x2bff, 0x2c00,
    0x2e80, 0x2ef3, 0x2ef4, 0x2f00, 0x2fd5, 0x2fd6, 0x3000, 0x3005, 0x3006, 0x3007, 0x3008, 0x303f,
    0x3040, 0x3041, 0x309f, 0x30a0, 0x30a1, 0x30fc, 0x30fd, 0x30ff, 0x3100, 0x31f0, 0x31ff, 0x3200,
    0x32fe, 0x32ff, 0x3300, 0x33ff, 0x3400, 0x4db5, 0x4db6, 0x4e00, 0x4e01, 0x4e03, 0x4e04, 0x4e07,
    0x4e08, 0x4e09, 0x4e0a, 0x4e5d, 0x4e5e, 0x4e8c, 0x4e8d, 0x4e94, 0x4e95, 0x5104, 0x5105, 0x5146,
    0x5147, 0x516b, 0x516c, 0x516d, 0x516e, 0x5341, 0x5342, 0x5343, 0x5344, 0x56db, 0x56dc, 0x767e,
    0x767f, 0x9fa5, 0x9fa6, 0xf900, 0xfa2d, 0xfa2e, 0xfa30, 0xfa6a, 0xfa6b, 0xfe30, 0xfe4f, 0xfe50,
    0xfe6b, 0xfe6c, 0xff01, 0xff0f, 0xff10, 0xff19, 0xff1a, 0xff1f, 0xff20, 0xff21, 0xff3a, 0xff3b,
    0xff40, 0xff41, 0xff5a, 0xff5b, 0xff65, 0xff66, 0xff9d, 0xff9e, 0xff9f, 0xffa0, 0xffe0, 0xffef,
    0xfff0,
];

/// A code point range of one `char.def` class (or a block outside every class, i.e. DEFAULT).
const CLASS_RANGES: &[(u32, u32)] = &[
    (0x4E00, 0x9FA5),
    (0x3400, 0x4DB5),
    (0xF900, 0xFA2D),
    (0xFA30, 0xFA6A),
    (0x2E80, 0x2EF3),
    (0x2F00, 0x2FD5),
    (0x3041, 0x309F),
    (0x30A1, 0x30FF),
    (0x31F0, 0x31FF),
    (0xFF66, 0xFF9F),
    (0x0400, 0x04F9),
    (0x0500, 0x050F),
    (0x0374, 0x03FB),
    (0x00C0, 0x024F),
    (0x1E00, 0x1EF9),
    (0x00A1, 0x00BF),
    (0x2000, 0x206F),
    (0x2070, 0x209F),
    (0x20A0, 0x20FF),
    (0x2100, 0x218F),
    (0x2190, 0x23FF),
    (0x2460, 0x2BFF),
    (0x3000, 0x303F),
    (0x3200, 0x33FF),
    (0xFE30, 0xFE6B),
    (0xFF01, 0xFF65),
    (0xFFE0, 0xFFEF),
    (0x0590, 0x05FF),
    (0x0600, 0x06FF),
    (0x0E00, 0x0E7F),
    (0x0900, 0x097F),
    (0x1100, 0x11FF),
    (0x3130, 0x318F),
    (0xAC00, 0xD7A3),
    (0xE000, 0xF8FF),
    (0x2400, 0x245F),
    (0x2C00, 0x2E7F),
    (0x1F300, 0x1F64F),
    (0x20000, 0x2A6DF),
    (0x1D400, 0x1D7FF),
    (0x10000, 0x100FF),
];

const KANJI_DIGITS: &[char] = &['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
const KANJI_SMALL_EXP: &[char] = &['十', '百', '千'];
const KANJI_LARGE_EXP: &[char] = &['万', '億', '兆', '京', '垓'];
const COUNTERS: &[&str] = &[
    "円",
    "人",
    "個",
    "歳",
    "時",
    "分",
    "秒",
    "年",
    "月",
    "日",
    "回",
    "台",
    "本",
    "枚",
    "匹",
    "番",
    "階",
    "％",
    "%",
    "℃",
    "㎞",
    "キロ",
    "メートル",
    "ドル",
    "つ",
    "か月",
    "年間",
    "万円",
    "億円",
    "千円",
    "時間",
    "番目",
    "位",
    "倍",
    "点",
];

fn japanese_word(rng: &mut Rng, s: &mut String) {
    match rng.below(40) {
        0..=9 => s.push_str(rng.pick(NOUNS)),
        10..=12 => s.push_str(rng.pick(PROPER)),
        13..=15 => s.push_str(rng.pick(COMPOUNDS)),
        16..=20 => s.push_str(rng.pick(VERBS)),
        21..=23 => s.push_str(rng.pick(ADJECTIVES)),
        24 => s.push_str(rng.pick(AUXILIARIES)),
        25..=26 => s.push_str(rng.pick(ADVERBS)),
        27..=28 => s.push_str(rng.pick(PRONOUNS)),
        29 => s.push_str(rng.pick(ADNOMINALS)),
        30 => s.push_str(rng.pick(CONJUNCTIONS)),
        31 => s.push_str(rng.pick(INTERJECTIONS)),
        32 => s.push_str(rng.pick(FILLERS)),
        33..=34 => s.push_str(rng.pick(KATAKANA_SHORT)),
        35 => s.push_str(rng.pick(KATAKANA_LONG)),
        36 => s.push_str(rng.pick(KATAKANA_STEM)),
        37 => s.push_str(rng.pick(HIRAGANA_PHRASES)),
        38 => {
            // random hiragana: unknown words are grouped
            for _ in 0..rng.range(1, 8) {
                s.push(rng.char_in(0x3041, 0x3096));
            }
        }
        _ => {
            // noun + noun compound, e.g. 東京大学 or 日本語検索
            s.push_str(rng.pick(NOUNS));
            s.push_str(rng.pick(NOUNS));
        }
    }
    if rng.chance(45) {
        s.push_str(rng.pick(PARTICLES));
    }
}

fn number(rng: &mut Rng, s: &mut String) {
    let fullwidth = rng.chance(25);
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
        s.push(*rng.pick(&['-', '+', '－', '＋', '−', 'ー', '＿']));
    }
    match rng.below(10) {
        0..=3 => {
            // Arabic digits with optional decimal points and thousands separators
            for i in 0..rng.range(1, 9) {
                if i > 0 && rng.chance(18) {
                    s.push(*rng.pick(&['.', ',', '．', '，']));
                }
                s.push(digit(rng));
            }
        }
        4..=6 => {
            // kanji numerals in realistic combinations: 十万二千五百, 一億二千万, 二〇二四
            if rng.chance(20) {
                s.push_str(rng.pick(&[
                    "十万二千五百",
                    "三千2百２十三",
                    "一〇〇〇",
                    "二〇二四",
                    "一億二千万",
                    "三兆",
                    "千二百三十四万五千六百七十八",
                    "一二三四五六七八九十百千万億兆京垓",
                    "万万",
                    "十十",
                    "兆京垓",
                    "〇",
                    "零",
                    "百",
                    "千",
                    "万",
                    "十",
                    "五十",
                    "三百",
                    "二千",
                    "一万",
                    "九百九十九",
                    "一.五",
                    "十.五",
                    "三．一四",
                    "一，〇〇〇",
                    "２，３４５",
                    "１．５",
                ]));
            } else {
                for i in 0..rng.range(1, 4) {
                    if i > 0 || rng.chance(50) {
                        s.push(*rng.pick(KANJI_DIGITS));
                    }
                    s.push(*rng.pick(KANJI_SMALL_EXP));
                }
                if rng.chance(40) {
                    s.push(*rng.pick(KANJI_LARGE_EXP));
                    if rng.chance(50) {
                        s.push(*rng.pick(KANJI_DIGITS));
                        s.push(*rng.pick(KANJI_SMALL_EXP));
                    }
                }
                if rng.chance(30) {
                    s.push(*rng.pick(KANJI_DIGITS));
                }
            }
        }
        7 => {
            // mixed Arabic and kanji, digits and exponents interleaved
            for _ in 0..rng.range(2, 6) {
                match rng.below(4) {
                    0 => s.push(digit(rng)),
                    1 => s.push(*rng.pick(KANJI_DIGITS)),
                    2 => s.push(*rng.pick(KANJI_SMALL_EXP)),
                    _ => s.push(*rng.pick(KANJI_LARGE_EXP)),
                }
            }
        }
        8 => {
            // years
            s.push_str(rng.pick(&[
                "2024年",
                "２０２４年",
                "令和元年",
                "令和6年",
                "平成三十年",
                "昭和64年",
                "明治元年",
                "二〇二四年",
                "1999年12月31日",
                "2000年1月1日",
                "１９４５年８月１５日",
                "平成元年",
                "西暦2024年",
                "令和二年四月一日",
            ]));
        }
        _ => {
            // malformed: dangling or repeated separators
            s.push_str(rng.pick(&[
                "1.", ".5", "1..2", "1,,2", ",", ".", "．", "，", "1,", "，1", "1.2.3", "１．",
                "．５", "3.14.15", "1,000.", "0.0.0", "100,", "１，", "〇.", ".〇", "一..二",
            ]));
        }
    }
    if rng.chance(35) {
        s.push_str(rng.pick(COUNTERS));
    }
}

fn run(rng: &mut Rng, s: &mut String, garbage: bool) {
    // Japanese-looking text draws mostly dictionary words; garbage draws uniformly.
    let roll = if garbage {
        rng.below(100)
    } else {
        rng.below(130).saturating_sub(30)
    };
    match roll {
        0..=38 => japanese_word(rng, s),
        39..=46 => number(rng, s),
        47..=50 => s.push_str(rng.pick(ITERATION)),
        51..=54 => s.push_str(rng.pick(PUNCT)),
        55..=57 => s.push_str(rng.pick(LATIN)),
        58..=60 => s.push_str(rng.pick(HALFWIDTH)),
        61..=62 => s.push_str(rng.pick(FULLWIDTH)),
        63..=64 => s.push_str(rng.pick(SMALL_KANA)),
        65..=67 => s.push_str(rng.pick(SYMBOLS)),
        68..=69 => s.push_str(rng.pick(OTHER_SCRIPTS)),
        70..=72 => s.push_str(rng.pick(EMOJI)),
        73..=74 => s.push_str(rng.pick(FORMAT_AND_MARKS)),
        75..=77 => s.push(char::from_u32(*rng.pick(BOUNDARY)).unwrap()),
        78..=80 => {
            // a random code point from one char.def class range
            let (lo, hi) = *rng.pick(CLASS_RANGES);
            for _ in 0..rng.range(1, 3) {
                s.push(rng.char_in(lo, hi));
            }
        }
        81..=83 => {
            // random katakana, possibly with prolonged sound marks
            for _ in 0..rng.range(1, 9) {
                if rng.chance(15) {
                    s.push('ー');
                } else {
                    s.push(rng.char_in(0x30A1, 0x30FA));
                }
            }
        }
        84..=85 => {
            // random kanji: mostly unknown to the dictionary
            for _ in 0..rng.range(1, 6) {
                s.push(rng.char_in(0x4E00, 0x9FA5));
            }
        }
        86..=87 => {
            // mixed kana: hiragana and katakana interleaved
            for _ in 0..rng.range(2, 8) {
                if rng.chance(50) {
                    s.push(rng.char_in(0x3041, 0x3096));
                } else {
                    s.push(rng.char_in(0x30A1, 0x30FA));
                }
            }
        }
        88..=90 => {
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
        91..=93 => {
            // a repeated character (grouping, or not, by class)
            let c = *rng.pick(&[
                '!', '。', '?', '〜', '★', 'a', '1', 'あ', 'ア', '😀', '一', 'ー', '-', '…', '々',
                'ゝ', 'ヾ', 'ッ', 'ﾞ', '　', '〇', '・', '〻',
            ]);
            for _ in 0..rng.range(2, 12) {
                s.push(c);
            }
        }
        94..=95 => {
            // a kanji compound: hit search mode's penalty with and without dictionary entries
            s.push_str(rng.pick(COMPOUNDS));
            if rng.chance(50) {
                s.push_str(rng.pick(NOUNS));
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

/// The rare long inputs: runs crossing the 1024-unit unknown-word cap and the 1024-position
/// forced backtrace.
fn long_run(rng: &mut Rng, s: &mut String) {
    match rng.below(5) {
        0 => {
            // over 1024 code units of one grouped class without a dictionary word
            let n = rng.range(1030, 1400);
            match rng.below(3) {
                0 => {
                    let c = *rng.pick(&['★', '〜', '→', '…', '＃', '§', '〓']);
                    for _ in 0..n {
                        s.push(c);
                    }
                }
                1 => {
                    let c = *rng.pick(&['ヸ', 'ヹ', 'ヺ', 'ヷ']);
                    for _ in 0..n {
                        s.push(c);
                    }
                }
                _ => {
                    for _ in 0..n {
                        s.push(rng.char_in(0x30A1, 0x30FA));
                    }
                }
            }
        }
        1 => {
            // a run of emoji with an odd number of BMP DEFAULT-class characters in front, so the
            // 1024-code-unit cut lands inside a surrogate pair
            for _ in 0..*rng.pick(&[1usize, 3, 5]) {
                s.push(*rng.pick(&['가', '나', 'ก', 'ا', 'א']));
            }
            let e = *rng.pick(&['😀', '🍣', '👍', '🎌']);
            for _ in 0..rng.range(520, 760) {
                s.push(if rng.chance(90) { e } else { '😂' });
            }
        }
        2 => {
            // 1500+ characters of hiragana and kanji with no spaces: forces the 1024-position
            // backtrace in the middle of running text
            let target = rng.range(1500, 2200);
            while s.chars().count() < target {
                match rng.below(8) {
                    0..=4 => japanese_word(rng, s),
                    5 => s.push_str(rng.pick(HIRAGANA_PHRASES)),
                    6 => s.push_str(rng.pick(ITERATION)),
                    _ => {
                        for _ in 0..rng.range(1, 6) {
                            s.push(rng.char_in(0x3041, 0x3096));
                        }
                    }
                }
            }
        }
        3 => {
            // 1023 × あ + 手紙: the unknown word fills the cap exactly, then a known word
            for _ in 0..1023 {
                s.push('あ');
            }
            s.push_str("手紙");
            if rng.chance(50) {
                s.push_str(rng.pick(&["を書いた", "。", "です", "ー", "々"]));
            }
        }
        _ => {
            // a long generic mix
            let target = rng.range(800, 2000);
            let garbage = rng.chance(30);
            while s.chars().count() < target {
                run(rng, s, garbage);
                if rng.chance(20) {
                    s.push_str(rng.pick(WHITESPACE));
                }
            }
        }
    }
}

fn generate(rng: &mut Rng) -> String {
    let mut s = String::new();
    // Length classes: mostly 20 to 300 characters, a few short, 3% long enough to cross the
    // 1024 limits. (The golden files grow with the input, so long inputs are rare.)
    let target = match rng.below(100) {
        0..=1 => 0,
        2..=9 => rng.range(1, 20),
        10..=66 => rng.range(20, 120),
        67..=96 => rng.range(120, 300),
        _ => {
            long_run(rng, &mut s);
            return s;
        }
    };
    if target == 0 {
        if rng.chance(50) {
            s.push_str(rng.pick(WHITESPACE));
        }
        return s;
    }
    let garbage = rng.chance(25);
    // 0: no spaces at all, 1: spaces everywhere, 2..=3: rare (Japanese text), else: mixed
    let space_style = rng.below(8);
    while s.chars().count() < target {
        run(rng, &mut s, garbage);
        let want_space = match space_style {
            0 => false,
            1 => true,
            2 | 3 => rng.chance(10),
            _ => rng.chance(45),
        };
        if want_space {
            s.push_str(rng.pick(WHITESPACE));
        } else if !garbage && rng.chance(12) {
            s.push_str(rng.pick(&["。", "、", "！", "？", "…", "・"]));
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
