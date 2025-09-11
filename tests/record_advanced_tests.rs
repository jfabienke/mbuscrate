use mbus_rs::constants::*;
use mbus_rs::error::MBusError;
use mbus_rs::payload::record::*;

#[test]
fn test_parse_fixed_record_valid_bcd() {
    // Valid fixed record with BCD counter
    let input = vec![
        0x12, 0x34, 0x56, 0x78, // Device ID (BCD)
        0x04, 0x43, // Manufacturer (0x0443 = "ABC")
        0x01, // Version
        0x10, // Medium (m^3 - Volume)
        0x05, // Access number
        0x80, // Status (BCD format)
        0x00, 0x00, // Signature
        0x12, 0x34, 0x56, 0x78, // Counter 1 (BCD)
    ];

    let result = parse_fixed_record(&input);
    assert!(result.is_ok());
    let record = result.unwrap();
    assert_eq!(record.storage_number, 12345678); // BCD decoded correctly
    assert_eq!(record.drh.vib.vif, 0x10); // Medium
    assert!(matches!(record.value, MBusRecordValue::Numeric(_)));
}

#[test]
fn test_parse_fixed_record_valid_int() {
    // Valid fixed record with integer counter
    let input = vec![
        0x00, 0x00, 0x00, 0x99, // Device ID (BCD)
        0x04, 0x43, // Manufacturer (0x0443 = "ABC")
        0x01, // Version
        0x10, // Medium (m^3 - Volume)
        0x05, // Access number
        0x80, // Status (int format - 0x80 bit set)
        0x00, 0x00, // Signature
        0x00, 0x00, 0x10, 0x00, // Counter 1 (int)
    ];

    let result = parse_fixed_record(&input);
    assert!(result.is_ok());
    let record = result.unwrap();
    assert_eq!(record.storage_number, 99); // BCD decoded correctly
}

#[test]
fn test_parse_fixed_record_all_medium_types() {
    // Test that all defined medium types are handled correctly
    let mediums_to_test = vec![
        0x00, // Energy (Wh)
        0x10, // Volume (m^3)
        0x18, // Mass (kg)
        0x20, // On time (s)
        0x28, // Power (W)
        0x38, // Volume flow (m^3/h)
        0x50, // Mass flow (kg/h)
        0x58, // Flow temperature (°C)
        0x5C, // Return temperature (°C)
        0x60, // Temperature difference (K)
        0x68, // Pressure (bar)
        0x78, // Fabrication No
    ];

    for medium in mediums_to_test {
        let input = vec![
            0x00, 0x00, 0x00, 0x01, // Device ID (BCD)
            0x04, 0x43,   // Manufacturer (valid)
            0x01,   // Version
            medium, // Medium to test
            0x05,   // Access number
            0x80,   // Status (int format - 0x80 bit set)
            0x00, 0x00, // Signature
            0x00, 0x00, 0x00, 0x64, // Counter 1 = 100 (int)
        ];

        let result = parse_fixed_record(&input);
        assert!(result.is_ok(), "Failed to parse medium 0x{:02X}", medium);
        let record = result.unwrap();
        assert!(!record.unit.is_empty());
        assert!(!record.quantity.is_empty());
    }
}

#[test]
fn test_parse_variable_record_idle_filler() {
    // Test that idle filler bytes are properly skipped
    let input = vec![
        0x2F, // IDLE_FILLER (should be skipped)
        0x01, // DIF: 1-byte integer
        0x13, // VIF: Volume (10^-3 m^3)
        0x42, // Data: 66 liters
    ];

    let result = parse_variable_record(&input);
    assert!(result.is_ok());
    let record = result.unwrap();
    assert_eq!(record.drh.dib.dif, 0x01);
    assert_eq!(record.drh.vib.vif, 0x13);
    assert_eq!(record.data[0], 0x42);
    assert_eq!(record.data_len, 1);
}

#[test]
fn test_parse_variable_record_manufacturer_specific() {
    // Test manufacturer-specific data handling
    let input = vec![
        0x0F, // DIF: Manufacturer specific
        0xAA, 0xBB, 0xCC, 0xDD, // Manufacturer data
    ];

    let result = parse_variable_record(&input);
    assert!(result.is_ok());
    let record = result.unwrap();
    assert_eq!(record.drh.dib.dif, MBUS_DIB_DIF_MANUFACTURER_SPECIFIC);
    assert_eq!(record.quantity, "Manufacturer specific");
    assert_eq!(record.data_len, 4);
    assert_eq!(&record.data[..4], &[0xAA, 0xBB, 0xCC, 0xDD]);
}

#[test]
fn test_parse_variable_record_more_records_follow() {
    // Test more records follow flag
    let input = vec![
        0x1F, // DIF: More records follow
        0xAA, 0xBB, // Some data
    ];

    let result = parse_variable_record(&input);
    assert!(result.is_ok());
    let record = result.unwrap();
    assert_eq!(record.drh.dib.dif, MBUS_DIB_DIF_MORE_RECORDS_FOLLOW);
    assert_eq!(record.more_records_follow, 1);
    assert_eq!(record.data_len, 2);
}

#[test]
fn test_parse_variable_record_with_dife_extensions() {
    // Test DIF extensions
    // DIF with extension bit set means next byte is DIFE
    // DIFE without extension bit means it's the last DIFE
    let input = vec![
        0x81, // DIF: 1-byte integer with extension bit set
        0x40, // DIFE: Storage number 1 (no extension bit)
        0x13, // VIF: Volume
        0x42, // Data
    ];

    let result = parse_variable_record(&input);
    assert!(result.is_ok());
    let record = result.unwrap();
    assert_eq!(record.drh.dib.dif, 0x81);
    assert_eq!(record.drh.dib.ndife, 1);
    assert_eq!(record.drh.dib.dife[0], 0x40);
}

#[test]
fn test_parse_variable_record_with_vife_extensions() {
    // Test VIF extensions
    let input = vec![
        0x01, // DIF: 1-byte integer
        0x93, // VIF: Volume with extension bit
        0x3C, // VIFE: multiplicative correction 10^-4
        0x42, // Data
    ];

    let result = parse_variable_record(&input);
    assert!(result.is_ok());
    let record = result.unwrap();
    assert_eq!(record.drh.vib.vif, 0x93);
    assert_eq!(record.drh.vib.nvife, 1);
    assert_eq!(record.drh.vib.vife[0], 0x3C);
}

#[test]
fn test_parse_variable_record_custom_vif() {
    // Test custom VIF (0x7C)
    let input = vec![
        0x01, // DIF: 1-byte integer
        0x7C, // VIF: Plain text VIF
        0x05, // Length of custom VIF
        b'T', b'e', b's', b't', b'1', // Custom VIF text (reversed in M-Bus)
        0x42, // Data
    ];

    let result = parse_variable_record(&input);
    assert!(result.is_ok());
    let record = result.unwrap();
    assert_eq!(record.drh.vib.vif, 0x7C);
    assert_eq!(record.drh.vib.custom_vif, "1tseT"); // Reversed
}

#[test]
fn test_parse_variable_record_variable_length() {
    // Test variable length data (DIF = 0x0D)
    let input = vec![
        0x0D, // DIF: Variable length
        0x13, // VIF: Volume
        0x04, // Length byte: 4 bytes
        0x11, 0x22, 0x33, 0x44, // Data
    ];

    let result = parse_variable_record(&input);
    assert!(result.is_ok());
    let record = result.unwrap();
    assert_eq!(record.drh.dib.dif, 0x0D);
    assert_eq!(record.data_len, 4);
    assert_eq!(&record.data[..4], &[0x11, 0x22, 0x33, 0x44]);
}

#[test]
fn test_parse_variable_record_extended_variable_length() {
    // Test extended variable length encodings
    let test_cases = vec![
        (0xC0, 0),    // Even length: 0 bytes
        (0xC1, 2),    // Even length: 2 bytes
        (0xCF, 30),   // Even length: 30 bytes
        (0xD0, 1),    // Odd length: 1 byte
        (0xD1, 3),    // Odd length: 3 bytes
        (0xDF, 31),   // Odd length: 31 bytes
        (0xE0, 64),   // Large even: 64 bytes
        (0xEF, 79),   // Large even: 79 bytes
        (0xF0, 1120), // Large odd: 1120 bytes (max we'll test with limited data)
    ];

    for (length_byte, expected_len) in test_cases {
        // Create enough data for the test
        let mut input = vec![
            0x0D,        // DIF: Variable length
            0x13,        // VIF: Volume
            length_byte, // Length encoding
        ];

        // Add enough data bytes (limit to reasonable test size)
        let test_len = expected_len.min(100);
        for i in 0..test_len {
            input.push((i & 0xFF) as u8);
        }

        let result = parse_variable_record(&input);
        if test_len == expected_len {
            assert!(
                result.is_ok(),
                "Failed for length byte 0x{:02X}",
                length_byte
            );
            let record = result.unwrap();
            assert_eq!(
                record.data_len, expected_len,
                "Wrong length for byte 0x{:02X}",
                length_byte
            );
        }
    }
}

#[test]
fn test_parse_variable_record_all_dif_types() {
    // Test all DIF data types
    let dif_configs = vec![
        (0x00, 0), // No data
        (0x01, 1), // 8 bit integer
        (0x02, 2), // 16 bit integer
        (0x03, 3), // 24 bit integer
        (0x04, 4), // 32 bit integer
        (0x05, 6), // 48 bit integer
        (0x06, 8), // 64 bit integer
        (0x07, 0), // Selection for Readout
        (0x08, 0), // Special functions
        (0x09, 1), // 2 digit BCD
        (0x0A, 2), // 4 digit BCD
        (0x0B, 3), // 6 digit BCD
        (0x0C, 4), // 8 digit BCD
        // 0x0D is variable length, tested separately
        (0x0E, 6), // 12 digit BCD
        (0x0F, 0), // Special: Manufacturer specific or More records follow
    ];

    for (dif, expected_len) in dif_configs {
        if dif == 0x0F || dif == 0x1F {
            continue; // These are special cases tested separately
        }

        let mut input = vec![
            dif,  // DIF
            0x13, // VIF: Volume
        ];

        // Add data bytes
        for i in 0..expected_len {
            input.push((0xA0 + i) as u8); // Safe bytes
        }

        let result = parse_variable_record(&input);
        assert!(result.is_ok(), "Failed for DIF 0x{:02X}", dif);
        let record = result.unwrap();
        assert_eq!(record.drh.dib.dif, dif);
        assert_eq!(
            record.data_len, expected_len,
            "Wrong data length for DIF 0x{:02X}",
            dif
        );
    }
}

#[test]
fn test_parse_variable_record_storage_and_tariff() {
    // Test storage number and tariff encoding via DIF extensions
    let input = vec![
        0x42, // DIF: 16-bit int, storage 1
        0x13, // VIF: Volume
        0x34, 0x12, // Data: 0x1234
    ];

    let result = parse_variable_record(&input);
    assert!(result.is_ok());
    let record = result.unwrap();

    // Check storage number extraction from DIF
    let storage = (record.drh.dib.dif >> 6) & 0x01;
    assert_eq!(storage, 1);
}

#[test]
fn test_parse_variable_record_function_field() {
    // Test function field encoding in DIF
    let function_types = vec![
        (0x00, "Instantaneous"),
        (0x10, "Maximum"),
        (0x20, "Minimum"),
        (0x30, "Error"),
    ];

    for (function_bits, _name) in function_types {
        let input = vec![
            0x01 | function_bits, // DIF with function field
            0x13,                 // VIF: Volume
            0x42,                 // Data
        ];

        let result = parse_variable_record(&input);
        assert!(result.is_ok());
        let record = result.unwrap();

        // Check function field extraction
        let function = (record.drh.dib.dif >> 4) & 0x03;
        assert_eq!(function, function_bits >> 4);
    }
}

#[test]
fn test_parse_variable_record_error_cases() {
    // Test premature end at data
    let input = vec![
        0x04, // DIF: 4-byte integer
        0x13, // VIF: Volume
        0x11, 0x22, // Only 2 bytes instead of 4
    ];

    let result = parse_variable_record(&input);
    assert!(result.is_err());
    assert!(matches!(result, Err(MBusError::PrematureEndAtData)));

    // Test invalid variable length
    let input = vec![
        0x0D, // DIF: Variable length
        0x13, // VIF: Volume
        0xFF, // Invalid length byte
    ];

    let result = parse_variable_record(&input);
    assert!(result.is_err());

    // Test custom VIF with excessive length
    let mut input = vec![
        0x01, // DIF: 1-byte integer
        0x7C, // VIF: Plain text VIF
        0xFF, // Length too large
    ];
    // Add some data
    for _ in 0..20 {
        input.push(0xAA);
    }

    let result = parse_variable_record(&input);
    assert!(result.is_err());
}

#[test]
fn test_parse_fixed_record_combined_counters() {
    // Test that fixed record properly combines counter values
    // This tests the internal normalize_fixed logic indirectly
    let input = vec![
        0x00, 0x00, 0x00, 0x01, // Device ID (BCD)
        0x04, 0x43, // Manufacturer (valid)
        0x01, // Version
        0x10, // Medium (m^3 - Volume)
        0x05, // Access number
        0x80, // Status (int format - 0x80 bit set)
        0x00, 0x00, // Signature
        0x00, 0x00, 0x03, 0xE8, // Counter = 1000 (int)
    ];

    let result = parse_fixed_record(&input);
    assert!(result.is_ok());
    let record = result.unwrap();

    // Check that units and quantities are set
    assert!(record.unit.contains("m^3"));
    assert!(record.quantity.contains("Volume"));

    // Value should be normalized: 1000 * 1e-6 = 0.001
    if let MBusRecordValue::Numeric(val) = record.value {
        // The value combines counter1 and counter2 (which is 0)
        // So it should be 0.001 + 0 = 0.001
        assert!((val - 0.001).abs() < 0.0001, "Expected ~0.001, got {}", val);
    } else {
        panic!("Expected numeric value");
    }
}

#[test]
fn test_mbus_record_value_enum() {
    // Test numeric value
    let numeric = MBusRecordValue::Numeric(42.5);
    if let MBusRecordValue::Numeric(val) = numeric {
        assert_eq!(val, 42.5);
    } else {
        panic!("Expected numeric value");
    }

    // Test string value
    let string = MBusRecordValue::String("Test".to_string());
    if let MBusRecordValue::String(val) = string {
        assert_eq!(val, "Test");
    } else {
        panic!("Expected string value");
    }
}

#[test]
fn test_parse_fixed_record_edge_boundaries() {
    // Test boundary manufacturer values
    let mut input = vec![
        0x00, 0x00, 0x00, 0x01, // Device ID
        0x04, 0x21, // Min valid manufacturer (0x0421)
        0x01, 0x10, 0x05, 0x00, // Version, medium, access, status
        0x00, 0x00, // Signature
        0x00, 0x00, 0x00, 0x64, // Counter
    ];

    let result = parse_fixed_record(&input);
    assert!(result.is_ok());

    // Max valid manufacturer
    input[4] = 0x6B;
    input[5] = 0x5A;
    let result = parse_fixed_record(&input);
    assert!(result.is_ok());

    // Just above max - invalid
    input[4] = 0x6B;
    input[5] = 0x5B;
    let result = parse_fixed_record(&input);
    assert!(result.is_err());

    // Just below min - invalid
    input[4] = 0x04;
    input[5] = 0x20;
    let result = parse_fixed_record(&input);
    assert!(result.is_err());
}

#[test]
fn test_parse_variable_record_multiple_extensions() {
    // Test multiple DIFE extensions
    let input = vec![
        0x81, // DIF with extension
        0xC0, // DIFE 1 with extension
        0x40, // DIFE 2
        0x13, // VIF
        0x42, // Data
    ];

    let result = parse_variable_record(&input);
    assert!(result.is_ok());
    let record = result.unwrap();
    assert_eq!(record.drh.dib.ndife, 2);
    assert_eq!(record.drh.dib.dife[0], 0xC0);
    assert_eq!(record.drh.dib.dife[1], 0x40);

    // Test multiple VIFE extensions
    let input = vec![
        0x01, // DIF
        0x93, // VIF with extension
        0xBC, // VIFE 1 with extension
        0x3C, // VIFE 2
        0x42, // Data
    ];

    let result = parse_variable_record(&input);
    assert!(result.is_ok());
    let record = result.unwrap();
    assert_eq!(record.drh.vib.nvife, 2);
    assert_eq!(record.drh.vib.vife[0], 0xBC);
    assert_eq!(record.drh.vib.vife[1], 0x3C);
}

#[test]
fn test_data_record_structures() {
    // Test that structures can be created and fields accessed
    let dib = MBusDataInformationBlock {
        dif: 0x04,
        ndife: 1,
        dife: [0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    };
    assert_eq!(dib.dif, 0x04);
    assert_eq!(dib.ndife, 1);
    assert_eq!(dib.dife[0], 0x40);

    let vib = MBusValueInformationBlock {
        vif: 0x13,
        nvife: 0,
        vife: [0; 10],
        custom_vif: "Test".to_string(),
    };
    assert_eq!(vib.vif, 0x13);
    assert_eq!(vib.custom_vif, "Test");

    let drh = MBusDataRecordHeader { dib, vib };
    assert_eq!(drh.dib.dif, 0x04);
    assert_eq!(drh.vib.vif, 0x13);
}
