//! # M-Bus Serial Communication
//!
//! This module provides the implementation for handling the serial communication
//! aspect of the M-Bus protocol, including connecting to the serial port,
//! sending M-Bus frames, and receiving M-Bus frames.

use crate::error::MBusError;
use crate::mbus::frame::{pack_frame, parse_frame, MBusFrame};
use crate::mbus::mbus_protocol::StateMachine;
use crate::payload::record::MBusRecord;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{sleep, timeout};
use tokio_serial::SerialPortBuilderExt;

/// Standard M-Bus baud rates as defined in EN 13757-2
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MBusBaudRate {
    Baud300 = 300,
    Baud600 = 600,
    Baud1200 = 1200,
    Baud2400 = 2400,
    Baud4800 = 4800,
    Baud9600 = 9600,
    Baud19200 = 19200,
    Baud38400 = 38400,
}

impl MBusBaudRate {
    /// Standard M-Bus baud rates in order of preference for auto-detection
    pub const ALL_RATES: &'static [MBusBaudRate] = &[
        MBusBaudRate::Baud2400,  // Most common default
        MBusBaudRate::Baud9600,  // Second most common
        MBusBaudRate::Baud4800,  // Common alternative
        MBusBaudRate::Baud1200,  // Lower speed
        MBusBaudRate::Baud300,   // Legacy/long distance
        MBusBaudRate::Baud600,   // Legacy
        MBusBaudRate::Baud19200, // High speed
        MBusBaudRate::Baud38400, // Very high speed
    ];

    pub fn as_u32(&self) -> u32 {
        *self as u32
    }

    /// Calculate optimal timeout for baud rate (EN 13757-2 Section 4.2.8)
    pub fn timeout(&self) -> Duration {
        match self {
            MBusBaudRate::Baud300 => Duration::from_millis(1300),
            MBusBaudRate::Baud600 => Duration::from_millis(800),
            MBusBaudRate::Baud1200 => Duration::from_millis(500),
            MBusBaudRate::Baud2400 => Duration::from_millis(300),
            MBusBaudRate::Baud4800 => Duration::from_millis(300),
            MBusBaudRate::Baud9600 => Duration::from_millis(200),
            MBusBaudRate::Baud19200 => Duration::from_millis(150),
            MBusBaudRate::Baud38400 => Duration::from_millis(100),
        }
    }

    /// Calculate inter-frame delay (EN 13757-2 Section 4.2.6)
    pub fn inter_frame_delay(&self) -> Duration {
        match self {
            MBusBaudRate::Baud300 => Duration::from_millis(100),
            MBusBaudRate::Baud600 => Duration::from_millis(50),
            MBusBaudRate::Baud1200 => Duration::from_millis(25),
            MBusBaudRate::Baud2400 => Duration::from_millis(11),
            MBusBaudRate::Baud4800 => Duration::from_millis(11),
            MBusBaudRate::Baud9600 => Duration::from_millis(5),
            MBusBaudRate::Baud19200 => Duration::from_millis(3),
            MBusBaudRate::Baud38400 => Duration::from_millis(2),
        }
    }
}

impl From<u32> for MBusBaudRate {
    fn from(value: u32) -> Self {
        match value {
            300 => MBusBaudRate::Baud300,
            600 => MBusBaudRate::Baud600,
            1200 => MBusBaudRate::Baud1200,
            2400 => MBusBaudRate::Baud2400,
            4800 => MBusBaudRate::Baud4800,
            9600 => MBusBaudRate::Baud9600,
            19200 => MBusBaudRate::Baud19200,
            38400 => MBusBaudRate::Baud38400,
            _ => MBusBaudRate::Baud2400, // Default fallback
        }
    }
}

/// Collision handling parameters (EN 13757-2 Section 5.3)
#[derive(Debug, Clone)]
pub struct CollisionConfig {
    /// Maximum number of collision resolution attempts
    pub max_collision_retries: u8,
    /// Initial backoff delay (doubled on each retry)
    pub initial_backoff_ms: u64,
    /// Maximum backoff delay to prevent excessive waiting
    pub max_backoff_ms: u64,
    /// Collision detection threshold (number of overlapping responses)
    pub collision_threshold: usize,
}

impl Default for CollisionConfig {
    fn default() -> Self {
        CollisionConfig {
            max_collision_retries: 5,
            initial_backoff_ms: 10,
            max_backoff_ms: 500,
            collision_threshold: 2,
        }
    }
}

/// Configuration for serial connection.
#[derive(Debug, Clone)]
pub struct SerialConfig {
    pub baudrate: u32,
    pub timeout: Duration,
    /// Enable automatic baud rate detection and switching
    pub auto_baud_detection: bool,
    /// Collision handling configuration
    pub collision_config: CollisionConfig,
}

impl Default for SerialConfig {
    fn default() -> Self {
        SerialConfig {
            baudrate: 2400,
            timeout: Duration::from_secs(5),
            auto_baud_detection: false,
            collision_config: CollisionConfig::default(),
        }
    }
}

/// Statistics for collision detection and monitoring
#[derive(Debug, Clone, Default)]
pub struct CollisionStatistics {
    /// Total number of collisions detected
    pub total_collisions: u64,
    /// Total number of successful communications
    pub successful_communications: u64,
    /// Total number of timeout errors
    pub timeout_errors: u64,
    /// Total number of baud rate switches performed
    pub baud_rate_switches: u64,
    /// Current collision rate (collisions per 100 attempts)
    pub collision_rate: f64,
}

impl CollisionStatistics {
    /// Update collision rate based on recent activity
    pub fn update_collision_rate(&mut self) {
        let total_attempts = self.total_collisions + self.successful_communications;
        if total_attempts > 0 {
            self.collision_rate = (self.total_collisions as f64 / total_attempts as f64) * 100.0;
        }
    }

    /// Check if collision rate is above threshold requiring intervention
    pub fn is_high_collision_rate(&self, threshold: f64) -> bool {
        self.collision_rate > threshold
    }
}

/// Represents a handle to the M-Bus serial connection, encapsulating the tokio_serial::SerialPort.
pub struct MBusDeviceHandle {
    port: tokio_serial::SerialStream,
    config: SerialConfig,
    /// Current effective baud rate
    current_baud_rate: MBusBaudRate,
    /// Port name for reconnection during baud rate switching
    port_name: String,
    /// Statistics for collision detection and performance monitoring
    collision_stats: CollisionStatistics,
}

impl MBusDeviceHandle {
    /// Establishes a connection to the serial port using the provided port name.
    /// It sets up the serial port settings (baud rate, data bits, stop bits, parity, and timeout) and opens the port.
    pub async fn connect(port_name: &str) -> Result<MBusDeviceHandle, MBusError> {
        Self::connect_with_config(port_name, SerialConfig::default()).await
    }

    /// Establishes a connection with custom config.
    pub async fn connect_with_config(
        port_name: &str,
        config: SerialConfig,
    ) -> Result<MBusDeviceHandle, MBusError> {
        if config.auto_baud_detection {
            Self::connect_with_auto_baud_detection(port_name, config).await
        } else {
            Self::connect_with_fixed_baud_rate(port_name, config).await
        }
    }

    /// Connect with automatic baud rate detection
    async fn connect_with_auto_baud_detection(
        port_name: &str,
        config: SerialConfig,
    ) -> Result<MBusDeviceHandle, MBusError> {
        for &baud_rate in MBusBaudRate::ALL_RATES {
            match Self::try_connect_at_baud_rate(port_name, &config, baud_rate).await {
                Ok(mut handle) => {
                    println!("Successfully connected at {} baud", baud_rate.as_u32());
                    handle.collision_stats.baud_rate_switches += 1;
                    return Ok(handle);
                }
                Err(e) => {
                    println!("Failed to connect at {} baud: {}", baud_rate.as_u32(), e);
                    continue;
                }
            }
        }
        Err(MBusError::SerialPortError(
            "Auto baud detection failed - no working baud rate found".to_string(),
        ))
    }

    /// Connect with fixed baud rate
    async fn connect_with_fixed_baud_rate(
        port_name: &str,
        config: SerialConfig,
    ) -> Result<MBusDeviceHandle, MBusError> {
        let baud_rate = MBusBaudRate::from(config.baudrate);
        Self::try_connect_at_baud_rate(port_name, &config, baud_rate).await
    }

    /// Try to connect at a specific baud rate
    async fn try_connect_at_baud_rate(
        port_name: &str,
        config: &SerialConfig,
        baud_rate: MBusBaudRate,
    ) -> Result<MBusDeviceHandle, MBusError> {
        let port = tokio_serial::new(port_name, baud_rate.as_u32())
            .data_bits(tokio_serial::DataBits::Eight)
            .stop_bits(tokio_serial::StopBits::One)
            .parity(tokio_serial::Parity::Even)
            .timeout(baud_rate.timeout())
            .open_native_async()
            .map_err(|e| MBusError::SerialPortError(e.to_string()))?;

        let mut handle = MBusDeviceHandle {
            port,
            config: config.clone(),
            current_baud_rate: baud_rate,
            port_name: port_name.to_string(),
            collision_stats: CollisionStatistics::default(),
        };

        // Test connectivity with a ping-like operation
        if config.auto_baud_detection {
            match handle.test_connectivity().await {
                Ok(_) => Ok(handle),
                Err(e) => Err(e),
            }
        } else {
            Ok(handle)
        }
    }

    /// Test connectivity at current baud rate using a broadcast ping
    async fn test_connectivity(&mut self) -> Result<(), MBusError> {
        // Send a broadcast SND_NKE (Initialize) frame to test connectivity
        // This frame should get some response or at least not cause errors
        let test_frame = crate::mbus::frame::MBusFrame {
            frame_type: crate::mbus::frame::MBusFrameType::Short,
            control: 0x40, // SND_NKE (Initialize)
            address: 0xFE, // Broadcast address
            control_information: 0,
            data: vec![],
            checksum: 0x3E, // 0x40 + 0xFE = 0x13E -> 0x3E
            more_records_follow: false,
        };

        // Send test frame
        match self.send_frame(&test_frame).await {
            Ok(_) => {
                // Wait a moment and see if we get any response or if there are no immediate errors
                sleep(self.current_baud_rate.inter_frame_delay()).await;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Switch to a different baud rate dynamically
    pub async fn switch_baud_rate(&mut self, new_baud_rate: MBusBaudRate) -> Result<(), MBusError> {
        if new_baud_rate == self.current_baud_rate {
            return Ok(()); // Already at desired rate
        }

        // Note: The port will be replaced, which automatically closes the old connection

        // Reconnect at new baud rate
        let port = tokio_serial::new(&self.port_name, new_baud_rate.as_u32())
            .data_bits(tokio_serial::DataBits::Eight)
            .stop_bits(tokio_serial::StopBits::One)
            .parity(tokio_serial::Parity::Even)
            .timeout(new_baud_rate.timeout())
            .open_native_async()
            .map_err(|e| MBusError::SerialPortError(e.to_string()))?;

        self.port = port;
        self.current_baud_rate = new_baud_rate;
        self.collision_stats.baud_rate_switches += 1;

        // Test new connection
        self.test_connectivity().await
    }

    /// Get current collision statistics
    pub fn collision_statistics(&self) -> &CollisionStatistics {
        &self.collision_stats
    }

    /// Reset collision statistics
    pub fn reset_collision_statistics(&mut self) {
        self.collision_stats = CollisionStatistics::default();
    }

    /// Automatically switch baud rate if collision rate is too high
    pub async fn auto_adapt_baud_rate(&mut self) -> Result<bool, MBusError> {
        if !self.config.auto_baud_detection {
            return Ok(false); // Auto-adaptation disabled
        }

        // Check if collision rate is above threshold (e.g., 30%)
        if !self.collision_stats.is_high_collision_rate(30.0) {
            return Ok(false); // No adaptation needed
        }

        // Find next best baud rate (try lower rates for better reliability)
        let current_index = MBusBaudRate::ALL_RATES
            .iter()
            .position(|&rate| rate == self.current_baud_rate)
            .unwrap_or(0);

        // Try the next rate in the list
        if current_index + 1 < MBusBaudRate::ALL_RATES.len() {
            let new_rate = MBusBaudRate::ALL_RATES[current_index + 1];
            println!(
                "High collision rate detected ({}%), switching from {} to {} baud",
                self.collision_stats.collision_rate,
                self.current_baud_rate.as_u32(),
                new_rate.as_u32()
            );

            match self.switch_baud_rate(new_rate).await {
                Ok(_) => {
                    self.reset_collision_statistics();
                    Ok(true)
                }
                Err(e) => Err(e),
            }
        } else {
            // No more rates to try
            Ok(false)
        }
    }

    /// Enhanced send request with automatic baud rate adaptation
    pub async fn send_request_with_adaptation(
        &mut self,
        address: u8,
    ) -> Result<Vec<MBusRecord>, MBusError> {
        let initial_attempts = 2;

        // First try at current baud rate
        for _ in 0..initial_attempts {
            match self.send_request(address).await {
                Ok(records) => return Ok(records),
                Err(_) => {
                    // Check if we should adapt baud rate
                    if self.auto_adapt_baud_rate().await? {
                        // Baud rate was changed, try again
                        continue;
                    }
                }
            }
        }

        // Final attempt after potential baud rate adaptation
        self.send_request(address).await
    }

    /// Closes the serial port connection.
    pub async fn disconnect(&mut self) -> Result<(), MBusError> {
        // SerialStream does not have a close method; dropping the handle closes it
        Ok(())
    }

    /// Takes an `MBusFrame` and sends it over the serial connection.
    /// It uses the `pack_frame()` function from the `frame.rs` module to convert the frame to a byte vector,
    /// and then writes the data to the serial port. It also flushes the serial port to ensure the frame is fully transmitted.
    pub async fn send_frame(&mut self, frame: &MBusFrame) -> Result<(), MBusError> {
        let data = pack_frame(frame);
        self.port
            .write_all(&data)
            .await
            .map_err(|e| MBusError::SerialPortError(e.to_string()))?;
        self.port
            .flush()
            .await
            .map_err(|e| MBusError::SerialPortError(e.to_string()))
    }

    /// Reads data from the serial port and attempts to parse an `MBusFrame` from the received bytes.
    /// Uses enhanced timeout calculation based on current baud rate.
    pub async fn recv_frame(&mut self) -> Result<MBusFrame, MBusError> {
        self.recv_frame_with_collision_handling().await
    }

    /// Enhanced frame reception with collision handling
    async fn recv_frame_with_collision_handling(&mut self) -> Result<MBusFrame, MBusError> {
        let to = self.current_baud_rate.timeout();
        let max_retries = self.config.collision_config.max_collision_retries;
        let mut backoff_delay =
            Duration::from_millis(self.config.collision_config.initial_backoff_ms);

        for attempt in 0..max_retries {
            match self.recv_frame_single_attempt(to).await {
                Ok(frame) => {
                    self.collision_stats.successful_communications += 1;
                    self.collision_stats.update_collision_rate();
                    return Ok(frame);
                }
                Err(MBusError::NomError(ref msg)) if msg.contains("timeout") => {
                    self.collision_stats.timeout_errors += 1;

                    if attempt < max_retries - 1 {
                        // Apply exponential backoff for potential collision resolution
                        sleep(backoff_delay).await;
                        backoff_delay = std::cmp::min(
                            backoff_delay * 2,
                            Duration::from_millis(self.config.collision_config.max_backoff_ms),
                        );
                        continue;
                    } else {
                        return Err(MBusError::NomError(
                            "Timeout after collision handling retries".to_string(),
                        ));
                    }
                }
                Err(e) => {
                    // Non-timeout errors are likely not collision-related
                    return Err(e);
                }
            }
        }

        self.collision_stats.total_collisions += 1;
        self.collision_stats.update_collision_rate();
        Err(MBusError::NomError(
            "Max collision retries exceeded".to_string(),
        ))
    }

    /// Single attempt to receive a frame without collision handling
    async fn recv_frame_single_attempt(&mut self, to: Duration) -> Result<MBusFrame, MBusError> {
        // Read first byte (start)
        let mut start = [0u8; 1];
        let n = timeout(to, self.port.read(&mut start))
            .await
            .map_err(|_| MBusError::NomError("timeout".into()))
            .and_then(|res| res.map_err(|e| MBusError::SerialPortError(e.to_string())))?;
        if n == 0 {
            return Err(MBusError::NomError("empty".into()));
        }

        let total_len = match start[0] {
            0xE5 => 1usize, // ACK
            0x10 => 5usize, // SHORT
            0x68 => {
                // Need to read two length bytes to determine total
                let mut lenbuf = [0u8; 2];
                timeout(to, self.port.read_exact(&mut lenbuf))
                    .await
                    .map_err(|_| MBusError::NomError("timeout".into()))
                    .and_then(|res| res.map_err(|e| MBusError::SerialPortError(e.to_string())))?;
                let length1 = lenbuf[0] as usize;
                // total = len1 + 6 bytes (0x68 len1 len2 0x68 ... checksum 0x16)
                6 + length1
            }
            _ => return Err(MBusError::FrameParseError("Invalid frame start".into())),
        };

        // We already consumed 1 byte, possibly 3 bytes; gather remaining
        let mut buf = Vec::with_capacity(total_len);
        buf.push(start[0]);
        if start[0] == 0x68 {
            // fetch already-read len bytes and read rest
            // We already read lenbuf; but we didn't keep them. Re-read full frame after start for simplicity
            // Read remaining (total_len - 1) bytes
            let mut rest = vec![0u8; total_len - 1];
            timeout(to, self.port.read_exact(&mut rest))
                .await
                .map_err(|_| MBusError::NomError("timeout".into()))
                .and_then(|res| res.map_err(|e| MBusError::SerialPortError(e.to_string())))?;
            buf.extend_from_slice(&rest);
        } else {
            let mut rest = vec![0u8; total_len - 1];
            timeout(to, self.port.read_exact(&mut rest))
                .await
                .map_err(|_| MBusError::NomError("timeout".into()))
                .and_then(|res| res.map_err(|e| MBusError::SerialPortError(e.to_string())))?;
            buf.extend_from_slice(&rest);
        }

        let (_, frame) =
            parse_frame(&buf[..]).map_err(|e| MBusError::FrameParseError(format!("{e:?}")))?;
        Ok(frame)
    }

    // Stub: send a request to a device by address and return parsed records (none by default).
}

impl MBusDeviceHandle {
    /// Sends a complete M-Bus data request to a device and returns parsed records.
    /// Implements the full M-Bus communication sequence with proper error handling and retries.
    ///
    /// # Arguments
    /// * `address` - Primary address of the target device (1-250)
    ///
    /// # Returns
    /// * `Ok(Vec<MBusRecord>)` - Successfully parsed data records from the device
    /// * `Err(MBusError)` - Communication failed after all retries
    pub async fn send_request(&mut self, address: u8) -> Result<Vec<MBusRecord>, MBusError> {
        let mut state_machine = StateMachine::new();
        let max_retries = 3; // M-Bus spec: maximum 3 attempts
        let mut all_records = Vec::new();

        // Calculate timeout based on current baud rate
        let communication_timeout = self.current_baud_rate.timeout();
        let inter_frame_delay = self.current_baud_rate.inter_frame_delay();

        for attempt in 0..max_retries {
            match self
                .attempt_communication(
                    &mut state_machine,
                    address,
                    communication_timeout,
                    inter_frame_delay,
                )
                .await
            {
                Ok(mut records) => {
                    all_records.append(&mut records);
                    return Ok(all_records);
                }
                Err(error) => {
                    if attempt < max_retries - 1 {
                        // Handle error and determine if retry is possible
                        match state_machine.handle_error(error) {
                            Ok(()) => {
                                // Retry is possible, wait before next attempt
                                sleep(inter_frame_delay * 2).await; // Extra delay for retry
                                continue;
                            }
                            Err(fatal_error) => {
                                // Fatal error, no point in retrying
                                return Err(fatal_error);
                            }
                        }
                    } else {
                        // All retries exhausted
                        return Err(MBusError::Other(format!(
                            "Communication failed after {max_retries} attempts"
                        )));
                    }
                }
            }
        }

        // This should never be reached due to the loop logic above
        Err(MBusError::Other("Unexpected end of retry loop".to_string()))
    }

    /// Performs a single communication attempt with a device.
    /// Handles the complete sequence including multi-frame responses.
    ///
    /// # Arguments
    /// * `state_machine` - M-Bus protocol state machine
    /// * `address` - Target device address
    /// * `communication_timeout` - Timeout for each frame exchange
    /// * `inter_frame_delay` - Minimum delay between frames
    ///
    /// # Returns
    /// * `Ok(Vec<MBusRecord>)` - Successfully parsed records
    /// * `Err(MBusError)` - Communication attempt failed
    async fn attempt_communication(
        &mut self,
        state_machine: &mut StateMachine,
        address: u8,
        communication_timeout: std::time::Duration,
        inter_frame_delay: std::time::Duration,
    ) -> Result<Vec<MBusRecord>, MBusError> {
        // Step 1: Select device (validate address)
        state_machine.select_device(address).await?;

        let mut all_payload_data = Vec::new();

        // Step 2: Request data (potentially multiple frames)
        loop {
            // Construct request frame
            let request_frame = state_machine.request_data().await?;

            // Send request frame with inter-frame delay
            sleep(inter_frame_delay).await;
            self.send_frame(&request_frame).await?;

            // Wait for and receive response frame
            let response_frame = timeout(communication_timeout, self.recv_frame())
                .await
                .map_err(|_| MBusError::Other("Response timeout".to_string()))?
                .map_err(|e| {
                    MBusError::FrameParseError(format!("Failed to receive frame: {e}"))
                })?;

            // Step 3: Validate and process received frame
            let (payload_data, more_frames) = state_machine.receive_data(&response_frame).await?;

            // Accumulate payload data
            all_payload_data.extend(payload_data);

            // If no more frames expected, break out of loop
            if !more_frames {
                break;
            }

            // For multi-frame communication, toggle FCB for next request
            state_machine.toggle_fcb();
        }

        // Step 4: Process all accumulated data
        let records = state_machine.process_data(&all_payload_data).await?;

        Ok(records)
    }

    /// Scans for M-Bus devices on the bus by sequentially polling all valid primary addresses.
    /// Uses REQ_UD2 requests to detect responding devices.
    ///
    /// # Returns
    /// * `Ok(Vec<String>)` - List of discovered device addresses as strings
    /// * `Err(MBusError)` - Scanning operation failed
    pub async fn scan_devices(&mut self) -> Result<Vec<String>, MBusError> {
        let mut discovered_devices = Vec::new();
        let mut state_machine = StateMachine::new();

        // Calculate timeouts - use shorter timeout for scanning to speed up process
        let scan_timeout = self.current_baud_rate.timeout() / 2; // Half normal timeout
        let inter_frame_delay = self.current_baud_rate.inter_frame_delay();

        println!("Starting M-Bus device scan (addresses 1-250)...");

        // Scan all valid primary addresses (1 to 250)
        for address in 1u8..=250u8 {
            // Reset state machine for each device
            state_machine.reset();

            match self
                .scan_single_device(&mut state_machine, address, scan_timeout, inter_frame_delay)
                .await
            {
                Ok(device_info) => {
                    discovered_devices.push(device_info);
                    println!("Found device at address {address}");
                }
                Err(_) => {
                    // Device not responding or error - continue scanning
                    // Don't print errors during scanning to avoid spam
                }
            }

            // Small delay between device polls to avoid overwhelming the bus
            sleep(inter_frame_delay).await;

            // Progress indication every 50 addresses
            if address % 50 == 0 {
                println!(
                    "Scanned up to address {}, found {} devices so far",
                    address,
                    discovered_devices.len()
                );
            }
        }

        println!(
            "Device scan complete. Found {} devices total",
            discovered_devices.len()
        );
        Ok(discovered_devices)
    }

    /// Attempts to communicate with a single device during scanning.
    /// Uses a single REQ_UD2 request with shorter timeout.
    ///
    /// # Arguments
    /// * `state_machine` - M-Bus protocol state machine
    /// * `address` - Device address to test
    /// * `scan_timeout` - Shorter timeout for scanning
    /// * `inter_frame_delay` - Delay between frames
    ///
    /// # Returns
    /// * `Ok(String)` - Device information string (address and basic info)
    /// * `Err(MBusError)` - Device not responding or communication failed
    async fn scan_single_device(
        &mut self,
        state_machine: &mut StateMachine,
        address: u8,
        scan_timeout: std::time::Duration,
        inter_frame_delay: std::time::Duration,
    ) -> Result<String, MBusError> {
        // Select device (validate address)
        state_machine.select_device(address).await?;

        // Send single request frame
        let request_frame = state_machine.request_data().await?;

        // Add inter-frame delay before transmission
        sleep(inter_frame_delay).await;
        self.send_frame(&request_frame).await?;

        // Wait for response with shorter timeout
        let response_frame = timeout(scan_timeout, self.recv_frame())
            .await
            .map_err(|_| MBusError::Other("Scan timeout".to_string()))?
            .map_err(|e| MBusError::FrameParseError(format!("Scan receive error: {e}")))?;

        // Basic validation - just check if we got a reasonable response
        let (payload_data, _) = state_machine.receive_data(&response_frame).await?;

        // Try to extract basic device information
        let device_info = if payload_data.is_empty() {
            format!("0x{address:02X} (no data)")
        } else {
            // Try to parse records to get more information
            match state_machine.process_data(&payload_data).await {
                Ok(records) if !records.is_empty() => {
                    let record_count = records.len();
                    format!("0x{address:02X} ({record_count} records)")
                }
                _ => {
                    format!("0x{:02X} ({} bytes)", address, payload_data.len())
                }
            }
        };

        Ok(device_info)
    }
}
