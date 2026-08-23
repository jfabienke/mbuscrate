//! M-Bus data-record decoding: VIF resolution and value encodings.
//!
//! The Value Information Field says what a number *means* — its unit, its quantity and
//! its decimal exponent. Resolution is table-driven and the tables are `const`, so this
//! layer allocates nothing and needs no clock.

pub mod data_encoding;
pub mod quirk;
pub mod record;
pub mod record_value;
pub mod text;
pub mod vif;
pub mod vif_maps;
