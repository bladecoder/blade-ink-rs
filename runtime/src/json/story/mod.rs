//! Readers for compiled Ink story JSON.

#[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
pub(crate) mod read_dom;
#[cfg(feature = "stream-json-parser")]
pub(crate) mod read_stream;
