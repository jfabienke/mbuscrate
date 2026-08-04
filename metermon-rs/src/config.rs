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

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceConfig {
    #[serde(rename = "type")]
    pub dev_type: String,
    pub spidev: Option<String>,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path.as_ref())?;
        let cfg: Config = serde_json::from_str(&text)?;
        Ok(cfg)
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
    }
}
