//! Transport-layer (TPL) header location — re-exported from `mbus-core`.
//!
//! Locates the records offset and the ciphertext offset behind a TPL CI (`0x78` no
//! header, `0x7A`/`0x7B` short, `0x72`/`0x73` long), so consumers stop re-deriving the
//! walk and getting the records offset silently wrong. Pure framing, no crypto feature.

pub use mbus_core::wmbus::tpl::{parse_tpl_header, TplError, TplHeader};
