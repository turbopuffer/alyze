//! High-performance text analysis for full-text search.
//!
//! Currently provides Unicode word and sentence segmentation (UAX #29) via a hand-rolled DFA.
//!
//! ```
//! let mut breaks = Vec::new();
//! alyze::uax29::word::tokenize("Hello, world!", Default::default(), |bp, _| {
//!     breaks.push(bp);
//!     true // return false to stop tokenization early
//! });
//! assert_eq!(breaks, vec![0, 5, 6, 7, 12, 13]);
//! ```

pub mod analyze;
pub mod features;
pub mod uax29;
