//! Lightweight Broker Mesh (LBM) Core for LoRa Mesh Networking
//!
//! Implements pub-sub topics and QoS for gateway-level relaying,
//! enabling mesh networking for distant devices (e.g., Kamstrup meters).
//! Inspired by LoRaWAN Private Network Server (LNS) concepts.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, RwLock};
use serde::{Serialize, Deserialize};
use log::{debug, info, warn};

/// Quality of Service levels for mesh messages
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum QoS {
    /// Best effort delivery (no retries)
    AtMostOnce = 0,

    /// Guaranteed delivery with retries
    AtLeastOnce = 1,

    /// Exactly once delivery (with deduplication)
    ExactlyOnce = 2,
}

/// Mesh message with routing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshMessage {
    /// Unique message ID
    pub id: u64,

    /// Source node address
    pub source: String,

    /// Destination (can be wildcard for broadcast)
    pub destination: String,

    /// Topic for pub-sub
    pub topic: String,

    /// Message payload
    pub payload: Vec<u8>,

    /// Quality of Service level
    pub qos: QoS,

    /// Time-to-live (hop count)
    pub ttl: u8,

    /// Timestamp
    pub timestamp: u64,

    /// Optional reply-to address
    pub reply_to: Option<String>,
}

/// Node information for mesh topology
#[derive(Debug, Clone)]
pub struct NodeInfo {
    /// Node address
    pub address: String,

    /// Last seen timestamp
    pub last_seen: Instant,

    /// Link quality (RSSI/SNR based)
    pub link_quality: f32,

    /// Subscribed topics
    pub topics: HashSet<String>,

    /// Is this a gateway node?
    pub is_gateway: bool,

    /// Hop count from this node
    pub hop_count: u8,
}

/// LBM Core for mesh networking
pub struct LbmCore {
    /// Our node address
    node_address: String,

    /// Known nodes in the mesh
    nodes: Arc<RwLock<HashMap<String, NodeInfo>>>,

    /// Topic subscriptions
    subscriptions: Arc<RwLock<HashMap<String, HashSet<String>>>>,

    /// Message cache for deduplication
    message_cache: Arc<Mutex<VecDeque<(u64, Instant)>>>,

    /// Pending acknowledgments (for QoS > 0)
    pending_acks: Arc<Mutex<HashMap<u64, PendingAck>>>,

    /// Message send channel
    tx: mpsc::Sender<MeshMessage>,

    /// Message receive channel
    #[allow(dead_code)]
    rx: Arc<Mutex<mpsc::Receiver<MeshMessage>>>,

    /// Gateway mode enabled
    is_gateway: bool,
}

/// Pending acknowledgment tracking
struct PendingAck {
    message: MeshMessage,
    retry_count: u8,
    next_retry: Instant,
}

impl LbmCore {
    /// Create a new LBM core instance
    pub fn new(node_address: String, is_gateway: bool) -> Self {
        let (tx, rx) = mpsc::channel(100);

        Self {
            node_address,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            message_cache: Arc::new(Mutex::new(VecDeque::with_capacity(1000))),
            pending_acks: Arc::new(Mutex::new(HashMap::new())),
            tx,
            rx: Arc::new(Mutex::new(rx)),
            is_gateway,
        }
    }

    /// Subscribe to a topic
    pub async fn subscribe(&self, topic: &str) -> Result<(), String> {
        let mut subs = self.subscriptions.write().await;
        subs.entry(topic.to_string())
            .or_insert_with(HashSet::new)
            .insert(self.node_address.clone());

        info!("Node {} subscribed to topic: {}", self.node_address, topic);

        // Broadcast subscription to mesh
        self.broadcast_subscription(topic).await?;
        Ok(())
    }

    /// Publish a message to a topic
    pub async fn publish(&self, topic: &str, payload: Vec<u8>, qos: QoS) -> Result<u64, String> {
        let msg_id = self.generate_message_id();

        let message = MeshMessage {
            id: msg_id,
            source: self.node_address.clone(),
            destination: format!("topic://{topic}"),
            topic: topic.to_string(),
            payload,
            qos,
            ttl: 16, // Default TTL
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            reply_to: None,
        };

        // Store for retry if QoS > 0
        if qos != QoS::AtMostOnce {
            let mut pending = self.pending_acks.lock().await;
            pending.insert(msg_id, PendingAck {
                message: message.clone(),
                retry_count: 0,
                next_retry: Instant::now() + Duration::from_secs(5),
            });
        }

        // Send to mesh
        self.route_message(message).await?;

        Ok(msg_id)
    }

    /// Route a message through the mesh
    async fn route_message(&self, mut message: MeshMessage) -> Result<(), String> {
        // Check TTL
        if message.ttl == 0 {
            debug!("Message {} dropped: TTL expired", message.id);
            return Ok(());
        }
        message.ttl -= 1;

        // Check for duplicate
        if self.is_duplicate(&message).await {
            debug!("Message {} dropped: duplicate", message.id);
            return Ok(());
        }

        // Store in cache
        self.cache_message(&message).await;

        // Determine next hop(s)
        let next_hops = self.determine_next_hops(&message).await?;

        for hop in next_hops {
            debug!("Routing message {} to {}", message.id, hop);
            // In real implementation, this would send via LoRa
            self.tx.send(message.clone()).await
                .map_err(|e| format!("Failed to route: {e}"))?;
        }

        Ok(())
    }

    /// Determine next hops for a message
    async fn determine_next_hops(&self, message: &MeshMessage) -> Result<Vec<String>, String> {
        let nodes = self.nodes.read().await;
        let mut next_hops = Vec::new();

        // Topic-based routing
        if message.destination.starts_with("topic://") {
            let topic = message.destination.strip_prefix("topic://").unwrap();
            let subs = self.subscriptions.read().await;

            if let Some(subscribers) = subs.get(topic) {
                for subscriber in subscribers {
                    if subscriber != &message.source {
                        next_hops.push(subscriber.clone());
                    }
                }
            }
        } else {
            // Direct routing
            if let Some(_node) = nodes.get(&message.destination) {
                next_hops.push(message.destination.clone());
            }
        }

        // Gateway relay
        if self.is_gateway && next_hops.is_empty() {
            // Relay to other gateways
            for (addr, info) in nodes.iter() {
                if info.is_gateway && addr != &message.source {
                    next_hops.push(addr.clone());
                }
            }
        }

        Ok(next_hops)
    }

    /// Check if message is duplicate
    async fn is_duplicate(&self, message: &MeshMessage) -> bool {
        let cache = self.message_cache.lock().await;
        cache.iter().any(|(id, _)| *id == message.id)
    }

    /// Cache message for deduplication
    async fn cache_message(&self, message: &MeshMessage) {
        let mut cache = self.message_cache.lock().await;
        cache.push_back((message.id, Instant::now()));

        // Limit cache size
        while cache.len() > 1000 {
            cache.pop_front();
        }

        // Remove old entries (> 5 minutes)
        let cutoff = Instant::now() - Duration::from_secs(300);
        cache.retain(|(_, time)| *time > cutoff);
    }

    /// Broadcast subscription update
    async fn broadcast_subscription(&self, topic: &str) -> Result<(), String> {
        let msg = MeshMessage {
            id: self.generate_message_id(),
            source: self.node_address.clone(),
            destination: "broadcast".to_string(),
            topic: "_mesh/subscribe".to_string(),
            payload: topic.as_bytes().to_vec(),
            qos: QoS::AtMostOnce,
            ttl: 8,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            reply_to: None,
        };

        self.route_message(msg).await
    }

    /// Process retry queue for QoS > 0
    pub async fn process_retries(&self) {
        let mut pending = self.pending_acks.lock().await;
        let now = Instant::now();
        let mut to_retry = Vec::new();

        for (id, ack) in pending.iter_mut() {
            if now >= ack.next_retry {
                if ack.retry_count < 3 {
                    ack.retry_count += 1;
                    ack.next_retry = now + Duration::from_secs(5 * (ack.retry_count as u64));
                    to_retry.push(ack.message.clone());
                } else {
                    warn!("Message {id} failed after 3 retries");
                }
            }
        }

        // Clean up failed messages
        pending.retain(|_, ack| ack.retry_count < 3);

        // Retry messages
        for msg in to_retry {
            if let Err(e) = self.route_message(msg).await {
                warn!("Retry failed: {e}");
            }
        }
    }

    /// Update node information
    pub async fn update_node(&self, address: String, rssi: i16, is_gateway: bool) {
        let mut nodes = self.nodes.write().await;

        let link_quality = ((rssi + 120) as f32 / 70.0).clamp(0.0, 1.0);

        nodes.entry(address.clone())
            .and_modify(|info| {
                info.last_seen = Instant::now();
                info.link_quality = link_quality;
            })
            .or_insert(NodeInfo {
                address,
                last_seen: Instant::now(),
                link_quality,
                topics: HashSet::new(),
                is_gateway,
                hop_count: 1,
            });
    }

    /// Generate unique message ID
    fn generate_message_id(&self) -> u64 {
        use rand::Rng;
        rand::thread_rng().gen()
    }

    /// Get mesh statistics
    pub async fn get_stats(&self) -> MeshStats {
        let nodes = self.nodes.read().await;
        let subs = self.subscriptions.read().await;
        let pending = self.pending_acks.lock().await;
        let cache = self.message_cache.lock().await;

        MeshStats {
            node_count: nodes.len(),
            gateway_count: nodes.values().filter(|n| n.is_gateway).count(),
            topic_count: subs.len(),
            pending_messages: pending.len(),
            cached_messages: cache.len(),
        }
    }
}

/// Mesh network statistics
#[derive(Debug, Clone, Serialize)]
pub struct MeshStats {
    pub node_count: usize,
    pub gateway_count: usize,
    pub topic_count: usize,
    pub pending_messages: usize,
    pub cached_messages: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lbm_pubsub() {
        let lbm = LbmCore::new("node1".to_string(), false);

        // Subscribe to topic
        lbm.subscribe("sensors/temperature").await.unwrap();

        // Publish message
        let msg_id = lbm.publish(
            "sensors/temperature",
            b"25.5".to_vec(),
            QoS::AtLeastOnce
        ).await.unwrap();

        assert!(msg_id > 0);
    }

    #[tokio::test]
    async fn test_mesh_stats() {
        let lbm = LbmCore::new("gateway1".to_string(), true);

        // Update some nodes
        lbm.update_node("node2".to_string(), -70, false).await;
        lbm.update_node("gateway2".to_string(), -65, true).await;

        let stats = lbm.get_stats().await;
        assert_eq!(stats.node_count, 2);
        assert_eq!(stats.gateway_count, 1);
    }
}