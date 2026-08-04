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
}
