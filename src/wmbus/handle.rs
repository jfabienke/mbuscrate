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
//! use mbus_rs::wmbus::radio::hal::RaspberryPiHalBuilder;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize with HAL for your platform
//!     let hal = RaspberryPiHalBuilder::new()
//!         .spi_device("/dev/spidev0.0")
//!         .configure_pins(/* GPIO configuration */)
//!         .build()?;
//!     
//!     // Create wM-Bus handle
//!     let mut wmbus = WMBusHandle::new(hal).await?;
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
use crate::wmbus::radio::driver::{DriverError, LbtConfig, Sx126xDriver, RadioStats, DeviceErrors, RadioStatusReport};
use crate::wmbus::radio::irq::IrqStatus;
use crate::wmbus::radio::hal::Hal;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{sleep, timeout, Duration};

/// Type aliases for complex types to improve readability
type FrameReceiver = Arc<RwLock<Option<mpsc::UnboundedReceiver<(WMBusFrame, i16)>>>>;
type FrameSender = mpsc::UnboundedSender<(WMBusFrame, i16)>;
type UnsolicitedCallback = Arc<dyn Fn(&WMBusFrame) + Send + Sync>;
type WMBusFuture<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<(WMBusFrame, i16), WMBusError>> + Send + 'a>>;
type SendFuture<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), WMBusError>> + Send + 'a>>;

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

            loop {
                // Set radio to continuous receive mode
                {
                    let mut driver_guard = driver.lock().await;
                    if let Err(e) = driver_guard.set_rx_continuous() {
                        log::error!("Failed to set RX continuous: {e:?}");
                        sleep(Duration::from_millis(1000)).await;
                        continue;
                    }
                }

                // Poll for received frames
                tokio::time::sleep(Duration::from_millis(10)).await;

                let result = {
                    let mut driver_guard = driver.lock().await;
                    driver_guard.process_irqs()
                };

                match result {
                    Ok(Some(payload)) => {
                        consecutive_errors = 0;

                        // Parse wM-Bus frame
                        match crate::wmbus::frame::parse_wmbus_frame(&payload) {
                            Ok(frame) => {
                                // Get RSSI for this frame
                                let rssi = {
                                    let mut driver_guard = driver.lock().await;
                                    driver_guard.get_rssi_instant().unwrap_or(-100)
                                };

                                // Update device registry
                                Self::update_device_registry(&devices, &frame, rssi).await;

                                // Send frame to channel
                                if tx_sender.send((frame.clone(), rssi)).is_err() {
                                    log::warn!("Frame channel receiver dropped");
                                    break;
                                }

                                // Call unsolicited callback if registered
                                if let Some(callback) = &unsolicited_callback {
                                    callback(&frame);
                                }
                            }
                            Err(e) => {
                                log::debug!("Failed to parse frame: {e:?}");
                            }
                        }
                    }
                    Ok(None) => {
                        // No frame received, continue polling
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        log::warn!(
                            "Radio error in receiver: {e:?} (consecutive: {consecutive_errors})"
                        );

                        // If too many consecutive errors, back off
                        if consecutive_errors > 10 {
                            log::error!("Too many consecutive radio errors, backing off");
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

    /// Receive a frame with timeout
    ///
    /// Waits for the next received frame or times out.
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Timeout in milliseconds (None for default)
    ///
    /// # Returns
    ///
    /// * `Ok((frame, rssi))` - Received frame and signal strength
    /// * `Err(WMBusError::Timeout)` - No frame received within timeout
    /// * `Err(WMBusError)` - Other error
    pub async fn recv_frame(
        &mut self,
        timeout_ms: Option<u32>,
    ) -> Result<(WMBusFrame, i16), WMBusError> {
        let timeout_duration =
            Duration::from_millis(timeout_ms.unwrap_or(self.config.rx_timeout_ms) as u64);

        let mut rx_guard = self.rx_channel.write().await;
        let rx_channel = rx_guard
            .as_mut()
            .ok_or_else(|| WMBusError::InvalidConfig("RX channel not available".to_string()))?;

        match timeout(timeout_duration, rx_channel.recv()).await {
            Ok(Some(frame_and_rssi)) => Ok(frame_and_rssi),
            Ok(None) => Err(WMBusError::Network("Frame channel closed".to_string())),
            Err(_) => Err(WMBusError::Timeout),
        }
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
    fn send_frame<'a>(
        &'a self,
        frame: &'a WMBusFrame,
    ) -> SendFuture<'a>;

    /// Receive a frame with timeout
    fn recv_frame<'a>(
        &'a mut self,
        timeout_ms: Option<u32>,
    ) -> WMBusFuture<'a>;

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
        use crate::wmbus::radio::hal::Hal;

        // Mock HAL for testing - always available for development and testing
        #[derive(Debug)]
        struct MockHal;

        impl Hal for MockHal {
            fn write_command(
                &mut self,
                _opcode: u8,
                _data: &[u8],
            ) -> Result<(), crate::wmbus::radio::hal::HalError> {
                Ok(())
            }

            fn read_command(
                &mut self,
                _opcode: u8,
                buffer: &mut [u8],
            ) -> Result<(), crate::wmbus::radio::hal::HalError> {
                buffer.fill(0);
                Ok(())
            }

            fn write_register(
                &mut self,
                _address: u16,
                _data: &[u8],
            ) -> Result<(), crate::wmbus::radio::hal::HalError> {
                Ok(())
            }

            fn read_register(
                &mut self,
                _address: u16,
                buffer: &mut [u8],
            ) -> Result<(), crate::wmbus::radio::hal::HalError> {
                buffer.fill(0);
                Ok(())
            }

            fn gpio_read(&mut self, _pin: u8) -> Result<bool, crate::wmbus::radio::hal::HalError> {
                Ok(false)
            }

            fn gpio_write(
                &mut self,
                _pin: u8,
                _state: bool,
            ) -> Result<(), crate::wmbus::radio::hal::HalError> {
                Ok(())
            }
        }

        let hal = MockHal;
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
