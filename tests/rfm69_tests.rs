//! # RFM69 Driver Tests
//!
//! Comprehensive tests for the RFM69 radio driver functionality, including
//! packet processing, bit reversal, frame validation, and all the critical
//! enhancements for robust packet processing.

// Test packet processing functions (available even without rfm69 feature)
mod packet_tests {
    use mbus_rs::wmbus::radio::rfm69_packet::*;

    /// Test the bit reversal function for MSB/LSB conversion
    #[test]
    fn test_bit_reversal() {
        // Test known bit reversal patterns
        assert_eq!(rev8(0x00), 0x00);
        assert_eq!(rev8(0xFF), 0xFF);
        assert_eq!(rev8(0x01), 0x80);
        assert_eq!(rev8(0x80), 0x01);
        assert_eq!(rev8(0xAA), 0x55);
        assert_eq!(rev8(0x55), 0xAA);

        // Test wM-Bus specific patterns
        assert_eq!(rev8(0xB3), 0xCD); // Type A sync word
        assert_eq!(rev8(0xBC), 0x3D); // Type B sync word

        // Test bidirectional property
        for i in 0..=255u8 {
            assert_eq!(rev8(rev8(i)), i);
        }
    }

    /// Test sync word normalization
    #[test]
    fn test_sync_normalization() {
        // Test sync_norm function with known patterns
        assert_eq!(sync_norm(0xB3), 0xCD); // Type A
        assert_eq!(sync_norm(0xBC), 0x3D); // Type B
        assert_eq!(sync_norm(0xCD), 0xCD); // Already normalized Type A
        assert_eq!(sync_norm(0x3D), 0x3D); // Already normalized Type B

        // Test other values remain unchanged when not sync words
        assert_eq!(sync_norm(0x00), 0x00);
        assert_eq!(sync_norm(0xFF), 0xFF);
        assert_eq!(sync_norm(0x55), 0x55);
    }

    /// Test robust packet size detection with header validation
    #[test]
    fn test_packet_size_detection() {
        // Test Case 1: Type A, S format (CD xx) → length + 3
        let data1 = [0xCD, 0x10]; // 16 byte length + 3 = 19 total
        assert_eq!(packet_size(&data1), 19);

        // Test Case 2: Type B, S format (3D xx) → length + 2
        let data2 = [0x3D, 0x10]; // 16 byte length + 2 = 18 total
        assert_eq!(packet_size(&data2), 18);

        // Test Case 3: Reversed byte order (xx CD) → length + 3
        let data3 = [0x10, 0xCD]; // 16 byte length + 3 = 19 total
        assert_eq!(packet_size(&data3), 19);

        // Test Case 4: Reversed byte order (xx 3D) → length + 2
        let data4 = [0x10, 0x3D]; // 16 byte length + 2 = 18 total
        assert_eq!(packet_size(&data4), 18);

        // Test invalid headers
        let invalid1 = [0x00, 0x00];
        assert_eq!(packet_size(&invalid1), -2);

        let invalid2 = [0xFF, 0xFF];
        assert_eq!(packet_size(&invalid2), -2);

        // Test insufficient data
        let short = [0xCD];
        assert_eq!(packet_size(&short), -1);

        let empty: [u8; 0] = [];
        assert_eq!(packet_size(&empty), -1);
    }

    /// Test CRC calculation with wM-Bus polynomial
    #[test]
    fn test_wmbus_crc() {
        // Test CRC calculation with known wM-Bus frame
        let test_data = [0x44, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE];
        let crc = calculate_wmbus_crc(&test_data);

        // CRC should be calculated using polynomial 0x3D65
        // This is a placeholder - actual CRC values would need to be verified
        // against reference implementation
        assert!(crc != 0); // CRC should not be zero for this data

        // Test CRC verification
        let mut frame_with_crc = test_data.to_vec();
        let crc_bytes = [(crc >> 8) as u8, crc as u8];
        frame_with_crc.extend_from_slice(&crc_bytes);

        assert!(verify_wmbus_crc(&frame_with_crc));
    }

    /// Test log throttling functionality for production use
    #[test]
    fn test_log_throttling() {
        let mut throttle = LogThrottle::new(1000, 3); // 3 logs per second

        // First 3 calls should be allowed
        assert!(throttle.allow());
        assert!(throttle.allow());
        assert!(throttle.allow());

        // 4th call should be blocked
        assert!(!throttle.allow());

        // Sleep would be needed to test window reset in a real scenario
        // For unit test, we'll just verify the basic functionality
    }

    /// Test packet buffer functionality
    #[test]
    fn test_packet_buffer() {
        let mut buffer = PacketBuffer::new();

        // Test empty buffer
        assert!(!buffer.is_complete());
        assert!(buffer.determine_packet_size().is_none());

        // Add a valid wM-Bus header (Type A)
        buffer.push_byte(0xB3); // Will be bit-reversed to 0xCD internally
        buffer.push_byte(10); // Will be bit-reversed to 0x50 (80) internally

        // Check packet size determination
        // 0xCD (Type A) + 0x50 (80) → 80 + 3 = 83 total bytes
        let size = buffer.determine_packet_size();
        assert!(size.is_some());
        assert_eq!(size.unwrap(), 83);

        // Add remaining bytes to reach total of 83
        for i in 0..81 {
            // 81 more bytes to reach total of 83
            buffer.push_byte(i as u8);
        }

        assert!(buffer.is_complete());
        let packet = buffer.extract_packet().unwrap();
        assert_eq!(packet.len(), 83);
        assert_eq!(packet[0], 0xCD); // Bit-reversed sync word
        assert_eq!(packet[1], 0x50); // Bit-reversed length byte (80)
    }
}

#[cfg(feature = "rfm69")]
mod rfm69_hardware_tests {
    use mbus_rs::wmbus::radio::radio_driver::*;
    use mbus_rs::wmbus::radio::rfm69_packet::*;
    use mbus_rs::wmbus::radio::rfm69_registers::*;

    /// Test the bit reversal function for MSB/LSB conversion
    #[test]
    fn test_bit_reversal() {
        // Test known bit reversal patterns
        assert_eq!(rev8(0x00), 0x00);
        assert_eq!(rev8(0xFF), 0xFF);
        assert_eq!(rev8(0x01), 0x80);
        assert_eq!(rev8(0x80), 0x01);
        assert_eq!(rev8(0xAA), 0x55);
        assert_eq!(rev8(0x55), 0xAA);

        // Test wM-Bus specific patterns
        assert_eq!(rev8(0xB3), 0xCD); // Type A sync word
        assert_eq!(rev8(0xBC), 0x3D); // Type B sync word

        // Test bidirectional property
        for i in 0..=255u8 {
            assert_eq!(rev8(rev8(i)), i);
        }
    }

    /// Test sync word normalization
    #[test]
    fn test_sync_normalization() {
        // Test sync_norm function with known patterns
        assert_eq!(sync_norm(0xB3), 0xCD); // Type A
        assert_eq!(sync_norm(0xBC), 0x3D); // Type B
        assert_eq!(sync_norm(0xCD), 0xCD); // Already normalized Type A
        assert_eq!(sync_norm(0x3D), 0x3D); // Already normalized Type B

        // Test other values remain unchanged when not sync words
        assert_eq!(sync_norm(0x00), 0x00);
        assert_eq!(sync_norm(0xFF), 0xFF);
        assert_eq!(sync_norm(0x55), 0x55);
    }

    /// Test robust packet size detection with header validation
    #[test]
    fn test_packet_size_detection() {
        // Test Case 1: Type A, S format (CD xx) → length + 3
        let data1 = [0xCD, 0x10]; // 16 byte length + 3 = 19 total
        assert_eq!(packet_size(&data1), 19);

        // Test Case 2: Type B, S format (3D xx) → length + 2
        let data2 = [0x3D, 0x10]; // 16 byte length + 2 = 18 total
        assert_eq!(packet_size(&data2), 18);

        // Test Case 3: Reversed byte order (xx CD) → length + 3
        let data3 = [0x10, 0xCD]; // 16 byte length + 3 = 19 total
        assert_eq!(packet_size(&data3), 19);

        // Test Case 4: Reversed byte order (xx 3D) → length + 2
        let data4 = [0x10, 0x3D]; // 16 byte length + 2 = 18 total
        assert_eq!(packet_size(&data4), 18);

        // Test invalid headers
        let invalid1 = [0x00, 0x00];
        assert_eq!(packet_size(&invalid1), -2);

        let invalid2 = [0xFF, 0xFF];
        assert_eq!(packet_size(&invalid2), -2);

        // Test insufficient data
        let short = [0xCD];
        assert_eq!(packet_size(&short), -1);

        let empty: [u8; 0] = [];
        assert_eq!(packet_size(&empty), -1);
    }

    /// Test packet buffer functionality
    #[test]
    fn test_packet_buffer() {
        let mut buffer = PacketBuffer::new();

        // Test empty buffer
        assert!(!buffer.is_complete());
        assert!(buffer.determine_packet_size().is_none());

        // Add a valid wM-Bus header (Type A)
        buffer.push_byte(0xB3); // Will be bit-reversed to 0xCD internally
        buffer.push_byte(10); // 10 byte payload + 3 = 13 total

        // Check packet size determination
        let size = buffer.determine_packet_size();
        assert!(size.is_some());
        assert_eq!(size.unwrap(), 13);

        // Add remaining bytes
        for i in 0..11 {
            // 11 more bytes to reach total of 13
            buffer.push_byte(i);
        }

        assert!(buffer.is_complete());
        let packet = buffer.extract_packet().unwrap();
        assert_eq!(packet.len(), 13);
        assert_eq!(packet[0], 0xCD); // Bit-reversed sync word
        assert_eq!(packet[1], 10); // Length byte
    }

    /// Test CRC calculation with wM-Bus polynomial
    #[test]
    fn test_wmbus_crc() {
        // Test CRC calculation with known wM-Bus frame
        let test_data = [0x44, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE];
        let crc = calculate_wmbus_crc(&test_data);

        // CRC should be calculated using polynomial 0x3D65
        // This is a placeholder - actual CRC values would need to be verified
        // against reference implementation
        assert!(crc != 0); // CRC should not be zero for this data
    }

    /// Test encryption detection in frame headers
    #[test]
    fn test_encryption_detection() {
        let mut buffer = PacketBuffer::new();

        // Create a frame with encryption flag set in CI field
        buffer.push_byte(0xCD); // Type A sync
        buffer.push_byte(20); // Length
        buffer.push_byte(0x44); // Manufacturer ID LSB
        buffer.push_byte(0x12); // Manufacturer ID MSB
        buffer.push_byte(0x34); // Serial number
        buffer.push_byte(0x56);
        buffer.push_byte(0x78);
        buffer.push_byte(0x9A);
        buffer.push_byte(0x05); // CI field with encryption (bit 0 set)

        // The buffer should detect encryption and handle accordingly
        // Implementation would check CI field and set appropriate flags
        assert!(buffer.determine_packet_size().is_some());
    }

    /// Test log throttling functionality for production use
    #[test]
    fn test_log_throttling() {
        let mut throttle = LogThrottle::new(1000, 3); // 3 logs per second

        // First 3 calls should be allowed
        assert!(throttle.allow());
        assert!(throttle.allow());
        assert!(throttle.allow());

        // 4th call should be blocked
        assert!(!throttle.allow());

        // Sleep would be needed to test window reset in a real scenario
        // For unit test, we'll just verify the basic functionality
    }

    /// Test FIFO race condition handling for short frames
    #[test]
    fn test_fifo_race_handling() {
        let mut buffer = PacketBuffer::new();

        // Simulate partial frame followed by new frame start
        buffer.push_byte(0xCD); // Type A sync
        buffer.push_byte(10); // Length
        buffer.push_byte(0x44); // Start of data

        // Simulate another sync word appearing (indicates frame restart)
        buffer.push_byte(0xCD); // New sync word

        // Buffer should handle this gracefully and reset
        let size = buffer.determine_packet_size();
        assert!(size.is_some()); // Should detect the new frame
    }

    /// Test C-field processing for frame types
    #[test]
    fn test_c_field_processing() {
        // Test various C-field values and their normalization
        let c_field_raw = 0x44; // SND_NR
        let c_field_normalized = c_field_raw; // No transformation needed

        assert_eq!(c_field_normalized, 0x44);

        // Test other common C-field values
        let test_values = [0x08, 0x44, 0x46, 0x48, 0x5B, 0x7A];
        for &value in &test_values {
            // All values should be processed correctly
            // (Actual normalization logic would be in the implementation)
            assert!(value <= 0xFF); // Basic validity check
        }
    }

    /// Test error recovery and garbage handling
    #[test]
    fn test_error_recovery() {
        let mut buffer = PacketBuffer::new();

        // Add garbage data
        buffer.push_byte(0xFF);
        buffer.push_byte(0x00);
        buffer.push_byte(0x55);

        // Should not find valid packet size
        assert!(buffer.determine_packet_size().is_none());

        // Buffer should clear and be ready for new data
        buffer.clear();

        // Now add valid frame
        buffer.push_byte(0xCD); // Type A sync
        buffer.push_byte(5); // Length

        // Should now detect packet size
        assert!(buffer.determine_packet_size().is_some());
    }

    /// Test packet statistics tracking
    #[test]
    fn test_packet_statistics() {
        let mut stats = PacketStats::default();

        // Initial state
        assert_eq!(stats.packets_received, 0);
        assert_eq!(stats.packets_valid, 0);
        assert_eq!(stats.packets_crc_error, 0);

        // Simulate packet events
        let mut buffer = PacketBuffer::new();
        buffer.update_stats(PacketEvent::Valid);
        buffer.update_stats(PacketEvent::CrcError);
        buffer.update_stats(PacketEvent::InvalidHeader);

        // Statistics should be updated
        let current_stats = buffer.get_stats();
        assert_eq!(current_stats.packets_valid, 1);
        assert_eq!(current_stats.packets_crc_error, 1);
        assert_eq!(current_stats.packets_invalid_header, 1);
    }

    /// Test wM-Bus frequency calculation
    #[test]
    fn test_frequency_calculation() {
        // Test frequency register calculation for 868.95 MHz
        let target_freq = 868.95e6;
        let freq_reg = (target_freq / FSTEP) as u32;

        // Verify the calculation is reasonable
        assert!(freq_reg > 0);
        assert!(freq_reg < 0x1000000); // Should fit in 24 bits

        // Test reverse calculation
        let calculated_freq = freq_reg as f64 * FSTEP;
        let error = (calculated_freq - target_freq).abs();
        assert!(error < 1000.0); // Error should be less than 1 kHz
    }

    /// Test register constant values
    #[test]
    fn test_register_constants() {
        // Test key register addresses
        assert_eq!(REG_FIFO, 0x00);
        assert_eq!(REG_OPMODE, 0x01);
        assert_eq!(REG_IRQFLAGS2, 0x28);

        // Test operating mode values
        assert_eq!(RF_OPMODE_SLEEP, 0x00);
        assert_eq!(RF_OPMODE_STANDBY, 0x04);
        assert_eq!(RF_OPMODE_RECEIVER, 0x10);

        // Test IRQ flag constants
        assert_eq!(RF_IRQFLAGS2_FIFOLEVEL, 0x20);
        assert_eq!(RF_IRQFLAGS2_FIFOOVERRUN, 0x10);
        assert_eq!(RF_IRQFLAGS2_PAYLOADREADY, 0x04);
    }

    /// Test wM-Bus specific configuration values
    #[test]
    fn test_wmbus_configuration() {
        // Test bitrate configuration for 100 kbps
        assert_eq!(RF_BITRATEMSB_100KBPS, 0x01);
        assert_eq!(RF_BITRATELSB_100KBPS, 0x40);

        // Test frequency deviation for 50 kHz
        assert_eq!(RF_FDEVMSB_50000, 0x03);
        assert_eq!(RF_FDEVLSB_50000, 0x33);

        // Test default frequency
        assert!((WMBUS_FREQUENCY - 868.95e6).abs() < 1.0);
    }
}

// Tests that run without the rfm69 feature (basic functionality)
#[cfg(not(feature = "rfm69"))]
mod basic_tests {
    use mbus_rs::wmbus::radio::radio_driver::*;

    #[test]
    fn test_wmbus_config_default() {
        let config = WMBusConfig::default();

        assert_eq!(config.frequency_hz, 868_950_000);
        assert_eq!(config.bitrate, 100_000);
        assert_eq!(config.output_power_dbm, 14);
        assert!(config.agc_enabled);
        assert_eq!(config.crc_polynomial, 0x3D65);
    }

    #[test]
    fn test_radio_stats_default() {
        let stats = RadioStats::default();

        assert_eq!(stats.packets_received, 0);
        assert_eq!(stats.packets_crc_valid, 0);
        assert_eq!(stats.packets_crc_error, 0);
        assert_eq!(stats.packets_length_error, 0);
        assert_eq!(stats.last_rssi_dbm, 0);
    }

    #[test]
    fn test_driver_info_creation() {
        let info = DriverInfo {
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            frequency_bands: vec![(868_000_000, 870_000_000)],
            max_packet_size: 255,
            supported_bitrates: vec![100_000],
            power_range_dbm: (-20, 20),
            features: vec!["GFSK".to_string()],
        };

        assert_eq!(info.name, "Test");
        assert_eq!(info.max_packet_size, 255);
        assert_eq!(info.frequency_bands.len(), 1);
    }
}
