//! # mbus-rs - A Rust Crate for M-Bus (Meter-Bus) Protocol Communication
//!
//! The mbus-rs crate provides a Rust-based implementation of the M-Bus (Meter-Bus) protocol,
//! which is a European standard for data exchange with utility meters, such as electricity, gas, water, and heat meters.
//!
//! ## Features
//!
//! - Connect to M-Bus devices using a serial port connection
//! - Send requests to M-Bus devices and receive their responses
//! - Scan the network for available M-Bus devices
//! - Parse and process M-Bus frames, including fixed-length and variable-length data records
//! - Normalize the data values using the Value Information Field (VIF) and Value Information Block (VIB) information
//! - Provide a high-level API for interacting with M-Bus devices
//! - Support for logging and error handling
//!
//! ## Usage
//!
//! To use the mbus-rs crate in your Rust project, add the following to your Cargo.toml file:
//!
//! ```toml
//! [dependencies]
//! mbus-rs = "0.1.0"
//! ```
//!
//! Then, in your Rust code, you can import the necessary modules and functions:
//!
//! ```rust
//! use mbus_rs::{
//!     connect, disconnect, send_request, scan_devices,
//!     MBusRecord, MBusRecordValue, MBusError, init_logger, log_info,
//!     MBusFrame, MBusFrameType,
//! };
//! ```

pub mod constants;
pub mod error;
pub mod instrumentation;
pub mod logging;
pub mod mbus;
pub mod mbus_device_manager;
pub mod payload;
pub mod util;
pub mod vendors;
pub mod wmbus;

pub use crate::error::MBusError;
pub use crate::logging::{init_logger, log_info};

// Core M-Bus types
pub use mbus::serial::MBusDeviceHandle;
pub use mbus::{MBusFrame, MBusFrameType};
pub use mbus_device_manager::MBusDeviceManager;
pub use payload::{mbus_data_record_decode, normalize_vib, MBusRecord, MBusRecordValue};

// Vendor extension system
pub use vendors::{
    VendorExtension, VendorRegistry, VendorDataRecord, VendorVariable, VendorDeviceInfo,
    manufacturer_id_to_string, parse_manufacturer_id,
};

// Vendor-specific extensions
pub use vendors::qundis_hca::QundisHcaExtension;

// Unified instrumentation model
pub use instrumentation::{
    UnifiedInstrumentation, DeviceType, ProtocolType, RadioMetrics, BatteryStatus,
    DeviceStatus, FrameStatistics, Reading, ReadingQuality, InstrumentationSource,
};

// Instrumentation converters
pub use instrumentation::converters::{
    from_mbus_frame, from_wmbus_frame, /* from_lora_metering_data, */ from_vendor_device_info,
};

/// Connect to M-Bus device via serial port.
///
/// # Arguments
/// * `port` - Serial port path (e.g., "/dev/ttyUSB0" on Linux, "COM3" on Windows)
///
/// # Returns
/// * `Ok(MBusDeviceHandle)` - Connected device handle for communication
/// * `Err(MBusError)` - Connection failed
pub async fn connect(port: &str) -> Result<MBusDeviceHandle, MBusError> {
    MBusDeviceHandle::connect(port).await
}

/// Disconnect from M-Bus device.
///
/// # Arguments
/// * `handle` - Device handle to disconnect
///
/// # Returns
/// * `Ok(())` - Successfully disconnected
/// * `Err(MBusError)` - Disconnection failed
pub async fn disconnect(handle: &mut MBusDeviceHandle) -> Result<(), MBusError> {
    handle.disconnect().await
}

/// Receive a frame from the M-Bus device.
///
/// # Arguments
/// * `handle` - Device handle to receive from
///
/// # Returns
/// * `Ok(MBusFrame)` - Received and parsed frame
/// * `Err(MBusError)` - Reception or parsing failed
pub async fn recv_frame(handle: &mut MBusDeviceHandle) -> Result<MBusFrame, MBusError> {
    handle.recv_frame().await
}

/// Scan for available M-Bus devices on the network.
///
/// # Arguments
/// * `handle` - Device handle to use for scanning
///
/// # Returns
/// * `Ok(Vec<String>)` - List of discovered device addresses
/// * `Err(MBusError)` - Scanning failed
pub async fn scan_devices(handle: &mut MBusDeviceHandle) -> Result<Vec<String>, MBusError> {
    handle.scan_devices().await
}

/// Send a frame to the M-Bus device.
///
/// # Arguments
/// * `handle` - Device handle to send through
/// * `frame` - Frame to send
///
/// # Returns
/// * `Ok(())` - Frame sent successfully
/// * `Err(MBusError)` - Send failed
pub async fn send_frame(handle: &mut MBusDeviceHandle, frame: &MBusFrame) -> Result<(), MBusError> {
    handle.send_frame(frame).await
}

/// Send a data request to a specific M-Bus device and retrieve records.
///
/// # Arguments
/// * `handle` - Device handle to communicate through
/// * `address` - Target device address (1-250)
///
/// # Returns
/// * `Ok(Vec<MBusRecord>)` - Parsed data records from the device
/// * `Err(MBusError)` - Request failed
pub async fn send_request(
    handle: &mut MBusDeviceHandle,
    address: u8,
) -> Result<Vec<MBusRecord>, MBusError> {
    handle.send_request(address).await
}

#[cfg(feature = "rtt-logging")]
pub mod defmt_timestamp;
