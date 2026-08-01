//! MQTT publisher — mirrors metermon's data topic so a live run drops into the
//! same place in the pipeline (or a shadow topic for a non-destructive compare).

use anyhow::Result;
use rumqttc::{Client, Event, Incoming, MqttOptions, QoS};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::config::MqttConfig;

type ControlHandler = Arc<Mutex<Option<Box<dyn Fn(&serde_json::Value) + Send>>>>;

pub struct Publisher {
    client: Client,
    topic: String,
    // Shared slot the connection drainer invokes on each incoming control message.
    control: ControlHandler,
}

impl Publisher {
    /// Connect using the config's host/clientid. `topic_override` lets a shadow
    /// run publish to e.g. `meter/data/6543-rust` instead of the live topic.
    ///
    /// The connection event loop must be polled for either publishing or receiving
    /// to work, so it is always driven on a background thread here; incoming control
    /// messages are routed to a handler slot that `subscribe_control` may fill.
    pub fn connect(cfg: &MqttConfig, topic_override: Option<&str>) -> Result<Self> {
        let mut opts = MqttOptions::new(cfg.clientid.clone(), cfg.host.clone(), cfg.port);
        opts.set_keep_alive(Duration::from_secs(30));
        let (client, mut connection) = Client::new(opts, 16);

        let control: ControlHandler = Arc::new(Mutex::new(None));
        let control_cb = control.clone();
        std::thread::spawn(move || {
            for event in connection.iter() {
                if let Ok(Event::Incoming(Incoming::Publish(p))) = event {
                    if let Some(handler) = control_cb.lock().unwrap().as_ref() {
                        if let Ok(msg) = serde_json::from_slice::<serde_json::Value>(&p.payload) {
                            handler(&msg);
                        }
                    }
                }
            }
        });

        Ok(Self {
            client,
            topic: topic_override.unwrap_or(&cfg.data_topic).to_string(),
            control,
        })
    }

    /// Subscribe to the control topic and hand each incoming JSON message to
    /// `handler` (mirrors metermon's control listener; used to install keys).
    pub fn subscribe_control<F>(&mut self, control_topic: &str, handler: F) -> Result<()>
    where
        F: Fn(&serde_json::Value) + Send + 'static,
    {
        *self.control.lock().unwrap() = Some(Box::new(handler));
        self.client.subscribe(control_topic, QoS::AtLeastOnce)?;
        Ok(())
    }

    pub fn publish_json(&mut self, value: &serde_json::Value) -> Result<()> {
        let payload = serde_json::to_vec(value)?;
        self.client
            .publish(&self.topic, QoS::AtLeastOnce, false, payload)?;
        Ok(())
    }

    pub fn topic(&self) -> &str {
        &self.topic
    }
}
