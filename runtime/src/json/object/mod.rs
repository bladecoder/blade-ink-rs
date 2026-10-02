//! JSON conversion for individual runtime objects.

#[cfg(all(
    not(any(feature = "stream-json-parser", feature = "binary-image")),
    feature = "serde-json-parser"
))]
pub(crate) mod read_dom;
#[cfg(any(feature = "stream-json-parser", feature = "binary-image"))]
pub(crate) mod read_stream;
#[cfg(all(
    not(any(feature = "stream-json-parser", feature = "binary-image")),
    feature = "serde-json-parser"
))]
pub(crate) mod write_dom;
#[cfg(any(feature = "stream-json-parser", feature = "binary-image"))]
pub(crate) mod write_stream;
