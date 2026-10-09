//! User dictionaries (Elasticsearch's `user_dictionary` / `user_dictionary_rules`): extra nouns,
//! optionally with a segmentation into sub-nouns, that win over system-dictionary words.

use std::fmt;

/// A user dictionary, built from rules of the form `surface` or `surface part part ...`
/// (whitespace-separated; `#` starts a comment). Every entry is a general noun (NNG) with
/// Lucene's fixed cost and connection ids; entries with a segmentation are compounds that the
/// decompound modes decompose into consecutive slices of the surface form with the parts'
/// lengths (Lucene only keeps the lengths, so `서울특별시 서울 시` decomposes into 서울 + 특).
///
/// Two Lucene quirks are reproduced: the right connection id is chosen by the last character of
/// the whole rule after comment removal (a part's last character, or even a trailing space), and
/// `hasCoda` is applied to any HANGUL-class character, jamo included.
#[derive(Clone, Debug)]
pub struct UserDictionary {
    _private: (),
}

impl UserDictionary {
    /// Parses rules, one per line. A surface form that appears twice is an error unless
    /// `lenient`, in which case the first rule wins (Lucene's behaviour; Elasticsearch 8.13+
    /// rejects duplicates unless its `lenient` setting is on). A segmentation longer than the
    /// surface form is always an error.
    pub fn parse(rules: &str, lenient: bool) -> Result<UserDictionary, UserDictionaryError> {
        let _ = (rules, lenient);
        todo!("nori user dictionary")
    }

    /// True when no rule survived parsing (only comments and blank lines).
    pub fn is_empty(&self) -> bool {
        todo!("nori user dictionary")
    }

    /// Every entry, in the order of the lookup structure (surface forms sorted by UTF-16 code
    /// units), for tests.
    #[cfg(test)]
    pub(crate) fn entries(&self) -> Vec<UserEntry> {
        todo!("nori user dictionary")
    }
}

/// One user-dictionary entry, as [`UserDictionary::entries`] reports it.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UserEntry {
    pub surface: String,
    /// Lucene's right connection id for the entry: depends on whether the rule's last character
    /// is a Hangul syllable and whether that syllable has a final consonant.
    pub right_id: u16,
    /// The decomposition (consecutive slices of the surface form with the rule's part lengths),
    /// or `None` for a simple noun.
    pub segmentation: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserDictionaryError {
    /// The same surface form appears on two rules (1-based line numbers of the second).
    Duplicate { surface: String, line: usize },
    /// A rule's segmentation is longer than its surface form.
    SegmentationTooLong { rule: String, line: usize },
}

impl fmt::Display for UserDictionaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UserDictionaryError::Duplicate { surface, line } => {
                write!(
                    f,
                    "duplicate term [{surface}] in user dictionary at line [{line}]"
                )
            }
            UserDictionaryError::SegmentationTooLong { rule, line } => write!(
                f,
                "illegal user dictionary entry [{rule}] at line [{line}]: the segmentation is bigger than the surface form"
            ),
        }
    }
}

impl std::error::Error for UserDictionaryError {}
