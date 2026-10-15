//! Codecs for complete saved runtime state.

#[cfg(all(
    not(any(feature = "stream-json-parser", feature = "binary-image")),
    feature = "serde-json-parser"
))]
mod dom;
mod format;
#[cfg(any(feature = "stream-json-parser", feature = "binary-image"))]
mod stream;
