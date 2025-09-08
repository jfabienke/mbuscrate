//! Unit tests for the `frame.rs` module, which includes the parsing, packing, and verification of M-Bus frames.

use mbus_rs::mbus::frame::{pack_frame, parse_frame, verify_frame, MBusFrame, MBusFrameType};

/// Tests that an ACK frame is correctly parsed.
#[test]
fn test_parse_ack_frame() {
    let frame_data = &[0xE5];
    let (_, frame) = parse_frame(frame_data).unwrap();
    assert_eq!(frame.frame_type, MBusFrameType::Ack);
    assert_eq!(frame.control, 0);
    assert_eq!(frame.address, 0);
    assert_eq!(frame.control_information, 0);
    assert_eq!(frame.data, Vec::new());
    assert_eq!(frame.checksum, 0);
}

/// Tests that a Short frame is correctly parsed.
#[test]
fn test_parse_short_frame() {
    let frame_data = &[0x10, 0x53, 0x01, 0x54, 0x16];
    let (_, frame) = parse_frame(frame_data).unwrap();
    assert_eq!(frame.frame_type, MBusFrameType::Short);
    assert_eq!(frame.control, 0x53);
    assert_eq!(frame.address, 0x01);
    assert_eq!(frame.control_information, 0);
    assert_eq!(frame.data, Vec::new());
    assert_eq!(frame.checksum, 0x54);
}

/// Tests that a Control frame is correctly parsed.
#[test]
fn test_parse_control_frame() {
    let frame_data = &[0x68, 0x03, 0x03, 0x68, 0x53, 0x01, 0x00, 0x54, 0x16];
    let (_, frame) = parse_frame(frame_data).unwrap();
    assert_eq!(frame.frame_type, MBusFrameType::Control);
    assert_eq!(frame.control, 0x53);
    assert_eq!(frame.address, 0x01);
    assert_eq!(frame.control_information, 0x00);
    assert_eq!(frame.data, Vec::new());
    assert_eq!(frame.checksum, 0x54);
}

/// Tests that a Long frame is correctly parsed.
#[test]
fn test_parse_long_frame() {
    let frame_data = &[
        0x68, 0x08, 0x08, 0x68, 0x53, 0x01, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x63, 0x16,
    ];
    let (_, frame) = parse_frame(frame_data).unwrap();
    assert_eq!(frame.frame_type, MBusFrameType::Long);
    assert_eq!(frame.control, 0x53);
    assert_eq!(frame.address, 0x01);
    assert_eq!(frame.control_information, 0x00);
    assert_eq!(frame.data, &[0x01, 0x02, 0x03, 0x04, 0x05]);
    assert_eq!(frame.checksum, 0x63);
}

/// Tests that an ACK frame is correctly packed.
#[test]
fn test_pack_ack_frame() {
    let frame = MBusFrame {
        frame_type: MBusFrameType::Ack,
        control: 0,
        address: 0,
        control_information: 0,
        data: Vec::new(),
        checksum: 0,
        more_records_follow: false,
    };
    let packed_data = pack_frame(&frame);
    assert_eq!(packed_data, &[0xE5]);
}

/// Tests that a Short frame is correctly packed.
#[test]
fn test_pack_short_frame() {
    let frame = MBusFrame {
        frame_type: MBusFrameType::Short,
        control: 0x53,
        address: 0x01,
        control_information: 0,
        data: Vec::new(),
        checksum: 0x54,
        more_records_follow: false,
    };
    let packed_data = pack_frame(&frame);
    assert_eq!(packed_data, &[0x10, 0x53, 0x01, 0x54, 0x16]);
}

/// Tests that a Control frame is correctly packed.
#[test]
fn test_pack_control_frame() {
    let frame = MBusFrame {
        frame_type: MBusFrameType::Control,
        control: 0x53,
        address: 0x01,
        control_information: 0x00,
        data: Vec::new(),
        checksum: 0x54,
        more_records_follow: false,
    };
    let packed_data = pack_frame(&frame);
    assert_eq!(packed_data, &[0x68, 0x03, 0x03, 0x68, 0x53, 0x01, 0x00, 0x54, 0x16]);
}

/// Tests that a Long frame is correctly packed.
#[test]
fn test_pack_long_frame() {
    let frame = MBusFrame {
        frame_type: MBusFrameType::Long,
        control: 0x53,
        address: 0x01,
        control_information: 0x00,
        data: vec![0x01, 0x02, 0x03, 0x04, 0x05],
        checksum: 0x63,
        more_records_follow: false,
    };
    let packed_data = pack_frame(&frame);
    assert_eq!(
        packed_data,
        &[0x68, 0x08, 0x08, 0x68, 0x53, 0x01, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x63, 0x16]
    );
}

/// Tests that an ACK frame is correctly verified.
#[test]
fn test_verify_ack_frame() {
    let frame = MBusFrame {
        frame_type: MBusFrameType::Ack,
        control: 0,
        address: 0,
        control_information: 0,
        data: Vec::new(),
        checksum: 0,
        more_records_follow: false,
    };
    assert!(verify_frame(&frame).is_ok());
}

/// Tests that a Short frame is correctly verified.
#[test]
fn test_verify_short_frame() {
    let frame = MBusFrame {
        frame_type: MBusFrameType::Short,
        control: 0x53,
        address: 0x01,
        control_information: 0,
        data: Vec::new(),
        checksum: 0x54,
        more_records_follow: false,
    };
    assert!(verify_frame(&frame).is_ok());
}

/// Tests that a Control frame is correctly verified.
#[test]
fn test_verify_control_frame() {
    let frame = MBusFrame {
        frame_type: MBusFrameType::Control,
        control: 0x53,
        address: 0x01,
        control_information: 0x00,
        data: Vec::new(),
        checksum: 0x54,
        more_records_follow: false,
    };
    assert!(verify_frame(&frame).is_ok());
}

/// Tests that a Long frame is correctly verified.
#[test]
fn test_verify_long_frame() {
    let frame = MBusFrame {
        frame_type: MBusFrameType::Long,
        control: 0x53,
        address: 0x01,
        control_information: 0x00,
        data: vec![0x01, 0x02, 0x03, 0x04, 0x05],
        checksum: 0x63,
        more_records_follow: false,
    };
    assert!(verify_frame(&frame).is_ok());
}
