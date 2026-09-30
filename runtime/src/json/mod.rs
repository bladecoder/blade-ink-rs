#[allow(unused_imports)]
use crate::prelude::*;

#[cfg(feature = "stream-json-parser")]
pub(crate) mod flat_json_stream;
#[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
pub mod json_read;
#[cfg(feature = "stream-json-parser")]
pub mod json_read_stream;
#[cfg(feature = "stream-json-parser")]
pub(crate) mod json_state_stream;
#[cfg(feature = "stream-json-parser")]
pub(crate) mod json_tokenizer;
#[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
pub mod json_write;
#[cfg(feature = "stream-json-parser")]
pub(crate) mod json_write_stream;
#[cfg(feature = "stream-json-parser")]
pub(crate) mod json_writer;
