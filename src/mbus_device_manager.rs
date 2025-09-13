//! # M-Bus Device Manager
//!
//! This module provides the MBusDeviceManager struct, which serves as the main entry point for
//! interacting with both wired M-Bus and wireless wM-Bus devices using the mbus-rs crate.
//!
//! The MBusDeviceManager maintains separate handles for M-Bus and wM-Bus connections, allowing
//! the client to manage both types of devices concurrently.

use crate::error::MBusError;
use crate::mbus::secondary_addressing::{
    build_secondary_selection_frame, SecondaryAddress, WildcardResult, WildcardSearchManager,
};
use crate::mbus::serial::{MBusDeviceHandle, SerialConfig};
use crate::wmbus::handle::{WMBusHandleFactory, WMBusHandleWrapper};
use std::collections::HashMap;
use std::time::Duration;

/// Represents a manager for handling both wired M-Bus and wireless wM-Bus devices.
pub struct MBusDeviceManager {
    /// Stores the handles for the wired M-Bus connections.
    mbus_handles: HashMap<String, MBusDeviceHandle>,
    /// Stores the handles for the wireless wM-Bus connections.
    wmbus_handles: HashMap<String, Box<dyn WMBusHandleWrapper>>,
}

impl MBusDeviceManager {
    /// Creates a new MBusDeviceManager instance.
    pub async fn new() -> Result<Self, MBusError> {
        Ok(MBusDeviceManager {
            mbus_handles: HashMap::new(),
            wmbus_handles: HashMap::new(),
        })
    }

    /// Adds a new wired M-Bus handle to the manager.
    pub async fn add_mbus_handle(&mut self, port_name: &str) -> Result<(), MBusError> {
        let handle = MBusDeviceHandle::connect(port_name).await?;
        self.mbus_handles.insert(port_name.to_string(), handle);
        Ok(())
    }

    /// Adds a new wired M-Bus handle with config.
    pub async fn add_mbus_handle_with_config(
        &mut self,
        port_name: &str,
        baudrate: u32,
    ) -> Result<(), MBusError> {
        let config = SerialConfig {
            baudrate,
            timeout: std::time::Duration::from_secs(5),
            auto_baud_detection: false,
            collision_config: crate::mbus::serial::CollisionConfig::default(),
        };
        let handle = MBusDeviceHandle::connect_with_config(port_name, config).await?;
        self.mbus_handles.insert(port_name.to_string(), handle);
        Ok(())
    }

    /// Adds a new wireless wM-Bus handle to the manager using a mock HAL for testing.
    pub async fn add_wmbus_handle_mock(&mut self, device_id: &str) -> Result<(), MBusError> {
        let handle = WMBusHandleFactory::create_mock()
            .await
            .map_err(MBusError::from)?;
        self.wmbus_handles.insert(device_id.to_string(), handle);
        Ok(())
    }

    /// Adds a new wireless wM-Bus handle for Raspberry Pi with default configuration.
    #[cfg(feature = "raspberry-pi")]
    pub async fn add_wmbus_handle_raspberry_pi(
        &mut self,
        device_id: &str,
    ) -> Result<(), MBusError> {
        let handle = WMBusHandleFactory::create_raspberry_pi()
            .await
            .map_err(|e| MBusError::from(e))?;
        self.wmbus_handles.insert(device_id.to_string(), handle);
        Ok(())
    }

    /// Adds a new wireless wM-Bus handle for Raspberry Pi with custom configuration.
    #[cfg(feature = "raspberry-pi")]
    pub async fn add_wmbus_handle_raspberry_pi_custom(
        &mut self,
        device_id: &str,
        spi_bus: u8,
        spi_speed: u32,
        busy_pin: u8,
        dio1_pin: u8,
        dio2_pin: Option<u8>,
        reset_pin: Option<u8>,
    ) -> Result<(), MBusError> {
        let handle = WMBusHandleFactory::create_raspberry_pi_custom(
            spi_bus, spi_speed, busy_pin, dio1_pin, dio2_pin, reset_pin,
        )
        .await
        .map_err(|e| MBusError::from(e))?;
        self.wmbus_handles.insert(device_id.to_string(), handle);
        Ok(())
    }

    /// Sends a request to all connected M-Bus and wM-Bus devices and collects the responses.
    pub async fn send_request(
        &mut self,
        address: u8,
    ) -> Result<Vec<crate::payload::record::MBusRecord>, MBusError> {
        let mut records = Vec::new();

        // Send the request to all connected M-Bus devices
        for (_, handle) in self.mbus_handles.iter_mut() {
            records.extend(handle.send_request(address).await?);
        }

        // Note: wM-Bus devices use a different paradigm - they are discovered via
        // scanning and frames are received asynchronously. Direct request/response
        // is not typically used in wM-Bus as devices broadcast periodically.

        Ok(records)
    }

    /// Scans for available M-Bus and wM-Bus devices and returns their addresses.
    pub async fn scan_devices(&mut self) -> Result<Vec<String>, MBusError> {
        let mut addresses = Vec::new();

        // Scan for available M-Bus devices
        for (_, handle) in self.mbus_handles.iter_mut() {
            addresses.extend(handle.scan_devices().await?);
        }

        // Scan for available wM-Bus devices
        for (_, handle) in self.wmbus_handles.iter_mut() {
            let wmbus_devices = handle
                .scan_devices()
                .await
                .map_err(MBusError::from)?;
            // Convert DeviceInfo to String addresses
            addresses.extend(
                wmbus_devices
                    .iter()
                    .map(|device| format!("0x{:08X}", device.address)),
            );
        }

        Ok(addresses)
    }

    /// Disconnects from all connected M-Bus and wM-Bus devices.
    pub async fn disconnect_all(&mut self) -> Result<(), MBusError> {
        // Disconnect from all M-Bus devices
        for (_, handle) in self.mbus_handles.iter_mut() {
            handle.disconnect().await?;
        }

        // Disconnect from all wM-Bus devices
        for (_, handle) in self.wmbus_handles.iter_mut() {
            handle.stop_receiver().await;
        }

        Ok(())
    }

    /// Discover M-Bus devices using secondary addressing with wildcard search
    /// Implements the collision resolution algorithm from EN 13757-2
    pub async fn discover_secondary_devices(
        &mut self,
        port_name: &str,
    ) -> Result<Vec<SecondaryAddress>, MBusError> {
        // Check if handle exists first
        if !self.mbus_handles.contains_key(port_name) {
            return Err(MBusError::DeviceDiscoveryError(format!(
                "M-Bus handle not found for port: {port_name}"
            )));
        }

        let mut manager = WildcardSearchManager::new();

        // Start with full wildcard search
        let mut search_pattern = [0xF; 8];
        Self::wildcard_search_recursive_impl(
            &mut self.mbus_handles,
            port_name,
            &mut manager,
            &mut search_pattern,
            0,
        )
        .await?;

        Ok(manager.discovered_addresses().to_vec())
    }

    /// Static recursive implementation to avoid borrowing issues
    fn wildcard_search_recursive_impl<'a>(
        handles: &'a mut HashMap<String, MBusDeviceHandle>,
        port_name: &'a str,
        manager: &'a mut WildcardSearchManager,
        pattern: &'a mut [u8; 8],
        position: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), MBusError>> + 'a>> {
        Box::pin(async move {
            if position >= 8 {
                // Pattern fully specified, should have single device
                if let Some(addr) = Self::query_specific_secondary_impl(pattern).await? {
                    manager.add_discovered(addr);
                }
                return Ok(());
            }

            // Try all hex values for current position
            for hex_digit in 0x0..=0xF {
                pattern[position] = hex_digit;

                match Self::test_wildcard_pattern_impl(handles, port_name, pattern).await? {
                    WildcardResult::Single => {
                        // Found single device, query it specifically
                        if let Some(addr) = Self::query_specific_secondary_impl(pattern).await? {
                            manager.add_discovered(addr);
                        }
                    }
                    WildcardResult::Multiple => {
                        // Collision, recurse to next position
                        Self::wildcard_search_recursive_impl(
                            handles,
                            port_name,
                            manager,
                            pattern,
                            position + 1,
                        )
                        .await?;
                    }
                    WildcardResult::None => {
                        // No devices match this pattern, continue
                    }
                }
            }

            pattern[position] = 0xF; // Reset wildcard for backtracking
            Ok(())
        })
    }

    /// Static implementation for testing wildcard patterns
    async fn test_wildcard_pattern_impl(
        _handles: &mut HashMap<String, MBusDeviceHandle>,
        _port_name: &str,
        pattern: &[u8; 8],
    ) -> Result<WildcardResult, MBusError> {
        // Send secondary selection frame
        let _selection_frame = build_secondary_selection_frame(pattern);

        // Note: In a real implementation, this would use the actual frame sending
        // For now, we simulate the behavior based on the pattern

        // Count responses with timeout (simulate collision detection)
        let response_count = Self::count_responses_with_timeout_impl().await?;

        match response_count {
            0 => Ok(WildcardResult::None),
            1 => Ok(WildcardResult::Single),
            _ => Ok(WildcardResult::Multiple),
        }
    }

    /// Test a wildcard pattern and determine response type (none/single/multiple)
    #[allow(dead_code)]
    async fn test_wildcard_pattern(
        &self,
        handle: &mut MBusDeviceHandle,
        pattern: &[u8; 8],
    ) -> Result<WildcardResult, MBusError> {
        // Send secondary selection frame
        let _selection_frame = build_secondary_selection_frame(pattern);

        // Note: In a real implementation, this would use the actual frame sending
        // For now, we simulate the behavior based on the pattern

        // Send the frame (this is a placeholder - actual implementation would send via serial)
        // let response = handle.send_raw_frame(&selection_frame).await?;

        // Count responses with timeout (simulate collision detection)
        let response_count = self
            .count_responses_with_timeout(handle, Duration::from_millis(100))
            .await?;

        match response_count {
            0 => Ok(WildcardResult::None),
            1 => Ok(WildcardResult::Single),
            _ => Ok(WildcardResult::Multiple),
        }
    }

    /// Static implementation for querying specific secondary address
    async fn query_specific_secondary_impl(
        pattern: &[u8; 8],
    ) -> Result<Option<SecondaryAddress>, MBusError> {
        // If pattern has wildcards, we can't get a specific address
        if pattern.contains(&0xF) {
            return Ok(None);
        }

        // Convert pattern to secondary address
        match SecondaryAddress::from_bytes(pattern) {
            Ok(addr) => Ok(Some(addr)),
            Err(_) => Ok(None),
        }
    }

    /// Query a specific secondary address (when pattern is fully specified or single device found)
    #[allow(dead_code)]
    async fn query_specific_secondary(
        &self,
        _handle: &mut MBusDeviceHandle,
        pattern: &[u8; 8],
    ) -> Result<Option<SecondaryAddress>, MBusError> {
        Self::query_specific_secondary_impl(pattern).await
    }

    /// Static implementation for counting responses
    async fn count_responses_with_timeout_impl() -> Result<usize, MBusError> {
        // This is a simplified implementation
        // In reality, this would monitor the bus for E5h responses or similar

        // For now, simulate based on device availability
        // In a real implementation, this would:
        // 1. Send the selection frame
        // 2. Wait for responses (E5h ACK frames)
        // 3. Count unique responses within timeout period
        // 4. Detect collisions by monitoring bus activity

        // Placeholder implementation returns 0 (no devices found)
        Ok(0)
    }

    /// Count responses within timeout period (collision detection)
    #[allow(dead_code)]
    async fn count_responses_with_timeout(
        &self,
        _handle: &mut MBusDeviceHandle,
        _timeout: Duration,
    ) -> Result<usize, MBusError> {
        Self::count_responses_with_timeout_impl().await
    }

    /// Send a request to a specific secondary address
    pub async fn send_request_to_secondary(
        &mut self,
        port_name: &str,
        secondary_addr: &SecondaryAddress,
    ) -> Result<Vec<crate::payload::record::MBusRecord>, MBusError> {
        let _handle = self.mbus_handles.get_mut(port_name).ok_or_else(|| {
            MBusError::DeviceDiscoveryError(format!(
                "M-Bus handle not found for port: {port_name}"
            ))
        })?;

        // Build secondary selection frame
        let _selection_frame = build_secondary_selection_frame(&secondary_addr.to_bytes());

        // In a complete implementation, this would:
        // 1. Send the secondary selection frame
        // 2. Wait for device response
        // 3. Parse the response into MBusRecord structures
        // 4. Return the parsed records

        // For now, return empty vector as placeholder
        Ok(Vec::new())
    }
}
