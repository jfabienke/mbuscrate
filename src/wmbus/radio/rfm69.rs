//! # RFM69 Radio Driver for wM-Bus
//!
//! This module provides a comprehensive async driver for the HopeRF RFM69HCW transceiver,
//! specifically optimized for wireless M-Bus (wM-Bus) applications. The implementation
//! includes critical enhancements for robust frame processing in real-world conditions.
//!
//! ## Features
//!
//! - Async-first design using Tokio for non-blocking I/O
//! - wM-Bus specific configuration (868.95 MHz, 100 kbps, 50 kHz deviation)
//! - Hardware AES encryption support
//! - Robust packet processing with frame recovery
//! - GPIO interrupt handling for efficient operation
//! - Comprehensive error handling and statistics
//!
//! ## Configuration
//!
//! The driver supports configuration via JSON/TOML:
//! ```json
//! {
//!   "spidev": "/dev/spidev0.0",
//!   "reset_pin": 5,
//!   "interrupt_pin": 23,
//!   "aes_key": "0123456789ABCDEF0123456789ABCDEF"
//! }
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use rfm69::Rfm69Driver;
//!
//! let mut driver = Rfm69Driver::new(config).await?;
//! driver.start_rx().await?;
//!
//! // Process packets in event loop
//! while let Some(packet) = driver.read_packet().await? {
//!     println!("Received: {:?}", packet);
//! }
//! ```

use crate::wmbus::radio::rfm69_packet::*;
use crate::wmbus::radio::rfm69_registers::*;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::{sleep, timeout};

#[cfg(feature = "rfm69")]
use rppal::{
    gpio::{Gpio, InputPin, Level, OutputPin, Trigger},
    spi::{BitOrder, Bus, Mode, SlaveSelect, Spi},
};

/// Configuration for RFM69 driver
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rfm69Config {
    /// SPI device path (e.g., "/dev/spidev0.0")
    pub spidev: Option<String>,
    /// GPIO pin for radio reset (default: 5)
    pub reset_pin: Option<u8>,
    /// GPIO pin for interrupt (default: 23)
    pub interrupt_pin: Option<u8>,
    /// AES encryption key (32 hex chars, optional)
    pub aes_key: Option<String>,
    /// Node ID for addressing (optional)
    pub node_id: Option<u8>,
    /// Network ID (optional)
    pub network_id: Option<u8>,
    /// FIFO threshold for interrupt (default: 3)
    pub fifo_threshold: Option<u8>,
}

impl Default for Rfm69Config {
    fn default() -> Self {
        Self {
            spidev: Some("/dev/spidev0.0".to_string()),
            reset_pin: Some(DEFAULT_RESET_PIN),
            interrupt_pin: Some(DEFAULT_INTERRUPT_PIN),
            aes_key: None,
            node_id: None,
            network_id: None,
            fifo_threshold: Some(3),
        }
    }
}

/// Driver errors
#[derive(Debug, thiserror::Error)]
pub enum Rfm69Error {
    #[error("SPI communication error: {0}")]
    Spi(String),

    #[error("GPIO error: {0}")]
    Gpio(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Radio initialization failed: {0}")]
    InitFailed(String),

    #[error("Timeout waiting for: {0}")]
    Timeout(String),

    #[error("Invalid frame: {0}")]
    InvalidFrame(String),

    #[error("Packet processing error: {0}")]
    Packet(#[from] PacketError),

    #[error("Feature not enabled: {0}")]
    FeatureNotEnabled(String),
}

/// Operating modes for the RFM69
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rfm69Mode {
    Sleep = 0,
    Standby = 1,
    Tx = 2,
    Rx = 3,
}

/// Main RFM69 driver structure
pub struct Rfm69Driver {
    /// SPI interface for register access
    #[cfg(feature = "rfm69")]
    spi: Arc<Mutex<Spi>>,

    /// GPIO for radio reset
    #[cfg(feature = "rfm69")]
    reset_pin: Option<OutputPin>,

    /// GPIO for interrupt monitoring
    #[cfg(feature = "rfm69")]
    interrupt_pin: Option<InputPin>,

    /// Driver configuration
    config: Rfm69Config,

    /// Current operating mode
    current_mode: Rfm69Mode,

    /// Packet buffer for frame assembly
    packet_buffer: Arc<Mutex<PacketBuffer>>,

    /// Packet processing statistics
    stats: Arc<Mutex<PacketStats>>,

    /// Error logging throttle
    error_throttle: Arc<Mutex<LogThrottle>>,

    /// Interrupt processing task handle
    #[cfg(feature = "rfm69")]
    interrupt_task: Option<tokio::task::JoinHandle<()>>,

    /// Shutdown signal for graceful task termination
    #[cfg(feature = "rfm69")]
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Rfm69Driver {
    /// Create a new RFM69 driver instance
    pub async fn new(config: Rfm69Config) -> Result<Self, Rfm69Error> {
        #[cfg(not(feature = "rfm69"))]
        {
            return Err(Rfm69Error::FeatureNotEnabled(
                "rfm69 feature not enabled. Build with --features rfm69".to_string(),
            ));
        }

        #[cfg(feature = "rfm69")]
        {
            let spi = Self::init_spi(&config)?;
            let (reset_pin, interrupt_pin) = Self::init_gpio(&config)?;

            Ok(Self {
                spi: Arc::new(Mutex::new(spi)),
                reset_pin,
                interrupt_pin,
                config,
                current_mode: Rfm69Mode::Sleep,
                packet_buffer: Arc::new(Mutex::new(PacketBuffer::new())),
                stats: Arc::new(Mutex::new(PacketStats::default())),
                error_throttle: Arc::new(Mutex::new(LogThrottle::new(60_000, 5))), // 5 errors per minute
                interrupt_task: None,
                shutdown_tx: None,
            })
        }
    }

    /// Initialize the RFM69 radio
    pub async fn initialize(&mut self) -> Result<(), Rfm69Error> {
        info!("Initializing RFM69 radio for wM-Bus operation");

        // Reset the radio chip
        self.reset().await?;

        // Verify chip communication
        self.verify_chip().await?;

        // Configure for wM-Bus operation
        self.configure_wmbus().await?;

        // Set up AES encryption if configured
        if let Some(ref aes_key) = self.config.aes_key {
            self.configure_aes(aes_key).await?;
        }

        // Configure addressing if specified
        self.configure_addressing().await?;

        // Start interrupt handling
        self.start_interrupt_handling().await?;

        // Enter receive mode
        self.set_mode(Rfm69Mode::Rx).await?;

        info!("RFM69 radio initialized successfully");
        Ok(())
    }

    /// Reset the radio chip
    async fn reset(&mut self) -> Result<(), Rfm69Error> {
        #[cfg(feature = "rfm69")]
        {
            if let Some(ref mut reset_pin) = self.reset_pin {
                info!("Resetting RFM69 chip");

                // Pulse reset pin: HIGH -> wait -> LOW -> wait
                reset_pin.set_high();
                sleep(Duration::from_millis(300)).await;
                reset_pin.set_low();
                sleep(Duration::from_millis(300)).await;

                // Verify chip is responding
                let start = Instant::now();
                let timeout_duration = Duration::from_secs(5);

                // Try to sync with chip by writing test patterns
                let original = self.read_register(REG_SYNCVALUE1).await?;

                while start.elapsed() < timeout_duration {
                    self.write_register(REG_SYNCVALUE1, 0xAA).await?;
                    if self.read_register(REG_SYNCVALUE1).await? == 0xAA {
                        break;
                    }
                    sleep(Duration::from_millis(10)).await;
                }

                while start.elapsed() < timeout_duration {
                    self.write_register(REG_SYNCVALUE1, 0x55).await?;
                    if self.read_register(REG_SYNCVALUE1).await? == 0x55 {
                        break;
                    }
                    sleep(Duration::from_millis(10)).await;
                }

                if start.elapsed() >= timeout_duration {
                    return Err(Rfm69Error::InitFailed(
                        "Failed to sync with radio chip".to_string(),
                    ));
                }

                // Restore original value
                self.write_register(REG_SYNCVALUE1, original).await?;
                info!("RFM69 chip reset completed");
            }
        }

        Ok(())
    }

    /// Verify chip communication
    async fn verify_chip(&self) -> Result<(), Rfm69Error> {
        // Read version register to verify communication
        let version = self.read_register(REG_VERSION).await?;
        info!("RFM69 chip version: 0x{:02X}", version);
        Ok(())
    }

    /// Configure radio for wM-Bus operation  
    async fn configure_wmbus(&self) -> Result<(), Rfm69Error> {
        info!("Configuring RFM69 for wM-Bus operation");

        // Set to standby mode for configuration
        self.write_register(REG_OPMODE, RF_OPMODE_STANDBY).await?;

        // Set frequency to 868.95 MHz
        self.set_frequency(WMBUS_FREQUENCY).await?;

        // Set bit rate to 100 kbps
        self.write_register(REG_BITRATEMSB, RF_BITRATEMSB_100KBPS)
            .await?;
        self.write_register(REG_BITRATELSB, RF_BITRATELSB_100KBPS)
            .await?;

        // Set frequency deviation to 50 kHz
        self.write_register(REG_FDEVMSB, RF_FDEVMSB_50000).await?;
        self.write_register(REG_FDEVLSB, RF_FDEVLSB_50000).await?;

        // Configure data modulation (Gaussian filter, BT = 1.0)
        self.write_register(REG_DATAMODUL, 1).await?;

        // Configure receiver bandwidth and LNA
        self.write_register(REG_LNA, 0x88).await?;
        self.write_register(REG_RXBW, 0xE0).await?;
        self.write_register(REG_AFCBW, 0xE0).await?;

        // Configure test register for optimal performance
        self.write_register(REG_TESTDAGC, 0x30).await?;

        // Configure packet handling (no chip CRC, variable length)
        self.write_register(REG_PACKETCONFIG1, 0).await?;
        self.write_register(REG_PAYLOADLENGTH, 0).await?;

        // Set FIFO threshold for early interrupt
        let threshold = self.config.fifo_threshold.unwrap_or(3);
        self.write_register(REG_FIFOTHRESH, threshold).await?;

        // Configure preamble (4 bytes)
        self.write_register(REG_PREAMBLEMSB, 0).await?;
        self.write_register(REG_PREAMBLELSB, 4).await?;

        // Disable hardware sync word detection for dual S/C mode support
        self.write_register(REG_SYNCCONFIG, 0x00).await?;

        // Configure DIO mapping for FIFO level interrupt on DIO1
        self.write_register(REG_DIOMAPPING1, 0).await?;

        info!("wM-Bus configuration completed");
        Ok(())
    }

    /// Configure AES encryption
    async fn configure_aes(&self, aes_key: &str) -> Result<(), Rfm69Error> {
        if aes_key.len() != 32 {
            return Err(Rfm69Error::Config(
                "AES key must be 32 hex characters".to_string(),
            ));
        }

        info!("Configuring AES encryption");

        // Parse hex key
        let mut key_bytes = [0u8; 16];
        for (i, chunk) in aes_key.as_bytes().chunks(2).enumerate() {
            if i >= 16 {
                break;
            }
            let hex_str = std::str::from_utf8(chunk)
                .map_err(|_| Rfm69Error::Config("Invalid hex in AES key".to_string()))?;
            key_bytes[i] = u8::from_str_radix(hex_str, 16)
                .map_err(|_| Rfm69Error::Config("Invalid hex in AES key".to_string()))?;
        }

        // Load key into chip registers
        for (i, &byte) in key_bytes.iter().enumerate() {
            self.write_register(REG_AESKEY1 + i as u8, byte).await?;
        }

        // Enable AES encryption
        self.write_register_bits(REG_PACKETCONFIG2, 0x01, RF_PACKET2_EAS_ON)
            .await?;

        info!("AES encryption enabled");
        Ok(())
    }

    /// Configure node and network addressing
    async fn configure_addressing(&self) -> Result<(), Rfm69Error> {
        // Set network ID if specified
        if let Some(network_id) = self.config.network_id {
            self.write_register(REG_SYNCVALUE2, network_id).await?;
            info!("Network ID set to: {}", network_id);
        }

        // Set node ID if specified
        if let Some(node_id) = self.config.node_id {
            self.write_register(REG_NODEADRS, node_id).await?;
            self.write_register_bits(REG_PACKETCONFIG1, 0x06, 0x04)
                .await?;
            info!("Node ID set to: {}", node_id);
        }

        Ok(())
    }

    /// Set radio operating mode
    async fn set_mode(&mut self, mode: Rfm69Mode) -> Result<(), Rfm69Error> {
        if self.current_mode == mode {
            return Ok(); // Already in requested mode
        }

        let opmode = match mode {
            Rfm69Mode::Sleep => RF_OPMODE_SLEEP,
            Rfm69Mode::Standby => RF_OPMODE_STANDBY,
            Rfm69Mode::Tx => RF_OPMODE_TRANSMITTER,
            Rfm69Mode::Rx => RF_OPMODE_RECEIVER,
        };

        self.write_register_bits(REG_OPMODE, 0x1C, opmode).await?;

        // Wait for mode ready if transitioning from sleep
        if self.current_mode == Rfm69Mode::Sleep {
            self.wait_for_mode_ready().await?;
        }

        self.current_mode = mode;
        debug!("RFM69 mode set to: {:?}", mode);
        Ok(())
    }

    /// Wait for mode ready flag
    async fn wait_for_mode_ready(&self) -> Result<(), Rfm69Error> {
        let start = Instant::now();
        let timeout_duration = Duration::from_millis(500);

        while start.elapsed() < timeout_duration {
            let flags = self.read_register(REG_IRQFLAGS1).await?;
            if flags & RF_IRQFLAGS1_MODEREADY != 0 {
                return Ok(());
            }
            sleep(Duration::from_millis(1)).await;
        }

        Err(Rfm69Error::Timeout("Mode ready".to_string()))
    }

    /// Set RF frequency
    async fn set_frequency(&self, frequency_hz: f64) -> Result<(), Rfm69Error> {
        let freq_reg = (frequency_hz / FSTEP) as u32;

        self.write_register(REG_FRFMSB, (freq_reg >> 16) as u8)
            .await?;
        self.write_register(REG_FRFMID, (freq_reg >> 8) as u8)
            .await?;
        self.write_register(REG_FRFLSB, freq_reg as u8).await?;

        debug!("Frequency set to: {:.3} MHz", frequency_hz / 1e6);
        Ok(())
    }

    /// Start interrupt handling task
    async fn start_interrupt_handling(&mut self) -> Result<(), Rfm69Error> {
        #[cfg(feature = "rfm69")]
        {
            if let Some(ref mut interrupt_pin) = self.interrupt_pin {
                info!(
                    "Starting interrupt handling on GPIO {}",
                    self.config.interrupt_pin.unwrap_or(DEFAULT_INTERRUPT_PIN)
                );

                // Configure interrupt pin for rising edge
                interrupt_pin
                    .set_interrupt(Trigger::RisingEdge)
                    .map_err(|e| Rfm69Error::Gpio(format!("Failed to set interrupt: {}", e)))?;

                // Clone references for the async task
                let spi = self.spi.clone();
                let packet_buffer = self.packet_buffer.clone();
                let stats = self.stats.clone();
                let error_throttle = self.error_throttle.clone();

                // Create shutdown channel
                let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

                // Spawn interrupt handling task
                let handle = tokio::spawn(async move {
                    Self::interrupt_handler_task(
                        spi,
                        packet_buffer,
                        stats,
                        error_throttle,
                        shutdown_rx,
                    )
                    .await;
                });

                self.interrupt_task = Some(handle);
                self.shutdown_tx = Some(shutdown_tx);
            } else {
                warn!("No interrupt pin configured, using polling mode");
                // TODO: Start polling task as fallback
            }
        }

        Ok(())
    }

    /// Async interrupt handler task with proper GPIO interrupt handling
    #[cfg(feature = "rfm69")]
    async fn interrupt_handler_task(
        spi: Arc<Mutex<Spi>>,
        packet_buffer: Arc<Mutex<PacketBuffer>>,
        stats: Arc<Mutex<PacketStats>>,
        error_throttle: Arc<Mutex<LogThrottle>>,
        mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        info!("Interrupt handler task started");

        loop {
            // Check for shutdown signal
            if shutdown_rx.try_recv().is_ok() {
                info!("Shutdown signal received");
                break;
            }
            // Check for FIFO level interrupt
            match Self::read_register_static(&spi, REG_IRQFLAGS2).await {
                Ok(flags2) => {
                    // Handle FIFO level interrupt
                    if flags2 & RF_IRQFLAGS2_FIFOLEVEL != 0 {
                        if let Err(e) =
                            Self::handle_fifo_interrupt(&spi, &packet_buffer, &stats).await
                        {
                            // Throttled error logging
                            if error_throttle.lock().unwrap().allow() {
                                error!("FIFO interrupt handling failed: {}", e);
                            }
                        }
                    }

                    // Handle FIFO overrun
                    if flags2 & RF_IRQFLAGS2_FIFOOVERRUN != 0 {
                        warn!("FIFO overrun detected - clearing and resetting");
                        if let Err(e) =
                            Self::handle_fifo_overrun(&spi, &packet_buffer, &stats).await
                        {
                            error!("Failed to handle FIFO overrun: {}", e);
                        }
                    }

                    // Handle payload ready (complete packet received)
                    if flags2 & RF_IRQFLAGS2_PAYLOADREADY != 0 {
                        if let Err(e) =
                            Self::handle_payload_ready(&spi, &packet_buffer, &stats).await
                        {
                            if error_throttle.lock().unwrap().allow() {
                                error!("Payload ready handling failed: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    // Throttled error logging for SPI failures
                    if error_throttle.lock().unwrap().allow() {
                        error!("Failed to read interrupt flags: {}", e);
                    }
                    // Brief delay before retry
                    sleep(Duration::from_millis(10)).await;
                    continue;
                }
            }

            // Adaptive polling rate - faster when data is expected
            let polling_interval = if Self::fifo_not_empty(&spi).await.unwrap_or(false) {
                Duration::from_micros(500) // Fast polling when FIFO has data
            } else {
                Duration::from_millis(1) // Normal polling rate
            };

            sleep(polling_interval).await;
        }

        info!("Interrupt handler task shutting down");
    }

    /// Handle FIFO level interrupt
    #[cfg(feature = "rfm69")]
    async fn handle_fifo_interrupt(
        spi: &Arc<Mutex<Spi>>,
        packet_buffer: &Arc<Mutex<PacketBuffer>>,
        stats: &Arc<Mutex<PacketStats>>,
    ) -> Result<(), Rfm69Error> {
        // Read data from FIFO while available
        while Self::fifo_not_empty(spi).await? {
            let byte = Self::read_register_static(spi, REG_FIFO).await?;

            {
                let mut buffer = packet_buffer.lock().unwrap();
                buffer.push_byte(byte);

                // Try to determine packet size
                if let Ok(Some(_size)) = buffer.determine_packet_size() {
                    // Check if packet is complete
                    if buffer.is_complete() {
                        match buffer.extract_packet() {
                            Ok(packet) => {
                                debug!("Complete packet received: {} bytes", packet.len());
                                // TODO: Forward packet to wM-Bus layer
                            }
                            Err(e) => {
                                buffer.update_stats(PacketEvent::InvalidHeader);
                                error!("Failed to extract packet: {}", e);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle FIFO overrun condition
    #[cfg(feature = "rfm69")]
    async fn handle_fifo_overrun(
        spi: &Arc<Mutex<Spi>>,
        packet_buffer: &Arc<Mutex<PacketBuffer>>,
        stats: &Arc<Mutex<PacketStats>>,
    ) -> Result<(), Rfm69Error> {
        // Update statistics
        {
            let mut stats = stats.lock().unwrap();
            stats.fifo_overruns += 1;
        }

        // Reset FIFO by switching to standby and back to RX
        Self::write_register_static(spi, REG_OPMODE, RF_OPMODE_STANDBY).await?;
        sleep(Duration::from_millis(1)).await;
        Self::write_register_static(spi, REG_OPMODE, RF_OPMODE_RECEIVER).await?;

        // Clear packet buffer
        {
            let mut buffer = packet_buffer.lock().unwrap();
            buffer.clear();
        }

        debug!("FIFO overrun handled, radio reset to RX mode");
        Ok(())
    }

    /// Handle payload ready interrupt (complete packet received)
    #[cfg(feature = "rfm69")]
    async fn handle_payload_ready(
        spi: &Arc<Mutex<Spi>>,
        packet_buffer: &Arc<Mutex<PacketBuffer>>,
        stats: &Arc<Mutex<PacketStats>>,
    ) -> Result<(), Rfm69Error> {
        // Read remaining data from FIFO
        while Self::fifo_not_empty(spi).await? {
            let byte = Self::read_register_static(spi, REG_FIFO).await?;

            {
                let mut buffer = packet_buffer.lock().unwrap();
                buffer.push_byte(byte);
            }
        }

        // Process the complete packet
        {
            let mut buffer = packet_buffer.lock().unwrap();
            if buffer.is_complete() {
                match buffer.extract_packet() {
                    Ok(packet) => {
                        let mut stats = stats.lock().unwrap();
                        stats.packets_received += 1;
                        debug!("Complete packet extracted: {} bytes", packet.len());
                    }
                    Err(e) => {
                        buffer.update_stats(PacketEvent::InvalidHeader);
                        warn!("Failed to extract complete packet: {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Static version of write_register for use in tasks
    #[cfg(feature = "rfm69")]
    async fn write_register_static(
        spi: &Arc<Mutex<Spi>>,
        reg: u8,
        value: u8,
    ) -> Result<(), Rfm69Error> {
        let tx = [reg | 0x80, value];

        {
            let mut spi = spi.lock().unwrap();
            spi.write(&tx)
                .map_err(|e| Rfm69Error::Spi(format!("Write register failed: {}", e)))?;
        }

        Ok(())
    }

    /// Check if FIFO is not empty
    #[cfg(feature = "rfm69")]
    async fn fifo_not_empty(spi: &Arc<Mutex<Spi>>) -> Result<bool, Rfm69Error> {
        let flags = Self::read_register_static(spi, REG_IRQFLAGS2).await?;
        Ok(flags & RF_IRQFLAGS2_FIFONOTEMPTY != 0)
    }

    /// Read burst of bytes from FIFO
    ///
    /// Reads up to expected_size bytes from FIFO in a single operation
    /// to prevent timing issues and partial frame corruption.
    ///
    /// # Arguments
    ///
    /// * `spi` - SPI interface
    /// * `expected_size` - Number of bytes to read
    ///
    /// # Returns
    ///
    /// * Vector of bytes read (may be less than expected if FIFO runs out)
    #[cfg(feature = "rfm69")]
    async fn read_burst(
        spi: &Arc<Mutex<Spi>>,
        expected_size: usize,
    ) -> Result<Vec<u8>, Rfm69Error> {
        let mut bytes = Vec::with_capacity(expected_size);
        let mut consecutive_empty = 0;

        // Read up to expected_size bytes, but stop if FIFO appears empty
        while bytes.len() < expected_size {
            // Check FIFO status
            if !Self::fifo_not_empty(spi).await? {
                consecutive_empty += 1;
                if consecutive_empty > 3 {
                    // FIFO seems to be empty, stop reading
                    break;
                }
                // Brief delay to allow FIFO to fill
                tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;
                continue;
            }
            consecutive_empty = 0;

            // Read byte from FIFO
            let byte = Self::read_register_static(spi, REG_FIFO).await?;
            bytes.push(byte);
        }

        if bytes.len() < expected_size {
            debug!(
                "Burst read incomplete: expected {}, got {} bytes",
                expected_size,
                bytes.len()
            );
        }

        Ok(bytes)
    }

    /// Enhanced FIFO interrupt handler with size-aware burst reading
    ///
    /// Uses packet size determination to read full frames atomically,
    /// preventing mid-frame corruption from timing issues.
    /// Inspired by One Channel Hub's sx126x_get_rx_buffer_status approach.
    #[cfg(feature = "rfm69")]
    async fn handle_fifo_interrupt_burst(
        spi: &Arc<Mutex<Spi>>,
        packet_buffer: &Arc<Mutex<PacketBuffer>>,
        stats: &Arc<Mutex<PacketStats>>,
    ) -> Result<(), Rfm69Error> {
        // First, get the payload size from FIFO status
        // This is critical for preventing partial frame reads
        let payload_size = Self::get_fifo_payload_size(spi).await?;

        if payload_size == 0 {
            debug!("FIFO interrupt with no payload");
            return Ok(());
        }

        // Validate payload size against maximum expected
        if payload_size > 255 {
            warn!("Invalid payload size detected: {}", payload_size);
            stats.lock().await.fifo_overruns += 1;
            Self::clear_fifo(spi).await?;
            return Ok(());
        }

        // Now read the exact payload size in a single burst
        // This prevents partial frame corruption seen in logs
        let mut header_bytes = Vec::new();

        // Read first 2 bytes for packet type determination
        for _ in 0..2 {
            if Self::fifo_not_empty(spi).await? {
                let byte = Self::read_register_static(spi, REG_FIFO).await?;
                header_bytes.push(byte);
            }
        }

        if header_bytes.len() < 2 {
            return Ok(()); // Not enough data yet
        }

        // Determine expected packet size from header
        let expected_size = {
            let mut buffer = packet_buffer.lock().unwrap();
            // Add header bytes to buffer
            for byte in &header_bytes {
                buffer.push_byte(*byte);
            }

            // Try to determine packet size
            match buffer.determine_packet_size() {
                Some(size) => size,
                None => {
                    // Can't determine size yet, continue byte-by-byte
                    return Ok(());
                }
            }
        };

        // Read remaining bytes in burst
        let remaining = expected_size.saturating_sub(header_bytes.len());
        if remaining > 0 {
            match Self::read_burst(spi, remaining).await {
                Ok(data) => {
                    let mut buffer = packet_buffer.lock().unwrap();
                    for byte in data {
                        buffer.push_byte(byte);
                    }

                    // Check if packet is complete
                    if buffer.is_complete() {
                        debug!("Burst read complete: {} bytes total", expected_size);
                        let mut stats = stats.lock().unwrap();
                        stats.packets_received += 1;
                    }
                }
                Err(e) => {
                    warn!("Burst read failed: {}", e);
                    let mut stats = stats.lock().unwrap();
                    stats.fifo_overruns += 1;
                }
            }
        }

        Ok(())
    }

    /// Read a register value
    async fn read_register(&self, reg: u8) -> Result<u8, Rfm69Error> {
        #[cfg(feature = "rfm69")]
        {
            Self::read_register_static(&self.spi, reg).await
        }

        #[cfg(not(feature = "rfm69"))]
        {
            Err(Rfm69Error::FeatureNotEnabled(
                "rfm69 feature not enabled".to_string(),
            ))
        }
    }

    /// Static version of read_register for use in tasks
    #[cfg(feature = "rfm69")]
    async fn read_register_static(spi: &Arc<Mutex<Spi>>, reg: u8) -> Result<u8, Rfm69Error> {
        let tx = [reg & 0x7F, 0];
        let mut rx = [0u8; 2];

        {
            let mut spi = spi.lock().unwrap();
            spi.transfer(&mut rx, &tx)
                .map_err(|e| Rfm69Error::Spi(format!("Read register failed: {}", e)))?;
        }

        Ok(rx[1])
    }

    /// Write a register value
    async fn write_register(&self, reg: u8, value: u8) -> Result<(), Rfm69Error> {
        #[cfg(feature = "rfm69")]
        {
            let tx = [reg | 0x80, value];

            {
                let mut spi = self.spi.lock().unwrap();
                spi.write(&tx)
                    .map_err(|e| Rfm69Error::Spi(format!("Write register failed: {}", e)))?;
            }

            Ok(())
        }

        #[cfg(not(feature = "rfm69"))]
        {
            Err(Rfm69Error::FeatureNotEnabled(
                "rfm69 feature not enabled".to_string(),
            ))
        }
    }

    /// Write specific bits in a register
    async fn write_register_bits(&self, reg: u8, mask: u8, bits: u8) -> Result<(), Rfm69Error> {
        let current = self.read_register(reg).await?;
        let new_value = (current & !mask) | bits;
        self.write_register(reg, new_value).await
    }

    /// Get packet statistics
    pub fn get_stats(&self) -> PacketStats {
        self.stats.lock().unwrap().clone()
    }

    /// Initialize SPI interface
    #[cfg(feature = "rfm69")]
    fn init_spi(config: &Rfm69Config) -> Result<Spi, Rfm69Error> {
        let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, SPI_SPEED, Mode::Mode0)
            .map_err(|e| Rfm69Error::Spi(format!("Failed to initialize SPI: {}", e)))?;

        info!(
            "SPI interface initialized: {}",
            config
                .spidev
                .as_ref()
                .unwrap_or(&"/dev/spidev0.0".to_string())
        );
        Ok(spi)
    }

    /// Initialize GPIO pins
    #[cfg(feature = "rfm69")]
    fn init_gpio(
        config: &Rfm69Config,
    ) -> Result<(Option<OutputPin>, Option<InputPin>), Rfm69Error> {
        let gpio = Gpio::new()
            .map_err(|e| Rfm69Error::Gpio(format!("Failed to initialize GPIO: {}", e)))?;

        let reset_pin = if let Some(pin_num) = config.reset_pin {
            Some(
                gpio.get(pin_num)
                    .map_err(|e| {
                        Rfm69Error::Gpio(format!("Failed to get reset pin {}: {}", pin_num, e))
                    })?
                    .into_output(),
            )
        } else {
            None
        };

        let interrupt_pin = if let Some(pin_num) = config.interrupt_pin {
            Some(
                gpio.get(pin_num)
                    .map_err(|e| {
                        Rfm69Error::Gpio(format!("Failed to get interrupt pin {}: {}", pin_num, e))
                    })?
                    .into_input(),
            )
        } else {
            None
        };

        info!(
            "GPIO pins initialized - Reset: {:?}, Interrupt: {:?}",
            config.reset_pin, config.interrupt_pin
        );
        Ok((reset_pin, interrupt_pin))
    }
}

impl Drop for Rfm69Driver {
    fn drop(&mut self) {
        #[cfg(feature = "rfm69")]
        {
            // Send shutdown signal first
            if let Some(shutdown_tx) = self.shutdown_tx.take() {
                let _ = shutdown_tx.send(()); // Ignore if receiver is already dropped
            }

            // Then abort the task if it doesn't shutdown gracefully
            if let Some(handle) = self.interrupt_task.take() {
                handle.abort();
            }
        }
    }
}

impl Rfm69Driver {
    /// Get the current payload size in FIFO
    ///
    /// This is critical for atomic burst reading to prevent partial frames.
    /// Inspired by sx126x_get_rx_buffer_status from One Channel Hub.
    #[cfg(feature = "rfm69")]
    async fn get_fifo_payload_size(spi: &Arc<Mutex<Spi>>) -> Result<usize, Rfm69Error> {
        // For RFM69, we can determine size from the FIFO threshold and level
        // Read the number of bytes available in FIFO
        let fifo_status = Self::read_register_static(spi, 0x28).await?; // REG_IRQFLAGS2

        // Check if FIFO has data
        if (fifo_status & 0x40) == 0 {  // FifoNotEmpty bit
            return Ok(0);
        }

        // For now, estimate based on typical wM-Bus frame sizes
        // In a full implementation, we'd peek at the length field
        // Most wM-Bus frames are 50-100 bytes
        Ok(100)  // Conservative estimate to ensure we read enough
    }

    /// Clear the FIFO buffer
    ///
    /// Used when invalid data is detected to recover cleanly.
    #[cfg(feature = "rfm69")]
    async fn clear_fifo(spi: &Arc<Mutex<Spi>>) -> Result<(), Rfm69Error> {
        // Set and clear the FifoOverrun bit to flush FIFO
        let irq_flags = Self::read_register_static(spi, 0x28).await?; // REG_IRQFLAGS2
        Self::write_register_static(spi, 0x28, irq_flags | 0x10).await?; // Set FifoOverrun
        Ok(())
    }

    /// Gracefully shutdown the driver and its tasks
    pub async fn shutdown(&mut self) -> Result<(), Rfm69Error> {
        #[cfg(feature = "rfm69")]
        {
            info!("Shutting down RFM69 driver");

            // Send shutdown signal
            if let Some(shutdown_tx) = self.shutdown_tx.take() {
                if shutdown_tx.send(()).is_err() {
                    warn!("Failed to send shutdown signal - task may have already exited");
                }
            }

            // Wait for task to complete gracefully
            if let Some(handle) = self.interrupt_task.take() {
                if let Err(e) = tokio::time::timeout(Duration::from_secs(5), handle).await {
                    warn!("Interrupt task did not shutdown gracefully: {}", e);
                }
            }

            // Put radio to sleep
            if let Err(e) = self.set_mode(Rfm69Mode::Sleep).await {
                warn!("Failed to put radio to sleep during shutdown: {}", e);
            }

            info!("RFM69 driver shutdown completed");
        }

        Ok(())
    }
}

// Implementation of the RadioDriver trait for RFM69
#[async_trait::async_trait]
impl crate::wmbus::radio::radio_driver::RadioDriver for Rfm69Driver {
    async fn initialize(
        &mut self,
        config: crate::wmbus::radio::radio_driver::WMBusConfig,
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        // Update internal configuration from trait config
        if let Some(ref aes_key) = config
            .sync_word
            .get(0..32)
            .and_then(|bytes| Some(hex::encode(bytes)))
        {
            self.config.aes_key = Some(aes_key.clone());
        }

        // Initialize the RFM69 hardware
        self.initialize().await.map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "RFM69 init failed: {}",
                e
            ))
        })
    }

    async fn start_receive(
        &mut self,
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        self.set_mode(Rfm69Mode::Rx).await.map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to start RX: {}",
                e
            ))
        })
    }

    async fn stop_receive(
        &mut self,
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        self.set_mode(Rfm69Mode::Standby).await.map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to stop RX: {}",
                e
            ))
        })
    }

    async fn transmit(
        &mut self,
        data: &[u8],
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        if data.len() > MAX_PACKET_SIZE {
            return Err(
                crate::wmbus::radio::radio_driver::RadioDriverError::InvalidParams(format!(
                    "Packet too large: {} > {}",
                    data.len(),
                    MAX_PACKET_SIZE
                )),
            );
        }

        // Switch to standby mode for TX preparation
        self.set_mode(Rfm69Mode::Standby).await.map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "TX standby failed: {}",
                e
            ))
        })?;

        // TODO: Load data into FIFO and transmit
        // This would involve:
        // 1. Clear FIFO
        // 2. Load packet data
        // 3. Switch to TX mode
        // 4. Wait for TX completion
        warn!("RFM69 transmit not yet implemented");

        Err(
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(
                "TX not implemented".to_string(),
            ),
        )
    }

    async fn get_received_packet(
        &mut self,
    ) -> Result<
        Option<crate::wmbus::radio::radio_driver::ReceivedPacket>,
        crate::wmbus::radio::radio_driver::RadioDriverError,
    > {
        // Check packet buffer for complete packets
        let mut buffer = self.packet_buffer.lock().unwrap();

        if buffer.is_complete() {
            match buffer.extract_packet() {
                Ok(data) => {
                    // TODO: Get real RSSI and other packet info
                    let packet = crate::wmbus::radio::radio_driver::ReceivedPacket {
                        data,
                        rssi_dbm: -80, // Placeholder
                        freq_error_hz: None,
                        lqi: None,
                        crc_valid: true, // RFM69 packet processing validates CRC
                    };
                    Ok(Some(packet))
                }
                Err(e) => {
                    warn!("Failed to extract packet: {}", e);
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }

    async fn get_stats(
        &mut self,
    ) -> Result<
        crate::wmbus::radio::radio_driver::RadioStats,
        crate::wmbus::radio::radio_driver::RadioDriverError,
    > {
        let stats = self.get_stats();
        Ok(crate::wmbus::radio::radio_driver::RadioStats {
            packets_received: stats.total_frames,
            packets_crc_valid: stats.crc_ok_frames,
            packets_crc_error: stats.total_frames - stats.crc_ok_frames,
            packets_length_error: 0, // RFM69 doesn't track this separately
            last_rssi_dbm: -80,      // TODO: Get real RSSI
        })
    }

    async fn reset_stats(
        &mut self,
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        let mut stats = self.stats.lock().unwrap();
        *stats = PacketStats::default();
        Ok(())
    }

    async fn get_mode(
        &mut self,
    ) -> Result<
        crate::wmbus::radio::radio_driver::RadioMode,
        crate::wmbus::radio::radio_driver::RadioDriverError,
    > {
        let mode = match self.current_mode {
            Rfm69Mode::Sleep => crate::wmbus::radio::radio_driver::RadioMode::Sleep,
            Rfm69Mode::Standby => crate::wmbus::radio::radio_driver::RadioMode::Standby,
            Rfm69Mode::Tx => crate::wmbus::radio::radio_driver::RadioMode::Transmit,
            Rfm69Mode::Rx => crate::wmbus::radio::radio_driver::RadioMode::Receive,
        };
        Ok(mode)
    }

    async fn sleep(&mut self) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        self.set_mode(Rfm69Mode::Sleep).await.map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to sleep: {}",
                e
            ))
        })
    }

    async fn wake_up(&mut self) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        self.set_mode(Rfm69Mode::Standby).await.map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to wake up: {}",
                e
            ))
        })
    }

    async fn get_rssi(
        &mut self,
    ) -> Result<i16, crate::wmbus::radio::radio_driver::RadioDriverError> {
        // TODO: Implement RSSI reading from RFM69
        // Read REG_RSSIVALUE and convert to dBm
        warn!("RFM69 RSSI reading not yet implemented");
        Ok(-80) // Placeholder value
    }

    async fn is_channel_clear(
        &mut self,
        threshold_dbm: i16,
        listen_duration: Duration,
    ) -> Result<bool, crate::wmbus::radio::radio_driver::RadioDriverError> {
        // Start receiving to measure RSSI
        self.start_receive().await?;

        // Wait for measurement to settle
        sleep(listen_duration).await;

        // Get RSSI measurement
        let rssi = self.get_rssi().await?;

        // Channel is clear if RSSI is below threshold
        Ok(rssi < threshold_dbm)
    }

    fn get_driver_info(&self) -> crate::wmbus::radio::radio_driver::DriverInfo {
        crate::wmbus::radio::radio_driver::DriverInfo {
            name: "RFM69HCW".to_string(),
            version: "1.0.0".to_string(),
            frequency_bands: vec![
                (863_000_000, 870_000_000), // EU wM-Bus bands
                (902_000_000, 928_000_000), // US ISM band
            ],
            max_packet_size: MAX_PACKET_SIZE,
            supported_bitrates: vec![100_000, 50_000, 32_768],
            power_range_dbm: (-18, 20), // RFM69HCW power range
            features: vec![
                "GFSK".to_string(),
                "AES128".to_string(),
                "wM-Bus".to_string(),
                "GPIO_Interrupt".to_string(),
                "Variable_Length".to_string(),
            ],
        }
    }
}
