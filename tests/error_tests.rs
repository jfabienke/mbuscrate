//! Unit tests for the `MBusError` enum and its associated `Display` trait implementation.

use mbus_rs::error::MBusError;

/// Tests that the `SerialPortError` variant is correctly formatted.
#[test]
fn test_serial_port_error() {
    let err = MBusError::SerialPortError("Test error".to_string());
    assert_eq!(err.to_string(), "Serial port error: Test error");
}

/// Tests that the `FrameParseError` variant is correctly formatted.
#[test]
fn test_frame_parse_error() {
    let err = MBusError::FrameParseError("bad".to_string());
    assert_eq!(err.to_string(), "Error parsing M-Bus frame: bad");
}

/// Tests that the `UnknownVif` variant is correctly formatted.
#[test]
fn test_unknown_vif_error() {
    let err = MBusError::UnknownVif(0x12);
    assert_eq!(err.to_string(), "Unknown VIF: 0x12");
}

/// Tests that the `UnknownVife` variant is correctly formatted.
#[test]
fn test_unknown_vife_error() {
    let err = MBusError::UnknownVife(0x34);
    assert_eq!(err.to_string(), "Unknown VIFE: 0x34");
}

/// Tests that the `InvalidHexString` variant is correctly formatted.
#[test]
fn test_invalid_hex_string_error() {
    let err = MBusError::InvalidHexString;
    assert_eq!(err.to_string(), "Invalid hexadecimal string");
}

/// Tests that the `Other` variant is correctly formatted.
#[test]
fn test_other_error() {
    let err = MBusError::Other("Test error message".to_string());
    assert_eq!(err.to_string(), "Other error: Test error message");
}
