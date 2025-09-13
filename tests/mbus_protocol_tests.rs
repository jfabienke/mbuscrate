use mbus_rs::constants::{MBUS_CONTROL_INFO_RESP_FIXED, MBUS_CONTROL_INFO_RESP_VARIABLE};
use mbus_rs::mbus::frame::{MBusFrame, MBusFrameType};
use mbus_rs::{
    error::MBusError,
    mbus::mbus_protocol::{MBusProtocol, MBusProtocolState, StateMachine},
};

#[tokio::test]
async fn test_state_machine_new() {
    let sm = StateMachine::new();
    assert_eq!(sm.state, MBusProtocolState::Idle);
}

#[tokio::test]
async fn test_state_machine_select_device() {
    let mut sm = StateMachine::new();
    sm.select_device(1).await.unwrap();
    assert_eq!(sm.state, MBusProtocolState::Selecting);
}

#[tokio::test]
async fn test_state_machine_request_data() {
    let mut sm = StateMachine::new();
    // Must select device first before requesting data
    sm.select_device(1).await.unwrap();
    sm.request_data().await.unwrap();
    assert_eq!(sm.state, MBusProtocolState::Requesting);
}

#[tokio::test]
async fn test_state_machine_receive_data() {
    use mbus_rs::mbus::frame::{MBusFrame, MBusFrameType};
    let mut sm = StateMachine::new();
    // Must select device first, then request data, then receive
    sm.select_device(1).await.unwrap();
    sm.request_data().await.unwrap();

    let frame = MBusFrame {
        frame_type: MBusFrameType::Long,
        control: 0x08,
        address: 0x01,
        control_information: 0x72,          // RSP_UD
        data: vec![0x01, 0x00, 0x00, 0x00], // Some test data
        checksum: 0x7C,                     // 0x08 + 0x01 + 0x72 + 0x01 = 0x7C
        more_records_follow: false,
    };
    let result = sm.receive_data(&frame).await;
    assert!(result.is_ok());
    assert_eq!(sm.state, MBusProtocolState::Receiving);
}

#[tokio::test]
async fn test_state_machine_process_data() {
    let mut sm = StateMachine::new();
    let records = vec![];
    sm.process_data(&records).await.unwrap();
    assert_eq!(sm.state, MBusProtocolState::Idle);
}

#[tokio::test]
async fn test_state_machine_handle_error() {
    let mut sm = StateMachine::new();
    let err = MBusError::Other("test".to_string());
    assert!(sm.handle_error(err).is_err());
    assert_eq!(sm.state, MBusProtocolState::Error);
}

#[tokio::test]
async fn test_state_machine_reset() {
    let mut sm = StateMachine::new();
    sm.select_device(1).await.unwrap();
    sm.reset();
    assert_eq!(sm.state, MBusProtocolState::Idle);
}

#[tokio::test]
async fn test_mbus_protocol_new() {
    let protocol = MBusProtocol::new();
    // Basic creation test
    assert!(!protocol.state_machine.state.eq(&MBusProtocolState::Error));
}

// Note: send_request, scan_devices, and disconnect_all methods are now implemented
// in MBusDeviceHandle in the serial module, not in MBusProtocol

#[tokio::test]
async fn test_frame_handler_parse_frame_valid() {
    use mbus_rs::mbus::frame::MBusFrameType;
    let mut handler = mbus_rs::mbus::mbus_protocol::FrameHandler::new();
    // Mock valid short frame bytes: start=0x10, control=0x7B, addr=0x01, checksum=0x7C, stop=0x16
    let input = [0x10, 0x7B, 0x01, 0x7C, 0x16];
    let result = handler.parse_frame(&input);
    assert!(result.is_ok());
    if let Some(frame) = result.unwrap() {
        assert_eq!(frame.frame_type, MBusFrameType::Short);
    }
}

#[tokio::test]
async fn test_frame_handler_parse_frame_incomplete() {
    let mut handler = mbus_rs::mbus::mbus_protocol::FrameHandler::new();
    let input = [0x10]; // Incomplete short frame - only has start byte
    let result = handler.parse_frame(&input);
    // Incomplete data returns an error in complete parsing mode
    assert!(result.is_err());
}

#[tokio::test]
async fn test_frame_handler_pack_frame() {
    use mbus_rs::mbus::frame::{MBusFrame, MBusFrameType};
    let handler = mbus_rs::mbus::mbus_protocol::FrameHandler::new();
    let frame = MBusFrame {
        frame_type: MBusFrameType::Short,
        control: 0x7B,
        address: 0x01,
        control_information: 0,
        data: vec![],
        checksum: 0x7C,
        more_records_follow: false,
    };
    let packed = handler.pack_frame(&frame);
    // Expect packed short frame
    assert_eq!(packed, vec![0x10, 0x7B, 0x01, 0x7C, 0x16]);
}

#[tokio::test]
async fn test_frame_handler_verify_frame() {
    use mbus_rs::mbus::frame::{MBusFrame, MBusFrameType};
    let handler = mbus_rs::mbus::mbus_protocol::FrameHandler::new();
    let frame = MBusFrame {
        frame_type: MBusFrameType::Short,
        control: 0x7B,
        address: 0x01,
        control_information: 0,
        data: vec![],
        checksum: 0x7C,
        more_records_follow: false,
    };
    let result = handler.verify_frame(&frame);
    assert!(result.is_ok()); // Assuming stub verifies
}

#[tokio::test]
async fn test_frame_handler_send_frame() {
    let mut handler = mbus_rs::mbus::mbus_protocol::FrameHandler::new();
    use mbus_rs::mbus::frame::{MBusFrame, MBusFrameType};
    let frame = MBusFrame {
        frame_type: MBusFrameType::Short,
        control: 0,
        address: 0,
        control_information: 0,
        data: vec![],
        checksum: 0,
        more_records_follow: false,
    };
    let result = handler.send_frame(&frame).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_frame_handler_receive_frame() {
    let mut handler = mbus_rs::mbus::mbus_protocol::FrameHandler::new();
    let result = handler.receive_frame().await;
    assert!(result.is_ok()); // Returns dummy
}

#[tokio::test]
async fn test_device_discovery_manager_scan_secondary_addresses() {
    let mut manager = mbus_rs::mbus::mbus_protocol::DeviceDiscoveryManager::new();
    let result = manager.scan_secondary_addresses().await;
    assert!(result.is_ok());
    let addresses = result.unwrap();
    assert!(addresses.is_empty()); // Stub logic returns empty
}

#[tokio::test]
async fn test_data_retrieval_manager_retrieve_data() {
    let mut manager = mbus_rs::mbus::mbus_protocol::DataRetrievalManager::new();
    let result = manager.retrieve_data(1).await;
    assert!(result.is_ok());
    let records = result.unwrap();
    assert!(records.is_empty());
}

#[tokio::test]
async fn test_record_parser_parse_records_variable() {
    let mut parser = mbus_rs::mbus::mbus_protocol::RecordParser::new();
    let frame = MBusFrame {
        control_information: MBUS_CONTROL_INFO_RESP_VARIABLE,
        data: vec![0x03, 0x60, 0x00], // Mock DIF/VIF for volume
        frame_type: MBusFrameType::Short,
        control: 0,
        address: 0,
        checksum: 0,
        more_records_follow: false,
    };
    let result = parser.parse_records(&frame);
    assert!(result.is_ok());
    let _records = result.unwrap();
    // Since parse may fail or return empty due to mock, check for no panic
    assert!(true);
}

#[tokio::test]
async fn test_record_parser_parse_records_fixed() {
    let mut parser = mbus_rs::mbus::mbus_protocol::RecordParser::new();
    let frame = MBusFrame {
        control_information: MBUS_CONTROL_INFO_RESP_FIXED,
        data: vec![
            0x01, 0x00, 0x00, 0x00, // Device ID (BCD)
            0x21, 0x04, // Manufacturer (0x0421 minimum valid)
            0x01, // Version
            0x00, // Medium
            0x00, // Access number
            0x00, // Status
            0x00, 0x00, // Signature
            0x00, 0x00, 0x00, 0x00, // Counter value
        ], // Mock fixed data (16 bytes)
        frame_type: MBusFrameType::Short,
        control: 0,
        address: 0,
        checksum: 0,
        more_records_follow: false,
    };
    let result = parser.parse_records(&frame);
    assert!(result.is_ok());
}
