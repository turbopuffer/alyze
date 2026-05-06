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

#[cfg(not(feature = "tpuf-vendored"))]
extern crate icu_properties as icu;

#[cfg(feature = "tpuf-vendored")]
extern crate tpuf_icu_properties_211 as icu;

pub mod uax29;
