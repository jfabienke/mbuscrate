// Record parsing tests are pending API exposure; placeholders below.

use mbus_rs;

#[test]
fn test_parse_fixed_record() {
    let data = [0u8; 16]; // Minimal fixed data
    let result = mbus_rs::payload::record::parse_fixed_record(&data);
    assert!(result.is_ok());
}

#[test]
fn test_parse_variable_record() {
    let data = [0x2F, 0x01, 0x00, 0x00]; // Idle filler, DIF, VIF, data
    let result = mbus_rs::payload::record::parse_variable_record(&data);
    assert!(result.is_ok());
}

