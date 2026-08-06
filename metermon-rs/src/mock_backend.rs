//! Mock backend Device Manager — the profile channel's reference implementation.
//!
//! Serves device profiles over MQTT exactly as the (future) backend Device Manager
//! will (`docs/design/vendor-layers.md` §7.2, mbus-rs repo): it watches the gateway's
//! data topic for `op:startup` / `op:profile_request` and answers with one
//! `op:profile` message per known device on the gateway's control topic.
//!
//! It exists so the gateway side of migration step 4 is implementable and testable
//! end to end before the Device Manager repo does; it defines the wire contract and
//! will be superseded by that repo, not extended here. The catalog is a plain JSON
//! file — `{"<meterid>": {"model": "...", "firmware": null}, ...}` — holding model
//! names only.
//!
//! It can also serve **AES keys** (`--keys`), restoring the provisioning chain the
//! retired upstream backend used to provide: an `op:startup` announcement is answered
//! with one `op:key` per known meter, exactly as the C++ backend did. Key handling is
//! deliberately constrained:
//!
//! - keys live in a **separate, gitignored file**, never in the committed catalog;
//! - every key is validated as 32 hex characters before it is served;
//! - **key values are never logged** — only meter ids and counts;
//! - keys are **not** loaded unless `--keys` is passed, so the default mock is
//!   key-free.
//!
//! Note that `op:key` crosses the broker in cleartext on port 1883. That is what the
//! original system did and what the gateway expects, but it means the broker and the
//! network path see the key: serve real keys only over a broker and network you trust,
//! and prefer a synthetic key when all you need is to exercise the channel.

use crate::config::Config;
use crate::keystore::KeyStore;
use anyhow::{Context, Result};
use rumqttc::{Client, Event, Incoming, MqttOptions, QoS};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

/// One catalog entry: what the backend knows about a device.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CatalogEntry {
    pub model: String,
    #[serde(default)]
    pub firmware: Option<String>,
}

/// meterid -> entry. BTreeMap so responses are deterministically ordered.
pub type Catalog = BTreeMap<u32, CatalogEntry>;

pub fn load_catalog(path: &str) -> Result<Catalog> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read catalog {path}"))?;
    let raw: BTreeMap<String, CatalogEntry> =
        serde_json::from_str(&text).with_context(|| format!("parse catalog {path}"))?;
    let mut catalog = Catalog::new();
    for (id, entry) in raw {
        let id: u32 = id
            .parse()
            .with_context(|| format!("catalog meter id {id:?} is not a decimal address"))?;
        catalog.insert(id, entry);
    }
    Ok(catalog)
}

/// A well-formed AES-128 key is exactly 32 hex characters. Validated before a key is
/// ever put on the wire; the value itself is never logged.
fn is_valid_aes128_hex(key: &str) -> bool {
    key.len() == 32 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The pure request→responses mapping, split out so the protocol is unit-testable
/// without a broker.
///
/// - `op:profile_request` with a `meters` list → profiles for the intersection of
///   that list with the catalog;
/// - `op:startup` → everything known: profiles from the catalog **and** `op:key` for
///   every valid key held, mirroring how the original backend answered an
///   announcement;
/// - anything else → nothing.
///
/// `keys` is empty unless the operator passed `--keys`, so a default mock never puts
/// key material on the wire.
pub fn responses_for(msg: &Value, catalog: &Catalog, keys: &KeyStore) -> Vec<Value> {
    let profile = |id: u32, e: &CatalogEntry| json!({ "op": "profile", "meterid": id, "model": e.model, "firmware": e.firmware });
    match msg.get("op").and_then(|v| v.as_str()) {
        Some("profile_request") => {
            let requested: Vec<u32> = msg
                .get("meters")
                .and_then(|m| m.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_u64().map(|n| n as u32))
                        .collect()
                })
                .unwrap_or_default();
            requested
                .iter()
                .filter_map(|id| catalog.get(id).map(|e| profile(*id, e)))
                .collect()
        }
        Some("startup") => {
            let mut out: Vec<Value> = catalog.iter().map(|(id, e)| profile(*id, e)).collect();
            // Malformed keys are dropped rather than served: a bad key on the wire
            // would install and then silently fail every decrypt.
            let mut keyed: Vec<(u32, &str)> = keys
                .iter()
                .filter(|(_, k)| is_valid_aes128_hex(k))
                .collect();
            keyed.sort_by_key(|(id, _)| *id);
            out.extend(
                keyed
                    .into_iter()
                    .map(|(id, k)| json!({ "op": "key", "meterid": id, "key": k })),
            );
            out
        }
        _ => Vec::new(),
    }
}

/// Run the mock: subscribe to the gateway's data topic, answer on its control topic.
/// Blocks until killed.
pub fn run(config_path: &str, catalog_path: &str, keys_path: Option<&str>) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let catalog = load_catalog(catalog_path)?;
    let keys = match keys_path {
        Some(path) => {
            let ks = KeyStore::load_file(path).with_context(|| format!("read key file {path}"))?;
            let valid = ks.iter().filter(|(_, k)| is_valid_aes128_hex(k)).count();
            // Ids and counts only — never a key value.
            log::warn!(
                "serving {valid} AES key(s) of {} loaded; keys cross the broker in \
                 cleartext — use a trusted broker",
                ks.len()
            );
            ks
        }
        None => KeyStore::new(),
    };
    let data_topic = cfg.mqtt.data_topic.clone();
    let control_topic = cfg
        .mqtt
        .control_topic
        .clone()
        .context("config has no control-topic to answer on")?;

    let mut opts = MqttOptions::new(
        format!("{}-mockdm", cfg.mqtt.clientid),
        cfg.mqtt.host.clone(),
        cfg.mqtt.port,
    );
    opts.set_keep_alive(Duration::from_secs(30));
    let (client, mut connection) = Client::new(opts, 16);
    client.subscribe(&data_topic, QoS::AtLeastOnce)?;

    log::info!(
        "mock Device Manager: {} device(s) in catalog, {} key(s), watching {data_topic}, answering on {control_topic}",
        catalog.len(),
        keys.len()
    );

    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Incoming::Publish(p))) => {
                let Ok(msg) = serde_json::from_slice::<Value>(&p.payload) else {
                    continue;
                };
                let responses = responses_for(&msg, &catalog, &keys);
                if responses.is_empty() {
                    continue;
                }
                // Count by op — never the payloads, which may carry key material.
                let n_keys = responses.iter().filter(|r| r["op"] == "key").count();
                log::info!(
                    "answering {} with {} profile(s) and {n_keys} key(s)",
                    msg.get("op").and_then(|v| v.as_str()).unwrap_or("?"),
                    responses.len() - n_keys
                );
                for r in responses {
                    if let Err(e) = client.publish(
                        &control_topic,
                        QoS::AtLeastOnce,
                        false,
                        serde_json::to_vec(&r)?,
                    ) {
                        log::warn!("profile publish failed: {e}");
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                log::warn!("mqtt error: {e}; retrying");
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        let mut c = Catalog::new();
        c.insert(
            74644444,
            CatalogEntry {
                model: "MULTICAL 21".into(),
                firmware: None,
            },
        );
        c.insert(
            63398862,
            CatalogEntry {
                model: "MULTICAL 21".into(),
                firmware: None,
            },
        );
        c
    }

    #[test]
    fn profile_request_answers_only_the_intersection() {
        let out = responses_for(
            &json!({"op": "profile_request", "gw": "6543", "meters": [74644444, 99999999]}),
            &catalog(),
            &KeyStore::new(),
        );
        assert_eq!(out.len(), 1, "unknown meters get no invented profile");
        assert_eq!(out[0]["meterid"], 74644444);
        assert_eq!(out[0]["model"], "MULTICAL 21");
        assert_eq!(out[0]["op"], "profile");
    }

    #[test]
    fn startup_is_an_implicit_request_for_everything() {
        let out = responses_for(
            &json!({"op": "startup", "gw": "6543"}),
            &catalog(),
            &KeyStore::new(),
        );
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn other_ops_are_ignored() {
        assert!(responses_for(
            &json!({"op": "key", "meterid": 1}),
            &catalog(),
            &KeyStore::new()
        )
        .is_empty());
        assert!(responses_for(&json!({"foo": "bar"}), &catalog(), &KeyStore::new()).is_empty());
    }

    /// Keys are served on an announcement (as the original backend did), validated
    /// first, and — critically — everything served is accepted by the gateway's own
    /// key parser, so the two halves of the contract cannot drift.
    #[test]
    fn startup_serves_valid_keys_and_drops_malformed_ones() {
        let mut keys = KeyStore::new();
        // Published test key, not a real meter key.
        keys.install(74644444, "000102030405060708090a0b0c0d0e0f".to_string());
        keys.install(63398862, "nothex".to_string()); // malformed: must not be served
        let out = responses_for(&json!({"op": "startup"}), &catalog(), &keys);

        let key_msgs: Vec<_> = out.iter().filter(|r| r["op"] == "key").collect();
        assert_eq!(key_msgs.len(), 1, "only the well-formed key is served");
        assert_eq!(key_msgs[0]["meterid"], 74644444);
        assert!(
            KeyStore::parse_key_message(key_msgs[0]).is_some(),
            "mock served a key the gateway rejects"
        );
        assert_eq!(out.iter().filter(|r| r["op"] == "profile").count(), 2);
    }

    #[test]
    fn keys_are_never_served_without_opt_in_or_on_profile_requests() {
        let mut keys = KeyStore::new();
        keys.install(74644444, "000102030405060708090a0b0c0d0e0f".to_string());
        // A profile_request must never carry key material.
        let out = responses_for(
            &json!({"op": "profile_request", "meters": [74644444]}),
            &catalog(),
            &keys,
        );
        assert!(out.iter().all(|r| r["op"] != "key"));
        // And with no --keys, an announcement serves none.
        let out = responses_for(&json!({"op": "startup"}), &catalog(), &KeyStore::new());
        assert!(out.iter().all(|r| r["op"] != "key"));
    }
    #[test]
    fn served_profiles_pass_the_gateway_parser() {
        // The contract's two halves must agree: everything the mock serves must be
        // accepted by the gateway's parse_profile_message.
        for r in responses_for(&json!({"op": "startup"}), &catalog(), &KeyStore::new()) {
            assert!(
                crate::profiles::ProfileStore::parse_profile_message(&r).is_some(),
                "mock served a profile the gateway rejects: {r}"
            );
        }
    }
}
