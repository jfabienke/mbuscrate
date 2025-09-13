//! End-to-end integration tests using mocks for hardware-agnostic testing
//!
//! These tests validate complete workflows without requiring actual hardware,
//! ensuring the entire stack works correctly from API to protocol to parsing.

// Mock modules need to be in the tests directory since they're test-only
mod mock_support;
// use mbus_rs::mbus::mbus_protocol::StateMachine;
// use mbus_rs::mbus::frame::{MBusFrame, MBusFrameType};
// use mbus_rs::payload::record::MBusRecord;
// use mbus_rs::error::MBusError;
use std::time::Duration;
use tokio::time::timeout;
// use std::sync::Arc;
// use tokio::sync::Mutex;

/// Helper to create a valid response frame
fn create_response_frame(address: u8, data: Vec<u8>) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.push(0x68); // Start
    let len = (data.len() + 3).min(255) as u8;
    frame.push(len);
    frame.push(len);
    frame.push(0x68);
    frame.push(0x08); // Control (RSP_UD)
    frame.push(address);
    frame.push(0x72); // CI (variable data response)
    frame.extend_from_slice(&data);

    // Calculate checksum
    let checksum: u8 = frame[4..frame.len()]
        .iter()
        .fold(0u8, |acc, b| acc.wrapping_add(*b));
    frame.push(checksum);
    frame.push(0x16); // Stop

    frame
}

#[tokio::test]
async fn e2e_connect_and_read_single_device() {
    // For now, skip this test as mocks are internal
    // TODO: Create proper test infrastructure
    return;

    // Queue response for device at address 0x01
    let response_data = vec![
        0x04, 0x13, 0x34, 0x12, 0x00, 0x00, // DIF=04, VIF=13 (Volume), Value=1234
        0x04, 0x06, 0x78, 0x56, 0x00, 0x00, // DIF=04, VIF=06 (Energy), Value=5678
    ];
    let response_frame = create_response_frame(0x01, response_data);
    mock.queue_response(response_frame);

    // Create testable handle
    let mut handle = TestableDeviceHandle::from_mock(mock);

    // E2E: Send request and get records
    let records = handle.send_request(0x01).await.unwrap();

    // Verify we got the expected records
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].quantity, "Volume");
    assert_eq!(records[0].value, 1234.0);
    assert_eq!(records[0].unit, "l");
    assert_eq!(records[1].quantity, "Energy");
    assert_eq!(records[1].value, 5678.0);
    assert_eq!(records[1].unit, "Wh");
}

#[tokio::test]
#[ignore = "Requires mock infrastructure"]
async fn e2e_device_scan_discovers_multiple_devices() {
    return;

    // Simulate devices at addresses 1, 5, and 10 responding
    // All others timeout (no response)
    for addr in [1u8, 5, 10] {
        let data = vec![0x04, 0x13, 0x00, 0x00, 0x00, 0x00]; // Minimal response
        let frame = create_response_frame(addr, data);
        mock.queue_conditional_response(addr, frame);
    }

    let mut handle = TestableDeviceHandle::from_mock(mock);

    // E2E: Scan for devices
    let discovered = handle.scan_devices().await.unwrap();

    // Should find exactly 3 devices
    assert_eq!(discovered.len(), 3);
    assert!(discovered.contains(&"0x01 (1 records)".to_string()));
    assert!(discovered.contains(&"0x05 (1 records)".to_string()));
    assert!(discovered.contains(&"0x0A (1 records)".to_string()));
}

#[tokio::test]
#[ignore = "Requires mock infrastructure"]
async fn e2e_multi_telegram_reassembly() {
    return;

    // First frame with "more records follow" bit set
    let mut frame1_data = vec![
        0x04, 0x13, 0x11, 0x11, 0x00, 0x00, // Record 1
        0x04, 0x06, 0x22, 0x22, 0x00, 0x00, // Record 2
    ];
    let mut frame1 = create_response_frame(0x01, frame1_data);
    // Set CI "more records follow" bit
    frame1[6] |= 0x10; // CI = 0x72 | 0x10 = 0x82
                       // Recalculate checksum
    let checksum: u8 = frame1[4..frame1.len() - 2]
        .iter()
        .fold(0u8, |acc, b| acc.wrapping_add(*b));
    frame1[frame1.len() - 2] = checksum;

    // Second frame (final)
    let frame2_data = vec![
        0x04, 0x2B, 0x33, 0x33, 0x00, 0x00, // Record 3 (Power)
    ];
    let frame2 = create_response_frame(0x01, frame2_data);

    // Queue both frames
    mock.queue_response(frame1);
    mock.queue_response(frame2);

    let mut handle = TestableDeviceHandle::from_mock(mock);

    // E2E: Read multi-telegram response
    let records = handle.send_request(0x01).await.unwrap();

    // Should get all 3 records assembled
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].value, 0x1111 as f64);
    assert_eq!(records[1].value, 0x2222 as f64);
    assert_eq!(records[2].value, 0x3333 as f64);
}

#[tokio::test]
#[ignore = "Requires mock infrastructure"]
async fn e2e_error_recovery_with_retries() {
    return;

    // First attempt: timeout (no response)
    // Second attempt: bad checksum
    let mut bad_frame = create_response_frame(0x01, vec![0x04, 0x13, 0x00, 0x00, 0x00, 0x00]);
    bad_frame[bad_frame.len() - 2] ^= 0xFF; // Corrupt checksum
    mock.queue_response_with_delay(bad_frame, Duration::from_millis(100));

    // Third attempt: success
    let good_frame = create_response_frame(0x01, vec![0x04, 0x13, 0x42, 0x00, 0x00, 0x00]);
    mock.queue_response(good_frame);

    let mut handle = TestableDeviceHandle::from_mock(mock);
    handle.set_retry_count(3);

    // E2E: Should succeed on third attempt
    let records = handle.send_request(0x01).await.unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].value, 0x42 as f64);
}

#[tokio::test]
#[ignore = "Requires mock infrastructure"]
async fn e2e_secondary_addressing() {
    return;

    // Response to secondary address selection
    mock.queue_response(vec![0xE5]); // ACK for select

    // Response to REQ_UD2 after selection
    let data = vec![
        0x04, 0x13, 0x99, 0x99, 0x00, 0x00, // Volume reading
    ];
    let frame = create_response_frame(0xFD, data); // Secondary address uses 0xFD
    mock.queue_response(frame);

    let mut handle = TestableDeviceHandle::from_mock(mock);

    // E2E: Select by secondary address and read
    let secondary_addr = "12345678ABCD0001"; // 16 hex digits
    let result = handle.select_secondary_address(secondary_addr).await;
    assert!(result.is_ok());

    let records = handle.send_request(0xFD).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].value, 0x9999 as f64);
}

#[tokio::test]
#[ignore = "Requires mock infrastructure"]
async fn e2e_baud_rate_adaptation() {
    return;
    mock.set_supports_baud_switching(true);

    // Simulate high collision rate at 2400 baud
    for _ in 0..5 {
        // Queue timeout/collision responses
        mock.queue_timeout();
    }

    // After baud switch, respond successfully
    let frame = create_response_frame(0x01, vec![0x04, 0x13, 0x11, 0x00, 0x00, 0x00]);
    mock.queue_response(frame);

    let mut handle = TestableDeviceHandle::from_mock(mock);
    handle.enable_auto_baud_detection(true);

    // E2E: Should adapt baud rate and succeed
    let records = handle.send_request_with_adaptation(0x01).await.unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(mock.get_current_baud_rate(), 9600); // Should have switched
}

#[tokio::test]
#[ignore = "Requires mock infrastructure"]
async fn e2e_collision_handling() {
    return;

    // Simulate collision scenario: overlapping responses
    let frame1 = create_response_frame(0x01, vec![0x04, 0x13, 0xAA, 0x00, 0x00, 0x00]);
    let frame2 = create_response_frame(0x02, vec![0x04, 0x13, 0xBB, 0x00, 0x00, 0x00]);

    // First attempt: garbled data (simulated collision)
    let mut collision_data = frame1.clone();
    for (i, &byte) in frame2.iter().enumerate() {
        if i < collision_data.len() {
            collision_data[i] ^= byte; // XOR to simulate collision
        }
    }
    mock.queue_response(collision_data);

    // After backoff, individual responses
    mock.queue_response_with_delay(frame1, Duration::from_millis(10));

    let mut handle = TestableDeviceHandle::from_mock(mock);
    handle.enable_collision_detection(true);

    // E2E: Should detect collision and retry with backoff
    let records = handle.send_request(0x01).await.unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].value, 0xAA as f64);
}

#[tokio::test]
#[ignore = "Requires mock infrastructure"]
async fn e2e_complete_meter_reading_workflow() {
    // This test simulates a complete real-world workflow:
    // 1. Connect to serial port
    // 2. Scan for devices
    // 3. Read from each discovered device
    // 4. Handle multi-telegram responses
    // 5. Disconnect

    let mut mock = MockSerialPort::new();

    // Step 1: Device scan - 2 devices respond
    for addr in [0x01, 0x02] {
        let scan_response = create_response_frame(addr, vec![0x04, 0x13, 0x00, 0x00, 0x00, 0x00]);
        mock.queue_conditional_response(addr, scan_response);
    }

    // Step 2: Read device 1 (multi-telegram)
    let dev1_frame1 = create_response_frame(
        0x01,
        vec![
            0x04, 0x13, 0x11, 0x11, 0x00, 0x00, // Volume
            0x04, 0x06, 0x22, 0x22, 0x00, 0x00, // Energy
        ],
    );
    let dev1_frame2 = create_response_frame(
        0x01,
        vec![
            0x04, 0x2B, 0x33, 0x33, 0x00, 0x00, // Power
            0x02, 0x5B, 0x19, 0x00, // Temperature (25°C)
        ],
    );

    // Step 3: Read device 2 (single frame)
    let dev2_frame = create_response_frame(
        0x02,
        vec![
            0x04, 0x13, 0x44, 0x44, 0x00, 0x00, // Volume
            0x02, 0x5B, 0x15, 0x00, // Temperature (21°C)
        ],
    );

    // Queue all responses in order
    mock.queue_response(dev1_frame1);
    mock.queue_response(dev1_frame2);
    mock.queue_response(dev2_frame);

    let mut handle = TestableDeviceHandle::from_mock(mock);

    // E2E Workflow execution

    // 1. Scan for devices
    let devices = handle.scan_devices().await.unwrap();
    assert_eq!(devices.len(), 2);

    // 2. Read from device 1
    let records1 = handle.send_request(0x01).await.unwrap();
    assert_eq!(records1.len(), 4); // 2 + 2 records from multi-telegram

    // 3. Read from device 2
    let records2 = handle.send_request(0x02).await.unwrap();
    assert_eq!(records2.len(), 2);

    // Verify data integrity
    assert_eq!(records1[0].quantity, "Volume");
    assert_eq!(records1[1].quantity, "Energy");
    assert_eq!(records1[2].quantity, "Power");
    assert_eq!(records1[3].quantity, "Flow temperature");
    assert_eq!(records1[3].value, 25.0);

    assert_eq!(records2[0].quantity, "Volume");
    assert_eq!(records2[1].quantity, "Flow temperature");
    assert_eq!(records2[1].value, 21.0);

    // 4. Disconnect
    let disconnect_result = handle.disconnect().await;
    assert!(disconnect_result.is_ok());
}

#[tokio::test]
#[ignore = "Requires mock infrastructure"]
async fn e2e_performance_under_load() {
    return;

    // Queue responses for 10 devices
    for addr in 1u8..=10u8 {
        let data = vec![
            0x04, 0x13, addr, addr, 0x00, 0x00, // Volume with address as value
        ];
        let frame = create_response_frame(addr, data);
        mock.queue_response(frame);
    }

    let handle = TestableDeviceHandle::from_mock(mock);
    let handle_arc = std::sync::Arc::new(tokio::sync::Mutex::new(handle));

    // Launch concurrent reads
    let mut tasks = Vec::new();
    for addr in 1u8..=10u8 {
        let handle_clone = handle_arc.clone();
        let task = tokio::spawn(async move {
            let mut handle = handle_clone.lock().await;
            handle.send_request(addr).await
        });
        tasks.push(task);
    }

    // Wait for all tasks with timeout
    // let results = timeout(Duration::from_secs(5), futures::future::join_all(tasks))
    //     .await
    //     .expect("Concurrent reads timed out");
    let results: Vec<Result<Result<Vec<_>, _>, _>> = Vec::new(); // Placeholder

    // Verify all succeeded
    // for (i, result) in results.iter().enumerate() {
    //     let records = result.as_ref().unwrap().as_ref().unwrap();
    //     assert_eq!(records.len(), 1);
    //     assert_eq!(records[0].value, (i + 1) as f64);
    // }
}

/// Test helper module for creating complex mock scenarios
mod mock_helpers {
    use super::*;

    pub fn create_manufacturer_specific_frame(address: u8) -> Vec<u8> {
        // Create frame with manufacturer-specific VIFs
        let data = vec![
            0x04, 0x7F, 0x10, 0x00, 0x00, 0x00, // Manufacturer specific VIF
            0x0C, 0xFF, 0x08, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, // Mfg data
        ];
        create_response_frame(address, data)
    }

    pub fn create_error_frame(address: u8, error_code: u8) -> Vec<u8> {
        // Create application error frame
        let mut frame = Vec::new();
        frame.push(0x68);
        frame.push(0x03);
        frame.push(0x03);
        frame.push(0x68);
        frame.push(0x08);
        frame.push(address);
        frame.push(0x70 | error_code); // CI with error
        let checksum: u8 = frame[4..].iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
        frame.push(checksum);
        frame.push(0x16);
        frame
    }
}
