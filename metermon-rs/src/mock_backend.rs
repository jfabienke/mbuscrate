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
//! names only, never key material.

use crate::config::Config;
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

/// The pure request→responses mapping, split out so the protocol is unit-testable
/// without a broker.
///
/// - `op:profile_request` with a `meters` list → profiles for the intersection of
///   that list with the catalog;
/// - `op:startup` → everything the catalog knows (the announcement is an implicit
///   request, mirroring how the old key backend behaved);
/// - anything else → nothing.
pub fn responses_for(msg: &Value, catalog: &Catalog) -> Vec<Value> {
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
        Some("startup") => catalog.iter().map(|(id, e)| profile(*id, e)).collect(),
        _ => Vec::new(),
    }
}

/// Run the mock: subscribe to the gateway's data topic, answer on its control topic.
/// Blocks until killed. Never touches key material.
pub fn run(config_path: &str, catalog_path: &str) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let catalog = load_catalog(catalog_path)?;
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
        "mock Device Manager: {} device(s) in catalog, watching {data_topic}, answering on {control_topic}",
        catalog.len()
    );

    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Incoming::Publish(p))) => {
                let Ok(msg) = serde_json::from_slice::<Value>(&p.payload) else {
                    continue;
                };
                let responses = responses_for(&msg, &catalog);
                if responses.is_empty() {
                    continue;
                }
                log::info!(
                    "answering {} with {} profile(s)",
                    msg.get("op").and_then(|v| v.as_str()).unwrap_or("?"),
                    responses.len()
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
        );
        assert_eq!(out.len(), 1, "unknown meters get no invented profile");
        assert_eq!(out[0]["meterid"], 74644444);
        assert_eq!(out[0]["model"], "MULTICAL 21");
        assert_eq!(out[0]["op"], "profile");
    }

    #[test]
    fn startup_is_an_implicit_request_for_everything() {
        let out = responses_for(&json!({"op": "startup", "gw": "6543"}), &catalog());
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn other_ops_are_ignored() {
        assert!(responses_for(&json!({"op": "key", "meterid": 1}), &catalog()).is_empty());
        assert!(responses_for(&json!({"foo": "bar"}), &catalog()).is_empty());
    }

    #[test]
    fn served_profiles_pass_the_gateway_parser() {
        // The contract's two halves must agree: everything the mock serves must be
        // accepted by the gateway's parse_profile_message.
        for r in responses_for(&json!({"op": "startup"}), &catalog()) {
            assert!(
                crate::profiles::ProfileStore::parse_profile_message(&r).is_some(),
                "mock served a profile the gateway rejects: {r}"
            );
        }
    }
}
