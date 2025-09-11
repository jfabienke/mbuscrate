// Record parsing tests are pending API exposure; placeholders below.

#[test]
fn test_parse_fixed_record() {
    let data = [
        0x00, 0x00, 0x00, 0x00, // Device ID (BCD)
        0x21, 0x04, // Manufacturer (0x0421 - minimum valid)
        0x00, // Version
        0x00, // Medium
        0x00, // Access number
        0x00, // Status
        0x00, 0x00, // Signature
        0x00, 0x00, 0x00, 0x00, // Counter value
    ];
    let result = mbus_rs::payload::record::parse_fixed_record(&data);
    assert!(result.is_ok());
}

#[test]
fn test_parse_variable_record() {
    let data = [0x2F, 0x01, 0x00, 0x00]; // Idle filler, DIF, VIF, data
    let result = mbus_rs::payload::record::parse_variable_record(&data);
    assert!(result.is_ok());
}
