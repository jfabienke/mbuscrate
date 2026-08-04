//! Runtime AES keystore, matching how the C++ metermon actually gets keys.
//!
//! metermon does NOT store keys in `metermon.conf`. It subscribes to the control
//! topic (`meter/control/<gwid>`) and installs keys as they arrive upstream:
//!
//! ```json
//! { "op": "key", "meterid": 305419896, "key": "0102...0f" }
//! ```
//!
//! (`metermon.cc:273-278` → `CryptoKey::Add(meterid, hex)`; meterid is the decimal
//! device address.) This keystore mirrors that: fed live from the control topic, or
//! offline from a captured key file so the replay A/B can decrypt the same frames.

use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct KeyStore {
    /// meterid (decimal device address) -> 32-hex AES-128 key.
    keys: HashMap<u32, String>,
}

impl KeyStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Install a key for a meter (as the `op:key` control message does).
    pub fn install(&mut self, meterid: u32, hexkey: String) {
        self.keys.insert(meterid, hexkey);
    }

    /// Seed from a config's optional `keys` map (offline/testing convenience).
    pub fn seed_from_config(&mut self, cfg: &crate::config::Config) {
        for (id_str, hexkey) in &cfg.keys {
            if let Ok(id) = id_str.parse::<u32>() {
                self.install(id, hexkey.clone());
            }
        }
    }

    pub fn get(&self, meterid: u32) -> Option<&str> {
        self.keys.get(&meterid).map(String::as_str)
    }

    /// Iterate (meterid, hexkey) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &str)> {
        self.keys.iter().map(|(id, k)| (*id, k.as_str()))
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Handle one control message. Returns the meterid if it was an `op:key`
    /// message that installed a key. Mirrors metermon's control handler.
    pub fn handle_control(&mut self, msg: &serde_json::Value) -> Option<u32> {
        if msg.get("op").and_then(|v| v.as_str()) != Some("key") {
            return None;
        }
        let meterid = msg.get("meterid").and_then(|v| v.as_u64())? as u32;
        let key = msg.get("key").and_then(|v| v.as_str())?;
        if meterid != 0 && !key.is_empty() {
            self.install(meterid, key.to_string());
            return Some(meterid);
        }
        None
    }

    /// Load keys captured for offline replay. Accepts either:
    ///   - a JSON object `{ "305419896": "0102..0f", ... }`, or
    ///   - newline-delimited control messages `{"op":"key","meterid":..,"key":".."}`
    ///     (as recorded straight off the control topic).
    pub fn load_file(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_str(&text)
    }

    pub fn from_str(text: &str) -> anyhow::Result<Self> {
        let mut store = Self::new();

        // Try a flat {meterid: hexkey} object first.
        if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(text.trim()) {
            for (k, v) in map {
                if let Ok(id) = k.parse::<u32>() {
                    store.install(id, v);
                }
            }
            if !store.is_empty() {
                return Ok(store);
            }
        }

        // Otherwise treat each non-empty line as a control message.
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) {
                store.handle_control(&msg);
            }
        }
        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn installs_from_control_key_message() {
        let mut ks = KeyStore::new();
        let installed = ks.handle_control(&json!({
            "op": "key", "meterid": 305419896u64, "key": "000102030405060708090a0b0c0d0e0f"
        }));
        assert_eq!(installed, Some(305419896));
        assert_eq!(ks.get(305419896).unwrap().len(), 32);
    }

    #[test]
    fn ignores_non_key_control_ops() {
        let mut ks = KeyStore::new();
        assert_eq!(ks.handle_control(&json!({"op": "rescan"})), None);
        assert!(ks.is_empty());
    }

    #[test]
    fn loads_flat_map_and_control_lines() {
        let flat = r#"{"305419896":"000102030405060708090a0b0c0d0e0f"}"#;
        assert_eq!(KeyStore::from_str(flat).unwrap().len(), 1);

        let lines = "{\"op\":\"key\",\"meterid\":1,\"key\":\"aa\"}\n# c\n{\"op\":\"rescan\"}\n";
        let ks = KeyStore::from_str(lines).unwrap();
        assert_eq!(ks.len(), 1);
        assert_eq!(ks.get(1), Some("aa"));
    }
}
