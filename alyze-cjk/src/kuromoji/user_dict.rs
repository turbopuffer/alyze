//! User dictionaries: Lucene's `UserDictionary` (Elasticsearch's `user_dictionary` /
//! `user_dictionary_rules`), with the duplicate detection Elasticsearch layers on top.

use std::fmt;

/// A user dictionary: rules in Lucene's CSV format, one per line:
///
/// ```text
/// <surface>,<segment 1> ... <segment n>,<reading 1> ... <reading n>,<part of speech>
/// ```
///
/// An input matching `<surface>` is tokenized into the space-separated segments, each carrying
/// its reading and the rule's part of speech. User words beat every dictionary and unknown word
/// (Lucene gives them cost -100000 and connection ids 5). A rule whose segmentation is the whole
/// surface form yields a single token.
///
/// Parsing follows Lucene's `UserDictionary.open` and `CSVUtil` exactly (a `#` at the start of a
/// line is a comment, whitespace-only lines are skipped, fields may be `"`-quoted with `""`
/// escapes, segments and readings are split on runs of ASCII spaces, the surface and the joined
/// segmentation must agree with all `\s` removed), plus Elasticsearch's rules for a dictionary
/// file: lines are trimmed and blank ones dropped before anything else, and a surface form that
/// repeats is an error (`lenient` keeps the first rule instead, with Lucene-style silence).
pub struct UserDictionary {}

impl fmt::Debug for UserDictionary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserDictionary").finish_non_exhaustive()
    }
}

impl UserDictionary {
    pub fn parse(rules: &str, lenient: bool) -> Result<UserDictionary, UserDictionaryError> {
        let _ = (rules, lenient);
        todo!()
    }

    /// Whether no rule survived parsing (Lucene then uses no user dictionary at all).
    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// Every rule as Lucene stores it, in term-index (UTF-16) order of the raw surface field.
    #[cfg(test)]
    pub(crate) fn entries(&self) -> Vec<UserEntry> {
        todo!()
    }
}

/// A parsed rule, as the tests see it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UserEntry {
    /// The raw first field, which is the term-index key (Lucene keeps its inner whitespace).
    pub surface: String,
    /// The segments, each the length Lucene cuts the match into.
    pub segments: Vec<String>,
    pub readings: Vec<String>,
    pub part_of_speech: String,
}

/// Why a rules file was rejected. `line` is 1-based and counts the non-blank lines of the input
/// (as Elasticsearch counts them for a dictionary file), including comments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserDictionaryError {
    /// A surface form that an earlier rule already defined (Elasticsearch, unless lenient).
    Duplicate { surface: String, line: usize },
    /// Fewer than four fields, or an odd number of `"` (Lucene throws an index error).
    Malformed { rule: String, line: usize },
    /// The number of segments differs from the number of readings.
    SegmentationReadingsMismatch { rule: String, line: usize },
    /// The segments don't join back into the surface form.
    SegmentationMismatch { rule: String, line: usize },
    /// A segment boundary falls inside a surrogate pair (Lucene would emit half characters; the
    /// port rejects the rule instead).
    SegmentationSplitsCharacter { rule: String, line: usize },
}

impl fmt::Display for UserDictionaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UserDictionaryError::Duplicate { surface, line } => write!(
                f,
                "Found duplicate term [{surface}] in user dictionary at line [{line}]"
            ),
            UserDictionaryError::Malformed { rule, line } => {
                write!(f, "Malformed user dictionary entry {rule} at line [{line}]")
            }
            UserDictionaryError::SegmentationReadingsMismatch { rule, line } => write!(
                f,
                "Illegal user dictionary entry {rule} at line [{line}] - the number of \
                 segmentations does not match the number of readings"
            ),
            UserDictionaryError::SegmentationMismatch { rule, line } => write!(
                f,
                "Illegal user dictionary entry {rule} at line [{line}] - the concatenated \
                 segmentation does not match the surface form"
            ),
            UserDictionaryError::SegmentationSplitsCharacter { rule, line } => write!(
                f,
                "Illegal user dictionary entry {rule} at line [{line}] - a segment boundary \
                 falls inside a surrogate pair"
            ),
        }
    }
}

impl std::error::Error for UserDictionaryError {}
