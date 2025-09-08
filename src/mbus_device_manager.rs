//! # M-Bus Device Manager
//!
//! This module provides the MBusDeviceManager struct, which serves as the main entry point for
//! interacting with both wired M-Bus and wireless wM-Bus devices using the mbus-rs crate.
//!
//! The MBusDeviceManager maintains separate handles for M-Bus and wM-Bus connections, allowing
//! the client to manage both types of devices concurrently.

use crate::error::MBusError;
use crate::mbus::serial::{MBusDeviceHandle, SerialConfig};
use crate::wmbus::handle::WMBusHandle;
use std::collections::HashMap;

/// Represents a manager for handling both wired M-Bus and wireless wM-Bus devices.
pub struct MBusDeviceManager {
    /// Stores the handles for the wired M-Bus connections.
    mbus_handles: HashMap<String, MBusDeviceHandle>,
    /// Stores the handles for the wireless wM-Bus connections.
    wmbus_handles: HashMap<String, WMBusHandle>,
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
    pub async fn add_mbus_handle_with_config(&mut self, port_name: &str, baudrate: u32) -> Result<(), MBusError> {
        let config = SerialConfig { baudrate, timeout: std::time::Duration::from_secs(5) };
        let handle = MBusDeviceHandle::connect_with_config(port_name, config).await?;
        self.mbus_handles.insert(port_name.to_string(), handle);
        Ok(())
    }

    /// Adds a new wireless wM-Bus handle to the manager.
    pub async fn add_wmbus_handle(&mut self, device_id: &str) -> Result<(), MBusError> {
        let handle = WMBusHandle::connect(device_id).await?;
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

        // Send the request to all connected wM-Bus devices
        for (_, handle) in self.wmbus_handles.iter_mut() {
            records.extend(handle.send_request(address).await?);
        }

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
            addresses.extend(handle.scan_devices().await?);
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
            handle.disconnect().await?;
        }

        Ok(())
    }
}
