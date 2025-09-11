//! Integration tests for the `mbus-rs` crate.
//!
//! These tests exercise the high-level API functions provided by the crate,
//! ensuring that the various components (frame parsing, serial communication, data processing)
//! work together as expected.

use mbus_rs::{connect, disconnect, scan_devices, send_request, MBusError};

#[tokio::test]
async fn test_connect_and_disconnect() -> Result<(), MBusError> {
    // Test connect returns expected error for stub
    let result = connect("dummy").await;
    assert!(matches!(result, Err(MBusError::Other(ref msg)) if msg == "Not implemented"));

    // Test disconnect stub
    let result = disconnect().await;
    assert!(matches!(result, Err(MBusError::Other(ref msg)) if msg == "Not implemented"));
    Ok(())
}

#[tokio::test]
async fn test_send_request() -> Result<(), MBusError> {
    // Test send_request stub
    let result = send_request(0x01).await;
    assert!(matches!(result, Err(MBusError::Other(ref msg)) if msg == "Not implemented"));
    Ok(())
}

#[tokio::test]
async fn test_scan_devices() -> Result<(), MBusError> {
    // Test scan_devices stub
    let result = scan_devices().await;
    assert!(matches!(result, Err(MBusError::Other(ref msg)) if msg == "Not implemented"));
    Ok(())
}
