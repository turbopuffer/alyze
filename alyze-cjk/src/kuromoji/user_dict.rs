//! User dictionaries: Lucene's `UserDictionary` (Elasticsearch's `user_dictionary` /
//! `user_dictionary_rules`), with the preprocessing and duplicate detection Elasticsearch layers
//! on top.

use std::fmt;

use crate::morph::java;
use crate::morph::term_index::{self, TermIndex};
use crate::morph::unicode::{self, GeneralCategory};

/// Lucene's fixed connection ids and cost for user words (`UserMorphData`).
pub(crate) const LEFT_ID: u16 = 5;
pub(crate) const RIGHT_ID: u16 = 5;
pub(crate) const WORD_COST: i32 = -100000;

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
/// Parsing follows what Elasticsearch does with a `user_dictionary` file and then Lucene's
/// `UserDictionary.open` and `CSVUtil`, exactly: lines are trimmed and blank ones dropped
/// (comments kept); a surface form (the first CSV field) that repeats is an error unless
/// `lenient`, which keeps the first rule; then a line starting with `#` is a comment, fields may
/// be `"`-quoted with `""` escapes (the quotes of a field with an inner quote are kept), segments
/// and readings are split on runs of ASCII spaces, and the surface form and the joined
/// segmentation must agree with all Java `\s` removed. The term-index key is the raw first
/// field, inner whitespace included, and a segment is the next so many UTF-16 code units of that
/// key, which is what Lucene cuts the match into.
pub struct UserDictionary {
    terms: TermIndex<Vec<u8>>,
    /// Per entry (ordinal), in term order.
    entries: Vec<Entry>,
}

#[derive(Clone, Debug)]
pub(crate) struct Entry {
    /// The term-index key (the raw first field), as UTF-16.
    pub key: Vec<u16>,
    /// The segments' lengths in code units (Lucene's `wordIdAndLength[1..]`).
    pub segments: Vec<u16>,
    /// One reading per segment.
    pub readings: Vec<String>,
    pub part_of_speech: String,
}

impl fmt::Debug for UserDictionary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserDictionary")
            .field("entries", &self.entries.len())
            .finish()
    }
}

impl UserDictionary {
    pub fn parse(rules: &str, lenient: bool) -> Result<UserDictionary, UserDictionaryError> {
        // Elasticsearch's Analysis.loadWordList: non-blank lines, trimmed, comments kept; the
        // line numbers of its duplicate error count these lines.
        let mut lines: Vec<(usize, &str)> = Vec::new();
        for line in java::lines(rules) {
            if !has_text(line) {
                continue;
            }
            lines.push((lines.len() + 1, java::trim(line)));
        }
        // Analysis.deDuplicateRules on the first CSV field.
        let mut keys: Vec<String> = Vec::new();
        let mut kept: Vec<(usize, &str)> = Vec::with_capacity(lines.len());
        for &(number, line) in &lines {
            if !line.starts_with('#') {
                let values = csv_parse(line);
                let Some(key) = values.first() else {
                    return Err(UserDictionaryError::Malformed {
                        rule: line.to_owned(),
                        line: number,
                    });
                };
                if keys.iter().any(|k| k == key) {
                    if lenient {
                        continue;
                    }
                    return Err(UserDictionaryError::Duplicate {
                        surface: key.clone(),
                        line: number,
                    });
                }
                keys.push(key.clone());
            }
            kept.push((number, line));
        }
        // Lucene's UserDictionary.open: comment lines are those starting with '#'.
        let mut parsed: Vec<(usize, Vec<String>)> = Vec::new();
        for (number, line) in kept {
            if line.starts_with('#') || java::trim(line).is_empty() {
                continue;
            }
            let values = csv_parse(line);
            if values.len() < 4 {
                return Err(UserDictionaryError::Malformed {
                    rule: line.to_owned(),
                    line: number,
                });
            }
            parsed.push((number, values));
        }
        // Sorted by the first field in UTF-16 order (String.compareTo); stable like Java's sort.
        parsed.sort_by(|a, b| a.1[0].encode_utf16().cmp(b.1[0].encode_utf16()));
        let mut entries: Vec<Entry> = Vec::with_capacity(parsed.len());
        for (number, values) in &parsed {
            let rule = values.join(",");
            let surface: String = values[0].chars().filter(|&c| !java::is_space(c)).collect();
            let concatenated: String = values[1].chars().filter(|&c| !java::is_space(c)).collect();
            let segmentation = java_split_spaces(&values[1]);
            let readings = java_split_spaces(&values[2]);
            if segmentation.len() != readings.len() {
                return Err(UserDictionaryError::SegmentationReadingsMismatch {
                    rule,
                    line: *number,
                });
            }
            if surface != concatenated {
                return Err(UserDictionaryError::SegmentationMismatch {
                    rule,
                    line: *number,
                });
            }
            let key: Vec<u16> = values[0].encode_utf16().collect();
            // Port divergence: a segmentation field with a leading space gives Lucene an empty
            // first segment and so an empty token; a segment over 65535 code units can't be
            // stored. Both are rejected as malformed.
            let mut segments: Vec<u16> = Vec::with_capacity(segmentation.len());
            for segment in &segmentation {
                match u16::try_from(segment.encode_utf16().count()) {
                    Ok(len) if len > 0 => segments.push(len),
                    _ => {
                        return Err(UserDictionaryError::Malformed {
                            rule,
                            line: *number,
                        });
                    }
                }
            }
            // Port divergence: a segment boundary inside a surrogate pair of the key would make
            // Lucene emit half characters; reject the rule instead.
            let mut boundary = 0usize;
            for &len in &segments {
                boundary += usize::from(len);
                if boundary < key.len()
                    && (0xD800..0xDC00).contains(&key[boundary - 1])
                    && (0xDC00..0xE000).contains(&key[boundary])
                {
                    return Err(UserDictionaryError::SegmentationSplitsCharacter {
                        rule,
                        line: *number,
                    });
                }
            }
            entries.push(Entry {
                key,
                segments,
                readings: readings.into_iter().map(str::to_owned).collect(),
                part_of_speech: values[3].clone(),
            });
        }
        let terms = term_index::build(
            entries
                .iter()
                .enumerate()
                .map(|(ord, e)| (e.key.clone(), ord as u64)),
        );
        Ok(UserDictionary { terms, entries })
    }

    /// Whether no rule survived parsing (Lucene then uses no user dictionary at all).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn terms(&self) -> &TermIndex<Vec<u8>> {
        &self.terms
    }

    pub(crate) fn entry(&self, ord: u32) -> &Entry {
        &self.entries[ord as usize]
    }

    /// Every rule as Lucene stores it, in term-index (UTF-16) order of the raw surface field.
    #[cfg(test)]
    pub(crate) fn entries(&self) -> Vec<UserEntry> {
        self.entries
            .iter()
            .map(|e| {
                let mut at = 0usize;
                let segments = e
                    .segments
                    .iter()
                    .map(|&len| {
                        let start = at.min(e.key.len());
                        let end = (at + usize::from(len)).min(e.key.len());
                        at += usize::from(len);
                        String::from_utf16_lossy(&e.key[start..end])
                    })
                    .collect();
                UserEntry {
                    surface: String::from_utf16(&e.key).unwrap(),
                    segments,
                    readings: e.readings.clone(),
                    part_of_speech: e.part_of_speech.clone(),
                }
            })
            .collect()
    }
}

/// Elasticsearch's `Strings.hasText`: some character is not Java whitespace
/// (`Character.isWhitespace`: separators other than the no-break ones, or U+0009..U+000D and
/// U+001C..U+001F).
fn has_text(line: &str) -> bool {
    line.encode_utf16().any(|u| {
        let separator = matches!(
            unicode::category(u),
            GeneralCategory::SpaceSeparator
                | GeneralCategory::LineSeparator
                | GeneralCategory::ParagraphSeparator
        ) && !matches!(u, 0x00A0 | 0x2007 | 0x202F);
        !(separator || matches!(u, 0x09..=0x0D | 0x1C..=0x1F))
    })
}

/// Lucene's `CSVUtil.parse`: fields split on commas outside `"` quotes; a field entirely in
/// quotes with no inner quote is unquoted, `""` becomes `"` in any field containing a quote; an
/// odd number of quotes yields no fields at all.
pub(crate) fn csv_parse(line: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut inside_quote = false;
    let mut quote_count = 0usize;
    let mut field = String::new();
    for c in line.chars() {
        if c == '"' {
            inside_quote = !inside_quote;
            quote_count += 1;
        }
        if c == ',' && !inside_quote {
            result.push(unquote_unescape(&field));
            field.clear();
            continue;
        }
        field.push(c);
    }
    result.push(field);
    if quote_count % 2 != 0 {
        return Vec::new();
    }
    result
}

fn unquote_unescape(original: &str) -> String {
    let mut result = original.to_owned();
    if result.contains('"') {
        // `^"([^"]+)"$`
        if let Some(inner) = original
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .filter(|inner| !inner.is_empty() && !inner.contains('"'))
        {
            result = inner.to_owned();
        }
        if result.contains("\"\"") {
            result = result.replace("\"\"", "\"");
        }
    }
    result
}

/// `String.split(" +")`: a leading run of spaces yields an empty first element, trailing empty
/// elements are dropped, and the empty string yields one empty element.
fn java_split_spaces(s: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut in_separator = false;
    for (i, c) in s.char_indices() {
        if c == ' ' {
            if !in_separator {
                parts.push(&s[start..i]);
                in_separator = true;
            }
        } else if in_separator {
            start = i;
            in_separator = false;
        }
    }
    if !in_separator {
        parts.push(&s[start..]);
    }
    while parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    parts
}

/// A parsed rule, as the tests see it.
#[cfg(test)]
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
    /// Fewer than four fields, or an odd number of `"` (Lucene throws an index error); or an
    /// empty segment (a leading space in the segmentation field, from which Lucene would emit an
    /// empty token) or a segment over 65535 code units.
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
