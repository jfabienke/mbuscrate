//! Unit tests for the `vif.rs` module, which includes the lookup and normalization of
//! VIF (Value Information Field) and VIFE (VIF Extension) information.

// VIF/VIFE helpers are not exposed; keep placeholders ignored until implemented.

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
    assert_eq!(info.unit, "Credit of 10nn-3 of the nominal local legal currency units");
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
