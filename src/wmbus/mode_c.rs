//! wM-Bus mode C link-layer decode — re-exported from `mbus-core`.
//!
//! Moved wholesale: the module was already pure, and its only apparent dependency on
//! `id_to_manufacturer` turned out to be a doc link rather than a use. `WMBusLinkFrame::
//! payload` is now a fixed-capacity `LinkPayload`; it derefs to `[u8]`.

pub use mbus_core::wmbus::mode_c::{decode_mode_c, LinkPayload, WMBusLinkFrame};
