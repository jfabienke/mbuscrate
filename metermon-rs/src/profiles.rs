//! Device profiles pushed down from the backend Device Manager.
//!
//! A profile resolves what the frame alone cannot: the actual product (and, where
//! known, firmware) behind a meter id, which selects model-specific interpretation in
//! the crate (`DeviceIdentity.profile`, vendor-layers §1.1 case 3). The channel is
//! request-driven and key-channel shaped: profiles arrive as `op:profile` control
//! messages, are persisted to redb **before** being installed in memory, and are
//! loaded from redb at startup before any broker contact — so device knowledge
//! survives restarts and broker outages exactly as AES keys do.
//!
//! Wire contract: `docs/design/vendor-layers.md` §7.2 (mbus-rs repo).

use mbus_rs::vendors::DeviceProfile;
use std::collections::HashMap;

/// In-memory profile map, mirroring the redb `profiles` table.
#[derive(Debug, Default, Clone)]
pub struct ProfileStore {
    profiles: HashMap<u32, DeviceProfile>,
}

/// Longest accepted model/firmware string. Profiles come from our own backend, but
/// the bound keeps a misbehaving publisher from bloating the store.
const MAX_FIELD: usize = 64;

impl ProfileStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn install(&mut self, meterid: u32, profile: DeviceProfile) {
        self.profiles.insert(meterid, profile);
    }

    pub fn get(&self, meterid: u32) -> Option<&DeviceProfile> {
        self.profiles.get(&meterid)
    }

    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }

    /// Parse an `op:profile` control message into `(meterid, profile)` **without**
    /// installing it, so the live path can persist durably first (same
    /// parse-then-persist-then-install shape as `KeyStore::parse_key_message`).
    /// Returns `None` for non-profile messages or ones failing validation.
    pub fn parse_profile_message(msg: &serde_json::Value) -> Option<(u32, DeviceProfile)> {
        if msg.get("op").and_then(|v| v.as_str()) != Some("profile") {
            return None;
        }
        let meterid = msg.get("meterid").and_then(|v| v.as_u64())? as u32;
        let model = msg.get("model").and_then(|v| v.as_str())?;
        if meterid == 0 || model.is_empty() || model.len() > MAX_FIELD {
            return None;
        }
        let firmware = msg
            .get("firmware")
            .and_then(|v| v.as_str())
            .filter(|f| !f.is_empty() && f.len() <= MAX_FIELD)
            .map(str::to_string);
        Some((
            meterid,
            DeviceProfile {
                model: model.to_string(),
                firmware,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_valid_profile_message() {
        let (id, p) = ProfileStore::parse_profile_message(&json!({
            "op": "profile", "meterid": 74644444u64, "model": "MULTICAL 21"
        }))
        .unwrap();
        assert_eq!(id, 74644444);
        assert_eq!(p.model, "MULTICAL 21");
        assert_eq!(p.firmware, None);
    }

    #[test]
    fn rejects_malformed_profiles() {
        for bad in [
            json!({"op": "key", "meterid": 1, "model": "X"}), // wrong op
            json!({"op": "profile", "meterid": 0, "model": "X"}), // meter 0
            json!({"op": "profile", "meterid": 1, "model": ""}), // empty model
            json!({"op": "profile", "meterid": 1}),           // no model
            json!({"op": "profile", "meterid": 1, "model": "y".repeat(65)}), // oversized
        ] {
            assert!(
                ProfileStore::parse_profile_message(&bad).is_none(),
                "accepted {bad}"
            );
        }
    }

    #[test]
    fn firmware_is_optional_and_validated() {
        let (_, p) = ProfileStore::parse_profile_message(&json!({
            "op": "profile", "meterid": 5, "model": "M", "firmware": "FW54"
        }))
        .unwrap();
        assert_eq!(p.firmware.as_deref(), Some("FW54"));
        // Oversized firmware is dropped, not fatal — the model still installs.
        let (_, p) = ProfileStore::parse_profile_message(&json!({
            "op": "profile", "meterid": 5, "model": "M", "firmware": "y".repeat(65)
        }))
        .unwrap();
        assert_eq!(p.firmware, None);
    }
}
