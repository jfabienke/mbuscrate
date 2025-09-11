use mbus_rs::mbus::frame::{
    pack_frame, pack_select_frame, parse_frame, verify_frame, MBusFrame, MBusFrameType,
};

#[test]
fn test_pack_select_frame() {
    let mut frame = MBusFrame {
        frame_type: MBusFrameType::Long,
        control: 0x53,
        address: 0x01,
        control_information: 0x00,
        data: vec![],
        checksum: 0,
        more_records_follow: false,
    };

    // Test valid mask (16 hex digits)
    let result = pack_select_frame(&mut frame, "1234567890ABCDEF");
    assert!(result.is_ok());
    assert_eq!(frame.data.len(), 8); // Should have packed address data

    // Test invalid mask (too short)
    let mut frame2 = MBusFrame {
        frame_type: MBusFrameType::Long,
        control: 0x53,
        address: 0x01,
        control_information: 0x00,
        data: vec![],
        checksum: 0,
        more_records_follow: false,
    };
    let result = pack_select_frame(&mut frame2, "12345");
    assert!(result.is_err());

    // Test mask with wildcards (16 hex digits with F's)
    let mut frame3 = MBusFrame {
        frame_type: MBusFrameType::Long,
        control: 0x53,
        address: 0x01,
        control_information: 0x00,
        data: vec![],
        checksum: 0,
        more_records_follow: false,
    };
    let result = pack_select_frame(&mut frame3, "12345678FFFFFFFF");
    assert!(result.is_ok());
}

#[test]
fn test_parse_frame_errors() {
    // Empty input
    let result = parse_frame(&[]);
    assert!(result.is_err());

    // Invalid start byte
    let result = parse_frame(&[0xFF, 0x00, 0x00]);
    assert!(result.is_err());

    // Incomplete short frame
    let result = parse_frame(&[0x10, 0x53]);
    assert!(result.is_err());

    // Incomplete long frame
    let result = parse_frame(&[0x68, 0x03]);
    assert!(result.is_err());
}

#[test]
fn test_verify_frame_checksum() {
    // Test ACK frame (no checksum verification)
    let ack_frame = MBusFrame {
        frame_type: MBusFrameType::Ack,
        control: 0,
        address: 0,
        control_information: 0,
        data: vec![],
        checksum: 0,
        more_records_follow: false,
    };
    assert!(verify_frame(&ack_frame).is_ok());

    // Test short frame with correct checksum
    let short_frame = MBusFrame {
        frame_type: MBusFrameType::Short,
        control: 0x53,
        address: 0x01,
        control_information: 0,
        data: vec![],
        checksum: 0x54, // Correct: 0x53 + 0x01 = 0x54
        more_records_follow: false,
    };
    assert!(verify_frame(&short_frame).is_ok());

    // Test short frame with incorrect checksum
    let bad_short_frame = MBusFrame {
        frame_type: MBusFrameType::Short,
        control: 0x53,
        address: 0x01,
        control_information: 0,
        data: vec![],
        checksum: 0x55, // Wrong checksum
        more_records_follow: false,
    };
    assert!(verify_frame(&bad_short_frame).is_err());

    // Test control frame
    let control_frame = MBusFrame {
        frame_type: MBusFrameType::Control,
        control: 0x53,
        address: 0x01,
        control_information: 0x00,
        data: vec![],
        checksum: 0x54, // 0x53 + 0x01 + 0x00 = 0x54
        more_records_follow: false,
    };
    assert!(verify_frame(&control_frame).is_ok());

    // Test long frame with data
    let long_frame = MBusFrame {
        frame_type: MBusFrameType::Long,
        control: 0x53,
        address: 0x01,
        control_information: 0x00,
        data: vec![0x01, 0x02, 0x03],
        checksum: 0x5A, // 0x53 + 0x01 + 0x00 + 0x01 + 0x02 + 0x03 = 0x5A
        more_records_follow: false,
    };
    assert!(verify_frame(&long_frame).is_ok());
}

#[test]
fn test_pack_frame_various_types() {
    // Test packing ACK frame
    let ack_frame = MBusFrame {
        frame_type: MBusFrameType::Ack,
        control: 0,
        address: 0,
        control_information: 0,
        data: vec![],
        checksum: 0,
        more_records_follow: false,
    };
    let packed = pack_frame(&ack_frame);
    assert_eq!(packed, vec![0xE5]);

    // Test packing Short frame
    let short_frame = MBusFrame {
        frame_type: MBusFrameType::Short,
        control: 0x53,
        address: 0x01,
        control_information: 0,
        data: vec![],
        checksum: 0x54,
        more_records_follow: false,
    };
    let packed = pack_frame(&short_frame);
    assert_eq!(packed, vec![0x10, 0x53, 0x01, 0x54, 0x16]);

    // Test packing Control frame
    let control_frame = MBusFrame {
        frame_type: MBusFrameType::Control,
        control: 0x53,
        address: 0x01,
        control_information: 0x00,
        data: vec![],
        checksum: 0x54,
        more_records_follow: false,
    };
    let packed = pack_frame(&control_frame);
    assert_eq!(
        packed,
        vec![0x68, 0x03, 0x03, 0x68, 0x53, 0x01, 0x00, 0x54, 0x16]
    );

    // Test packing Long frame with data
    let long_frame = MBusFrame {
        frame_type: MBusFrameType::Long,
        control: 0x53,
        address: 0x01,
        control_information: 0x00,
        data: vec![0x01, 0x02, 0x03, 0x04, 0x05],
        checksum: 0x5F,
        more_records_follow: false,
    };
    let packed = pack_frame(&long_frame);
    let expected = vec![
        0x68, 0x08, 0x08, 0x68, // Start bytes and length
        0x53, 0x01, 0x00, // Control, address, CI
        0x01, 0x02, 0x03, 0x04, 0x05, // Data
        0x5F, 0x16, // Checksum and stop
    ];
    assert_eq!(packed, expected);
}

#[test]
fn test_parse_frame_with_extra_bytes() {
    // Frame with trailing bytes (should be in remaining)
    let frame_data = &[0xE5, 0xFF, 0xFF];
    let (remaining, frame) = parse_frame(frame_data).unwrap();
    assert_eq!(frame.frame_type, MBusFrameType::Ack);
    assert_eq!(remaining, &[0xFF, 0xFF]);
}

#[test]
fn test_long_frame_length_mismatch() {
    // Long frame with mismatched length fields
    let frame_data = &[0x68, 0x03, 0x04, 0x68, 0x53, 0x01, 0x00, 0x54, 0x16];
    let result = parse_frame(frame_data);
    assert!(result.is_err());
}

#[test]
fn test_frame_max_length() {
    // Test maximum frame size (252 bytes of data - max that fits in u8 length field)
    let data = vec![0x01; 252];
    let long_frame = MBusFrame {
        frame_type: MBusFrameType::Long,
        control: 0x53,
        address: 0x01,
        control_information: 0x00,
        data: data.clone(),
        checksum: 0, // Will be calculated
        more_records_follow: false,
    };

    let packed = pack_frame(&long_frame);
    // Check header
    assert_eq!(packed[0], 0x68); // Start
    assert_eq!(packed[1], 0xFF); // Length (252 + 3 = 255)
    assert_eq!(packed[2], 0xFF); // Length repeated
    assert_eq!(packed[3], 0x68); // Start repeated

    // Parse it back
    let (_, parsed) = parse_frame(&packed).unwrap();
    assert_eq!(parsed.data.len(), 252);
}

#[test]
fn test_empty_data_frames() {
    // Control frame with empty data is valid
    let control_frame = MBusFrame {
        frame_type: MBusFrameType::Control,
        control: 0x53,
        address: 0x01,
        control_information: 0x00,
        data: vec![],
        checksum: 0x54,
        more_records_follow: false,
    };

    let packed = pack_frame(&control_frame);
    let (_, parsed) = parse_frame(&packed).unwrap();
    assert_eq!(parsed.data.len(), 0);
    assert_eq!(parsed.frame_type, MBusFrameType::Control);
}
