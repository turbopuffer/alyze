#[cfg(feature = "kuromoji")]
pub mod kuromoji;
#[cfg(any(feature = "nori", feature = "kuromoji"))]
#[allow(dead_code)]
pub(crate) mod morph;
#[cfg(feature = "nori")]
pub mod nori;
#[cfg(feature = "smartcn")]
pub mod smartcn;

#[cfg(test)]
#[allow(dead_code)]
pub(crate) mod testutil;
