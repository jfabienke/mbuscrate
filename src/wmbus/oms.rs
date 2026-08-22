//! OMS security mode 5 (AES-128-CBC) — re-exported from `mbus-core`.
//!
//! Moved wholesale. `cbc` is declared here with `features = ["alloc"]`, but the core takes
//! it without: `decrypt_padded_mut` works in place on a caller-supplied buffer, so the
//! allocator was never needed.

pub use mbus_core::wmbus::oms::{decrypt_mode5_cbc, decrypted_ok, mode5_cbc_iv, Mode5Error};
