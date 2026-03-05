//! Japanese morphological tokenizer based on the MeCab/IPADIC dictionary.
//!
//! This is a Rust implementation of the core Viterbi-based tokenization algorithm
//! used by Kuromoji (Apache Lucene) and MeCab. It finds the minimum-cost segmentation
//! of Japanese text using a dictionary of known words and their connection costs.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// A single dictionary entry for a known word.
#[derive(Debug, Clone)]
struct DictEntry {
    surface: String,
    left_id: u16,
    right_id: u16,
    word_cost: i16,
}

/// Character category definition from char.def.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
enum CharCategory {
    Default = 0,
    Space,
    Kanji,
    Symbol,
    Numeric,
    Alpha,
    Hiragana,
    Katakana,
    KanjiNumeric,
    Greek,
    Cyrillic,
}

impl CharCategory {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "DEFAULT" => Some(Self::Default),
            "SPACE" => Some(Self::Space),
            "KANJI" => Some(Self::Kanji),
            "SYMBOL" => Some(Self::Symbol),
            "NUMERIC" => Some(Self::Numeric),
            "ALPHA" => Some(Self::Alpha),
            "HIRAGANA" => Some(Self::Hiragana),
            "KATAKANA" => Some(Self::Katakana),
            "KANJINUMERIC" => Some(Self::KanjiNumeric),
            "GREEK" => Some(Self::Greek),
            "CYRILLIC" => Some(Self::Cyrillic),
            _ => None,
        }
    }

    const NUM: usize = 11;
}

/// Properties for a character category: whether to invoke unknown word processing,
/// whether to group consecutive same-category characters, and max unknown word length.
#[derive(Debug, Clone, Copy)]
struct CategoryDef {
    invoke: bool,
    group: bool,
    length: u8,
}

/// An unknown word template from unk.def.
#[derive(Debug, Clone)]
struct UnkEntry {
    left_id: u16,
    right_id: u16,
    word_cost: i16,
}

/// An edge in the Viterbi lattice. Represents a candidate token spanning
/// `byte_start..byte_end` in the input text.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Edge {
    byte_start: usize,
    byte_end: usize,
    left_id: u16,
    right_id: u16,
    word_cost: i16,
    path_cost: i64,
    /// Index into `ends_at[byte_start]` for the best predecessor edge.
    left_edge_idx: usize,
    is_bos: bool,
}

/// The loaded dictionary, containing all data needed for tokenization.
#[allow(dead_code)]
pub struct Dictionary {
    /// Maps surface form -> list of dictionary entries.
    entries: HashMap<String, Vec<DictEntry>>,
    /// Connection cost matrix: cost[right_id_of_left * num_right + left_id_of_right]
    /// Dimensions: forward_size x backward_size (both typically 1316 for IPADIC).
    connection_costs: Vec<i16>,
    forward_size: usize,
    backward_size: usize,
    /// Category definitions: invoke, group, length per category.
    category_defs: [CategoryDef; CharCategory::NUM],
    /// Unknown word entries per category.
    unknown_entries: [Vec<UnkEntry>; CharCategory::NUM],
    /// Unicode codepoint -> list of categories (first is primary).
    /// Stored as ranges for efficiency.
    char_ranges: Vec<CharRange>,
}

/// A range mapping Unicode codepoints to character categories.
#[derive(Debug)]
struct CharRange {
    start: u32,
    end: u32, // inclusive
    categories: Vec<CharCategory>,
}

impl Dictionary {
    /// Load the dictionary from a directory containing IPADIC CSV files.
    pub fn load(dir: &Path) -> Self {
        let entries = Self::load_csv_entries(dir);
        let (forward_size, backward_size, connection_costs) = Self::load_matrix(dir);
        let (category_defs, char_ranges) = Self::load_char_def(dir);
        let unknown_entries = Self::load_unk(dir);

        Dictionary {
            entries,
            connection_costs,
            forward_size,
            backward_size,
            category_defs,
            unknown_entries,
            char_ranges,
        }
    }

    fn load_csv_entries(dir: &Path) -> HashMap<String, Vec<DictEntry>> {
        let mut entries: HashMap<String, Vec<DictEntry>> = HashMap::new();
        let csv_files: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| {
                let e = e.unwrap();
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".csv") {
                    Some(e.path())
                } else {
                    None
                }
            })
            .collect();

        for csv_path in &csv_files {
            let f = File::open(csv_path).unwrap();
            let reader = BufReader::new(f);
            for line in reader.lines() {
                let line = line.unwrap();
                // CSV format: surface,left_id,right_id,cost,pos,...
                // We need to handle commas inside quoted fields.
                let fields = parse_csv_line(&line);
                if fields.len() < 4 {
                    continue;
                }
                let surface = fields[0].to_string();
                let left_id: u16 = fields[1].parse().unwrap();
                let right_id: u16 = fields[2].parse().unwrap();
                let word_cost: i16 = fields[3].parse().unwrap();

                entries.entry(surface.clone()).or_default().push(DictEntry {
                    surface,
                    left_id,
                    right_id,
                    word_cost,
                });
            }
        }
        entries
    }

    fn load_matrix(dir: &Path) -> (usize, usize, Vec<i16>) {
        let f = File::open(dir.join("matrix.def")).unwrap();
        let reader = BufReader::new(f);
        let mut lines = reader.lines();

        let header = lines.next().unwrap().unwrap();
        let dims: Vec<usize> = header
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        let (forward_size, backward_size) = (dims[0], dims[1]);

        let mut costs = vec![0i16; forward_size * backward_size];
        for line in lines {
            let line = line.unwrap();
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }
            let right_id: usize = parts[0].parse().unwrap();
            let left_id: usize = parts[1].parse().unwrap();
            let cost: i16 = parts[2].parse().unwrap();
            costs[right_id * backward_size + left_id] = cost;
        }

        (forward_size, backward_size, costs)
    }

    fn load_char_def(dir: &Path) -> ([CategoryDef; CharCategory::NUM], Vec<CharRange>) {
        let f = File::open(dir.join("char.def")).unwrap();
        let reader = BufReader::new(f);

        let mut category_defs = [CategoryDef {
            invoke: false,
            group: false,
            length: 0,
        }; CharCategory::NUM];
        let mut char_ranges = Vec::new();

        for line in reader.lines() {
            let line = line.unwrap();
            let line = line.trim().to_string();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Strip inline comment
            let line = if let Some(idx) = line.find('#') {
                line[..idx].trim().to_string()
            } else {
                line
            };

            if line.starts_with("0x") {
                // Unicode range mapping
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 2 {
                    continue;
                }
                let (start, end) = if parts[0].contains("..") {
                    let range_parts: Vec<&str> = parts[0].split("..").collect();
                    let start = u32::from_str_radix(&range_parts[0][2..], 16).unwrap();
                    let end = u32::from_str_radix(&range_parts[1][2..], 16).unwrap();
                    (start, end)
                } else {
                    let val = u32::from_str_radix(&parts[0][2..], 16).unwrap();
                    (val, val)
                };
                let categories: Vec<CharCategory> = parts[1..]
                    .iter()
                    .filter_map(|s| CharCategory::from_name(s))
                    .collect();
                if !categories.is_empty() {
                    char_ranges.push(CharRange {
                        start,
                        end,
                        categories,
                    });
                }
            } else {
                // Category definition: NAME INVOKE GROUP LENGTH
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    if let Some(cat) = CharCategory::from_name(parts[0]) {
                        category_defs[cat as usize] = CategoryDef {
                            invoke: parts[1] == "1",
                            group: parts[2] == "1",
                            length: parts[3].parse().unwrap_or(0),
                        };
                    }
                }
            }
        }

        // Sort ranges by start for efficient lookup
        char_ranges.sort_by_key(|r| r.start);

        (category_defs, char_ranges)
    }

    fn load_unk(dir: &Path) -> [Vec<UnkEntry>; CharCategory::NUM] {
        let f = File::open(dir.join("unk.def")).unwrap();
        let reader = BufReader::new(f);

        let mut unknown_entries: [Vec<UnkEntry>; CharCategory::NUM] =
            std::array::from_fn(|_| Vec::new());

        for line in reader.lines() {
            let line = line.unwrap();
            let fields = parse_csv_line(&line);
            if fields.len() < 4 {
                continue;
            }
            if let Some(cat) = CharCategory::from_name(fields[0]) {
                let left_id: u16 = fields[1].parse().unwrap();
                let right_id: u16 = fields[2].parse().unwrap();
                let word_cost: i16 = fields[3].parse().unwrap();
                unknown_entries[cat as usize].push(UnkEntry {
                    left_id,
                    right_id,
                    word_cost,
                });
            }
        }

        unknown_entries
    }

    /// Look up the primary character category for a Unicode codepoint.
    fn char_category(&self, c: char) -> CharCategory {
        self.char_categories(c)[0]
    }

    /// Look up all character categories for a Unicode codepoint.
    /// Specific (single-codepoint) entries take priority over range entries as the
    /// primary category, matching MeCab's behavior.
    fn char_categories(&self, c: char) -> Vec<CharCategory> {
        let cp = c as u32;
        let mut from_specific = Vec::new();
        let mut from_ranges = Vec::new();
        for range in &self.char_ranges {
            if cp >= range.start && cp <= range.end {
                let target = if range.start == range.end {
                    &mut from_specific
                } else {
                    &mut from_ranges
                };
                for &cat in &range.categories {
                    if !target.contains(&cat) {
                        target.push(cat);
                    }
                }
            }
        }
        // Specific entries first, then range entries (deduped)
        let mut result = from_specific;
        for cat in from_ranges {
            if !result.contains(&cat) {
                result.push(cat);
            }
        }
        if result.is_empty() {
            result.push(CharCategory::Default);
        }
        result
    }

    /// Get the connection cost between two edges.
    fn connection_cost(&self, left_right_id: u16, right_left_id: u16) -> i16 {
        self.connection_costs[left_right_id as usize * self.backward_size + right_left_id as usize]
    }

    /// Look up all dictionary entries whose surface is a prefix of `text[byte_pos..]`.
    fn prefix_entries(&self, text: &str, byte_pos: usize) -> Vec<&DictEntry> {
        let remaining = &text[byte_pos..];
        let mut results = Vec::new();
        let mut end = 0;
        for c in remaining.chars() {
            end += c.len_utf8();
            let prefix = &remaining[..end];
            if let Some(entries) = self.entries.get(prefix) {
                results.extend(entries.iter());
            }
        }
        results
    }
}

/// Tokenize text using the Viterbi algorithm over a lattice of candidate tokens.
/// Returns byte offsets of token boundaries.
pub fn tokenize(text: &str, dict: &Dictionary) -> Vec<usize> {
    if text.is_empty() {
        return Vec::new();
    }

    let byte_len = text.len();

    // ends_at[i] stores all edges that end at byte position i.
    // ends_at[0] contains only the BOS (beginning of sentence) edge.
    let mut ends_at: Vec<Vec<Edge>> = vec![Vec::new(); byte_len + 1];

    // BOS edge: a synthetic edge at position 0.
    ends_at[0].push(Edge {
        byte_start: 0,
        byte_end: 0,
        left_id: 0,
        right_id: 0,
        word_cost: 0,
        path_cost: 0,
        left_edge_idx: 0,
        is_bos: true,
    });

    // Forward pass: build lattice left-to-right.
    let mut byte_pos = 0;
    for (ci, c) in text.char_indices() {
        assert_eq!(ci, byte_pos);

        if ends_at[byte_pos].is_empty() {
            // No edges end here, skip (shouldn't happen if unknown word handling is correct).
            byte_pos += c.len_utf8();
            continue;
        }

        // 1. Dictionary lookup: find all dictionary entries starting at this position.
        let dict_entries = dict.prefix_entries(text, byte_pos);
        let has_dict_match = !dict_entries.is_empty();

        for entry in &dict_entries {
            let edge_end = byte_pos + entry.surface.len();
            add_edge(
                &mut ends_at,
                byte_pos,
                edge_end,
                entry.left_id,
                entry.right_id,
                entry.word_cost,
                dict,
            );
        }

        // 2. Unknown word handling.
        let categories = dict.char_categories(c);
        let primary_cat = categories[0];
        let cat_def = dict.category_defs[primary_cat as usize];

        // Invoke unknown word processing if:
        // - invoke=true for this category, OR
        // - no dictionary entries were found
        if cat_def.invoke || !has_dict_match {
            // For each category this character belongs to, try unknown word entries.
            for &cat in &categories {
                let cat_def = dict.category_defs[cat as usize];
                let unk_entries = &dict.unknown_entries[cat as usize];

                if unk_entries.is_empty() {
                    continue;
                }

                // If group=true, group consecutive characters of the same category.
                if cat_def.group {
                    let mut group_end = byte_pos + c.len_utf8();
                    for next_c in text[group_end..].chars() {
                        let next_cat = dict.char_category(next_c);
                        if next_cat != cat {
                            break;
                        }
                        group_end += next_c.len_utf8();
                    }
                    for unk in unk_entries {
                        add_edge(
                            &mut ends_at,
                            byte_pos,
                            group_end,
                            unk.left_id,
                            unk.right_id,
                            unk.word_cost,
                            dict,
                        );
                    }
                }

                // If length > 0, also add 1..=length character unknown words.
                if cat_def.length > 0 {
                    let mut unk_end = byte_pos;
                    let mut count = 0u8;
                    for next_c in text[byte_pos..].chars() {
                        let next_cat = dict.char_category(next_c);
                        if count > 0 && next_cat != cat {
                            break;
                        }
                        unk_end += next_c.len_utf8();
                        count += 1;
                        if count > cat_def.length {
                            break;
                        }
                        for unk in unk_entries {
                            add_edge(
                                &mut ends_at,
                                byte_pos,
                                unk_end,
                                unk.left_id,
                                unk.right_id,
                                unk.word_cost,
                                dict,
                            );
                        }
                    }
                }

                // If group=false and length=0, add single-character unknown word.
                if !cat_def.group && cat_def.length == 0 {
                    let single_end = byte_pos + c.len_utf8();
                    for unk in unk_entries {
                        add_edge(
                            &mut ends_at,
                            byte_pos,
                            single_end,
                            unk.left_id,
                            unk.right_id,
                            unk.word_cost,
                            dict,
                        );
                    }
                }
            }
        }

        byte_pos += c.len_utf8();
    }

    // EOS: find best path to end of text.
    // The EOS node has left_id=0, right_id=0.
    let mut best_cost = i64::MAX;
    let mut best_idx = 0;
    for (idx, edge) in ends_at[byte_len].iter().enumerate() {
        let conn = dict.connection_cost(edge.right_id, 0) as i64;
        let total = edge.path_cost + conn;
        if total < best_cost {
            best_cost = total;
            best_idx = idx;
        }
    }

    if ends_at[byte_len].is_empty() {
        // Fallback: character-by-character segmentation
        return text.char_indices().map(|(i, _)| i).collect();
    }

    // Backtrace: follow back-pointers from EOS to BOS.
    let mut boundaries = Vec::new();
    let mut pos = byte_len;
    let mut idx = best_idx;
    loop {
        let edge = &ends_at[pos][idx];
        if edge.is_bos {
            break;
        }
        boundaries.push(edge.byte_start);
        idx = edge.left_edge_idx;
        pos = edge.byte_start;
    }

    boundaries.reverse();
    boundaries.push(byte_len);
    boundaries
}

/// Add an edge to the lattice, computing its best path cost.
fn add_edge(
    ends_at: &mut [Vec<Edge>],
    byte_start: usize,
    byte_end: usize,
    left_id: u16,
    right_id: u16,
    word_cost: i16,
    dict: &Dictionary,
) {
    let mut best_cost = i64::MAX;
    let mut best_left_idx = 0;

    for (idx, left_edge) in ends_at[byte_start].iter().enumerate() {
        let conn = dict.connection_cost(left_edge.right_id, left_id) as i64;
        let total = left_edge.path_cost + conn + word_cost as i64;
        if total < best_cost {
            best_cost = total;
            best_left_idx = idx;
        }
    }

    if best_cost == i64::MAX {
        return; // No valid predecessor
    }

    ends_at[byte_end].push(Edge {
        byte_start,
        byte_end,
        left_id,
        right_id,
        word_cost,
        path_cost: best_cost,
        left_edge_idx: best_left_idx,
        is_bos: false,
    });
}

/// Tokenize text and return the surface forms of the tokens.
pub fn tokenize_to_strings<'a>(text: &'a str, dict: &Dictionary) -> Vec<&'a str> {
    let boundaries = tokenize(text, dict);
    if boundaries.is_empty() {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let mut prev = boundaries[0];
    for &boundary in &boundaries[1..] {
        tokens.push(&text[prev..boundary]);
        prev = boundary;
    }
    tokens
}

/// Simple CSV line parser that handles quoted fields.
fn parse_csv_line(line: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    let bytes = line.as_bytes();

    for i in 0..bytes.len() {
        if bytes[i] == b'"' {
            in_quotes = !in_quotes;
        } else if bytes[i] == b',' && !in_quotes {
            let field = &line[start..i];
            fields.push(field.trim_matches('"'));
            start = i + 1;
        }
    }
    // Last field
    if start <= line.len() {
        let field = &line[start..];
        fields.push(field.trim_matches('"'));
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    static DICT: OnceLock<Dictionary> = OnceLock::new();

    fn dict() -> &'static Dictionary {
        DICT.get_or_init(|| Dictionary::load(Path::new("testdata/ipadic")))
    }

    /// Load test cases from the test file.
    fn load_test_cases() -> Vec<(String, Vec<String>)> {
        let f = File::open("testdata/kuromoji_test.txt").unwrap();
        let reader = BufReader::new(f);
        let mut cases = Vec::new();
        for line in reader.lines() {
            let line = line.unwrap();
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 2 {
                continue;
            }
            let input = parts[0].to_string();
            let expected: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
            cases.push((input, expected));
        }
        cases
    }

    #[test]
    fn test_against_reference() {
        let dict = dict();
        let test_cases = load_test_cases();
        let mut passed = 0;
        let mut failures = Vec::new();

        for (input, expected) in &test_cases {
            let got = tokenize_to_strings(input, dict);
            let got_strings: Vec<String> = got.iter().map(|s| s.to_string()).collect();
            if got_strings == *expected {
                passed += 1;
            } else {
                failures.push((input.clone(), expected.clone(), got_strings));
            }
        }

        for (input, expected, got) in &failures {
            println!("FAIL: {:?}", input);
            println!("  expected: {:?}", expected);
            println!("       got: {:?}", got);
        }

        let total = passed + failures.len();
        assert_eq!(
            failures.len(),
            0,
            "{} / {} tests passed",
            passed,
            total
        );
    }

    #[test]
    fn test_empty_input() {
        let dict = dict();
        let tokens = tokenize_to_strings("", dict);
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_single_char() {
        let dict = dict();
        let tokens = tokenize_to_strings("a", dict);
        assert_eq!(tokens, vec!["a"]);
    }

    #[test]
    fn test_basic_japanese() {
        let dict = dict();
        let tokens = tokenize_to_strings("猫が好きです", dict);
        assert_eq!(tokens, vec!["猫", "が", "好き", "です"]);
    }

    #[test]
    fn test_dictionary_loading() {
        let dict = dict();
        // IPADIC should have ~392K entries
        assert!(dict.entries.len() > 100_000, "dict has {} entries", dict.entries.len());
        // Connection cost matrix should be 1316x1316
        assert_eq!(dict.forward_size, 1316);
        assert_eq!(dict.backward_size, 1316);
    }

    #[test]
    fn test_char_categories() {
        let dict = dict();
        assert_eq!(dict.char_category('あ'), CharCategory::Hiragana);
        assert_eq!(dict.char_category('ア'), CharCategory::Katakana);
        assert_eq!(dict.char_category('漢'), CharCategory::Kanji);
        assert_eq!(dict.char_category('A'), CharCategory::Alpha);
        assert_eq!(dict.char_category('1'), CharCategory::Numeric);
        assert_eq!(dict.char_category(' '), CharCategory::Space);
        assert_eq!(dict.char_category('!'), CharCategory::Symbol);
        // 々 (U+3005) should be KANJI despite being in 0x3000..0x303F SYMBOL range
        assert_eq!(dict.char_category('々'), CharCategory::Kanji);
        // ー (U+30FC) should be KATAKANA
        assert_eq!(dict.char_category('ー'), CharCategory::Katakana);
    }

}
