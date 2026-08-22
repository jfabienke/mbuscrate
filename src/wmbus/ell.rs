//! wM-Bus extended link layer (ELL) — re-exported from `mbus-core`.
//!
//! Moved once `AesKey` and `mode_c` were both in the core; nothing else stood in the way.
//! `ctr` is taken there without its `alloc` feature — `apply_keystream` works in place,
//! and CTR is length-preserving, so a decrypted payload is bounded by the same
//! `WMBUS_MAX_PAYLOAD` as the ciphertext it came from.
//!
//! `DecryptedEll::payload` is now a fixed-capacity `Payload`; it derefs to `[u8]`.

pub use mbus_core::wmbus::ell::{
    decrypt_ell_payload, decrypt_frame, ell_ctr_iv, is_plausible_tpl_ci, parse_ell, DecryptedEll,
    EllError, EllHeader, EllSecurity, CI_ELL_I, CI_ELL_II, CI_ELL_III, CI_ELL_IV,
};
