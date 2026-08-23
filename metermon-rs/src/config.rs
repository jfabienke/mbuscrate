//! Parser for `metermon.conf` — the same JSON config the C++ gateway reads.
//!
//! Shape observed on the deployed gateway (gwid 6543):
//! ```json
//! {
//!   "gwid": "6543",
//!   "mqtt": { "host": "...", "clientid": "meter6543",
//!             "data-topic": "meter/data/6543", "control-topic": "meter/control/6543" },
//!   "devices": { "wmbus0": { "type": "WMBUS", "spidev": "/dev/spidev0.1" } },
//!   "keys": { "<meterid>": "<32-hex AES-128 key>", ... }
//! }
//! ```
//! The `keys` map is optional here and may live in the same file or be supplied
//! separately; without it, encrypted frames decode only as far as the header.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub gwid: String,
    pub mqtt: MqttConfig,
    #[serde(default)]
    pub devices: BTreeMap<String, DeviceConfig>,
    /// Optional offline seed for the keystore: meterid(decimal) -> 32-hex key.
    /// The real gateway receives keys over the control topic at runtime (see
    /// `keystore`), so this is empty in production configs and used only for
    /// self-contained testing.
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
    /// Optional dedicated broker for the AES key pull, for deployments whose primary
    /// upstream has no key functionality. Only the control topic is subscribed there;
    /// data/status/health stay on the primary `mqtt` broker.
    #[serde(rename = "key-mqtt", default)]
    pub key_mqtt: Option<KeyMqttConfig>,
    /// Optional gpsd address (e.g. "127.0.0.1:2947"). When set, the monitor tags its
    /// `op:observed` up-sync with the gateway's position. Absent = no GPS.
    #[serde(default)]
    pub gps: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MqttConfig {
    pub host: String,
    #[serde(default = "default_mqtt_port")]
    pub port: u16,
    pub clientid: String,
    #[serde(rename = "data-topic")]
    pub data_topic: String,
    #[serde(rename = "control-topic")]
    pub control_topic: Option<String>,
    /// Optional override for the gateway health/status topic base. When absent it is
    /// derived from `gwid` (see [`Config::gateway_status_topic`]).
    #[serde(rename = "status-topic", default)]
    pub status_topic: Option<String>,
}

fn default_mqtt_port() -> u16 {
    1883
}

/// Broker carrying only the AES key control topic (see [`Config::key_mqtt`]).
#[derive(Debug, Clone, Deserialize)]
pub struct KeyMqttConfig {
    pub host: String,
    #[serde(default = "default_mqtt_port")]
    pub port: u16,
    /// Client id on the key broker. Must differ from the primary's if both point at
    /// the same broker; defaults to `<primary clientid>-keys`.
    pub clientid: Option<String>,
    /// Control topic to subscribe on the key broker; defaults to the primary
    /// `control-topic`.
    #[serde(rename = "control-topic")]
    pub control_topic: Option<String>,
    /// Gateway id to announce (`op:startup`) on the key broker, for keys provisioned
    /// under an older gateway identity. Defaults to the primary `gwid`. The backend
    /// pushes keys in response to the announcement — they are not retained.
    pub gwid: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceConfig {
    #[serde(rename = "type")]
    pub dev_type: String,
    pub spidev: Option<String>,
    /// Radio backend for this device: "sx1262" (default, mbus-rs's own driver),
    /// "seeed" (the seeed-wm1302 driver, board-selectable — see `board`), or "rfm69".
    /// The default tracks the hardware on the gateway; the RFM69 HAT has been replaced
    /// by the Waveshare SX1262 XXXM, but the driver remains selectable for other boards.
    #[serde(default)]
    pub driver: Option<String>,
    /// Radio HAT profile for the "seeed" driver: "waveshare-sx1262-pi5" (default) or
    /// "seeed-wm1302". Selects the electrical profile + pin wiring; the wM-Bus receive
    /// path is identical on both (wM-Bus is always on the SX1262). Ignored by other drivers.
    #[serde(default)]
    pub board: Option<String>,
    /// Regional band for the "seeed-wm1302" board profile (e.g. "eu868"). Ignored by the
    /// Waveshare profile and by other drivers.
    #[serde(default)]
    pub region: Option<String>,
    /// Periodic LoRa listen windows carved out of wM-Bus receive (SX1262 only).
    /// Absent = wM-Bus continuously, no switching.
    #[serde(default, rename = "lora-listen")]
    pub lora_listen: Option<LoraListenConfig>,
}

/// Dual-mode cadence: how often, and for how long, the radio leaves the wM-Bus
/// profile to listen for LoRa. Each window uses the next (frequency, SF) from the
/// rotation, so coverage of the sweep matrix accumulates across windows rather
/// than costing one long outage.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LoraListenConfig {
    /// Seconds between window starts.
    #[serde(default = "default_lora_period")]
    pub period_secs: u64,
    /// Seconds each window stays in LoRa.
    #[serde(default = "default_lora_window")]
    pub window_secs: u64,
    /// Frequencies to rotate through (Hz). Default: the EU868 join channels.
    #[serde(default = "default_lora_freqs")]
    pub freqs_hz: Vec<u32>,
    /// Spreading factors to rotate through. Default: SF12/9/7 — the join ladder's
    /// most common rungs, slowest first.
    #[serde(default = "default_lora_sfs")]
    pub sfs: Vec<u8>,
}

fn default_lora_period() -> u64 {
    120
}
fn default_lora_window() -> u64 {
    8
}
fn default_lora_freqs() -> Vec<u32> {
    vec![868_100_000, 868_300_000, 868_500_000]
}
fn default_lora_sfs() -> Vec<u8> {
    vec![12, 9, 7]
}

impl LoraListenConfig {
    /// The `(frequency, SF)` for rotation step `n`. Frequencies advance fastest, so
    /// one SF pass covers every channel before the ladder moves.
    ///
    /// `None` when either list is empty — a config [`Config::validate`] rejects, but
    /// guarded here too so no caller can index an empty vec (the panic finding).
    /// Pure and host-testable, independent of the `radio` feature that drives it.
    pub fn point_at(&self, n: usize) -> Option<(u32, u8)> {
        if self.freqs_hz.is_empty() || self.sfs.is_empty() {
            return None;
        }
        let f = self.freqs_hz[n % self.freqs_hz.len()];
        let sf = self.sfs[(n / self.freqs_hz.len()) % self.sfs.len()];
        Some((f, sf))
    }
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path.as_ref())?;
        let cfg: Config = serde_json::from_str(&text)?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Reject configs that would panic or misbehave at runtime. Today: a
    /// `lora-listen` block whose `freqs_hz` or `sfs` is explicitly empty — serde
    /// defaults are non-empty, but an explicit `[]` otherwise reaches the rotation
    /// and indexes an empty vec.
    fn validate(&self) -> anyhow::Result<()> {
        for (name, dev) in &self.devices {
            if let Some(l) = &dev.lora_listen {
                if l.freqs_hz.is_empty() {
                    anyhow::bail!("device {name}: lora-listen.freqs_hz must not be empty");
                }
                if l.sfs.is_empty() {
                    anyhow::bail!("device {name}: lora-listen.sfs must not be empty");
                }
            }
        }
        Ok(())
    }

    /// Retained topic carrying the gateway's `online`/`offline` state (the latter delivered
    /// by the broker via MQTT Last-Will). Alongside `meter/data`/`meter/control`.
    pub fn gateway_status_topic(&self) -> String {
        self.mqtt
            .status_topic
            .clone()
            .unwrap_or_else(|| format!("meter/gateway/{}/status", self.gwid))
    }

    /// Topic carrying periodic [`crate::health::GatewayHealth`] heartbeats.
    pub fn gateway_health_topic(&self) -> String {
        format!("meter/gateway/{}/health", self.gwid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_deployed_shape() {
        let json = r#"{
            "gwid": "6543",
            "mqtt": { "host": "mqtt.ringgaard.com", "clientid": "meter6543",
                      "data-topic": "meter/data/6543", "control-topic": "meter/control/6543" },
            "devices": { "wmbus0": { "type": "WMBUS", "spidev": "/dev/spidev0.1" } }
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.gwid, "6543");
        assert_eq!(cfg.mqtt.data_topic, "meter/data/6543");
        assert_eq!(cfg.mqtt.port, 1883); // defaulted
        assert_eq!(
            cfg.devices["wmbus0"].spidev.as_deref(),
            Some("/dev/spidev0.1")
        );
        assert!(cfg.keys.is_empty());
        assert!(cfg.key_mqtt.is_none());
    }

    #[test]
    fn parses_dedicated_key_broker() {
        let json = r#"{
            "gwid": "6543",
            "mqtt": { "host": "mqtt.ringgaard.com", "clientid": "meter6543",
                      "data-topic": "meter/data/6543", "control-topic": "meter/control/6543" },
            "key-mqtt": { "host": "192.168.50.101", "control-topic": "meter/control/gateway-001",
                          "gwid": "gateway-001" }
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        let kb = cfg.key_mqtt.expect("key-mqtt section");
        assert_eq!(kb.host, "192.168.50.101");
        assert_eq!(kb.port, 1883); // defaulted
        assert_eq!(kb.clientid, None); // derived at connect time
        assert_eq!(
            kb.control_topic.as_deref(),
            Some("meter/control/gateway-001")
        );
        assert_eq!(kb.gwid.as_deref(), Some("gateway-001"));
    }

    fn lora(freqs: Vec<u32>, sfs: Vec<u8>) -> LoraListenConfig {
        LoraListenConfig {
            period_secs: 120,
            window_secs: 8,
            freqs_hz: freqs,
            sfs,
        }
    }

    #[test]
    fn point_at_advances_frequencies_before_the_sf_ladder() {
        let l = lora(vec![868_100_000, 868_300_000, 868_500_000], vec![12, 9]);
        // One SF pass covers every channel before the ladder steps.
        assert_eq!(l.point_at(0), Some((868_100_000, 12)));
        assert_eq!(l.point_at(1), Some((868_300_000, 12)));
        assert_eq!(l.point_at(2), Some((868_500_000, 12)));
        assert_eq!(l.point_at(3), Some((868_100_000, 9)));
        assert_eq!(l.point_at(5), Some((868_500_000, 9)));
        assert_eq!(
            l.point_at(6),
            Some((868_100_000, 12)),
            "wraps after 3x2 points"
        );
    }

    #[test]
    fn point_at_is_none_for_empty_lists() {
        // The panic finding, at the pure boundary: empty lists yield None instead of
        // indexing an empty vec.
        assert_eq!(lora(vec![], vec![9]).point_at(0), None);
        assert_eq!(lora(vec![868_100_000], vec![]).point_at(0), None);
    }

    fn cfg_with_lora(freqs: &str, sfs: &str) -> Config {
        let json = format!(
            r#"{{"gwid":"6543",
                 "mqtt":{{"host":"h","clientid":"c","data-topic":"t"}},
                 "devices":{{"wmbus0":{{"type":"WMBUS","spidev":"/dev/x",
                     "lora-listen":{{"freqs_hz":{freqs},"sfs":{sfs}}}}}}}}}"#
        );
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn validate_rejects_empty_lora_lists() {
        assert!(cfg_with_lora("[]", "[9]").validate().is_err());
        assert!(cfg_with_lora("[868100000]", "[]").validate().is_err());
        assert!(cfg_with_lora("[868100000]", "[9]").validate().is_ok());
        // A config with no lora-listen at all is unaffected.
        let plain: Config = serde_json::from_str(
            r#"{"gwid":"6543","mqtt":{"host":"h","clientid":"c","data-topic":"t"}}"#,
        )
        .unwrap();
        assert!(plain.validate().is_ok());
    }

    #[test]
    fn load_runs_validation_on_the_file_path() {
        use std::io::Write;
        let mut path = std::env::temp_dir();
        path.push(format!("metermon-cfg-loadtest-{}.json", std::process::id()));
        let write = |body: &str| {
            std::fs::File::create(&path)
                .unwrap()
                .write_all(body.as_bytes())
                .unwrap();
        };

        // An empty freqs_hz must be rejected via load()'s validate() call, not just
        // when validate() is invoked directly.
        write(
            r#"{"gwid":"6543","mqtt":{"host":"h","clientid":"c","data-topic":"t"},
                "devices":{"wmbus0":{"type":"WMBUS","spidev":"/dev/x",
                    "lora-listen":{"freqs_hz":[],"sfs":[9]}}}}"#,
        );
        let err = Config::load(&path).unwrap_err().to_string();
        assert!(
            err.contains("freqs_hz"),
            "load must reject via validate: {err}"
        );

        // A valid config loads through the same path.
        write(
            r#"{"gwid":"6543","mqtt":{"host":"h","clientid":"c","data-topic":"t"},
                "devices":{"wmbus0":{"type":"WMBUS","spidev":"/dev/x",
                    "lora-listen":{"freqs_hz":[868100000],"sfs":[9]}}}}"#,
        );
        assert!(Config::load(&path).is_ok());
        let _ = std::fs::remove_file(&path);
    }
}
