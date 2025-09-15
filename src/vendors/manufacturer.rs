//! M-Bus Manufacturer ID Database and Conversion
//!
//! This module provides comprehensive manufacturer ID handling for M-Bus devices,
//! implementing the standard FLAG Association algorithm for 3-letter codes
//! and maintaining a database of known manufacturers with their specific quirks.
//!
//! ## Standard Algorithm
//!
//! M-Bus manufacturer IDs are calculated from 3-letter ASCII codes using:
//! ```text
//! id = (char1 - 64) * 32² + (char2 - 64) * 32 + (char3 - 64)
//! ```
//!
//! Valid range: 0x0421 (AAA) to 0x6B5A (ZZZ)
//!
//! ## Usage Example
//!
//! ```rust
//! use mbus_rs::vendors::manufacturer::{manufacturer_to_id, id_to_manufacturer};
//!
//! // Convert manufacturer code to ID
//! let id = manufacturer_to_id("QDS").unwrap(); // 0x5153
//!
//! // Convert ID back to code
//! let code = id_to_manufacturer(0x5153); // "QDS"
//!
//! // Get manufacturer info
//! let info = get_manufacturer_info(0x5153).unwrap();
//! println!("Manufacturer: {} ({})", info.name, info.code);
//! ```

use std::collections::HashMap;
use once_cell::sync::Lazy;

/// Information about a known M-Bus manufacturer
#[derive(Debug, Clone, PartialEq)]
pub struct ManufacturerInfo {
    /// 3-letter manufacturer code (e.g., "QDS")
    pub code: &'static str,
    /// Full manufacturer name (e.g., "Qundis")
    pub name: &'static str,
    /// Whether this manufacturer has known M-Bus quirks requiring vendor extensions
    pub has_quirks: bool,
    /// Optional description or notes
    pub description: Option<&'static str>,
}

impl ManufacturerInfo {
    pub const fn new(code: &'static str, name: &'static str, has_quirks: bool) -> Self {
        Self {
            code,
            name,
            has_quirks,
            description: None,
        }
    }

    pub const fn with_description(
        code: &'static str,
        name: &'static str,
        has_quirks: bool,
        description: &'static str,
    ) -> Self {
        Self {
            code,
            name,
            has_quirks,
            description: Some(description),
        }
    }
}

/// Database of known M-Bus manufacturers with their specific characteristics
pub static KNOWN_MANUFACTURERS: Lazy<HashMap<u16, ManufacturerInfo>> = Lazy::new(|| {
    let mut map = HashMap::new();

    // ===== HEAT COST ALLOCATOR MANUFACTURERS =====

    // Qundis - Major HCA manufacturer with proprietary extensions
    map.insert(0x4493, ManufacturerInfo::with_description(
        "QDS", "Qundis GmbH", true,
        "HCA manufacturer with proprietary VIF 0x04 date encoding"
    ));

    // Other HCA manufacturers
    map.insert(0x0907, ManufacturerInfo::new("BHG", "Brunata Hürth", false));
    map.insert(0x2674, ManufacturerInfo::new("IST", "ista International", false));
    map.insert(0x5068, ManufacturerInfo::new("TCH", "Techem GmbH", false));
    map.insert(0x6A4D, ManufacturerInfo::new("ZRM", "Minol Zenner Group", false));

    // ===== WATER METER MANUFACTURERS =====

    map.insert(0x05B4, ManufacturerInfo::new("AMT", "Aquametro AG", false));
    map.insert(0x2324, ManufacturerInfo::new("HYD", "Diehl Metering (Hydrometer)", false));
    map.insert(0x68AE, ManufacturerInfo::new("ZEN", "Zenner International", false));
    map.insert(0x1596, ManufacturerInfo::new("ELV", "Elvaco", false));
    map.insert(0x34B4, ManufacturerInfo::new("MET", "Metrix", false));

    // ===== HEAT/ENERGY METER MANUFACTURERS =====

    map.insert(0x4DEE, ManufacturerInfo::new("SON", "Sontex SA", false));
    map.insert(0x4024, ManufacturerInfo::new("PAD", "PadMess GmbH", false));
    map.insert(0x48AC, ManufacturerInfo::new("REL", "Relay GmbH", false));
    map.insert(0x14C5, ManufacturerInfo::new("EFE", "Efe", false));
    map.insert(0x15C7, ManufacturerInfo::new("ENG", "Engelmann", false));

    // ===== MULTI-UTILITY MANUFACTURERS =====

    map.insert(0x0442, ManufacturerInfo::new("ABB", "ABB (Asea Brown Boveri)", false));
    map.insert(0x0477, ManufacturerInfo::new("ACW", "Actaris (Itron)", false));
    map.insert(0x15A8, ManufacturerInfo::new("EMH", "EMH Energie-Messtechnik", false));
    map.insert(0x15B5, ManufacturerInfo::new("EMU", "EMU Electronic AG", false));
    map.insert(0x2697, ManufacturerInfo::new("ITW", "Itron", false));
    map.insert(0x2C2D, ManufacturerInfo::new("KAM", "Kamstrup", false));
    map.insert(0x32A7, ManufacturerInfo::new("LUG", "Landis+Gyr", false));
    map.insert(0x3B52, ManufacturerInfo::new("NZR", "Neue Zählerwerke", false));
    map.insert(0x4CAE, ManufacturerInfo::new("SEN", "Sensus Metering Systems", false));
    map.insert(0x4D25, ManufacturerInfo::new("SIE", "Siemens", false));

    // ===== GAS METER MANUFACTURERS =====

    map.insert(0x1593, ManufacturerInfo::new("ELS", "Elster (Honeywell)", false));
    map.insert(0x4965, ManufacturerInfo::new("RKE", "Raiffeisen Leasing", false));

    // ===== OTHER/SPECIALIZED MANUFACTURERS =====

    map.insert(0x1347, ManufacturerInfo::new("DZG", "DZG Metering", false));
    map.insert(0x3265, ManufacturerInfo::new("LSE", "LSE Industrie-Elektronik", false));

    // ===== REFERENCE/TEST MANUFACTURERS =====

    // CEN is used as example in M-Bus documentation
    map.insert(0x0CAE, ManufacturerInfo::new("CEN", "Example Manufacturer", false));

    map
});

/// Convert a 3-letter manufacturer code to M-Bus manufacturer ID
///
/// Implements the standard M-Bus manufacturer ID encoding as per EN 13757-3.
/// Formula: (char1 - 64) * 32² + (char2 - 64) * 32 + (char3 - 64)
///
/// # Arguments
/// * `manufacturer` - 3-letter ASCII code (case insensitive)
///
/// # Returns
/// * `Some(id)` - Valid manufacturer ID (15-bit value, MSB not set)
/// * `None` - Invalid input
///
/// # Examples
/// ```rust
/// assert_eq!(manufacturer_to_id("CEN"), Some(0x0CAE)); // 3246
/// assert_eq!(manufacturer_to_id("KAM"), Some(0x2C2D)); // 11309
/// assert_eq!(manufacturer_to_id("kam"), Some(0x2C2D)); // Case insensitive
/// assert_eq!(manufacturer_to_id("123"), None);
/// ```
pub fn manufacturer_to_id(manufacturer: &str) -> Option<u16> {
    if manufacturer.len() != 3 {
        return None;
    }

    let code = manufacturer.to_uppercase();
    let chars: Vec<char> = code.chars().collect();

    // All characters must be ASCII alphabetic (A-Z)
    if !chars.iter().all(|c| c.is_ascii_alphabetic() && c.is_uppercase()) {
        return None;
    }

    // Apply standard M-Bus encoding formula
    // Each character is mapped: A=1, B=2, ..., Z=26
    let val1 = (chars[0] as u16) - 64;
    let val2 = (chars[1] as u16) - 64;
    let val3 = (chars[2] as u16) - 64;

    // Validate range (1-26 for each character)
    if !(1..=26).contains(&val1) || !(1..=26).contains(&val2) || !(1..=26).contains(&val3) {
        return None;
    }

    // Standard formula: (c1 * 32²) + (c2 * 32) + c3
    let id = (val1 * 1024) + (val2 * 32) + val3;

    // The result is a 15-bit value (max value is 26*1024 + 26*32 + 26 = 27482 = 0x6B5A)
    Some(id)
}

/// Convert M-Bus manufacturer ID to 3-letter code
///
/// Decodes a 16-bit M-Bus manufacturer ID into its three-letter code.
/// The MSB (bit 15) indicates hard/soft address and is masked before decoding.
///
/// # Arguments
/// * `id` - Manufacturer ID (with or without MSB set)
///
/// # Returns
/// * 3-letter code for valid IDs
/// * "UNK" for invalid/unknown IDs
///
/// # Examples
/// ```rust
/// assert_eq!(id_to_manufacturer(0x0CAE), "CEN"); // 3246
/// assert_eq!(id_to_manufacturer(0x2C2D), "KAM"); // 11309
/// assert_eq!(id_to_manufacturer(0x8CAE), "CEN"); // With MSB set (soft address)
/// assert_eq!(id_to_manufacturer(0x0000), "UNK"); // Invalid
/// ```
pub fn id_to_manufacturer(id: u16) -> String {
    // Mask out the MSB (bit 15) which indicates hard/soft address
    let id_val = id & 0x7FFF;

    // Standard decoding using modulo arithmetic
    let val3 = id_val % 32;
    let val2 = (id_val / 32) % 32;
    let val1 = id_val / 1024;

    // Validate that values are in the valid range (1-26)
    if val1 == 0 || val1 > 26 || val2 == 0 || val2 > 26 || val3 == 0 || val3 > 26 {
        return "UNK".to_string();
    }

    // Convert values back to ASCII characters
    let char1 = ((val1 + 64) as u8) as char;
    let char2 = ((val2 + 64) as u8) as char;
    let char3 = ((val3 + 64) as u8) as char;

    format!("{}{}{}", char1, char2, char3)
}

/// Get detailed information about a manufacturer
///
/// Returns comprehensive information about known manufacturers,
/// including whether they require vendor-specific handling.
///
/// # Arguments
/// * `id` - Manufacturer ID
///
/// # Returns
/// * `Some(info)` - Detailed manufacturer information
/// * `None` - Unknown manufacturer
pub fn get_manufacturer_info(id: u16) -> Option<&'static ManufacturerInfo> {
    KNOWN_MANUFACTURERS.get(&id)
}

/// Get manufacturer name with fallback to generated code
///
/// Returns the full manufacturer name if known, otherwise
/// generates the 3-letter code from the ID.
///
/// # Arguments
/// * `id` - Manufacturer ID
///
/// # Returns
/// * Full manufacturer name or 3-letter code
pub fn get_manufacturer_name(id: u16) -> String {
    KNOWN_MANUFACTURERS
        .get(&id)
        .map(|info| info.name.to_string())
        .unwrap_or_else(|| id_to_manufacturer(id))
}

/// Check if a manufacturer has known M-Bus quirks
///
/// Returns true if the manufacturer requires vendor-specific
/// extensions for proper M-Bus frame parsing.
pub fn has_quirks(id: u16) -> bool {
    KNOWN_MANUFACTURERS
        .get(&id)
        .map(|info| info.has_quirks)
        .unwrap_or(false)
}

/// Get all known manufacturers
///
/// Returns an iterator over all manufacturers in the database.
pub fn all_manufacturers() -> impl Iterator<Item = (&'static u16, &'static ManufacturerInfo)> {
    KNOWN_MANUFACTURERS.iter()
}

/// Validate manufacturer ID range
///
/// Checks if the given ID falls within the valid FLAG Association range.
/// This checks the 15-bit value, ignoring the MSB.
pub fn is_valid_id(id: u16) -> bool {
    let id_val = id & 0x7FFF;
    // Minimum valid: AAA = (1*1024 + 1*32 + 1) = 1057 = 0x0421
    // Maximum valid: ZZZ = (26*1024 + 26*32 + 26) = 27482 = 0x6B5A
    (0x0421..=0x6B5A).contains(&id_val)
}

/// Check if manufacturer ID has the MSB set (soft address)
///
/// The MSB (bit 15) indicates whether the 6-byte address is:
/// - 0: Globally unique ("hard address") - manufacturer guarantees uniqueness
/// - 1: Locally unique ("soft address") - unique only within installation
///
/// # Examples
/// ```rust
/// assert!(!is_soft_address(0x0CAE)); // Hard address (MSB = 0)
/// assert!(is_soft_address(0x8CAE));  // Soft address (MSB = 1)
/// ```
pub fn is_soft_address(id: u16) -> bool {
    (id & 0x8000) != 0
}

/// Set or clear the soft address flag in a manufacturer ID
///
/// # Examples
/// ```rust
/// assert_eq!(set_soft_address(0x0CAE, true), 0x8CAE);  // Set MSB
/// assert_eq!(set_soft_address(0x8CAE, false), 0x0CAE); // Clear MSB
/// ```
pub fn set_soft_address(id: u16, soft: bool) -> u16 {
    if soft {
        id | 0x8000  // Set MSB
    } else {
        id & 0x7FFF  // Clear MSB
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_encoding() {
        // Test CEN from M-Bus documentation
        assert_eq!(manufacturer_to_id("CEN"), Some(0x0CAE)); // 3246

        // Test known manufacturers with correct standard values
        assert_eq!(manufacturer_to_id("QDS"), Some(0x4493)); // 17555
        assert_eq!(manufacturer_to_id("ZEN"), Some(0x68AE)); // 26798
        assert_eq!(manufacturer_to_id("KAM"), Some(0x2C2D)); // 11309
        assert_eq!(manufacturer_to_id("ABB"), Some(0x0442)); // 1090

        // Test case insensitivity
        assert_eq!(manufacturer_to_id("kam"), Some(0x2C2D));
        assert_eq!(manufacturer_to_id("Kam"), Some(0x2C2D));
        assert_eq!(manufacturer_to_id("KAM"), Some(0x2C2D));
    }

    #[test]
    fn test_standard_decoding() {
        // Test CEN from M-Bus documentation
        assert_eq!(id_to_manufacturer(0x0CAE), "CEN");

        // Test known manufacturers
        assert_eq!(id_to_manufacturer(0x4493), "QDS");
        assert_eq!(id_to_manufacturer(0x68AE), "ZEN");
        assert_eq!(id_to_manufacturer(0x2C2D), "KAM");
        assert_eq!(id_to_manufacturer(0x0442), "ABB");
    }

    #[test]
    fn test_msb_handling() {
        // Test that MSB (soft address flag) is properly handled in decoding
        assert_eq!(id_to_manufacturer(0x0CAE), "CEN"); // Hard address (MSB=0)
        assert_eq!(id_to_manufacturer(0x8CAE), "CEN"); // Soft address (MSB=1)

        assert_eq!(id_to_manufacturer(0x2C2D), "KAM"); // Hard address
        assert_eq!(id_to_manufacturer(0xAC2D), "KAM"); // Soft address

        // Test MSB functions
        assert!(!is_soft_address(0x0CAE));
        assert!(is_soft_address(0x8CAE));

        assert_eq!(set_soft_address(0x0CAE, true), 0x8CAE);
        assert_eq!(set_soft_address(0x8CAE, false), 0x0CAE);
    }

    #[test]
    fn test_boundary_conditions() {
        // Test minimum and maximum valid values
        assert_eq!(manufacturer_to_id("AAA"), Some(0x0421)); // 1*1024 + 1*32 + 1 = 1057
        assert_eq!(manufacturer_to_id("ZZZ"), Some(0x6B5A)); // 26*1024 + 26*32 + 26 = 27482

        // Verify decoding
        assert_eq!(id_to_manufacturer(0x0421), "AAA");
        assert_eq!(id_to_manufacturer(0x6B5A), "ZZZ");
    }

    #[test]
    fn test_invalid_inputs() {
        // Test invalid manufacturer codes
        assert_eq!(manufacturer_to_id(""), None);
        assert_eq!(manufacturer_to_id("AB"), None);      // Too short
        assert_eq!(manufacturer_to_id("ABCD"), None);   // Too long
        assert_eq!(manufacturer_to_id("123"), None);    // Non-alphabetic
        assert_eq!(manufacturer_to_id("A1B"), None);    // Mixed alphanumeric
        assert_eq!(manufacturer_to_id("A-B"), None);    // Special characters

        // Test invalid IDs
        assert_eq!(id_to_manufacturer(0x0000), "UNK"); // All zeros
        assert_eq!(id_to_manufacturer(0x0420), "UNK"); // Below minimum (AAA-1)
        assert_eq!(id_to_manufacturer(0x6B5B), "UNK"); // Above maximum (ZZZ+1)
    }

    #[test]
    fn test_encode_decode_symmetry() {
        // Test that encoding and decoding are symmetric
        let test_codes = ["CEN", "QDS", "ZEN", "KAM", "AAA", "ZZZ", "ABC", "XYZ"];

        for code in &test_codes {
            let id = manufacturer_to_id(code).expect(&format!("Failed to encode {}", code));
            let decoded = id_to_manufacturer(id);
            assert_eq!(decoded, code.to_uppercase(),
                "Round-trip failed for {}: 0x{:04X} -> {}", code, id, decoded);
        }
    }

    #[test]
    fn test_known_manufacturers_database() {
        // Test CEN (reference implementation)
        let cen_info = get_manufacturer_info(0x0CAE).unwrap();
        assert_eq!(cen_info.code, "CEN");
        assert_eq!(cen_info.name, "Example Manufacturer");
        assert!(!cen_info.has_quirks);

        // Test QUNDIS (has quirks)
        let qundis_info = get_manufacturer_info(0x4493).unwrap();
        assert_eq!(qundis_info.code, "QDS");
        assert_eq!(qundis_info.name, "Qundis GmbH");
        assert!(qundis_info.has_quirks);
        assert!(qundis_info.description.is_some());

        // Test Kamstrup (no quirks)
        let kamstrup_info = get_manufacturer_info(0x2C2D).unwrap();
        assert_eq!(kamstrup_info.code, "KAM");
        assert_eq!(kamstrup_info.name, "Kamstrup");
        assert!(!kamstrup_info.has_quirks);
    }

    #[test]
    fn test_utility_functions() {
        // Test has_quirks
        assert!(has_quirks(0x4493));  // Qundis has quirks
        assert!(!has_quirks(0x68AE)); // Zenner no quirks
        assert!(!has_quirks(0x0000)); // Unknown no quirks

        // Test get_manufacturer_name
        assert_eq!(get_manufacturer_name(0x4493), "Qundis GmbH");
        assert_eq!(get_manufacturer_name(0x0CAE), "Example Manufacturer");
        assert_eq!(get_manufacturer_name(0x0000), "UNK");

        // Test is_valid_id
        assert!(is_valid_id(0x0CAE)); // CEN
        assert!(is_valid_id(0x4493)); // QDS
        assert!(is_valid_id(0x0421)); // AAA
        assert!(is_valid_id(0x6B5A)); // ZZZ
        assert!(!is_valid_id(0x0000)); // Invalid
        assert!(!is_valid_id(0x6B5B)); // Too large

        // Test with MSB set (should still be valid)
        assert!(is_valid_id(0x8CAE)); // CEN with soft address flag
    }

    #[test]
    fn test_database_consistency() {
        // Ensure all entries in database have valid IDs and match encoding
        for (&id, info) in KNOWN_MANUFACTURERS.iter() {
            // ID should be valid
            assert!(is_valid_id(id),
                "Invalid ID 0x{:04X} for manufacturer {}", id, info.code);

            // Encoding should produce the stored ID
            let encoded_id = manufacturer_to_id(info.code);
            assert_eq!(encoded_id, Some(id),
                "Encoding mismatch for {}: expected 0x{:04X}, got {:?}",
                info.code, id, encoded_id);

            // Decoding should produce the stored code
            assert_eq!(id_to_manufacturer(id), info.code,
                "Decoding mismatch for 0x{:04X}: expected {}", id, info.code);
        }
    }

    #[test]
    fn test_all_manufacturers() {
        // Test the iterator function
        let manufacturers: Vec<_> = all_manufacturers().collect();

        // Should have 30+ manufacturers after expansion
        assert!(manufacturers.len() >= 30,
            "Expected at least 30 manufacturers, found {}", manufacturers.len());

        // Verify specific manufacturers are present by category

        // HCA manufacturers
        assert!(manufacturers.iter().any(|(_, info)| info.code == "QDS"));
        assert!(manufacturers.iter().any(|(_, info)| info.code == "BHG"));
        assert!(manufacturers.iter().any(|(_, info)| info.code == "IST"));
        assert!(manufacturers.iter().any(|(_, info)| info.code == "TCH"));
        assert!(manufacturers.iter().any(|(_, info)| info.code == "ZRM"));

        // Water meter manufacturers
        assert!(manufacturers.iter().any(|(_, info)| info.code == "AMT"));
        assert!(manufacturers.iter().any(|(_, info)| info.code == "HYD"));
        assert!(manufacturers.iter().any(|(_, info)| info.code == "ZEN"));

        // Multi-utility manufacturers
        assert!(manufacturers.iter().any(|(_, info)| info.code == "ABB"));
        assert!(manufacturers.iter().any(|(_, info)| info.code == "ACW"));
        assert!(manufacturers.iter().any(|(_, info)| info.code == "KAM"));

        // Reference manufacturer
        assert!(manufacturers.iter().any(|(_, info)| info.code == "CEN"));

        // Only QUNDIS should have quirks
        let quirky_count = manufacturers.iter()
            .filter(|(_, info)| info.has_quirks)
            .count();
        assert_eq!(quirky_count, 1, "Only QUNDIS should have quirks");
    }

    #[test]
    fn test_new_manufacturers_encoding() {
        // Test newly added manufacturers have correct encoding
        let test_cases = [
            ("ACW", 0x0477),  // Actaris
            ("AMT", 0x05B4),  // Aquametro
            ("BHG", 0x0907),  // Brunata
            ("EMH", 0x15A8),  // EMH
            ("EMU", 0x15B5),  // EMU Electronic
            ("HYD", 0x2324),  // Hydrometer
            ("IST", 0x2674),  // ista
            ("NZR", 0x3B52),  // NZR
            ("PAD", 0x4024),  // PadMess
            ("REL", 0x48AC),  // Relay
            ("ZRM", 0x6A4D),  // Minol Zenner
        ];

        for (code, expected_id) in &test_cases {
            let encoded = manufacturer_to_id(code);
            assert_eq!(encoded, Some(*expected_id),
                "Encoding mismatch for {}: expected 0x{:04X}, got {:?}",
                code, expected_id, encoded);

            let decoded = id_to_manufacturer(*expected_id);
            assert_eq!(decoded, *code,
                "Decoding mismatch for 0x{:04X}: expected {}, got {}",
                expected_id, code, decoded);
        }
    }
}