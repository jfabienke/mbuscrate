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

// Reference implementation of the (future) backend Device Manager: several projected
// types (observation inventory, position resolution) are defined ahead of being wired,
// so allow dead_code module-wide here rather than pepper per-item attributes.
#![allow(dead_code)]

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
        // Up-sync: the gateway reports the fleet it hears (see
        // docs/OP_OBSERVED_UPSYNC.md). Fire-and-forget for the observations; we
        // return an ack so the gateway can advance its sync watermark
        // deterministically. No profiles or keys are ever emitted in response.
        Some("observed") => {
            match (
                msg.get("gw").and_then(|v| v.as_str()),
                msg.get("seq").and_then(|v| v.as_u64()),
            ) {
                (Some(gw), Some(seq)) => {
                    vec![json!({ "op": "observed_ack", "gw": gw, "seq": seq })]
                }
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// One gateway's observation of one meter — the gateway-owned fields carried by
/// `op:observed`. Deliberately excludes identity/profile/keys (backend-owned).
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub gw: String,
    pub serial: u32,
    pub last_rssi: i64,
    pub last_seen: i64,
    pub gw_pos: Option<(f64, f64)>,
    /// Gateway RSSI-multilateration estimate — advisory only.
    pub pos_estimate: Option<(f64, f64)>,
}

/// Backend-side accumulator for `op:observed`, keyed `(gw, serial)` so gateways
/// never overwrite each other. Also tracks operator/app-supplied install-tags,
/// which always win over a gateway `pos_estimate`.
#[derive(Debug, Default)]
pub struct ObservedInventory {
    obs: BTreeMap<(String, u32), Observation>,
    /// Canonical meter positions set out-of-band (e.g. the technician app's
    /// install geotag). A present entry here is authoritative over any estimate.
    install_tags: BTreeMap<u32, (f64, f64)>,
    /// Highest `seq` applied per gateway — idempotency against QoS redelivery.
    seq_seen: BTreeMap<String, u64>,
}

impl ObservedInventory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an authoritative install-tag for a meter (app/backend side).
    pub fn set_install_tag(&mut self, serial: u32, lat: f64, lon: f64) {
        self.install_tags.insert(serial, (lat, lon));
    }

    /// Apply one `op:observed` message. Returns the number of observations stored,
    /// or `None` if the message is malformed or a stale/duplicate `seq`.
    pub fn apply(&mut self, msg: &Value) -> Option<usize> {
        if msg.get("op").and_then(|v| v.as_str()) != Some("observed") {
            return None;
        }
        let gw = msg.get("gw").and_then(|v| v.as_str())?.to_string();
        let seq = msg.get("seq").and_then(|v| v.as_u64())?;
        // Idempotent: ignore a seq we've already applied from this gateway.
        if let Some(&last) = self.seq_seen.get(&gw) {
            if seq <= last {
                return None;
            }
        }
        let gw_pos = msg
            .get("gw_pos")
            .and_then(|p| Some((p.get("lat")?.as_f64()?, p.get("lon")?.as_f64()?)));
        let meters = msg.get("meters").and_then(|m| m.as_array())?;
        let mut n = 0;
        for m in meters {
            let Some(serial) = m.get("serial").and_then(|v| v.as_u64()).map(|s| s as u32) else {
                continue;
            };
            let pos_estimate = m
                .get("pos_estimate")
                .and_then(|p| Some((p.get("lat")?.as_f64()?, p.get("lon")?.as_f64()?)));
            self.obs.insert(
                (gw.clone(), serial),
                Observation {
                    gw: gw.clone(),
                    serial,
                    last_rssi: m.get("last_rssi").and_then(|v| v.as_i64()).unwrap_or(0),
                    last_seen: m.get("last_seen").and_then(|v| v.as_i64()).unwrap_or(0),
                    gw_pos,
                    pos_estimate,
                },
            );
            n += 1;
        }
        self.seq_seen.insert(gw, seq);
        Some(n)
    }

    /// All observations of a meter, across gateways.
    pub fn observations(&self, serial: u32) -> Vec<&Observation> {
        self.obs
            .iter()
            .filter(|((_, s), _)| *s == serial)
            .map(|(_, o)| o)
            .collect()
    }

    /// Resolved position for a meter, applying the ownership rule: an install-tag
    /// always wins; otherwise the best (strongest-RSSI) gateway estimate, if any.
    pub fn resolved_position(&self, serial: u32) -> Option<(f64, f64)> {
        if let Some(&tag) = self.install_tags.get(&serial) {
            return Some(tag);
        }
        self.observations(serial)
            .into_iter()
            .filter_map(|o| o.pos_estimate.map(|p| (o.last_rssi, p)))
            .max_by_key(|(rssi, _)| *rssi) // strongest signal = least negative
            .map(|(_, p)| p)
    }

    /// True when a gateway estimate disagrees with the install-tag beyond
    /// `tol_deg` — a mislabel / moved-meter signal worth surfacing for review.
    pub fn position_conflict(&self, serial: u32, tol_deg: f64) -> bool {
        let Some(&(tlat, tlon)) = self.install_tags.get(&serial) else {
            return false;
        };
        self.observations(serial).into_iter().any(|o| {
            o.pos_estimate.is_some_and(|(elat, elon)| {
                (elat - tlat).abs() > tol_deg || (elon - tlon).abs() > tol_deg
            })
        })
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
mod observed_tests {
    use super::*;
    use serde_json::json;

    fn observed(gw: &str, seq: u64, gw_pos: Option<(f64, f64)>, meters: Value) -> Value {
        let mut m = json!({ "op": "observed", "ts": 1, "gw": gw, "seq": seq, "meters": meters });
        if let Some((lat, lon)) = gw_pos {
            m["gw_pos"] = json!({ "lat": lat, "lon": lon, "hdop": 0.8, "fix_ts": 1 });
        }
        m
    }

    #[test]
    fn observed_gets_an_ack_carrying_gw_and_seq() {
        let msg = observed("6543", 42, None, json!([]));
        let r = responses_for(&msg, &Catalog::new(), &KeyStore::new());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0]["op"], "observed_ack");
        assert_eq!(r[0]["gw"], "6543");
        assert_eq!(r[0]["seq"], 42);
    }

    #[test]
    fn observed_without_gw_or_seq_is_ignored() {
        let msg = json!({ "op": "observed", "meters": [] });
        assert!(responses_for(&msg, &Catalog::new(), &KeyStore::new()).is_empty());
    }

    #[test]
    fn responses_never_leak_keys_or_profiles_for_observed() {
        // Even when the observed meter IS in the catalog, an observed message
        // draws only an ack — never a profile or key.
        let mut cat = Catalog::new();
        cat.insert(
            74644444,
            CatalogEntry {
                model: "MULTICAL 21".into(),
                firmware: None,
            },
        );
        let msg = observed("6543", 1, None, json!([{ "serial": 74644444 }]));
        let r = responses_for(&msg, &cat, &KeyStore::new());
        assert!(r.iter().all(|m| m["op"] == "observed_ack"));
    }

    #[test]
    fn inventory_stores_observations_keyed_by_gw_and_serial() {
        let mut inv = ObservedInventory::new();
        let msg = observed(
            "6543",
            1,
            Some((56.16, 10.20)),
            json!([{ "serial": 85312884, "last_rssi": -63, "last_seen": 100 }]),
        );
        assert_eq!(inv.apply(&msg), Some(1));
        let obs = inv.observations(85312884);
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].last_rssi, -63);
        assert_eq!(obs[0].gw_pos, Some((56.16, 10.20)));
    }

    #[test]
    fn duplicate_or_stale_seq_is_idempotent() {
        let mut inv = ObservedInventory::new();
        let m = json!([{ "serial": 1, "last_rssi": -50, "last_seen": 10 }]);
        assert_eq!(inv.apply(&observed("6543", 5, None, m.clone())), Some(1));
        // same seq again → ignored
        assert_eq!(inv.apply(&observed("6543", 5, None, m.clone())), None);
        // lower seq → ignored
        assert_eq!(inv.apply(&observed("6543", 4, None, m)), None);
    }

    #[test]
    fn two_gateways_hearing_one_meter_are_kept_separately() {
        let mut inv = ObservedInventory::new();
        inv.apply(&observed(
            "gwA",
            1,
            Some((56.0, 10.0)),
            json!([{ "serial": 42, "last_rssi": -60 }]),
        ));
        inv.apply(&observed(
            "gwB",
            1,
            Some((56.1, 10.1)),
            json!([{ "serial": 42, "last_rssi": -80 }]),
        ));
        assert_eq!(inv.observations(42).len(), 2);
    }

    #[test]
    fn install_tag_wins_over_gateway_estimate() {
        let mut inv = ObservedInventory::new();
        inv.set_install_tag(42, 56.5, 10.5);
        inv.apply(&observed(
            "gwA",
            1,
            None,
            json!([{
                "serial": 42, "last_rssi": -60,
                "pos_estimate": { "lat": 56.0, "lon": 10.0, "confidence_m": 50 }
            }]),
        ));
        // Canonical position is the install-tag, not the estimate.
        assert_eq!(inv.resolved_position(42), Some((56.5, 10.5)));
        // …and the disagreement is flagged for review.
        assert!(inv.position_conflict(42, 0.01));
    }

    #[test]
    fn estimate_fills_the_gap_when_no_install_tag() {
        let mut inv = ObservedInventory::new();
        // Two gateways estimate; strongest RSSI wins.
        inv.apply(&observed(
            "gwA",
            1,
            None,
            json!([{
            "serial": 7, "last_rssi": -90,
            "pos_estimate": { "lat": 1.0, "lon": 1.0 } }]),
        ));
        inv.apply(&observed(
            "gwB",
            1,
            None,
            json!([{
            "serial": 7, "last_rssi": -55,
            "pos_estimate": { "lat": 2.0, "lon": 2.0 } }]),
        ));
        assert_eq!(inv.resolved_position(7), Some((2.0, 2.0)));
        assert!(!inv.position_conflict(7, 0.01)); // no install-tag → no conflict
    }
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
