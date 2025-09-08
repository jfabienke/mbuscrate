//! Unit tests for the `vif.rs` module, which includes the lookup and normalization of
//! VIF (Value Information Field) and VIFE (VIF Extension) information.
// VIF/VIFE helpers are not exposed; keep placeholders ignored until implemented.
///
/// Tests that the VIF information is correctly looked up.
#[test]
fn test_lookup_vif() {
    let vif_info = mbus_rs::payload::vif_maps::lookup_primary_vif(0x00);
    assert!(vif_info.is_some());
    let info = vif_info.unwrap();
    assert_eq!(info.unit, "Wh");
    assert_eq!(info.quantity, "Energy");
    assert_eq!(info.exponent, 1e-3);
}

/// Tests that the VIFE information is correctly looked up.
#[test]
fn test_lookup_vife() {
    let vife_info = mbus_rs::payload::vif_maps::lookup_vife_fd(0x00);
    assert!(vife_info.is_some());
    let info = vife_info.unwrap();
    assert_eq!(
        info.unit,
        "Credit of 10nn-3 of the nominal local legal currency units"
    );
    assert_eq!(info.quantity, "Credit");
}

/// Tests that the VIB normalization works as expected.
#[test]
fn test_normalize_vib() {
    let vib = vec![mbus_rs::payload::vif_maps::lookup_primary_vif(0x00).unwrap()];
    let result = mbus_rs::payload::vif::normalize_vib(&vib);
    assert!(result.is_ok());
    let (unit, _value, quantity) = result.unwrap();
    assert_eq!(unit, "Wh");
    assert_eq!(quantity, "Energy");
}

/// Tests VIFE parsing with edge cases, such as extensions beyond standard 0xFF (handled via FD/FB chaining).
/// Includes invalid codes and multi-extension chains.
#[test]
fn test_vife_parsing_edge_cases() {
    // Test invalid VIF (should return None for undefined, but per maps, check manufacturer specific)
    let invalid_vif = mbus_rs::payload::vif_maps::lookup_primary_vif(0xFE);
    assert!(invalid_vif.is_some()); // As per maps, 0xFE is Any VIF

    // Test VIFE FD with high value (e.g., 0xFF, assuming lookup handles it as None)
    let high_fd = mbus_rs::payload::vif_maps::lookup_vife_fd(0xFF);
    assert!(high_fd.is_none()); // Beyond defined codes in FD

    // Test VIFE FB with high value (empty map, so None)
    let high_fb = mbus_rs::payload::vif_maps::lookup_vife_fb(0xFF);
    assert!(high_fb.is_none());

    // Test chained VIFEs: Primary + FD extension (simulate extensions beyond single byte)
    use mbus_rs::payload::vif::parse_vib;
    // Mock input for VIF 0xFD (FD extension) followed by VIFE 0x08 (access number)
    let mock_input_fd = [0xFD, 0x08];
    let (_, vib_fd) = parse_vib(&mock_input_fd).expect("Parse should succeed for valid chain");
    assert_eq!(vib_fd.len(), 2);
    assert_eq!(vib_fd[0].vif, 0xFD as u16);
    assert_eq!(vib_fd[1].quantity, "Transmission Count"); // From lookup_vife_fd(0x08)

    // Edge case: Invalid extension bit without valid VIFE (e.g., FD with undefined VIFE)
    let invalid_chain = [0xFD, 0xFF]; // FD with invalid VIFE 0xFF
    let result_invalid = parse_vib(&invalid_chain);
    assert!(result_invalid.is_err()); // Should fail if lookup returns None and no fallback

    // Test extensions beyond 0xFF via multi-byte simulation (e.g., FB for voltage, but map empty; add fallback if needed)
    // For now, test that parse handles empty lookup gracefully (returns Err)
    let fb_mock = [0xFB, 0x40]; // FB extension for voltage
    parse_vib(&fb_mock).expect_err("Should err on undefined FB");
    // Note: Current impl may need enhancement for dynamic FB calculation (e.g., exponent from code)
}