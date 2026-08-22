#[test]
fn an_overlong_dife_chain_is_refused_not_silently_truncated() {
    use mbus_rs::payload::record::parse_variable_record;
    // DIF with the extension bit, then 11 DIFEs that all set it, then a VIF and data.
    // The standard permits at most 10 DIFEs. The live parser stops counting at 10 and
    // then reads the 11th DIFE *as the VIF*, so every field after it is misinterpreted —
    // it does not fail, it produces a confident wrong answer.
    let mut frame = vec![0x84u8]; // DIF, extension bit set
    frame.extend(std::iter::repeat_n(0x80u8, 11)); // 11 DIFEs, all extending
    frame.push(0x13); // VIF: volume
    frame.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]);

    let result = parse_variable_record(&frame);
    assert!(
        result.is_err(),
        "a DIFE chain longer than the standard permits must be refused, got {result:?}"
    );
}

#[test]
fn an_overlong_vife_chain_is_refused_too() {
    use mbus_rs::payload::record::parse_variable_record;
    // Same shape on the VIF side: a VIF with the extension bit, then 11 VIFEs that all
    // extend. The 11th would be read as the first data byte.
    let mut frame = vec![0x04u8]; // DIF: 32-bit integer, no extension
    frame.push(0x93); // VIF with extension bit
    frame.extend(std::iter::repeat_n(0x80u8, 11)); // 11 VIFEs, all extending
    frame.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]);

    let result = parse_variable_record(&frame);
    assert!(
        result.is_err(),
        "a VIFE chain longer than the standard permits must be refused, got {result:?}"
    );
}
