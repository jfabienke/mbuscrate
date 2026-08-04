//! MQTT publisher — mirrors metermon's data topic so a live run drops into the
//! same place in the pipeline (or a shadow topic for a non-destructive compare).

use anyhow::Result;
use rumqttc::{Client, Event, Incoming, LastWill, MqttOptions, QoS};
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
    pub fn connect(
        cfg: &MqttConfig,
        topic_override: Option<&str>,
        last_will: Option<(String, Vec<u8>)>,
    ) -> Result<Self> {
        Self::connect_inner(
            &cfg.host,
            cfg.port,
            &cfg.clientid,
            topic_override.unwrap_or(&cfg.data_topic),
            last_will,
        )
    }

    /// Connect to an arbitrary broker for a subscription-only role (e.g. the dedicated
    /// AES key broker): no data topic, no last will. Keep the returned `Publisher`
    /// alive — dropping it disconnects.
    pub fn connect_subscriber(host: &str, port: u16, clientid: &str) -> Result<Self> {
        Self::connect_inner(host, port, clientid, "", None)
    }

    fn connect_inner(
        host: &str,
        port: u16,
        clientid: &str,
        topic: &str,
        last_will: Option<(String, Vec<u8>)>,
    ) -> Result<Self> {
        let mut opts = MqttOptions::new(clientid, host, port);
        opts.set_keep_alive(Duration::from_secs(30));
        // Persistent session: the broker keeps the control-topic subscription (and queues
        // QoS1 messages) across reconnects. With a clean session, any reconnect silently
        // dropped the subscription and the key pull went dead until restart.
        opts.set_clean_session(false);
        // Register a retained Last-Will so the broker announces the gateway `offline` if it
        // drops off ungracefully (crash, power loss, network partition) — remote
        // dead-gateway detection without any polling upstream.
        if let Some((topic, payload)) = last_will {
            opts.set_last_will(LastWill::new(topic, payload, QoS::AtLeastOnce, true));
        }
        let (client, mut connection) = Client::new(opts, 16);

        let control: ControlHandler = Arc::new(Mutex::new(None));
        let control_cb = control.clone();
        std::thread::spawn(move || {
            for event in connection.iter() {
                match event {
                    Ok(Event::Incoming(Incoming::Publish(p))) => {
                        if let Some(handler) = control_cb.lock().unwrap().as_ref() {
                            if let Ok(msg) = serde_json::from_slice::<serde_json::Value>(&p.payload)
                            {
                                handler(&msg);
                            }
                        }
                    }
                    Ok(_) => {}
                    // Unreachable broker: back off instead of spinning through reconnects.
                    Err(_) => std::thread::sleep(Duration::from_secs(1)),
                }
            }
        });

        Ok(Self {
            client,
            topic: topic.to_string(),
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

    /// Publish JSON to an arbitrary topic (e.g. the gateway health heartbeat).
    pub fn publish_to(&mut self, topic: &str, value: &serde_json::Value) -> Result<()> {
        let payload = serde_json::to_vec(value)?;
        self.client
            .publish(topic, QoS::AtLeastOnce, false, payload)?;
        Ok(())
    }

    /// Publish a retained payload (e.g. the `online`/`offline` gateway status/birth
    /// message), so a subscriber always sees the last known state on connect.
    pub fn publish_retained(&mut self, topic: &str, payload: &[u8]) -> Result<()> {
        self.client
            .publish(topic, QoS::AtLeastOnce, true, payload.to_vec())?;
        Ok(())
    }

    pub fn topic(&self) -> &str {
        &self.topic
    }
}
