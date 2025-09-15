//! Comprehensive tests for the M-Bus manufacturer database and conversion system

use mbus_rs::{
    VendorRegistry,
    manufacturer_to_id, id_to_manufacturer, get_manufacturer_info,
    get_manufacturer_name, has_quirks, all_manufacturers, is_valid_id,
    manufacturer_id_to_string, parse_manufacturer_id,
};

#[test]
fn test_basic_manufacturer_conversion() {
    // Test known manufacturers
    assert_eq!(manufacturer_to_id("QDS").unwrap(), 0x4493);
    assert_eq!(manufacturer_to_id("ZEN").unwrap(), 0x68AE);
    assert_eq!(manufacturer_to_id("KAM").unwrap(), 0x2C2D);

    // Test reverse conversion
    assert_eq!(id_to_manufacturer(0x4493), "QDS");
    assert_eq!(id_to_manufacturer(0x68AE), "ZEN");
    assert_eq!(id_to_manufacturer(0x2C2D), "KAM");
}

#[test]
fn test_boundary_conditions() {
    // Test FLAG Association boundaries
    assert_eq!(manufacturer_to_id("AAA").unwrap(), 0x0421);
    assert_eq!(manufacturer_to_id("ZZZ").unwrap(), 0x6B5A);

    assert_eq!(id_to_manufacturer(0x0421), "AAA");
    assert_eq!(id_to_manufacturer(0x6B5A), "ZZZ");

    // Test invalid range
    assert_eq!(id_to_manufacturer(0x0420), "UNK");
    assert_eq!(id_to_manufacturer(0x6B5B), "UNK");
}

#[test]
fn test_case_insensitive_conversion() {
    // Test case variations
    assert_eq!(manufacturer_to_id("qds"), Some(0x4493));
    assert_eq!(manufacturer_to_id("Qds"), Some(0x4493));
    assert_eq!(manufacturer_to_id("QDS"), Some(0x4493));
    assert_eq!(manufacturer_to_id("qDs"), Some(0x4493));
}

#[test]
fn test_invalid_inputs() {
    // Test invalid manufacturer codes
    assert_eq!(manufacturer_to_id(""), None);
    assert_eq!(manufacturer_to_id("AB"), None);
    assert_eq!(manufacturer_to_id("123"), None);
    assert_eq!(manufacturer_to_id("A1B"), None);
    assert_eq!(manufacturer_to_id("AB!"), None);
}

#[test]
fn test_round_trip_conversion() {
    let test_codes = [
        "QDS", "ZEN", "KAM", "LUG", "ENG", "ELV", "LSE", "MET",
        "SON", "ITW", "EFE", "ELS", "RKE", "SIE", "ABB", "SEN",
        "AAA", "ZZZ", "ABC", "XYZ", "MNO", "PQR"
    ];

    for code in &test_codes {
        if let Some(id) = manufacturer_to_id(code) {
            assert_eq!(id_to_manufacturer(id), code.to_uppercase());
        }
    }
}

#[test]
fn test_manufacturer_database() {
    // Test QUNDIS (has quirks)
    let qundis_info = get_manufacturer_info(0x4493).unwrap();
    assert_eq!(qundis_info.code, "QDS");
    assert_eq!(qundis_info.name, "Qundis GmbH");
    assert!(qundis_info.has_quirks);
    assert!(qundis_info.description.is_some());
    assert!(qundis_info.description.unwrap().contains("VIF 0x04"));

    // Test manufacturer without quirks
    let zenner_info = get_manufacturer_info(0x68AE).unwrap();
    assert_eq!(zenner_info.code, "ZEN");
    assert_eq!(zenner_info.name, "Zenner International");
    assert!(!zenner_info.has_quirks);

    // Test unknown manufacturer
    assert!(get_manufacturer_info(0x0000).is_none());
}

#[test]
fn test_utility_functions() {
    // Test has_quirks function
    assert!(has_quirks(0x4493)); // QUNDIS has quirks
    assert!(!has_quirks(0x68AE)); // Zenner has no quirks
    assert!(!has_quirks(0x0000)); // Unknown has no quirks

    // Test get_manufacturer_name function
    assert_eq!(get_manufacturer_name(0x4493), "Qundis GmbH");
    assert_eq!(get_manufacturer_name(0x68AE), "Zenner International");
    assert_eq!(get_manufacturer_name(0x0000), "UNK");

    // Test is_valid_id function
    assert!(is_valid_id(0x4493));
    assert!(is_valid_id(0x0421));
    assert!(is_valid_id(0x6B5A));
    assert!(!is_valid_id(0x0000));
    assert!(!is_valid_id(0x0420));
    assert!(!is_valid_id(0x6B5B));
    assert!(!is_valid_id(0xFFFF));
}

#[test]
fn test_all_manufacturers_iterator() {
    let count = all_manufacturers().count();
    assert!(count > 0);
    assert!(count >= 10); // Should have at least 10 manufacturers

    // Test that QUNDIS is in the database
    let qundis_found = all_manufacturers()
        .any(|(_, info)| info.code == "QDS");
    assert!(qundis_found);

    // Test that at least one manufacturer has quirks
    let has_quirky_manufacturer = all_manufacturers()
        .any(|(_, info)| info.has_quirks);
    assert!(has_quirky_manufacturer);
}

#[test]
fn test_database_consistency() {
    // Ensure all entries in database have valid IDs and consistent data
    for (&id, info) in all_manufacturers() {
        // ID should be valid
        assert!(is_valid_id(id));

        // Code should round-trip correctly
        assert_eq!(manufacturer_to_id(info.code), Some(id));
        assert_eq!(id_to_manufacturer(id), info.code);

        // Name should not be empty
        assert!(!info.name.is_empty());

        // Code should be exactly 3 characters
        assert_eq!(info.code.len(), 3);

        // Code should be uppercase
        assert_eq!(info.code, info.code.to_uppercase());
    }
}

#[test]
fn test_backward_compatibility() {
    // Test that the legacy functions still work correctly

    // Test manufacturer_id_to_string (legacy function)
    assert_eq!(manufacturer_id_to_string(0x4493), "QDS");
    assert_eq!(manufacturer_id_to_string(0x68AE), "ZEN");
    assert_eq!(manufacturer_id_to_string(0x0000), "UNK");

    // Test parse_manufacturer_id (legacy function)
    assert_eq!(parse_manufacturer_id("QDS"), 0x4493);
    assert_eq!(parse_manufacturer_id("ZEN"), 0x68AE);
    assert_eq!(parse_manufacturer_id(""), 0);
    assert_eq!(parse_manufacturer_id("123"), 0);
}

#[test]
fn test_flag_association_compliance() {
    // Test that our algorithm matches the FLAG Association standard

    // Test the formula: id = (c1-64)*32² + (c2-64)*32 + (c3-64)
    let test_cases = [
        ("AAA", 0x0421), // (1 << 10) + (1 << 5) + 1 = 1024 + 32 + 1 = 1057 = 0x421
        ("ABC", 0x0443), // (1 << 10) + (2 << 5) + 3 = 1024 + 64 + 3 = 1091 = 0x443
        ("QDS", 0x4493), // Known manufacturer with correct standard ID
        ("ZZZ", 0x6B5A), // (26 << 10) + (26 << 5) + 26 = 26624 + 832 + 26 = 27482 = 0x6B5A
    ];

    for (code, expected_id) in &test_cases {
        assert_eq!(manufacturer_to_id(code), Some(*expected_id));
        assert_eq!(id_to_manufacturer(*expected_id), *code);
    }
}

#[test]
fn test_vendor_registry_integration() {
    // Test the integration with the vendor registry system

    // Test basic registry creation
    let registry = VendorRegistry::with_defaults().unwrap();
    assert!(registry.has_extension("QDS"));

    // Test manufacturer detection registry
    let auto_registry = VendorRegistry::with_manufacturer_detection().unwrap();
    assert!(auto_registry.has_extension("QDS"));

    // Test that non-quirky manufacturers are not automatically registered
    assert!(!auto_registry.has_extension("ZEN")); // Zenner has no quirks
}

#[test]
fn test_real_world_scenarios() {
    // Test scenarios that would occur in real M-Bus deployments

    // Scenario 1: Parse device with QUNDIS manufacturer
    let device_id = 0x4493;
    let manufacturer_name = get_manufacturer_name(device_id);
    assert_eq!(manufacturer_name, "Qundis GmbH");

    if has_quirks(device_id) {
        // Should automatically know QUNDIS needs special handling
        let registry = VendorRegistry::with_manufacturer_detection().unwrap();
        assert!(registry.has_extension("QDS"));
    }

    // Scenario 2: Unknown manufacturer handling
    let unknown_id = 0x1234; // Likely not a real manufacturer
    let unknown_name = get_manufacturer_name(unknown_id);
    assert_eq!(unknown_name, id_to_manufacturer(unknown_id)); // Should be 3-letter code
    assert!(!has_quirks(unknown_id)); // Unknown should have no quirks

    // Scenario 3: Logging/debugging scenarios
    for (&id, info) in all_manufacturers().take(5) { // Test first 5
        let code = id_to_manufacturer(id);
        assert_eq!(code, info.code);

        // This would be typical logging output
        let log_message = format!(
            "Detected device: {} ({}) - ID: 0x{:04X}{}",
            info.name,
            info.code,
            id,
            if info.has_quirks { " [QUIRKS]" } else { "" }
        );
        assert!(log_message.contains(&info.name));
    }
}

#[test]
fn test_performance_considerations() {
    // Test that lookups are reasonably fast for production use

    use std::time::Instant;

    let start = Instant::now();

    // Perform many conversions
    for _ in 0..1000 {
        let _ = manufacturer_to_id("QDS");
        let _ = id_to_manufacturer(0x4493);
        let _ = get_manufacturer_info(0x4493);
        let _ = has_quirks(0x4493);
    }

    let duration = start.elapsed();

    // Should complete very quickly (well under 1ms for 1000 operations)
    assert!(duration.as_millis() < 10, "Manufacturer operations too slow: {:?}", duration);
}

#[test]
fn test_error_handling() {
    // Test that invalid inputs are handled gracefully

    // Empty string
    assert_eq!(manufacturer_to_id(""), None);

    // Too short
    assert_eq!(manufacturer_to_id("AB"), None);

    // Non-alphabetic
    assert_eq!(manufacturer_to_id("12A"), None);
    assert_eq!(manufacturer_to_id("A2B"), None);
    assert_eq!(manufacturer_to_id("AB3"), None);

    // Special characters
    assert_eq!(manufacturer_to_id("A!B"), None);
    assert_eq!(manufacturer_to_id("A-B"), None);
    assert_eq!(manufacturer_to_id("A B"), None);

    // Invalid IDs
    assert_eq!(id_to_manufacturer(0x0000), "UNK");
    assert_eq!(id_to_manufacturer(0xFFFF), "UNK");

    // None returns for unknown manufacturers
    assert!(get_manufacturer_info(0x0000).is_none());
    assert!(get_manufacturer_info(0xFFFF).is_none());
}

#[test]
fn test_extended_character_sets() {
    // Test that the algorithm correctly handles edge cases in character conversion

    // Test all valid single-character combinations at boundaries
    assert_eq!(manufacturer_to_id("AAA"), Some(0x0421)); // Minimum: (1 << 10) + (1 << 5) + 1 = 1057
    assert_eq!(manufacturer_to_id("AAZ"), Some(0x043A)); // A, A, Z: (1 << 10) + (1 << 5) + 26 = 1082
    assert_eq!(manufacturer_to_id("AZA"), Some(0x0741)); // A, Z, A: (1 << 10) + (26 << 5) + 1 = 1857
    assert_eq!(manufacturer_to_id("ZAA"), Some(0x6821)); // Z, A, A: (26 << 10) + (1 << 5) + 1 = 26657
    assert_eq!(manufacturer_to_id("ZZZ"), Some(0x6B5A)); // Maximum: (26 << 10) + (26 << 5) + 26 = 27482

    // Verify reverse conversion
    assert_eq!(id_to_manufacturer(0x0421), "AAA");
    assert_eq!(id_to_manufacturer(0x043A), "AAZ");
    assert_eq!(id_to_manufacturer(0x0741), "AZA");
    assert_eq!(id_to_manufacturer(0x6821), "ZAA");
    assert_eq!(id_to_manufacturer(0x6B5A), "ZZZ");
}