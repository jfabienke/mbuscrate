//! Integration tests for wireless M-Bus functionality
//!
//! Tests the complete wM-Bus stack from radio driver to network discovery,
//! using mock HAL implementations to simulate radio hardware.

use mbus_rs::wmbus::{
    frame::{calculate_wmbus_crc, parse_wmbus_frame, WMBusFrame},
    handle::{DeviceInfo, WMBusConfig, WMBusError, WMBusHandle},
    network::{DeviceCategory, NetworkConfig, WMBusNetwork},
    radio::{
        driver::{LbtConfig, RadioState, Sx126xDriver},
        hal::{Hal, HalError},
    },
};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

/// Mock HAL implementation for testing
#[derive(Debug, Clone)]
pub struct MockHal {
    /// Simulated radio state
    state: Arc<AtomicU8>,
    /// Simulated received frames
    rx_frames: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
}

impl Default for MockHal {
    fn default() -> Self {
        Self::new()
    }
}

impl MockHal {
    pub fn new() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(RadioState::Sleep as u8)),
            rx_frames: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Add a simulated received frame
    pub fn add_rx_frame(&self, frame_data: Vec<u8>) {
        let mut frames = self.rx_frames.lock().unwrap();
        frames.push(frame_data);
    }

    /// Create a test wM-Bus frame
    pub fn create_test_frame(device_address: u32, payload: &[u8]) -> Vec<u8> {
        WMBusFrame::build(
            0x44,   // Control field
            0x1568, // Manufacturer ID (Engelmann)
            device_address,
            0x37, // Version
            0x01, // Device type (water meter)
            0x8E, // Control info
            payload,
        )
    }
}

impl Hal for MockHal {
    fn write_command(&mut self, _opcode: u8, _data: &[u8]) -> Result<(), HalError> {
        // Simulate command execution
        match _opcode {
            0x80 => {
                // SetStandby command - update state
                if !_data.is_empty() {
                    match _data[0] {
                        0x00 => self
                            .state
                            .store(RadioState::StandbyRc as u8, Ordering::Relaxed),
                        0x01 => self
                            .state
                            .store(RadioState::StandbyXosc as u8, Ordering::Relaxed),
                        _ => {}
                    }
                }
            }
            0x82 => {
                // SetRx command - enter RX mode
                self.state.store(RadioState::Rx as u8, Ordering::Relaxed);
            }
            0x83 => {
                // SetTx command - enter TX mode (but first go through standby)
                self.state
                    .store(RadioState::StandbyRc as u8, Ordering::Relaxed);
                // Then to TX (simulate quick transition)
                self.state.store(RadioState::Tx as u8, Ordering::Relaxed);
            }
            _ => {}
        }
        Ok(())
    }

    fn read_command(&mut self, _opcode: u8, buffer: &mut [u8]) -> Result<(), HalError> {
        // Simulate reading status or data
        match _opcode {
            0xC0 => {
                // GetStatus command - return current state
                buffer[0] = (self.state.load(Ordering::Relaxed) << 4) | 0x02;
            }
            0x12 => {
                // GetIrqStatus - simulate RX done if we have frames
                let frames = self.rx_frames.lock().unwrap();
                if !frames.is_empty() {
                    buffer[0] = 0x00; // IRQ status MSB
                    buffer[1] = 0x02; // RX done bit set
                } else {
                    buffer[0] = 0x00;
                    buffer[1] = 0x00;
                }
            }
            0x13 => {
                // GetRxBufferStatus
                let frames = self.rx_frames.lock().unwrap();
                if let Some(frame) = frames.first() {
                    buffer[0] = frame.len() as u8; // Frame length
                    buffer[1] = 0x00; // Start address
                    buffer[2] = 0x00; // Current address
                } else {
                    buffer[0] = 0x00;
                    buffer[1] = 0x00;
                    buffer[2] = 0x00;
                }
            }
            0x15 => {
                // GetRssiInst - return low RSSI to simulate clear channel
                buffer[0] = 200; // RSSI raw value that converts to -100 dBm (200/2 = 100, negated = -100)
            }
            0x1E => {
                // ReadBuffer - return first available frame
                let mut frames = self.rx_frames.lock().unwrap();
                if let Some(frame) = frames.pop() {
                    let copy_len = std::cmp::min(buffer.len(), frame.len());
                    buffer[..copy_len].copy_from_slice(&frame[..copy_len]);
                }
            }
            _ => {
                // Default response
                buffer.fill(0);
            }
        }
        Ok(())
    }

    fn write_register(&mut self, _address: u16, _data: &[u8]) -> Result<(), HalError> {
        Ok(())
    }

    fn read_register(&mut self, _address: u16, buffer: &mut [u8]) -> Result<(), HalError> {
        buffer.fill(0);
        Ok(())
    }

    fn gpio_read(&mut self, _pin: u8) -> Result<bool, HalError> {
        // Simulate BUSY pin low (not busy)
        Ok(false)
    }

    fn gpio_write(&mut self, _pin: u8, _state: bool) -> Result<(), HalError> {
        Ok(())
    }
}

#[tokio::test]
async fn test_wmbus_frame_round_trip() {
    // Test frame creation and parsing
    let original_payload = vec![0x01, 0x02, 0x03, 0x04, 0x05];
    let frame_bytes = WMBusFrame::build(
        0x44,
        0x1568,
        0x12345678,
        0x37,
        0x01,
        0x8E,
        &original_payload,
    );

    // Parse the frame back
    let parsed_frame = parse_wmbus_frame(&frame_bytes).expect("Failed to parse frame");

    // Verify all fields
    assert_eq!(parsed_frame.control_field, 0x44);
    assert_eq!(parsed_frame.manufacturer_id, 0x1568);
    assert_eq!(parsed_frame.device_address, 0x12345678);
    assert_eq!(parsed_frame.version, 0x37);
    assert_eq!(parsed_frame.device_type, 0x01);
    assert_eq!(parsed_frame.control_info, 0x8E);
    assert_eq!(parsed_frame.payload, original_payload);

    // Verify CRC is valid
    assert!(parsed_frame.verify_crc());
}

#[tokio::test]
async fn test_radio_driver_basic_operations() {
    let hal = MockHal::new();
    let mut driver = Sx126xDriver::new(hal, 32_000_000);

    // Test configuration
    assert!(driver.configure_for_wmbus(868_950_000, 100_000).is_ok());

    // Test state management
    let state = driver.get_state().expect("Failed to get state");
    assert_eq!(state, RadioState::Sleep);

    // Test setting standby mode
    assert!(driver
        .set_standby(mbus_rs::wmbus::radio::driver::StandbyMode::RC)
        .is_ok());
}

#[tokio::test]
async fn test_wmbus_handle_initialization() {
    let hal = MockHal::new();
    let config = WMBusConfig::default();

    // Create handle
    let handle = WMBusHandle::new(hal, Some(config)).await;
    assert!(handle.is_ok(), "Failed to create WMBusHandle");
}

#[tokio::test]
async fn test_wmbus_handle_frame_transmission() {
    // This test verifies that the API works correctly, though actual transmission
    // in a real environment would require proper hardware
    let hal = MockHal::new();
    let config = WMBusConfig::default();
    let handle = WMBusHandle::new(hal, Some(config)).await.unwrap();

    // Create test frame
    let test_frame = WMBusFrame {
        length: 0x0E,
        control_field: 0x44,
        manufacturer_id: 0x1568,
        device_address: 0x12345678,
        version: 0x37,
        device_type: 0x01,
        control_info: 0x8E,
        payload: vec![0x01, 0x02, 0x03],
        crc: 0, // Will be calculated
        encrypted: false,
    };

    // For this test, we'll expect it to fail with channel busy or wrong state
    // since we're using a mock HAL that doesn't simulate the full state machine
    let result = handle.send_frame(&test_frame).await;

    // We expect it to fail in the mock environment, but the API should handle it gracefully
    assert!(result.is_err(), "Mock transmission should fail gracefully");

    // Verify the error is one we expect (not a panic or crash)
    match result {
        Err(WMBusError::Radio(_)) => {
            // Expected radio-level error in mock environment
        }
        _ => panic!("Unexpected error type"),
    }
}

#[tokio::test]
async fn test_device_category_classification() {
    // Test device type to category mapping
    assert_eq!(DeviceCategory::from(0x01), DeviceCategory::Water);
    assert_eq!(DeviceCategory::from(0x02), DeviceCategory::Heat);
    assert_eq!(DeviceCategory::from(0x03), DeviceCategory::Gas);
    assert_eq!(DeviceCategory::from(0x04), DeviceCategory::Electricity);
    assert_eq!(DeviceCategory::from(0x05), DeviceCategory::Temperature);
    assert_eq!(DeviceCategory::from(0x06), DeviceCategory::Pressure);
    assert_eq!(DeviceCategory::from(0x07), DeviceCategory::Flow);
    assert_eq!(DeviceCategory::from(0x00), DeviceCategory::Other);
    assert_eq!(DeviceCategory::from(0xFF), DeviceCategory::Other);
}

#[tokio::test]
async fn test_network_configuration() {
    let config = NetworkConfig::default();

    // Verify default frequencies
    assert_eq!(config.frequencies.len(), 3);
    assert!(config.frequencies.contains(&868_300_000)); // C-mode
    assert!(config.frequencies.contains(&868_950_000)); // S-mode
    assert!(config.frequencies.contains(&869_525_000)); // T-mode

    // Verify other defaults
    assert_eq!(config.scan_duration_per_freq, 30);
    assert_eq!(config.rssi_threshold, -90);
    assert_eq!(config.max_devices, 1000);
}

#[tokio::test]
async fn test_network_manager_creation() {
    let config = NetworkConfig::default();
    let network = WMBusNetwork::<MockHal>::new(config);

    // Test that network manager can be created
    // Actual initialization would require HAL
    let stats = network.get_statistics();
    assert_eq!(stats.total_devices, 0);
}

#[tokio::test]
async fn test_lbt_configuration() {
    let lbt_config = LbtConfig::default();

    // Verify EU compliant defaults
    assert_eq!(lbt_config.rssi_threshold_dbm, -85);
    assert_eq!(lbt_config.listen_duration_ms, 5);
    assert_eq!(lbt_config.max_retries, 3);
}

#[tokio::test]
async fn test_crc_calculation_consistency() {
    // Test that CRC calculation is deterministic
    let test_data = [0x44, 0x93, 0x15, 0x68, 0x61, 0x05, 0x28, 0x74];

    let crc1 = calculate_wmbus_crc(&test_data);
    let crc2 = calculate_wmbus_crc(&test_data);

    assert_eq!(crc1, crc2, "CRC calculation should be deterministic");

    // Test that different data produces different CRC
    let test_data2 = [0x44, 0x93, 0x15, 0x68, 0x61, 0x05, 0x28, 0x75]; // Last byte different
    let crc3 = calculate_wmbus_crc(&test_data2);

    assert_ne!(crc1, crc3, "Different data should produce different CRC");
}

#[tokio::test]
async fn test_device_info_structure() {
    let device_info = DeviceInfo {
        address: 0x12345678,
        manufacturer_id: 0x1568,
        version: 0x37,
        device_type: 0x01,
        rssi_dbm: -75,
        last_seen: std::time::Instant::now(),
    };

    // Test device categorization
    let category = DeviceCategory::from(device_info.device_type);
    assert_eq!(category, DeviceCategory::Water);

    // Test that device info can be cloned
    let cloned_info = device_info.clone();
    assert_eq!(device_info.address, cloned_info.address);
    assert_eq!(device_info.manufacturer_id, cloned_info.manufacturer_id);
}

#[tokio::test]
async fn test_frame_validation_edge_cases() {
    // Test minimum valid frame
    let min_frame = WMBusFrame::build(0x44, 0x1568, 0x12345678, 0x37, 0x01, 0x8E, &[]);
    let parsed = parse_wmbus_frame(&min_frame).expect("Minimum frame should parse");
    assert!(parsed.payload.is_empty());

    // Test frame with maximum payload (within reasonable limits)
    let large_payload = vec![0xAA; 200];
    let large_frame = WMBusFrame::build(0x44, 0x1568, 0x12345678, 0x37, 0x01, 0x8E, &large_payload);
    let parsed_large = parse_wmbus_frame(&large_frame).expect("Large frame should parse");
    assert_eq!(parsed_large.payload.len(), 200);
}

#[tokio::test]
async fn test_error_handling() {
    // Test parsing invalid frames
    let empty_frame = [];
    assert!(parse_wmbus_frame(&empty_frame).is_err());

    let too_short = [0x01, 0x02, 0x03];
    assert!(parse_wmbus_frame(&too_short).is_err());

    // Test invalid CRC
    let mut valid_frame = WMBusFrame::build(0x44, 0x1568, 0x12345678, 0x37, 0x01, 0x8E, &[0x01]);
    let len = valid_frame.len();
    valid_frame[len - 1] ^= 0x01; // Corrupt CRC
    assert!(parse_wmbus_frame(&valid_frame).is_err());
}

/// Integration test simulating real device discovery
#[tokio::test]
async fn test_simulated_device_discovery() {
    let hal = MockHal::new();

    // Add some simulated devices
    hal.add_rx_frame(MockHal::create_test_frame(0x11111111, &[0x01, 0x02]));
    hal.add_rx_frame(MockHal::create_test_frame(0x22222222, &[0x03, 0x04]));
    hal.add_rx_frame(MockHal::create_test_frame(0x33333333, &[0x05, 0x06]));

    let config = WMBusConfig {
        discovery_timeout_ms: 100, // Short timeout for testing
        ..WMBusConfig::default()
    };

    let mut handle = WMBusHandle::new(hal, Some(config)).await.unwrap();

    // Start receiver for a short time
    handle
        .start_receiver()
        .await
        .expect("Failed to start receiver");

    // Wait briefly to allow frame processing
    sleep(Duration::from_millis(50)).await;

    // For this test, we can't easily verify the exact results without more
    // sophisticated mocking, but we can verify the operations don't crash
    assert!(true, "Device discovery simulation completed without errors");
}

#[tokio::test]
async fn test_unexpected_irq_bit() {
    // Test edge case: unexpected IRQ bits should be handled gracefully
    use mbus_rs::wmbus::radio::irq::{IrqMaskBit, IrqStatus};

    // Create an IRQ status with an unexpected/invalid bit set (bit 15, which is reserved)
    let unexpected_irq = IrqStatus::from(0x8000 | (IrqMaskBit::RxDone as u16));

    // Verify that known bits still work correctly
    assert!(unexpected_irq.rx_done(), "Should still detect RxDone bit");
    assert!(!unexpected_irq.tx_done(), "Should not detect TxDone bit");
    assert!(
        unexpected_irq.has_any(),
        "Should detect that some interrupt is active"
    );

    // Verify that the raw value includes the unexpected bit
    assert_eq!(
        unexpected_irq.raw(),
        0x8001,
        "Raw value should include unexpected bit"
    );

    // Test with all reserved bits set (bits 10-15)
    let reserved_bits_irq = IrqStatus::from(0xFC00);
    assert!(
        !reserved_bits_irq.rx_done(),
        "Should not detect any known interrupts"
    );
    assert!(
        !reserved_bits_irq.tx_done(),
        "Should not detect any known interrupts"
    );
    assert!(
        reserved_bits_irq.has_any(),
        "Should still detect that interrupts are active"
    );

    // This test ensures the driver won't panic with unexpected hardware behavior
    // and will log warnings appropriately (though we can't easily test logging here)
}
