//! High-performance text analysis for full-text search.
//!
//! Currently provides Unicode word and sentence segmentation (UAX #29) via a hand-rolled DFA.
//!
//! ```
//! let mut breaks = Vec::new();
//! alyze::uax29::word::tokenize("Hello, world!", &mut breaks, Default::default());
//! assert_eq!(breaks, vec![0, 5, 6, 7, 12, 13]);
//! ```

pub mod uax29;
