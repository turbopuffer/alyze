//! User dictionaries (Elasticsearch's `user_dictionary` / `user_dictionary_rules`): extra nouns,
//! optionally with a segmentation into sub-nouns, that win over system-dictionary words.
//! A port of Lucene's `UserDictionary` / `UserMorphData`.

use std::fmt;

use super::char_def;
use super::dict::TermIndex;

/// Lucene's fixed left connection id for user words (`NNG,상태변화` in `left-id.def`).
pub(crate) const LEFT_ID: u16 = 1781;
/// Lucene's fixed cost for user words: strongly preferred over anything else.
pub(crate) const WORD_COST: i32 = -100000;
/// Right id for a rule whose last character is not Hangul.
const RIGHT_ID: u16 = 3533;
/// Right id for a rule ending in a Hangul character with a final consonant.
const RIGHT_ID_T: u16 = 3535;
/// Right id for a rule ending in a Hangul character without one.
const RIGHT_ID_F: u16 = 3534;

/// A user dictionary, built from rules of the form `surface` or `surface part part ...`
/// (whitespace-separated; `#` starts a comment). Every entry is a general noun (NNG) with
/// Lucene's fixed cost and connection ids; entries with a segmentation are compounds that the
/// decompound modes decompose into consecutive slices of the surface form with the parts'
/// lengths (Lucene only keeps the lengths, so `서울특별시 서울 시` decomposes into 서울 + 특).
///
/// Two Lucene quirks are reproduced: the right connection id is chosen by the last character of
/// the whole rule after comment removal (a part's last character, or even a trailing space), and
/// `hasCoda` is applied to any HANGUL-class character, jamo included.
pub struct UserDictionary {
    terms: TermIndex<Vec<u8>>,
    /// Per entry (ordinal), in term order.
    entries: Vec<Entry>,
}

#[derive(Clone, Debug)]
struct Entry {
    surface: Vec<u16>,
    right_id: u16,
    /// Lengths of the segmentation's parts, or empty for a simple noun.
    segmentation: Vec<u16>,
}

impl fmt::Debug for UserDictionary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserDictionary")
            .field("entries", &self.entries.len())
            .finish()
    }
}

impl UserDictionary {
    /// Parses rules, one per line. A surface form that appears twice is an error unless
    /// `lenient`, in which case the first rule wins (Lucene's behaviour; Elasticsearch 8.13+
    /// rejects duplicates unless its `lenient` setting is on). A segmentation longer than the
    /// surface form is always an error.
    pub fn parse(rules: &str, lenient: bool) -> Result<UserDictionary, UserDictionaryError> {
        // (line number, rule without its comment)
        let mut lines: Vec<(usize, &str)> = Vec::new();
        for (i, line) in rules.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("");
            if line.trim().is_empty() {
                continue;
            }
            lines.push((i + 1, line));
        }
        // Lucene sorts by the first token (stable, so file order decides among equal surfaces)
        // and skips a rule whose surface equals the previous one.
        fn surface_of(rule: &str) -> &str {
            rule.split_whitespace().next().unwrap_or("")
        }
        lines.sort_by(|a, b| {
            let (sa, sb) = (surface_of(a.1), surface_of(b.1));
            sa.encode_utf16().cmp(sb.encode_utf16())
        });
        let mut entries: Vec<Entry> = Vec::with_capacity(lines.len());
        let mut last_surface: Option<&str> = None;
        for &(line_number, rule) in &lines {
            let mut parts = rule.split_whitespace();
            let surface = parts.next().unwrap();
            if last_surface == Some(surface) {
                if lenient {
                    continue;
                }
                return Err(UserDictionaryError::Duplicate {
                    surface: surface.to_owned(),
                    line: line_number,
                });
            }
            last_surface = Some(surface);
            // The rule's last character (after comment removal, so possibly whitespace).
            let last = rule.encode_utf16().last().unwrap();
            let right_id = if char_def::class(last) == char_def::CharClass::Hangul {
                if char_def::has_coda(last) {
                    RIGHT_ID_T
                } else {
                    RIGHT_ID_F
                }
            } else {
                RIGHT_ID
            };
            let surface_units: Vec<u16> = surface.encode_utf16().collect();
            let segmentation: Vec<u16> = parts
                .map(|p| u16::try_from(p.encode_utf16().count()).unwrap_or(u16::MAX))
                .collect();
            let total: usize = segmentation.iter().map(|&l| usize::from(l)).sum();
            if total > surface_units.len() {
                return Err(UserDictionaryError::SegmentationTooLong {
                    rule: rule.trim().to_owned(),
                    line: line_number,
                });
            }
            entries.push(Entry {
                surface: surface_units,
                right_id,
                segmentation,
            });
        }
        // Lucene re-sorts nothing here: entries are already in surface order, which (as UTF-16BE
        // bytes) is the order the FST builder needs; equal surfaces were dropped above.
        let mut builder = fst::MapBuilder::memory();
        for (ord, entry) in entries.iter().enumerate() {
            let key: Vec<u8> = entry.surface.iter().flat_map(|u| u.to_be_bytes()).collect();
            builder
                .insert(key, ord as u64)
                .expect("entries are sorted and unique");
        }
        let fst = fst::raw::Fst::new(builder.into_inner().expect("in-memory FST"))
            .expect("freshly built FST");
        Ok(UserDictionary {
            terms: TermIndex::new(fst),
            entries,
        })
    }

    /// True when no rule survived parsing (only comments and blank lines).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn terms(&self) -> &TermIndex<Vec<u8>> {
        &self.terms
    }

    pub(crate) fn right_id(&self, ord: u32) -> u16 {
        self.entries[ord as usize].right_id
    }

    /// The segmentation's part lengths (empty for a simple noun).
    pub(crate) fn segmentation(&self, ord: u32) -> &[u16] {
        &self.entries[ord as usize].segmentation
    }

    /// Every entry, in the order of the lookup structure (surface forms sorted by UTF-16 code
    /// units), for tests.
    #[cfg(test)]
    pub(crate) fn entries(&self) -> Vec<UserEntry> {
        self.entries
            .iter()
            .map(|e| {
                let surface = String::from_utf16(&e.surface).unwrap();
                let segmentation = (!e.segmentation.is_empty()).then(|| {
                    let mut at = 0usize;
                    e.segmentation
                        .iter()
                        .map(|&len| {
                            let part = &e.surface[at..at + usize::from(len)];
                            at += usize::from(len);
                            String::from_utf16(part).unwrap()
                        })
                        .collect()
                });
                UserEntry {
                    surface,
                    right_id: e.right_id,
                    segmentation,
                }
            })
            .collect()
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
    /// The same surface form appears on two rules (1-based line number of the second).
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
