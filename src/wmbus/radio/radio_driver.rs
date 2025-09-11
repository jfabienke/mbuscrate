//! # Radio Driver Trait and Common Types
//!
//! This module defines the `RadioDriver` trait that provides a common interface
//! for different radio drivers (SX126x, RFM69HCW, etc.) used in wireless M-Bus
//! applications. It abstracts the differences between radio chips while providing
//! a consistent API for the wmbus protocol layer.

use async_trait::async_trait;
use std::time::Duration;
use thiserror::Error;

/// Common radio driver errors
#[derive(Error, Debug)]
pub enum RadioDriverError {
    /// Hardware abstraction layer error
    #[error("HAL error: {0}")]
    Hal(String),
    /// Invalid configuration parameters
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
    /// Operation timed out
    #[error("Operation timeout")]
    Timeout,
    /// Radio is in wrong state for operation
    #[error("Wrong state: {0}")]
    WrongState(String),
    /// Channel is busy (for LBT operations)
    #[error("Channel busy: RSSI {rssi_dbm} dBm")]
    ChannelBusy { rssi_dbm: i16 },
    /// Device-specific error
    #[error("Device error: {0}")]
    DeviceError(String),
}

/// Radio operating modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioMode {
    /// Radio is sleeping (lowest power)
    Sleep,
    /// Radio is in standby (configuration possible)
    Standby,
    /// Radio is transmitting
    Transmit,
    /// Radio is receiving
    Receive,
}

/// Radio configuration for wM-Bus operation
#[derive(Debug, Clone)]
pub struct WMBusConfig {
    /// Operating frequency in Hz (e.g., 868_950_000)
    pub frequency_hz: u32,
    /// Data rate in bits per second (e.g., 100_000)
    pub bitrate: u32,
    /// Output power in dBm (e.g., 14)
    pub output_power_dbm: i8,
    /// Enable automatic gain control
    pub agc_enabled: bool,
    /// CRC polynomial (0x3D65 for wM-Bus)
    pub crc_polynomial: u16,
    /// Sync word for packet detection
    pub sync_word: Vec<u8>,
}

impl Default for WMBusConfig {
    fn default() -> Self {
        Self {
            frequency_hz: 868_950_000,  // EU wM-Bus S-mode
            bitrate: 100_000,           // 100 kbps
            output_power_dbm: 14,       // +14 dBm
            agc_enabled: true,
            crc_polynomial: 0x3D65,     // wM-Bus CRC
            sync_word: vec![0x54, 0x3D], // Common wM-Bus sync
        }
    }
}

/// Received packet information
#[derive(Debug, Clone)]
pub struct ReceivedPacket {
    /// Raw packet data
    pub data: Vec<u8>,
    /// RSSI in dBm when packet was received
    pub rssi_dbm: i16,
    /// Frequency error in Hz (if available)
    pub freq_error_hz: Option<i32>,
    /// Link quality indicator (if available)
    pub lqi: Option<u8>,
    /// CRC validation result
    pub crc_valid: bool,
}

/// Radio statistics for monitoring
#[derive(Debug, Clone, Copy, Default)]
pub struct RadioStats {
    /// Total packets received
    pub packets_received: u32,
    /// Packets with valid CRC
    pub packets_crc_valid: u32,
    /// Packets with CRC errors
    pub packets_crc_error: u32,
    /// Packets with length errors
    pub packets_length_error: u32,
    /// Last RSSI measurement in dBm
    pub last_rssi_dbm: i16,
}

/// Common radio driver trait for wireless M-Bus applications
///
/// This trait provides a unified interface for different radio drivers,
/// allowing the wmbus protocol layer to work with various radio chips
/// (SX126x, RFM69HCW, etc.) without modification.
#[async_trait]
pub trait RadioDriver: Send + Sync {
    /// Initialize the radio with the given configuration
    ///
    /// # Arguments
    /// * `config` - wM-Bus configuration parameters
    ///
    /// # Returns
    /// * `Ok(())` - Initialization successful
    /// * `Err(RadioDriverError)` - Initialization failed
    async fn initialize(&mut self, config: WMBusConfig) -> Result<(), RadioDriverError>;

    /// Start receiving packets
    ///
    /// Puts the radio in continuous receive mode. Received packets will be
    /// available through `get_received_packet()`.
    ///
    /// # Returns
    /// * `Ok(())` - RX mode started successfully
    /// * `Err(RadioDriverError)` - Failed to start RX mode
    async fn start_receive(&mut self) -> Result<(), RadioDriverError>;

    /// Stop receiving and enter standby mode
    ///
    /// # Returns
    /// * `Ok(())` - Successfully stopped receiving
    /// * `Err(RadioDriverError)` - Failed to stop receiving
    async fn stop_receive(&mut self) -> Result<(), RadioDriverError>;

    /// Transmit a packet
    ///
    /// Transmits the given data packet. This method handles all aspects of
    /// transmission including mode switching and completion detection.
    ///
    /// # Arguments
    /// * `data` - Data to transmit (must not exceed radio's maximum packet size)
    ///
    /// # Returns
    /// * `Ok(())` - Transmission completed successfully
    /// * `Err(RadioDriverError)` - Transmission failed
    async fn transmit(&mut self, data: &[u8]) -> Result<(), RadioDriverError>;

    /// Check for received packets
    ///
    /// Polls the radio for received packets and returns the next available packet.
    /// Should be called regularly in a receive loop.
    ///
    /// # Returns
    /// * `Ok(Some(packet))` - Packet received
    /// * `Ok(None)` - No packet available
    /// * `Err(RadioDriverError)` - Error checking for packets
    async fn get_received_packet(&mut self) -> Result<Option<ReceivedPacket>, RadioDriverError>;

    /// Get current radio statistics
    ///
    /// Returns statistics about packet reception and radio performance.
    ///
    /// # Returns
    /// * `Ok(stats)` - Current radio statistics
    /// * `Err(RadioDriverError)` - Failed to read statistics
    async fn get_stats(&mut self) -> Result<RadioStats, RadioDriverError>;

    /// Reset radio statistics counters
    ///
    /// # Returns
    /// * `Ok(())` - Statistics reset successfully
    /// * `Err(RadioDriverError)` - Failed to reset statistics
    async fn reset_stats(&mut self) -> Result<(), RadioDriverError>;

    /// Get current radio mode
    ///
    /// # Returns
    /// * `Ok(mode)` - Current radio mode
    /// * `Err(RadioDriverError)` - Failed to read mode
    async fn get_mode(&mut self) -> Result<RadioMode, RadioDriverError>;

    /// Enter sleep mode for power saving
    ///
    /// # Returns
    /// * `Ok(())` - Successfully entered sleep mode
    /// * `Err(RadioDriverError)` - Failed to enter sleep mode
    async fn sleep(&mut self) -> Result<(), RadioDriverError>;

    /// Wake up from sleep mode
    ///
    /// # Returns
    /// * `Ok(())` - Successfully woke up
    /// * `Err(RadioDriverError)` - Failed to wake up
    async fn wake_up(&mut self) -> Result<(), RadioDriverError>;

    /// Get instantaneous RSSI measurement
    ///
    /// The radio should be in receive mode for accurate measurements.
    ///
    /// # Returns
    /// * `Ok(rssi_dbm)` - Current RSSI in dBm
    /// * `Err(RadioDriverError)` - Failed to measure RSSI
    async fn get_rssi(&mut self) -> Result<i16, RadioDriverError>;

    /// Check if channel is clear for transmission (LBT compliance)
    ///
    /// # Arguments
    /// * `threshold_dbm` - RSSI threshold in dBm for channel clear determination
    /// * `listen_duration` - How long to listen before making determination
    ///
    /// # Returns
    /// * `Ok(true)` - Channel is clear
    /// * `Ok(false)` - Channel is busy
    /// * `Err(RadioDriverError)` - LBT check failed
    async fn is_channel_clear(
        &mut self,
        threshold_dbm: i16,
        listen_duration: Duration,
    ) -> Result<bool, RadioDriverError>;

    /// Get driver-specific information
    ///
    /// Returns information about the radio driver implementation,
    /// useful for debugging and feature detection.
    fn get_driver_info(&self) -> DriverInfo;
}

/// Information about the radio driver implementation
#[derive(Debug, Clone)]
pub struct DriverInfo {
    /// Driver name (e.g., "SX126x", "RFM69HCW")
    pub name: String,
    /// Driver version
    pub version: String,
    /// Supported frequency bands in Hz
    pub frequency_bands: Vec<(u32, u32)>,
    /// Maximum packet size in bytes
    pub max_packet_size: usize,
    /// Supported data rates in bps
    pub supported_bitrates: Vec<u32>,
    /// Power range in dBm (min, max)
    pub power_range_dbm: (i8, i8),
    /// Hardware features supported
    pub features: Vec<String>,
}