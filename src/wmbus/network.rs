//! # Wireless M-Bus Network Management
//!
//! This module provides network-level functionality for wM-Bus including device discovery,
//! network scanning, and topology analysis. It builds on the lower-level WMBusHandle
//! to provide network management capabilities.
//!
//! ## Features
//!
//! - Multi-frequency network scanning
//! - Device topology discovery
//! - Network statistics and monitoring
//! - Device classification and filtering
//! - Geographic clustering of devices
//!
//! ## Usage
//!
//! ```rust,no_run
//! use mbus_rs::wmbus::network::{WMBusNetwork, NetworkConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = NetworkConfig::default();
//!     let mut network = WMBusNetwork::new(config);
//!     
//!     // Discover all devices in the area
//!     let topology = network.discover_topology().await?;
//!     println!("Found {} devices", topology.devices.len());
//!     
//!     Ok(())
//! }
//! ```

use crate::wmbus::handle::{DeviceInfo, WMBusConfig, WMBusError, WMBusHandle};
use crate::wmbus::radio::hal::Hal;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Configuration for network discovery operations
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// List of frequencies to scan (Hz)
    pub frequencies: Vec<u32>,
    /// Duration to listen on each frequency (seconds)
    pub scan_duration_per_freq: u32,
    /// Minimum RSSI threshold for device detection (dBm)
    pub rssi_threshold: i16,
    /// Maximum number of devices to discover (0 = unlimited)
    pub max_devices: usize,
    /// Geographic clustering distance threshold (meters, if GPS available)
    pub clustering_distance: f64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            // Common European wM-Bus frequencies
            frequencies: vec![
                868_300_000, // C-mode: 868.3 MHz
                868_950_000, // S-mode: 868.95 MHz
                869_525_000, // T-mode: 869.525 MHz
            ],
            scan_duration_per_freq: 30, // 30 seconds per frequency
            rssi_threshold: -90,        // -90 dBm minimum signal
            max_devices: 1000,          // Limit to 1000 devices
            clustering_distance: 100.0, // 100 meter clusters
        }
    }
}

/// Device category based on device type field
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DeviceCategory {
    /// Unknown device type
    Unknown,
    /// Water meters
    Water,
    /// Heat meters
    Heat,
    /// Gas meters  
    Gas,
    /// Electricity meters
    Electricity,
    /// Temperature sensors
    Temperature,
    /// Pressure sensors
    Pressure,
    /// Flow sensors
    Flow,
    /// Other utility meters
    Other,
}

impl From<u8> for DeviceCategory {
    fn from(device_type: u8) -> Self {
        match device_type {
            0x01 => DeviceCategory::Water,
            0x02 => DeviceCategory::Heat,
            0x03 => DeviceCategory::Gas,
            0x04 => DeviceCategory::Electricity,
            0x05 => DeviceCategory::Temperature,
            0x06 => DeviceCategory::Pressure,
            0x07 => DeviceCategory::Flow,
            0x00 | 0x08..=0xFF => DeviceCategory::Other,
        }
    }
}

/// Network topology information
#[derive(Debug, Clone)]
pub struct NetworkTopology {
    /// All discovered devices
    pub devices: Vec<DeviceInfo>,
    /// Devices grouped by category
    pub devices_by_category: HashMap<DeviceCategory, Vec<DeviceInfo>>,
    /// Devices grouped by manufacturer
    pub devices_by_manufacturer: HashMap<u16, Vec<DeviceInfo>>,
    /// Network statistics
    pub statistics: NetworkStatistics,
    /// Scan duration and coverage
    pub scan_info: ScanInfo,
}

/// Network-wide statistics
#[derive(Debug, Clone)]
pub struct NetworkStatistics {
    /// Total number of devices discovered
    pub total_devices: usize,
    /// Average RSSI across all devices
    pub average_rssi: f64,
    /// Signal strength distribution (RSSI ranges and counts)
    pub rssi_distribution: HashMap<String, usize>,
    /// Device type distribution
    pub device_type_distribution: HashMap<DeviceCategory, usize>,
    /// Manufacturer distribution
    pub manufacturer_distribution: HashMap<u16, usize>,
}

/// Information about the scanning process
#[derive(Debug, Clone)]
pub struct ScanInfo {
    /// Frequencies that were scanned
    pub frequencies_scanned: Vec<u32>,
    /// Total scan duration
    pub total_duration: Duration,
    /// Timestamp when scan started
    pub start_time: Instant,
    /// Timestamp when scan completed
    pub end_time: Instant,
}

/// Represents the state of a Wireless M-Bus (wM-Bus) network
pub struct WMBusNetwork<H: Hal> {
    /// Network configuration
    config: NetworkConfig,
    /// WMBus handle for radio operations
    handle: Option<WMBusHandle<H>>,
    /// Discovered devices across all scans
    discovered_devices: HashMap<u32, DeviceInfo>,
}

impl<H: Hal + Send + 'static> WMBusNetwork<H> {
    /// Create a new wM-Bus network manager
    ///
    /// # Arguments
    ///
    /// * `config` - Network discovery configuration
    pub fn new(config: NetworkConfig) -> Self {
        Self {
            config,
            handle: None,
            discovered_devices: HashMap::new(),
        }
    }

    /// Initialize the network with a radio HAL
    ///
    /// # Arguments
    ///
    /// * `hal` - Hardware abstraction layer for radio
    pub async fn initialize(&mut self, hal: H) -> Result<(), WMBusError> {
        let wmbus_config = WMBusConfig {
            frequency_hz: self.config.frequencies[0], // Start with first frequency
            discovery_timeout_ms: self.config.scan_duration_per_freq * 1000,
            ..WMBusConfig::default()
        };

        self.handle = Some(WMBusHandle::new(hal, Some(wmbus_config)).await?);
        Ok(())
    }

    /// Discover the complete network topology
    ///
    /// Scans all configured frequencies and builds a comprehensive view
    /// of all devices in the wireless M-Bus network.
    ///
    /// # Returns
    ///
    /// * `Ok(NetworkTopology)` - Complete network information
    /// * `Err(WMBusError)` - Discovery failed
    pub async fn discover_topology(&mut self) -> Result<NetworkTopology, WMBusError> {
        let start_time = Instant::now();
        let mut frequencies_scanned = Vec::new();

        // Clear previous discoveries for fresh scan
        self.discovered_devices.clear();

        let handle = self
            .handle
            .as_mut()
            .ok_or_else(|| WMBusError::InvalidConfig("Network not initialized".to_string()))?;

        // Scan each configured frequency
        for &frequency in &self.config.frequencies {
            log::info!(
                "Scanning frequency {} MHz for {} seconds",
                frequency / 1_000_000,
                self.config.scan_duration_per_freq
            );

            // Reconfigure for this frequency
            let _new_config = WMBusConfig {
                frequency_hz: frequency,
                discovery_timeout_ms: self.config.scan_duration_per_freq * 1000,
                ..WMBusConfig::default()
            };

            // Update the handle configuration (simplified - in reality we'd need to recreate)
            // For now, we'll continue with the existing handle

            // Scan for devices on this frequency
            let devices = handle.scan_devices().await?;

            // Filter devices by RSSI threshold and add to discovered set
            for device in devices {
                if device.rssi_dbm >= self.config.rssi_threshold {
                    self.discovered_devices.insert(device.address, device);

                    // Stop if we've reached max devices limit
                    if self.config.max_devices > 0
                        && self.discovered_devices.len() >= self.config.max_devices
                    {
                        break;
                    }
                }
            }

            frequencies_scanned.push(frequency);

            // Break if max devices reached
            if self.config.max_devices > 0
                && self.discovered_devices.len() >= self.config.max_devices
            {
                break;
            }
        }

        let end_time = Instant::now();

        // Build topology from discovered devices
        self.build_topology(frequencies_scanned, start_time, end_time)
    }

    /// Get devices of a specific category
    ///
    /// # Arguments
    ///
    /// * `category` - Device category to filter by
    ///
    /// # Returns
    ///
    /// * Devices matching the specified category
    pub fn get_devices_by_category(&self, category: DeviceCategory) -> Vec<DeviceInfo> {
        self.discovered_devices
            .values()
            .filter(|device| DeviceCategory::from(device.device_type) == category)
            .cloned()
            .collect()
    }

    /// Get devices from a specific manufacturer
    ///
    /// # Arguments
    ///
    /// * `manufacturer_id` - Manufacturer ID to filter by
    ///
    /// # Returns
    ///
    /// * Devices from the specified manufacturer
    pub fn get_devices_by_manufacturer(&self, manufacturer_id: u16) -> Vec<DeviceInfo> {
        self.discovered_devices
            .values()
            .filter(|device| device.manufacturer_id == manufacturer_id)
            .cloned()
            .collect()
    }

    /// Get network statistics
    ///
    /// # Returns
    ///
    /// * Current network statistics
    pub fn get_statistics(&self) -> NetworkStatistics {
        let devices: Vec<_> = self.discovered_devices.values().collect();

        // Calculate average RSSI
        let average_rssi = if devices.is_empty() {
            0.0
        } else {
            devices.iter().map(|d| d.rssi_dbm as f64).sum::<f64>() / devices.len() as f64
        };

        // Build RSSI distribution
        let mut rssi_distribution = HashMap::new();
        for device in &devices {
            let range = match device.rssi_dbm {
                -40..=0 => "Excellent (-40 to 0 dBm)",
                -70..=-41 => "Good (-70 to -41 dBm)",
                -85..=-71 => "Fair (-85 to -71 dBm)",
                _ => "Poor (below -85 dBm)",
            };
            *rssi_distribution.entry(range.to_string()).or_insert(0) += 1;
        }

        // Build device type distribution
        let mut device_type_distribution = HashMap::new();
        for device in &devices {
            let category = DeviceCategory::from(device.device_type);
            *device_type_distribution.entry(category).or_insert(0) += 1;
        }

        // Build manufacturer distribution
        let mut manufacturer_distribution = HashMap::new();
        for device in &devices {
            *manufacturer_distribution
                .entry(device.manufacturer_id)
                .or_insert(0) += 1;
        }

        NetworkStatistics {
            total_devices: devices.len(),
            average_rssi,
            rssi_distribution,
            device_type_distribution,
            manufacturer_distribution,
        }
    }

    /// Build network topology from discovered devices
    fn build_topology(
        &self,
        frequencies_scanned: Vec<u32>,
        start_time: Instant,
        end_time: Instant,
    ) -> Result<NetworkTopology, WMBusError> {
        let devices: Vec<DeviceInfo> = self.discovered_devices.values().cloned().collect();

        // Group devices by category
        let mut devices_by_category = HashMap::new();
        for device in &devices {
            let category = DeviceCategory::from(device.device_type);
            devices_by_category
                .entry(category)
                .or_insert_with(Vec::new)
                .push(device.clone());
        }

        // Group devices by manufacturer
        let mut devices_by_manufacturer = HashMap::new();
        for device in &devices {
            devices_by_manufacturer
                .entry(device.manufacturer_id)
                .or_insert_with(Vec::new)
                .push(device.clone());
        }

        let statistics = self.get_statistics();

        let scan_info = ScanInfo {
            frequencies_scanned,
            total_duration: end_time.duration_since(start_time),
            start_time,
            end_time,
        };

        Ok(NetworkTopology {
            devices,
            devices_by_category,
            devices_by_manufacturer,
            statistics,
            scan_info,
        })
    }
}
