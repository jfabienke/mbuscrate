//! Integration tests for wM-Bus enhancements
//!
//! Tests the following improvements:
//! 1. Encryption detection in frame parsing
//! 2. Multi-block CRC validation
//! 3. Vendor CRC tolerance
//! 4. Per-device error statistics

use mbus_rs::wmbus::frame::{parse_wmbus_frame, is_encrypted_frame};
use mbus_rs::wmbus::block::{verify_blocks, calculate_block_crc, BLOCK_DATA_SIZE};
use mbus_rs::vendors::{VendorExtension, VendorRegistry, CrcErrorType, CrcErrorContext};
use mbus_rs::instrumentation::stats::{ErrorType, update_device_error, get_device_stats, clear_all_stats};
use mbus_rs::MBusError;
use std::sync::Arc;

#[test]
fn test_encryption_detection() {
    // Test frame with encrypted CI (0x7A)
    let mut frame_bytes = vec![
        0x44, // L-field
        0x44, // C-field (no ACC bit)
        0x2D, 0x2C, // Manufacturer ID (KAM)
        0x78, 0x56, 0x34, 0x12, // Device address
        0x01, // Version
        0x07, // Device type (Water)
        0x7A, // CI field (encrypted short format)
    ];

    // Add dummy payload and CRC
    frame_bytes.extend_from_slice(&[0; 50]);
    frame_bytes.push(0x00); // CRC low
    frame_bytes.push(0x00); // CRC high

    // Parse should detect encryption and skip CRC validation
    let result = parse_wmbus_frame(&frame_bytes);
    assert!(result.is_ok());
    let frame = result.unwrap();
    assert!(frame.encrypted);

    // Test with ACC bit set
    assert!(is_encrypted_frame(0x80, 0x72)); // ACC bit set
    assert!(!is_encrypted_frame(0x00, 0x72)); // ACC bit not set
}

#[test]
fn test_multi_block_validation() {
    // Create a 2-block payload
    let mut payload = Vec::new();

    // Block 1
    let block1_data = vec![0x01; BLOCK_DATA_SIZE];
    let block1_crc = calculate_block_crc(&block1_data);
    payload.extend_from_slice(&block1_data);
    payload.push((block1_crc & 0xFF) as u8);
    payload.push((block1_crc >> 8) as u8);

    // Block 2
    let block2_data = vec![0x02; BLOCK_DATA_SIZE];
    let block2_crc = calculate_block_crc(&block2_data);
    payload.extend_from_slice(&block2_data);
    payload.push((block2_crc & 0xFF) as u8);
    payload.push((block2_crc >> 8) as u8);

    // Verify blocks
    let blocks = verify_blocks(&payload, false).unwrap();
    assert_eq!(blocks.len(), 2);
    assert!(blocks[0].crc_valid);
    assert!(blocks[1].crc_valid);

    // Test with corrupted CRC
    payload[15] = 0xFF; // Corrupt block 1 CRC
    let blocks = verify_blocks(&payload, false).unwrap();
    assert!(!blocks[0].crc_valid);
    assert!(blocks[1].crc_valid);
}

#[test]
fn test_vendor_crc_tolerance() {
    // Mock vendor extension that tolerates block 3 errors
    struct TestVendorExtension;

    impl VendorExtension for TestVendorExtension {
        fn tolerate_crc_failure(
            &self,
            manufacturer_id: &str,
            _device_info: Option<&mbus_rs::vendors::VendorDeviceInfo>,
            error_type: &CrcErrorType,
            error_context: &CrcErrorContext,
        ) -> Result<Option<bool>, MBusError> {
            // Tolerate block 3 (index 2) CRC errors for "TEST" manufacturer
            if manufacturer_id == "TEST"
                && matches!(error_type, CrcErrorType::Block)
                && error_context.block_index == Some(2) {
                Ok(Some(true)) // Tolerate this error
            } else {
                Ok(None) // Use default validation
            }
        }
    }

    let registry = VendorRegistry::new();
    registry.register("TEST", Arc::new(TestVendorExtension)).unwrap();

    // Create error context for block 3
    let context = CrcErrorContext {
        block_index: Some(2),
        total_blocks: Some(5),
        crc_expected: 0x1234,
        crc_received: 0x5678,
        frame_type: Some("TypeA".to_string()),
        vendor_context: Default::default(),
    };

    // Should tolerate block 3 error for TEST manufacturer
    let result = mbus_rs::vendors::dispatch_crc_tolerance(
        &registry,
        "TEST",
        None,
        &CrcErrorType::Block,
        &context,
    ).unwrap();
    assert_eq!(result, Some(true));

    // Should not tolerate block 2 error
    let context2 = CrcErrorContext {
        block_index: Some(1),
        ..context
    };
    let result = mbus_rs::vendors::dispatch_crc_tolerance(
        &registry,
        "TEST",
        None,
        &CrcErrorType::Block,
        &context2,
    ).unwrap();
    assert_eq!(result, None);
}

#[test]
fn test_per_device_statistics() {
    clear_all_stats(); // Start fresh

    let device_id = "TESTDEV1";

    // Track some errors
    update_device_error(device_id, ErrorType::Crc);
    update_device_error(device_id, ErrorType::Crc);
    update_device_error(device_id, ErrorType::BlockCrc);

    // Get statistics
    let stats = get_device_stats(device_id);
    let stats = stats.lock().unwrap();

    assert_eq!(stats.get_error_count(ErrorType::Crc), 2);
    assert_eq!(stats.get_error_count(ErrorType::BlockCrc), 1);
    assert_eq!(stats.device_id, device_id);
}

#[test]
fn test_frame_with_stats_integration() {
    clear_all_stats();

    // Create a frame with bad CRC (will be tracked)
    let frame_bytes = vec![
        0x44, // L-field
        0x44, // C-field
        0x2D, 0x2C, // Manufacturer ID
        0x99, 0x88, 0x77, 0x66, // Device address
        0x01, // Version
        0x07, // Device type
        0x72, // CI field
        // Payload
        0x01, 0x02, 0x03, 0x04,
        0xFF, 0xFF, // Bad CRC
    ];

    // Parse should fail and track error
    let result = parse_wmbus_frame(&frame_bytes);
    assert!(result.is_err());

    // Check that error was tracked
    let device_id = "66778899"; // Device address as hex string
    let stats = get_device_stats(device_id);
    let stats = stats.lock().unwrap();
    assert_eq!(stats.get_error_count(ErrorType::Crc), 1);
}

#[test]
fn test_encrypted_frame_no_crc_check() {
    // Frame with encryption CI should not validate CRC
    let mut frame_bytes = vec![
        0x44, // L-field
        0x44, // C-field
        0x2D, 0x2C, // Manufacturer ID
        0xAA, 0xBB, 0xCC, 0xDD, // Device address
        0x01, // Version
        0x07, // Device type
        0x7B, // CI field (encrypted long format)
    ];

    // Add payload with intentionally bad CRC
    frame_bytes.extend_from_slice(&[0; 50]);
    frame_bytes.push(0xFF); // Bad CRC
    frame_bytes.push(0xFF);

    // Should parse successfully despite bad CRC (encrypted frames skip CRC)
    let result = parse_wmbus_frame(&frame_bytes);
    assert!(result.is_ok());
    let frame = result.unwrap();
    assert!(frame.encrypted);
    assert_eq!(frame.device_address, 0xDDCCBBAA);
}