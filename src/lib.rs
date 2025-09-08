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
pub mod logging;
pub mod mbus;
pub mod mbus_device_manager;
pub mod payload;
pub mod wmbus;

pub use mbus::{MBusFrame, MBusFrameType};
pub use mbus::serial::MBusDeviceHandle;
pub use mbus_device_manager::MBusDeviceManager;
pub use payload::{mbus_data_record_decode, normalize_vib, MBusRecord, MBusRecordValue};

/// Connect to M-Bus device (stub).
pub async fn connect(_port: &str) -> Result<MBusDeviceHandle, MBusError> {
    Err(MBusError::Other("Not implemented".to_string()))
}

/// Disconnect from M-Bus device (stub).
pub async fn disconnect(_handle: &mut MBusDeviceHandle) -> Result<(), MBusError> {
    Err(MBusError::Other("Not implemented".to_string()))
}

/// Receive frame (stub).
pub async fn recv_frame(_handle: &mut MBusDeviceHandle) -> Result<MBusFrame, MBusError> {
    Err(MBusError::Other("Not implemented".to_string()))
}

/// Scan devices (stub).
pub async fn scan_devices(_handle: &mut MBusDeviceHandle) -> Result<Vec<String>, MBusError> {
    Err(MBusError::Other("Not implemented".to_string()))
}

/// Send frame (stub).
pub async fn send_frame(_handle: &mut MBusDeviceHandle, _frame: &MBusFrame) -> Result<(), MBusError> {
    Err(MBusError::Other("Not implemented".to_string()))
}

/// Send request (stub).
pub async fn send_request(_handle: &mut MBusDeviceHandle, _address: u8) -> Result<Vec<MBusRecord>, MBusError> {
    Err(MBusError::Other("Not implemented".to_string()))
}
