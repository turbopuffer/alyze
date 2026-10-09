//! Part-of-speech tags and token types of mecab-ko-dic, as Lucene's `POS` class exposes them.

use std::fmt;

/// How a dictionary entry decomposes (Lucene's `POS.Type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// A simple morpheme.
    Morpheme,
    /// A compound noun; decomposes into nouns whose surface forms tile the token.
    Compound,
    /// An inflected form; decomposes into morphemes whose surface forms differ from the token.
    Inflect,
    /// A pre-analysed entry; decomposes like a compound.
    Preanalysis,
}

impl Type {
    pub fn name(self) -> &'static str {
        match self {
            Type::Morpheme => "MORPHEME",
            Type::Compound => "COMPOUND",
            Type::Inflect => "INFLECT",
            Type::Preanalysis => "PREANALYSIS",
        }
    }

    /// Lucene's `POS.resolveType`: `"*"` is a plain morpheme, otherwise the name, case-insensitively.
    pub fn from_name(name: &str) -> Option<Type> {
        match name.to_ascii_uppercase().as_str() {
            "*" | "MORPHEME" => Some(Type::Morpheme),
            "COMPOUND" => Some(Type::Compound),
            "INFLECT" => Some(Type::Inflect),
            "PREANALYSIS" => Some(Type::Preanalysis),
            _ => None,
        }
    }
}

macro_rules! tags {
    ($( $variant:ident = ($code:expr, $desc:literal), )*) => {
        /// A part-of-speech tag (Sejong corpus tag set, as in mecab-ko-dic's `pos-id.def`).
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum Tag {
            $( #[doc = $desc] $variant, )*
        }

        impl Tag {
            /// Every tag, in Lucene's declaration order (which is also the dictionary's byte
            /// encoding of a tag).
            pub const ALL: &'static [Tag] = &[$( Tag::$variant, )*];

            pub fn name(self) -> &'static str {
                match self { $( Tag::$variant => stringify!($variant), )* }
            }

            /// The code `pos-id.def` gives the tag (-1 for tags it doesn't list).
            pub fn code(self) -> i32 {
                match self { $( Tag::$variant => $code, )* }
            }

            pub fn description(self) -> &'static str {
                match self { $( Tag::$variant => $desc, )* }
            }

            /// Lucene's `POS.resolveTag(String)`: the name, case-insensitively.
            pub fn from_name(name: &str) -> Option<Tag> {
                match name.to_ascii_uppercase().as_str() {
                    $( stringify!($variant) => Some(Tag::$variant), )*
                    _ => None,
                }
            }
        }
    };
}

tags! {
    EP = (100, "Pre-final ending"),
    EF = (101, "Sentence-closing ending"),
    EC = (102, "Connective ending"),
    ETN = (103, "Nominal transformative ending"),
    ETM = (104, "Adnominal form transformative ending"),
    IC = (110, "Interjection"),
    JKS = (120, "Subject case marker"),
    JKC = (121, "Complement case marker"),
    JKG = (122, "Adnominal case marker"),
    JKO = (123, "Object case marker"),
    JKB = (124, "Adverbial case marker"),
    JKV = (125, "Vocative case marker"),
    JKQ = (126, "Quotative case marker"),
    JX = (127, "Auxiliary postpositional particle"),
    JC = (128, "Conjunctive postpositional particle"),
    MAG = (130, "General Adverb"),
    MAJ = (131, "Conjunctive adverb"),
    MM = (140, "Modifier"),
    NNG = (150, "General Noun"),
    NNP = (151, "Proper Noun"),
    NNB = (152, "Dependent noun"),
    NNBC = (153, "Dependent noun"),
    NP = (154, "Pronoun"),
    NR = (155, "Numeral"),
    SF = (160, "Terminal punctuation"),
    SH = (161, "Chinese Character"),
    SL = (162, "Foreign language"),
    SN = (163, "Number"),
    SP = (164, "Space"),
    SSC = (165, "Closing brackets"),
    SSO = (166, "Opening brackets"),
    SC = (167, "Separator"),
    SY = (168, "Other symbol"),
    SE = (169, "Ellipsis"),
    VA = (170, "Adjective"),
    VCN = (171, "Negative designator"),
    VCP = (172, "Positive designator"),
    VV = (173, "Verb"),
    VX = (174, "Auxiliary Verb or Adjective"),
    XPN = (181, "Prefix"),
    XR = (182, "Root"),
    XSA = (183, "Adjective Suffix"),
    XSN = (184, "Noun Suffix"),
    XSV = (185, "Verb Suffix"),
    UNKNOWN = (999, "Unknown"),
    UNA = (-1, "Unknown"),
    NA = (-1, "Unknown"),
    VSV = (-1, "Unknown"),
}

impl Tag {
    /// The tag with this index in [`Tag::ALL`] (the dictionary's byte encoding).
    pub(crate) fn from_index(index: u8) -> Option<Tag> {
        Tag::ALL.get(usize::from(index)).copied()
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A set of tags, e.g. the tags the part-of-speech stop filter removes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TagSet(u64);

impl TagSet {
    pub const EMPTY: TagSet = TagSet(0);

    /// Lucene's `KoreanPartOfSpeechStopFilter.DEFAULT_STOP_TAGS`, which Elasticsearch's `nori`
    /// analyzer and `nori_part_of_speech` filter use unless `stoptags` is given.
    pub const DEFAULT_STOP_TAGS: TagSet = TagSet::from_tags(&[
        Tag::EP,
        Tag::EF,
        Tag::EC,
        Tag::ETN,
        Tag::ETM,
        Tag::IC,
        Tag::JKS,
        Tag::JKC,
        Tag::JKG,
        Tag::JKO,
        Tag::JKB,
        Tag::JKV,
        Tag::JKQ,
        Tag::JX,
        Tag::JC,
        Tag::MAG,
        Tag::MAJ,
        Tag::MM,
        Tag::SP,
        Tag::SSC,
        Tag::SSO,
        Tag::SC,
        Tag::SE,
        Tag::XPN,
        Tag::XSA,
        Tag::XSN,
        Tag::XSV,
        Tag::UNA,
        Tag::NA,
        Tag::VSV,
    ]);

    pub const fn from_tags(tags: &[Tag]) -> TagSet {
        let mut bits = 0u64;
        let mut i = 0;
        while i < tags.len() {
            bits |= 1 << (tags[i] as u8);
            i += 1;
        }
        TagSet(bits)
    }

    /// Parses a comma- and/or whitespace-separated list of tag names, case-insensitively, the way
    /// Elasticsearch reads `stoptags` (`"NR, SP"`). An empty list is the empty set.
    pub fn parse(spec: &str) -> Result<TagSet, UnknownTag> {
        let mut set = TagSet::EMPTY;
        for name in spec.split([',', ' ', '\t', '\n']).filter(|s| !s.is_empty()) {
            set.insert(Tag::from_name(name).ok_or_else(|| UnknownTag(name.to_owned()))?);
        }
        Ok(set)
    }

    pub fn contains(self, tag: Tag) -> bool {
        self.0 & (1 << (tag as u8)) != 0
    }

    pub fn insert(&mut self, tag: Tag) {
        self.0 |= 1 << (tag as u8);
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Tag> {
        Tag::ALL.iter().copied().filter(move |&t| self.contains(t))
    }
}

/// A tag name that isn't one of [`Tag`]'s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownTag(pub String);

impl fmt::Display for UnknownTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown part-of-speech tag {:?}", self.0)
    }
}

impl std::error::Error for UnknownTag {}
