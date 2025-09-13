//! LoRaWAN Class B/C Support for Scheduled Reception
//!
//! Implements ping-slot timing for Class B and continuous reception
//! for Class C devices, enabling server-initiated downlinks.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, sleep};
use log::{debug, info, warn};

/// LoRaWAN device class
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceClass {
    /// Class A: Bi-directional with scheduled downlink after uplink
    ClassA,

    /// Class B: Scheduled receive windows (ping slots)
    ClassB,

    /// Class C: Continuous receive (except when transmitting)
    ClassC,
}

/// Class B beacon configuration
#[derive(Debug, Clone)]
pub struct BeaconConfig {
    /// Beacon period (typically 128 seconds)
    pub period: Duration,

    /// Ping slot periodicity (2^n slots per beacon)
    pub ping_nb: u8,

    /// Ping slot data rate
    pub ping_dr: u8,

    /// Ping slot frequency in Hz
    pub ping_freq: u32,

    /// Next beacon time
    pub next_beacon: Instant,
}

impl Default for BeaconConfig {
    fn default() -> Self {
        Self {
            period: Duration::from_secs(128),
            ping_nb: 3, // 8 ping slots per beacon
            ping_dr: 3,  // SF9/125kHz
            ping_freq: 869_525_000,
            next_beacon: Instant::now() + Duration::from_secs(128),
        }
    }
}

/// Multicast session for Class B/C
#[derive(Debug, Clone)]
pub struct MulticastSession {
    /// Multicast address
    pub address: u32,

    /// Network session key
    pub nwk_s_key: [u8; 16],

    /// Application session key
    pub app_s_key: [u8; 16],

    /// Downlink frame counter
    pub fcnt_down: u32,

    /// Data rate for this session
    pub data_rate: u8,

    /// Frequency for this session
    pub frequency: u32,
}

/// Class B/C controller
pub struct ClassBCController {
    /// Current device class
    device_class: Arc<RwLock<DeviceClass>>,

    /// Beacon configuration for Class B
    beacon_config: Arc<RwLock<BeaconConfig>>,

    /// Multicast sessions
    multicast_sessions: Arc<RwLock<Vec<MulticastSession>>>,

    /// Ping slot schedule
    ping_slots: Arc<Mutex<Vec<Instant>>>,

    /// Class C receive window state
    class_c_active: Arc<Mutex<bool>>,

    /// Beacon synchronization status
    beacon_locked: Arc<Mutex<bool>>,

    /// Last beacon reception time
    last_beacon: Arc<Mutex<Option<Instant>>>,
}

impl Default for ClassBCController {
    fn default() -> Self {
        Self::new()
    }
}

impl ClassBCController {
    /// Create a new Class B/C controller
    pub fn new() -> Self {
        Self {
            device_class: Arc::new(RwLock::new(DeviceClass::ClassA)),
            beacon_config: Arc::new(RwLock::new(BeaconConfig::default())),
            multicast_sessions: Arc::new(RwLock::new(Vec::new())),
            ping_slots: Arc::new(Mutex::new(Vec::new())),
            class_c_active: Arc::new(Mutex::new(false)),
            beacon_locked: Arc::new(Mutex::new(false)),
            last_beacon: Arc::new(Mutex::new(None)),
        }
    }

    /// Switch device class
    pub async fn set_device_class(&self, class: DeviceClass) -> Result<(), String> {
        let mut current = self.device_class.write().await;
        let old_class = *current;
        *current = class;

        info!("Switching from {old_class:?} to {class:?}");

        match class {
            DeviceClass::ClassA => {
                // Disable Class B/C features
                *self.class_c_active.lock().await = false;
                *self.beacon_locked.lock().await = false;
            }
            DeviceClass::ClassB => {
                // Start beacon search
                *self.class_c_active.lock().await = false;
                self.start_beacon_search().await?;
            }
            DeviceClass::ClassC => {
                // Enable continuous reception
                *self.beacon_locked.lock().await = false;
                *self.class_c_active.lock().await = true;
            }
        }

        Ok(())
    }

    /// Start beacon search for Class B
    async fn start_beacon_search(&self) -> Result<(), String> {
        info!("Starting beacon search for Class B operation");

        // In real implementation, this would configure radio for beacon reception
        // For now, simulate beacon lock after a delay
        sleep(Duration::from_secs(2)).await;

        *self.beacon_locked.lock().await = true;
        *self.last_beacon.lock().await = Some(Instant::now());

        // Calculate ping slots
        self.calculate_ping_slots().await?;

        info!("Beacon locked, Class B operation ready");
        Ok(())
    }

    /// Calculate ping slot schedule
    async fn calculate_ping_slots(&self) -> Result<(), String> {
        let config = self.beacon_config.read().await;
        let mut slots = self.ping_slots.lock().await;

        slots.clear();

        let num_slots = 1 << config.ping_nb;
        let slot_period = config.period.as_millis() / num_slots;

        let base_time = config.next_beacon;

        for i in 0..num_slots {
            let slot_time = base_time + Duration::from_millis(i as u64 * slot_period as u64);
            slots.push(slot_time);
        }

        debug!("Calculated {} ping slots", slots.len());
        Ok(())
    }

    /// Get next ping slot time
    pub async fn get_next_ping_slot(&self) -> Option<Instant> {
        let slots = self.ping_slots.lock().await;
        let now = Instant::now();

        slots.iter()
            .find(|&&slot| slot > now)
            .copied()
    }

    /// Add multicast session
    pub async fn add_multicast_session(&self, session: MulticastSession) {
        let mut sessions = self.multicast_sessions.write().await;

        // Remove existing session with same address
        sessions.retain(|s| s.address != session.address);

        info!("Added multicast session: 0x{:08X}", session.address);
        sessions.push(session);
    }

    /// Process Class B ping slot
    pub async fn process_ping_slot(&self) -> Result<(), String> {
        if !*self.beacon_locked.lock().await {
            return Err("Beacon not locked".to_string());
        }

        let config = self.beacon_config.read().await;

        debug!("Opening ping slot: DR{} @ {} MHz",
               config.ping_dr, config.ping_freq as f64 / 1_000_000.0);

        // In real implementation, configure radio for reception
        // Window duration depends on data rate
        let window_duration = self.calculate_window_duration(config.ping_dr);
        sleep(window_duration).await;

        Ok(())
    }

    /// Process Class C continuous reception
    pub async fn run_class_c(&self, mut shutdown_rx: tokio::sync::oneshot::Receiver<()>) {
        info!("Starting Class C continuous reception");

        let mut check_interval = interval(Duration::from_millis(100));

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    info!("Class C reception stopping");
                    break;
                }

                _ = check_interval.tick() => {
                    if !*self.class_c_active.lock().await {
                        continue;
                    }

                    // In real implementation, this would check for incoming packets
                    // For now, just maintain active state
                    debug!("Class C RX active");
                }
            }
        }
    }

    /// Calculate receive window duration based on data rate
    fn calculate_window_duration(&self, data_rate: u8) -> Duration {
        // Window duration increases with spreading factor
        match data_rate {
            0 => Duration::from_millis(100), // SF12
            1 => Duration::from_millis(80),  // SF11
            2 => Duration::from_millis(60),  // SF10
            3 => Duration::from_millis(40),  // SF9
            4 => Duration::from_millis(30),  // SF8
            5 => Duration::from_millis(20),  // SF7
            _ => Duration::from_millis(50),
        }
    }

    /// Handle beacon reception
    pub async fn handle_beacon(&self, timestamp: Instant) {
        *self.last_beacon.lock().await = Some(timestamp);
        *self.beacon_locked.lock().await = true;

        // Update next beacon time
        let mut config = self.beacon_config.write().await;
        config.next_beacon = timestamp + config.period;

        // Recalculate ping slots
        if let Err(e) = self.calculate_ping_slots().await {
            warn!("Failed to calculate ping slots: {e}");
        }
    }

    /// Get Class B/C status
    pub async fn get_status(&self) -> ClassBCStatus {
        ClassBCStatus {
            device_class: *self.device_class.read().await,
            beacon_locked: *self.beacon_locked.lock().await,
            class_c_active: *self.class_c_active.lock().await,
            multicast_sessions: self.multicast_sessions.read().await.len(),
            next_ping_slot: self.get_next_ping_slot().await,
        }
    }
}

/// Class B/C operation status
#[derive(Debug, Clone)]
pub struct ClassBCStatus {
    pub device_class: DeviceClass,
    pub beacon_locked: bool,
    pub class_c_active: bool,
    pub multicast_sessions: usize,
    pub next_ping_slot: Option<Instant>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_class_switching() {
        let controller = ClassBCController::new();

        // Start as Class A
        assert_eq!(*controller.device_class.read().await, DeviceClass::ClassA);

        // Switch to Class B
        controller.set_device_class(DeviceClass::ClassB).await.unwrap();
        assert_eq!(*controller.device_class.read().await, DeviceClass::ClassB);

        // Should have beacon lock after search
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert!(*controller.beacon_locked.lock().await);
    }

    #[tokio::test]
    async fn test_multicast_session() {
        let controller = ClassBCController::new();

        let session = MulticastSession {
            address: 0x12345678,
            nwk_s_key: [0; 16],
            app_s_key: [0; 16],
            fcnt_down: 0,
            data_rate: 3,
            frequency: 869_525_000,
        };

        controller.add_multicast_session(session).await;

        let status = controller.get_status().await;
        assert_eq!(status.multicast_sessions, 1);
    }
}