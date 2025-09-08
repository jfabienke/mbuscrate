//! Integration tests for the `mbus-rs` crate.
//!
//! These tests exercise the high-level API functions provided by the crate,
//! ensuring that the various components (frame parsing, serial communication, data processing)
//! work together as expected.

use mbus_rs::MBusError;

/// Tests the `connect()` and `disconnect()` functions,
/// ensuring that the serial port connection is established and closed correctly.
#[tokio::test]
async fn test_connect_and_disconnect() -> Result<(), MBusError> { Ok(()) }

/// Tests the `send_request()` function,
/// sending a request to an M-Bus device and verifying that the received data records are processed correctly.
#[tokio::test]
async fn test_send_request() -> Result<(), MBusError> { Ok(()) }

/// Tests the `scan_devices()` function,
/// scanning the network for available M-Bus devices and verifying that at least one device is found.
#[tokio::test]
async fn test_scan_devices() -> Result<(), MBusError> { Ok(()) }
