//! # Wireless M-Bus (wM-Bus) Handle
//!
//! This module provides the WMBusHandle struct, which represents a high-level handle to the
//! wireless M-Bus (wM-Bus) system. It integrates the radio driver, frame handling, and
//! device discovery to provide a simple async API for wM-Bus communication.
//!
//! ## Features
//!
//! - Automatic radio configuration for wM-Bus operation
//! - Frame transmission with LBT (Listen Before Talk) compliance
//! - Continuous frame reception with background processing
//! - Device discovery and network scanning
//! - Async/await interface for all operations
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use mbus_rs::wmbus::handle::WMBusHandle;
//! // On a Pi, build a `RaspberryPiHal` instead (requires the `raspberry-pi` feature).
//! use mbus_rs::wmbus::radio::hal::MockHal;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize with HAL for your platform
//!     let hal = MockHal::new();
//!
//!     // Create wM-Bus handle (None => default configuration)
//!     let mut wmbus = WMBusHandle::new(hal, None).await?;
//!     
//!     // Start receiving frames
//!     wmbus.start_receiver().await?;
//!     
//!     // Scan for devices
//!     let devices = wmbus.scan_devices().await?;
//!     println!("Found {} devices", devices.len());
//!     
//!     Ok(())
//! }
//! ```

use crate::wmbus::frame::{ParseError, WMBusFrame};
use crate::wmbus::radio::driver::{
    DeviceErrors, DriverError, LbtConfig, LoRaRxInfo, ModeTaggedPacket, RadioStats,
    RadioStatusReport, Sx126xDriver,
};
use crate::wmbus::radio::hal::Hal;
use crate::wmbus::radio::irq::IrqStatus;
use crate::wmbus::radio::modulation::PacketType;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{sleep, timeout, Duration, Instant};

/// Type aliases for complex types to improve readability
type FrameReceiver = Arc<RwLock<Option<mpsc::UnboundedReceiver<ReceivedItem>>>>;
type FrameSender = mpsc::UnboundedSender<ReceivedItem>;
type UnsolicitedCallback = Arc<dyn Fn(&WMBusFrame) + Send + Sync>;

/// A received item tagged with the modem it arrived on.
///
/// The receiver routes each packet by the driver's active modem, captured atomically with
/// the packet (see [`Sx126xDriver::process_irqs_with_mode`]): GFSK payloads are parsed as
/// wM-Bus frames; LoRa payloads are surfaced **raw** with their radio metadata (no payload
/// decoding happens here). Delivered over [`WMBusHandle::recv_item`].
#[derive(Debug, Clone)]
pub enum ReceivedItem {
    /// A parsed wM-Bus frame received on the GFSK modem.
    Wmbus {
        /// The parsed frame.
        frame: WMBusFrame,
        /// RSSI for this frame, in dBm.
        rssi_dbm: i16,
    },
    /// A raw LoRa payload with its receive metadata (undecoded).
    Lora {
        /// Raw payload bytes.
        payload: Vec<u8>,
        /// RSSI for this packet, in dBm.
        rssi_dbm: i16,
        /// LoRa demodulator metadata.
        lora: LoRaRxInfo,
    },
}

/// Route a mode-tagged packet from the driver into a [`ReceivedItem`], or `None` if it
/// should be dropped: a GFSK payload that fails wM-Bus parsing, or — defensively — a LoRa
/// packet missing its metadata (which [`Sx126xDriver::process_irqs_with_mode`]'s invariant
/// already prevents).
fn route_packet(packet: ModeTaggedPacket) -> Option<ReceivedItem> {
    let ModeTaggedPacket {
        mode,
        payload,
        rssi_dbm,
        lora,
    } = packet;
    match mode {
        PacketType::Gfsk => match crate::wmbus::frame::parse_wmbus_frame(&payload) {
            Ok(frame) => Some(ReceivedItem::Wmbus { frame, rssi_dbm }),
            Err(e) => {
                log::debug!("Dropping unparseable wM-Bus frame: {e:?}");
                None
            }
        },
        PacketType::LoRa => match lora {
            Some(lora) => Some(ReceivedItem::Lora {
                payload,
                rssi_dbm,
                lora,
            }),
            None => {
                log::error!("LoRa packet arrived without metadata; dropping");
                None
            }
        },
    }
}
type WMBusFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<(WMBusFrame, i16), WMBusError>> + Send + 'a>,
>;
type SendFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), WMBusError>> + Send + 'a>>;

/// wM-Bus handle errors
#[derive(Error, Debug)]
pub enum WMBusError {
    /// Radio driver error
    #[error("Radio error: {0}")]
    Radio(#[from] DriverError),
    /// Frame parsing error
    #[error("Frame parse error: {0:?}")]
    FrameParse(#[from] ParseError),
    /// Device not found
    #[error("Device not found: {address}")]
    DeviceNotFound { address: u32 },
    /// Communication timeout
    #[error("Communication timeout")]
    Timeout,
    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    /// Network error
    #[error("Network error: {0}")]
    Network(String),
}

/// Configuration for wM-Bus operation
#[derive(Debug, Clone)]
pub struct WMBusConfig {
    /// Operating frequency in Hz (e.g., 868_950_000 for EU S-mode)
    pub frequency_hz: u32,
    /// Data rate in bits per second (typically 100_000 for wM-Bus)
    pub bitrate: u32,
    /// Listen Before Talk configuration
    pub lbt_config: LbtConfig,
    /// Frame reception timeout in milliseconds
    pub rx_timeout_ms: u32,
    /// Device discovery timeout in milliseconds
    pub discovery_timeout_ms: u32,
}

impl Default for WMBusConfig {
    fn default() -> Self {
        Self {
            frequency_hz: 868_950_000,        // EU wM-Bus S-mode frequency
            bitrate: 100_000,                 // 100 kbps
            lbt_config: LbtConfig::default(), // EU compliant LBT settings
            rx_timeout_ms: 5000,              // 5 second receive timeout
            discovery_timeout_ms: 30000,      // 30 second discovery timeout
        }
    }
}

/// Builder for WMBusConfig with fluent API and preset configurations
pub struct WMBusConfigBuilder {
    config: WMBusConfig,
}

impl WMBusConfigBuilder {
    /// Create a new builder with default values
    pub fn new() -> Self {
        Self {
            config: WMBusConfig::default(),
        }
    }

    /// Configure for EU wM-Bus S-mode (868.95 MHz, 100 kbps)
    pub fn eu_s_mode() -> Self {
        Self {
            config: WMBusConfig {
                frequency_hz: 868_950_000,
                bitrate: 100_000,
                lbt_config: LbtConfig::default(),
                rx_timeout_ms: 5000,
                discovery_timeout_ms: 30000,
            },
        }
    }

    /// Configure for EU wM-Bus T-mode (868.3 MHz, 100 kbps)
    pub fn eu_t_mode() -> Self {
        Self {
            config: WMBusConfig {
                frequency_hz: 868_300_000,
                bitrate: 100_000,
                lbt_config: LbtConfig::default(),
                rx_timeout_ms: 5000,
                discovery_timeout_ms: 30000,
            },
        }
    }

    /// Configure for EU wM-Bus N-mode (multiple frequencies)
    /// Note: This sets the primary frequency; actual N-mode requires scanning multiple channels
    pub fn eu_n_mode() -> Self {
        Self {
            config: WMBusConfig {
                frequency_hz: 869_525_000, // Primary N-mode frequency
                bitrate: 4800,             // 4.8 kbps for N-mode
                lbt_config: LbtConfig::default(),
                rx_timeout_ms: 10000, // Longer timeout for slower data rate
                discovery_timeout_ms: 60000, // Longer discovery time
            },
        }
    }

    /// Configure for high-performance scenarios (fast scanning, short timeouts)
    pub fn fast_scan() -> Self {
        Self {
            config: WMBusConfig {
                frequency_hz: 868_950_000,
                bitrate: 100_000,
                lbt_config: LbtConfig {
                    rssi_threshold_dbm: -85,
                    listen_duration_ms: 2, // Shorter LBT duration
                    max_retries: 2,        // Fewer retries
                },
                rx_timeout_ms: 2000,         // Shorter timeout
                discovery_timeout_ms: 10000, // Faster discovery
            },
        }
    }

    /// Configure for long-range scenarios (sensitive reception, long timeouts)
    pub fn long_range() -> Self {
        Self {
            config: WMBusConfig {
                frequency_hz: 868_950_000,
                bitrate: 100_000,
                lbt_config: LbtConfig {
                    rssi_threshold_dbm: -95, // More sensitive
                    listen_duration_ms: 10,  // Longer LBT
                    max_retries: 5,          // More retries
                },
                rx_timeout_ms: 15000,         // Longer timeout
                discovery_timeout_ms: 120000, // Extended discovery
            },
        }
    }

    /// Set operating frequency in Hz
    pub fn frequency(mut self, frequency_hz: u32) -> Self {
        self.config.frequency_hz = frequency_hz;
        self
    }

    /// Set data rate in bits per second
    pub fn bitrate(mut self, bitrate: u32) -> Self {
        self.config.bitrate = bitrate;
        self
    }

    /// Set Listen Before Talk configuration
    pub fn lbt_config(mut self, lbt_config: LbtConfig) -> Self {
        self.config.lbt_config = lbt_config;
        self
    }

    /// Set receive timeout in milliseconds
    pub fn rx_timeout_ms(mut self, timeout_ms: u32) -> Self {
        self.config.rx_timeout_ms = timeout_ms;
        self
    }

    /// Set device discovery timeout in milliseconds
    pub fn discovery_timeout_ms(mut self, timeout_ms: u32) -> Self {
        self.config.discovery_timeout_ms = timeout_ms;
        self
    }

    /// Build the final configuration
    pub fn build(self) -> WMBusConfig {
        self.config
    }
}

impl Default for WMBusConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a discovered wM-Bus device
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Device address (A-field)
    pub address: u32,
    /// Manufacturer ID (M-field)
    pub manufacturer_id: u16,
    /// Device version
    pub version: u8,
    /// Device type
    pub device_type: u8,
    /// RSSI when last seen (dBm)
    pub rssi_dbm: i16,
    /// Timestamp of last frame reception
    pub last_seen: std::time::Instant,
}

/// Represents a handle to the Wireless M-Bus (wM-Bus) connection
pub struct WMBusHandle<H: Hal> {
    /// Radio driver for SX126x
    driver: Arc<Mutex<Sx126xDriver<H>>>,
    /// wM-Bus configuration
    config: WMBusConfig,
    /// Receiver task handle
    receiver_handle: Option<tokio::task::JoinHandle<()>>,
    /// Channel for received frames
    rx_channel: FrameReceiver,
    /// Sender for frame reception (internal)
    tx_sender: Option<FrameSender>,
    /// Device registry for discovered devices
    devices: Arc<RwLock<HashMap<u32, DeviceInfo>>>,
    /// Callback for unsolicited frames
    unsolicited_callback: Option<UnsolicitedCallback>,
}

impl<H: Hal + Send + 'static> WMBusHandle<H> {
    /// Create a new wM-Bus handle with the provided HAL
    ///
    /// Initializes the radio driver and configures it for wM-Bus operation.
    ///
    /// # Arguments
    ///
    /// * `hal` - Hardware abstraction layer implementation
    /// * `config` - wM-Bus configuration (optional, uses defaults if None)
    ///
    /// # Returns
    ///
    /// * `Ok(WMBusHandle)` - Successfully initialized handle
    /// * `Err(WMBusError)` - Initialization failed
    pub async fn new(hal: H, config: Option<WMBusConfig>) -> Result<Self, WMBusError> {
        let config = config.unwrap_or_default();

        // Initialize radio driver with 32MHz crystal (typical for SX126x)
        let mut driver = Sx126xDriver::new(hal, 32_000_000);

        // Configure radio for wM-Bus operation
        driver.configure_for_wmbus(config.frequency_hz, config.bitrate)?;

        // Set up communication channels
        let (tx_sender, rx_receiver) = mpsc::unbounded_channel();

        Ok(WMBusHandle {
            driver: Arc::new(Mutex::new(driver)),
            config,
            receiver_handle: None,
            rx_channel: Arc::new(RwLock::new(Some(rx_receiver))),
            tx_sender: Some(tx_sender),
            devices: Arc::new(RwLock::new(HashMap::new())),
            unsolicited_callback: None,
        })
    }

    /// The shared radio driver behind this handle, for constructing a
    /// [`ProfileScheduler`](crate::wmbus::radio::scheduler::ProfileScheduler) that drives
    /// profile transitions on the **same** radio instance — never a second driver.
    pub fn shared_driver(&self) -> Arc<Mutex<Sx126xDriver<H>>> {
        self.driver.clone()
    }

    /// Poll the radio once for a mode-tagged received item: lock the driver, capture the
    /// packet atomically with its modem (see [`Sx126xDriver::process_irqs_with_mode`]), and
    /// route it. Shared by the background receiver loop and the tests.
    async fn poll_once(
        driver: &Arc<Mutex<Sx126xDriver<H>>>,
    ) -> Result<Option<ReceivedItem>, DriverError> {
        let mut guard = driver.lock().await;
        Ok(guard.process_irqs_with_mode()?.and_then(route_packet))
    }

    /// Start continuous frame reception in background
    ///
    /// Spawns a background task that continuously monitors for incoming wM-Bus frames.
    /// Received frames are parsed and made available through the receive channel.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Receiver started successfully
    /// * `Err(WMBusError)` - Failed to start receiver
    pub async fn start_receiver(&mut self) -> Result<(), WMBusError> {
        if self.receiver_handle.is_some() {
            return Err(WMBusError::InvalidConfig(
                "Receiver already running".to_string(),
            ));
        }

        let driver = self.driver.clone();
        let tx_sender = self
            .tx_sender
            .take()
            .ok_or_else(|| WMBusError::InvalidConfig("TX sender not available".to_string()))?;
        let devices = self.devices.clone();
        let unsolicited_callback = self.unsolicited_callback.clone();

        // Spawn background receiver task
        let handle = tokio::spawn(async move {
            let mut consecutive_errors = 0;

            // Arm continuous RX once. If a ProfileScheduler shares this driver it re-arms RX
            // on every profile transition, so the receiver must NOT issue its own RX command
            // each iteration — that would compete with those transitions. RX is re-armed only
            // defensively after a run of poll errors.
            {
                let mut driver_guard = driver.lock().await;
                if let Err(e) = driver_guard.set_rx_continuous() {
                    log::error!("Failed to arm continuous RX: {e:?}");
                }
            }

            loop {
                tokio::time::sleep(Duration::from_millis(10)).await;

                // One atomic, mode-tagged poll under a single driver lock.
                match Self::poll_once(&driver).await {
                    Ok(Some(item)) => {
                        consecutive_errors = 0;

                        // Device registry + unsolicited callback apply to wM-Bus frames.
                        if let ReceivedItem::Wmbus { frame, rssi_dbm } = &item {
                            Self::update_device_registry(&devices, frame, *rssi_dbm).await;
                            if let Some(callback) = &unsolicited_callback {
                                callback(frame);
                            }
                        }

                        // Send the tagged item to the channel.
                        if tx_sender.send(item).is_err() {
                            log::warn!("Frame channel receiver dropped");
                            break;
                        }
                    }
                    Ok(None) => {
                        // No packet available; keep polling.
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        log::warn!(
                            "Radio error in receiver: {e:?} (consecutive: {consecutive_errors})"
                        );

                        // After a run of errors, re-arm RX and back off.
                        if consecutive_errors > 10 {
                            log::error!("Too many consecutive radio errors; re-arming RX");
                            {
                                let mut driver_guard = driver.lock().await;
                                let _ = driver_guard.set_rx_continuous();
                            }
                            sleep(Duration::from_millis(5000)).await;
                            consecutive_errors = 0;
                        }
                    }
                }
            }
        });

        self.receiver_handle = Some(handle);
        Ok(())
    }

    /// Stop the background frame receiver
    pub async fn stop_receiver(&mut self) {
        if let Some(handle) = self.receiver_handle.take() {
            handle.abort();
        }
    }

    /// Send a wM-Bus frame
    ///
    /// Transmits a frame using LBT (Listen Before Talk) compliance if configured.
    ///
    /// # Arguments
    ///
    /// * `frame` - Frame to transmit
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Frame transmitted successfully
    /// * `Err(WMBusError)` - Transmission failed
    pub async fn send_frame(&self, frame: &WMBusFrame) -> Result<(), WMBusError> {
        let frame_bytes = frame.to_bytes();
        let mut driver = self.driver.lock().await;

        // Use LBT transmission for regulatory compliance
        driver.lbt_transmit(&frame_bytes, self.config.lbt_config)?;

        log::info!("Transmitted frame to device {:#X}", frame.device_address);
        Ok(())
    }

    /// Receive the next tagged item (wM-Bus or LoRa) with timeout.
    ///
    /// This is the full dual-mode stream. Callers that only want wM-Bus frames can use the
    /// [`WMBusHandle::recv_frame`] compatibility wrapper instead.
    ///
    /// # Returns
    /// * `Ok(ReceivedItem)` - the next received item
    /// * `Err(WMBusError::Timeout)` - nothing received within the timeout
    /// * `Err(WMBusError)` - other error
    pub async fn recv_item(&mut self, timeout_ms: Option<u32>) -> Result<ReceivedItem, WMBusError> {
        let timeout_duration =
            Duration::from_millis(timeout_ms.unwrap_or(self.config.rx_timeout_ms) as u64);

        let mut rx_guard = self.rx_channel.write().await;
        let rx_channel = rx_guard
            .as_mut()
            .ok_or_else(|| WMBusError::InvalidConfig("RX channel not available".to_string()))?;

        match timeout(timeout_duration, rx_channel.recv()).await {
            Ok(Some(item)) => Ok(item),
            Ok(None) => Err(WMBusError::Network("Frame channel closed".to_string())),
            Err(_) => Err(WMBusError::Timeout),
        }
    }

    /// Receive the next **wM-Bus** frame with timeout (compatibility wrapper).
    ///
    /// Returns the next [`ReceivedItem::Wmbus`], **skipping and discarding** any
    /// [`ReceivedItem::Lora`] items that arrive first — all within one absolute deadline
    /// (the timeout does not reset per skipped item). Because skipped LoRa packets are
    /// consumed from the stream, callers that need *every* packet must use
    /// [`WMBusHandle::recv_item`] instead.
    ///
    /// # Returns
    /// * `Ok((frame, rssi))` - the next wM-Bus frame and its RSSI
    /// * `Err(WMBusError::Timeout)` - no wM-Bus frame arrived before the deadline
    /// * `Err(WMBusError)` - other error
    pub async fn recv_frame(
        &mut self,
        timeout_ms: Option<u32>,
    ) -> Result<(WMBusFrame, i16), WMBusError> {
        let timeout_duration =
            Duration::from_millis(timeout_ms.unwrap_or(self.config.rx_timeout_ms) as u64);
        let deadline = Instant::now() + timeout_duration;

        let mut rx_guard = self.rx_channel.write().await;
        let rx_channel = rx_guard
            .as_mut()
            .ok_or_else(|| WMBusError::InvalidConfig("RX channel not available".to_string()))?;

        loop {
            // One absolute deadline shared across skipped LoRa items.
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(WMBusError::Timeout);
            }
            match timeout(remaining, rx_channel.recv()).await {
                Ok(Some(ReceivedItem::Wmbus { frame, rssi_dbm })) => return Ok((frame, rssi_dbm)),
                Ok(Some(ReceivedItem::Lora { .. })) => continue, // skip/consume LoRa items
                Ok(None) => return Err(WMBusError::Network("Frame channel closed".to_string())),
                Err(_) => return Err(WMBusError::Timeout),
            }
        }
    }

    /// Test-only: inject an item directly into the receive channel (bypassing the radio),
    /// so `recv_item`/`recv_frame` routing can be tested without hardware.
    #[cfg(test)]
    fn inject_item(&self, item: ReceivedItem) {
        if let Some(sender) = &self.tx_sender {
            let _ = sender.send(item);
        }
    }

    /// Test-only: one mode-tagged poll of the shared driver (what the background receiver
    /// does each iteration), so an end-to-end test can step reception deterministically.
    #[cfg(test)]
    async fn poll_once_test(&self) -> Result<Option<ReceivedItem>, DriverError> {
        Self::poll_once(&self.driver).await
    }

    /// Scan for wM-Bus devices
    ///
    /// Listens for device transmissions for the configured discovery timeout
    /// and returns information about discovered devices.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<DeviceInfo>)` - List of discovered devices
    /// * `Err(WMBusError)` - Scan failed
    pub async fn scan_devices(&mut self) -> Result<Vec<DeviceInfo>, WMBusError> {
        // Clear device registry
        self.devices.write().await.clear();

        // Ensure receiver is running
        if self.receiver_handle.is_none() {
            self.start_receiver().await?;
        }

        log::info!(
            "Starting device discovery for {} seconds",
            self.config.discovery_timeout_ms / 1000
        );

        // Wait for discovery timeout
        sleep(Duration::from_millis(
            self.config.discovery_timeout_ms as u64,
        ))
        .await;

        // Return discovered devices
        let devices = self.devices.read().await;
        let device_list: Vec<DeviceInfo> = devices.values().cloned().collect();

        log::info!(
            "Device discovery completed: {} devices found",
            device_list.len()
        );
        Ok(device_list)
    }

    /// Get information about a specific device
    ///
    /// # Arguments
    ///
    /// * `address` - Device address to look up
    ///
    /// # Returns
    ///
    /// * `Ok(DeviceInfo)` - Device information
    /// * `Err(WMBusError::DeviceNotFound)` - Device not in registry
    pub async fn get_device_info(&self, address: u32) -> Result<DeviceInfo, WMBusError> {
        let devices = self.devices.read().await;
        devices
            .get(&address)
            .cloned()
            .ok_or(WMBusError::DeviceNotFound { address })
    }

    /// Register callback for unsolicited frames
    ///
    /// # Arguments
    ///
    /// * `callback` - Function to call when unsolicited frames are received
    pub fn register_unsolicited_data_callback<F>(&mut self, callback: F)
    where
        F: Fn(&WMBusFrame) + Send + Sync + 'static,
    {
        self.unsolicited_callback = Some(Arc::new(callback));
    }

    /// Get radio status for diagnostics
    ///
    /// # Returns
    ///
    /// * Radio driver status information
    pub async fn get_radio_status(
        &self,
    ) -> Result<crate::wmbus::radio::driver::RadioStatusReport, WMBusError> {
        let mut driver = self.driver.lock().await;
        let state = driver.get_state()?;

        // Build a basic RadioStatusReport
        Ok(RadioStatusReport {
            state,
            stats: RadioStats::default(),
            device_errors: DeviceErrors::default(),
            irq_status: IrqStatus::default(),
            last_state_change: None,
        })
    }

    /// Update device registry with information from received frame
    async fn update_device_registry(
        devices: &Arc<RwLock<HashMap<u32, DeviceInfo>>>,
        frame: &WMBusFrame,
        rssi_dbm: i16,
    ) {
        let device_info = DeviceInfo {
            address: frame.device_address,
            manufacturer_id: frame.manufacturer_id,
            version: frame.version,
            device_type: frame.device_type,
            rssi_dbm,
            last_seen: std::time::Instant::now(),
        };

        let mut devices_guard = devices.write().await;
        devices_guard.insert(frame.device_address, device_info);
    }
}

/// Type-erased wrapper for WMBusHandle to enable storage in device manager
///
/// This trait provides a common interface for WMBusHandle operations while hiding
/// the specific HAL implementation type. This allows the device manager to work
/// with different hardware platforms without being generic over the HAL type.
pub trait WMBusHandleWrapper: Send + Sync {
    /// Send a wM-Bus frame
    fn send_frame<'a>(&'a self, frame: &'a WMBusFrame) -> SendFuture<'a>;

    /// Receive a frame with timeout
    fn recv_frame<'a>(&'a mut self, timeout_ms: Option<u32>) -> WMBusFuture<'a>;

    /// Start the background receiver
    fn start_receiver<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), WMBusError>> + Send + 'a>>;

    /// Stop the background receiver
    fn stop_receiver<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>>;

    /// Scan for wM-Bus devices
    fn scan_devices<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<DeviceInfo>, WMBusError>> + Send + 'a>,
    >;

    /// Get information about a specific device
    fn get_device_info<'a>(
        &'a self,
        address: u32,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<DeviceInfo, WMBusError>> + Send + 'a>,
    >;

    /// Get radio status for diagnostics
    fn get_radio_status<'a>(
        &'a self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<crate::wmbus::radio::driver::RadioStatusReport, WMBusError>,
                > + Send
                + 'a,
        >,
    >;
}

/// Implementation of WMBusHandleWrapper for any HAL type
impl<H: Hal + Send + 'static> WMBusHandleWrapper for WMBusHandle<H> {
    fn send_frame<'a>(
        &'a self,
        frame: &'a WMBusFrame,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), WMBusError>> + Send + 'a>>
    {
        Box::pin(WMBusHandle::send_frame(self, frame))
    }

    fn recv_frame<'a>(
        &'a mut self,
        timeout_ms: Option<u32>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(WMBusFrame, i16), WMBusError>> + Send + 'a>,
    > {
        Box::pin(WMBusHandle::recv_frame(self, timeout_ms))
    }

    fn start_receiver<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), WMBusError>> + Send + 'a>>
    {
        Box::pin(WMBusHandle::start_receiver(self))
    }

    fn stop_receiver<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async {
            WMBusHandle::stop_receiver(self).await;
        })
    }

    fn scan_devices<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<DeviceInfo>, WMBusError>> + Send + 'a>,
    > {
        Box::pin(WMBusHandle::scan_devices(self))
    }

    fn get_device_info<'a>(
        &'a self,
        address: u32,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<DeviceInfo, WMBusError>> + Send + 'a>,
    > {
        Box::pin(WMBusHandle::get_device_info(self, address))
    }

    fn get_radio_status<'a>(
        &'a self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<crate::wmbus::radio::driver::RadioStatusReport, WMBusError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(WMBusHandle::get_radio_status(self))
    }
}

/// Factory methods for creating wM-Bus handles with different HAL implementations
pub struct WMBusHandleFactory;

impl WMBusHandleFactory {
    /// Create a new wM-Bus handle with mock HAL for testing
    ///
    /// This creates a handle that uses a mock hardware abstraction layer,
    /// suitable for unit testing and development without physical hardware.
    ///
    /// # Returns
    ///
    /// A boxed trait object that can be used in the device manager
    ///
    /// # Example
    ///
    /// ```rust
    /// use mbus_rs::wmbus::handle::WMBusHandleFactory;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let handle = WMBusHandleFactory::create_mock().await?;
    ///     // Use handle for testing...
    ///     Ok(())
    /// }
    /// ```
    pub async fn create_mock() -> Result<Box<dyn WMBusHandleWrapper>, WMBusError> {
        use crate::wmbus::radio::hal::MockHal;

        let hal = MockHal::new();
        let config = WMBusConfig::default();
        let handle = WMBusHandle::new(hal, Some(config)).await?;
        Ok(Box::new(handle))
    }

    #[cfg(feature = "raspberry-pi")]
    /// Create a new wM-Bus handle for Raspberry Pi with default configuration
    ///
    /// Uses the default GPIO pins and SPI settings suitable for most setups:
    /// - SPI0 (/dev/spidev0.0)
    /// - BUSY pin: GPIO 25
    /// - DIO1 pin: GPIO 24
    /// - 8 MHz SPI speed
    ///
    /// # Returns
    ///
    /// A boxed trait object that can be used in the device manager
    ///
    /// # Errors
    ///
    /// Returns an error if the GPIO or SPI initialization fails
    ///
    /// # Example
    ///
    /// ```rust
    /// use mbus_rs::wmbus::handle::WMBusHandleFactory;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let handle = WMBusHandleFactory::create_raspberry_pi().await?;
    ///     // Use handle for wM-Bus communication...
    ///     Ok(())
    /// }
    /// ```
    pub async fn create_raspberry_pi() -> Result<Box<dyn WMBusHandleWrapper>, WMBusError> {
        use crate::wmbus::radio::driver::DriverError;
        use crate::wmbus::radio::hal::raspberry_pi::RaspberryPiHalBuilder;

        let hal = RaspberryPiHalBuilder::default()
            .build()
            .map_err(|_| WMBusError::Radio(DriverError::InvalidParams))?;

        let config = WMBusConfigBuilder::eu_s_mode().build();
        let handle = WMBusHandle::new(hal, Some(config)).await?;
        Ok(Box::new(handle))
    }

    #[cfg(feature = "raspberry-pi")]
    /// Create a new wM-Bus handle for Raspberry Pi with custom configuration
    ///
    /// Allows full control over GPIO pins and SPI settings.
    ///
    /// # Arguments
    ///
    /// * `spi_bus` - SPI bus number (0 or 1)
    /// * `spi_speed` - SPI clock speed in Hz
    /// * `busy_pin` - BCM GPIO number for BUSY signal
    /// * `dio1_pin` - BCM GPIO number for DIO1 interrupt
    /// * `dio2_pin` - Optional BCM GPIO number for DIO2 interrupt
    /// * `reset_pin` - Optional BCM GPIO number for reset control
    ///
    /// # Returns
    ///
    /// A boxed trait object that can be used in the device manager
    ///
    /// # Example
    ///
    /// ```rust
    /// use mbus_rs::wmbus::handle::WMBusHandleFactory;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let handle = WMBusHandleFactory::create_raspberry_pi_custom(
    ///         0,        // SPI0
    ///         8_000_000, // 8 MHz
    ///         25,       // BUSY on GPIO 25
    ///         24,       // DIO1 on GPIO 24
    ///         Some(23), // DIO2 on GPIO 23
    ///         Some(22), // RESET on GPIO 22
    ///     ).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn create_raspberry_pi_custom(
        spi_bus: u8,
        spi_speed: u32,
        busy_pin: u8,
        dio1_pin: u8,
        dio2_pin: Option<u8>,
        reset_pin: Option<u8>,
    ) -> Result<Box<dyn WMBusHandleWrapper>, WMBusError> {
        use crate::wmbus::radio::driver::DriverError;
        use crate::wmbus::radio::hal::raspberry_pi::{GpioPins, RaspberryPiHalBuilder};

        let gpio_pins = GpioPins {
            busy: busy_pin,
            dio1: dio1_pin,
            dio2: dio2_pin,
            reset: reset_pin,
        };

        let hal = RaspberryPiHalBuilder::new()
            .spi_bus(spi_bus)
            .spi_speed(spi_speed)
            .gpio_pins(gpio_pins)
            .build()
            .map_err(|_| WMBusError::Radio(DriverError::InvalidParams))?;

        let config = WMBusConfigBuilder::eu_s_mode().build();
        let handle = WMBusHandle::new(hal, Some(config)).await?;
        Ok(Box::new(handle))
    }

    #[cfg(feature = "raspberry-pi")]
    /// Create a new wM-Bus handle for Raspberry Pi optimized for fast scanning
    pub async fn create_raspberry_pi_fast_scan() -> Result<Box<dyn WMBusHandleWrapper>, WMBusError>
    {
        use crate::wmbus::radio::driver::DriverError;
        use crate::wmbus::radio::hal::raspberry_pi::RaspberryPiHalBuilder;

        let hal = RaspberryPiHalBuilder::default()
            .build()
            .map_err(|_| WMBusError::Radio(DriverError::InvalidParams))?;

        let config = WMBusConfigBuilder::fast_scan().build();
        let handle = WMBusHandle::new(hal, Some(config)).await?;
        Ok(Box::new(handle))
    }

    #[cfg(feature = "raspberry-pi")]
    /// Create a new wM-Bus handle for Raspberry Pi optimized for long-range reception
    pub async fn create_raspberry_pi_long_range() -> Result<Box<dyn WMBusHandleWrapper>, WMBusError>
    {
        use crate::wmbus::radio::driver::DriverError;
        use crate::wmbus::radio::hal::raspberry_pi::RaspberryPiHalBuilder;

        let hal = RaspberryPiHalBuilder::default()
            .build()
            .map_err(|_| WMBusError::Radio(DriverError::InvalidParams))?;

        let config = WMBusConfigBuilder::long_range().build();
        let handle = WMBusHandle::new(hal, Some(config)).await?;
        Ok(Box::new(handle))
    }

    #[cfg(feature = "raspberry-pi")]
    /// Create a new wM-Bus handle for Raspberry Pi configured for EU T-mode
    pub async fn create_raspberry_pi_t_mode() -> Result<Box<dyn WMBusHandleWrapper>, WMBusError> {
        use crate::wmbus::radio::driver::DriverError;
        use crate::wmbus::radio::hal::raspberry_pi::RaspberryPiHalBuilder;

        let hal = RaspberryPiHalBuilder::default()
            .build()
            .map_err(|_| WMBusError::Radio(DriverError::InvalidParams))?;

        let config = WMBusConfigBuilder::eu_t_mode().build();
        let handle = WMBusHandle::new(hal, Some(config)).await?;
        Ok(Box::new(handle))
    }
}

#[cfg(test)]
mod tests {
    use super::{route_packet, ReceivedItem, WMBusError, WMBusHandle};
    use crate::wmbus::frame::WMBusFrame;
    use crate::wmbus::radio::driver::{LoRaRxInfo, ModeTaggedPacket};
    use crate::wmbus::radio::hal::MockHal;
    use crate::wmbus::radio::modulation::{LoRaBandwidth, PacketType, SpreadingFactor};

    fn gfsk_packet(payload: Vec<u8>) -> ModeTaggedPacket {
        ModeTaggedPacket {
            mode: PacketType::Gfsk,
            payload,
            rssi_dbm: -70,
            lora: None,
        }
    }

    fn lora_packet() -> ModeTaggedPacket {
        ModeTaggedPacket {
            mode: PacketType::LoRa,
            payload: vec![0xDE, 0xAD, 0xBE, 0xEF],
            rssi_dbm: -95,
            lora: Some(LoRaRxInfo {
                snr_db: 6.25,
                freq_error_hz: Some(120),
                sf: SpreadingFactor::SF7,
                bw: LoRaBandwidth::BW125,
            }),
        }
    }

    /// A complete, CRC-valid wM-Bus radio frame that `parse_wmbus_frame` accepts.
    fn valid_wmbus_bytes() -> Vec<u8> {
        WMBusFrame::build(0x44, 0x6815, 0x74280561, 0x37, 0x01, 0x8E, &[0x01, 0x02])
    }

    #[test]
    fn routes_gfsk_valid_to_wmbus_and_malformed_to_none() {
        // A valid GFSK payload parses into a wM-Bus item...
        match route_packet(gfsk_packet(valid_wmbus_bytes())) {
            Some(ReceivedItem::Wmbus { frame, rssi_dbm }) => {
                assert_eq!(frame.device_address, 0x74280561);
                assert_eq!(rssi_dbm, -70);
            }
            other => panic!("expected Wmbus, got {other:?}"),
        }
        // ...and a malformed GFSK payload is dropped rather than surfaced.
        assert!(route_packet(gfsk_packet(vec![0x00, 0x01, 0x02])).is_none());
    }

    #[test]
    fn routes_lora_to_raw_item_with_metadata() {
        match route_packet(lora_packet()) {
            Some(ReceivedItem::Lora {
                payload,
                rssi_dbm,
                lora,
            }) => {
                assert_eq!(payload, vec![0xDE, 0xAD, 0xBE, 0xEF]);
                assert_eq!(rssi_dbm, -95);
                assert_eq!(lora.snr_db, 6.25);
                assert_eq!(lora.sf, SpreadingFactor::SF7);
                assert_eq!(lora.freq_error_hz, Some(120));
            }
            other => panic!("expected Lora, got {other:?}"),
        }
    }

    #[test]
    fn mixed_order_lora_wmbus_lora_routes_each_correctly() {
        let items: Vec<_> = [
            lora_packet(),
            gfsk_packet(valid_wmbus_bytes()),
            lora_packet(),
        ]
        .into_iter()
        .map(route_packet)
        .collect();
        assert!(matches!(items[0], Some(ReceivedItem::Lora { .. })));
        assert!(matches!(items[1], Some(ReceivedItem::Wmbus { .. })));
        assert!(matches!(items[2], Some(ReceivedItem::Lora { .. })));
    }

    async fn test_handle() -> WMBusHandle<MockHal> {
        WMBusHandle::new(MockHal::new(), None).await.unwrap()
    }

    #[tokio::test]
    async fn recv_frame_skips_lora_and_returns_next_wmbus() {
        let mut handle = test_handle().await;
        // Two LoRa items ahead of the wM-Bus frame in the stream.
        handle.inject_item(route_packet(lora_packet()).unwrap());
        handle.inject_item(route_packet(lora_packet()).unwrap());
        handle.inject_item(route_packet(gfsk_packet(valid_wmbus_bytes())).unwrap());

        let (frame, rssi) = handle.recv_frame(Some(1000)).await.unwrap();
        assert_eq!(frame.device_address, 0x74280561);
        assert_eq!(rssi, -70);
    }

    #[tokio::test]
    async fn recv_item_returns_the_lora_item() {
        let mut handle = test_handle().await;
        handle.inject_item(route_packet(lora_packet()).unwrap());
        match handle.recv_item(Some(1000)).await.unwrap() {
            ReceivedItem::Lora { lora, .. } => assert_eq!(lora.sf, SpreadingFactor::SF7),
            other => panic!("expected Lora, got {other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn recv_frame_times_out_when_only_lora_arrives() {
        let mut handle = test_handle().await;
        // Only a LoRa item: recv_frame consumes it, finds no wM-Bus frame, and times out on
        // a single absolute deadline (the timeout does not reset per skipped LoRa item).
        handle.inject_item(route_packet(lora_packet()).unwrap());
        assert!(matches!(
            handle.recv_frame(Some(500)).await,
            Err(WMBusError::Timeout)
        ));
    }

    #[tokio::test]
    async fn scheduler_switching_produces_ordered_items_through_the_handle() {
        use crate::wmbus::radio::driver::{LoRaProfile, RadioProfile, WmbusProfile};
        use crate::wmbus::radio::hal::RecordingHal;
        use crate::wmbus::radio::modulation::CodingRate;
        use crate::wmbus::radio::scheduler::ProfileScheduler;

        fn wmbus() -> RadioProfile {
            RadioProfile::Wmbus(WmbusProfile::mode_c(868_950_000, 100_000))
        }
        fn lora() -> RadioProfile {
            RadioProfile::LoRa(LoRaProfile {
                frequency_hz: 868_100_000,
                sf: SpreadingFactor::SF7,
                bw: LoRaBandwidth::BW125,
                cr: CodingRate::CR4_5,
                power_dbm: 14,
                sync_word: None,
            })
        }

        // One RecordingHal, one Sx126xDriver, owned by the handle; the scheduler is built
        // from the handle's driver Arc via shared_driver() — never a second driver.
        let hal = RecordingHal::new();
        let probe = hal.clone();
        let handle = WMBusHandle::new(hal, None).await.unwrap();
        let scheduler = ProfileScheduler::new(handle.shared_driver(), wmbus());

        let mut items = Vec::new();

        // Base GFSK: a wM-Bus frame arrives -> Wmbus item.
        scheduler.switch_to(&wmbus()).await.unwrap();
        probe.queue_rx(valid_wmbus_bytes());
        items.push(handle.poll_once_test().await.unwrap());

        // Scheduler switches to LoRa: a raw payload arrives -> Lora item.
        scheduler.switch_to(&lora()).await.unwrap();
        probe.queue_rx(vec![0x01, 0x02, 0x03, 0x04]);
        items.push(handle.poll_once_test().await.unwrap());

        // Back to GFSK: another wM-Bus frame -> Wmbus item.
        scheduler.switch_to(&wmbus()).await.unwrap();
        probe.queue_rx(valid_wmbus_bytes());
        items.push(handle.poll_once_test().await.unwrap());

        // Reception followed the scheduler's switches, in order: Wmbus, Lora, Wmbus.
        assert!(matches!(items[0], Some(ReceivedItem::Wmbus { .. })));
        assert!(matches!(items[1], Some(ReceivedItem::Lora { .. })));
        assert!(matches!(items[2], Some(ReceivedItem::Wmbus { .. })));
    }
}
