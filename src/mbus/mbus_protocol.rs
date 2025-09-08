//! # M-Bus Protocol Implementation
//!
//! This module provides the implementation of the M-Bus protocol, including the state machine,
//! command handling, device discovery, and data retrieval functionality.

use crate::error::MBusError;
use crate::mbus::frame::{MBusFrame, MBusFrameType};
use crate::mbus::frame as frame;
use crate::payload::record::MBusRecord;

/// Represents the different states of the M-Bus protocol state machine.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MBusProtocolState {
    Idle,
    Selecting,
    Requesting,
    Receiving,
    Error,
}

/// Represents the M-Bus protocol state machine.
pub struct StateMachine {
    state: MBusProtocolState,
}

impl StateMachine {
    /// Creates a new instance of the M-Bus protocol state machine.
    pub fn new() -> Self {
        StateMachine {
            state: MBusProtocolState::Idle,
        }
    }

    pub async fn select_device(&mut self, _address: u8) -> Result<(), MBusError> {
        // TODO: Implement device selection logic
        self.state = MBusProtocolState::Selecting;
        Ok(())
    }

    pub async fn request_data(&mut self) -> Result<(), MBusError> {
        // TODO: Implement data request logic
        self.state = MBusProtocolState::Requesting;
        Ok(())
    }

    pub async fn receive_data(&mut self) -> Result<(), MBusError> {
        // TODO: Implement data reception logic
        self.state = MBusProtocolState::Receiving;
        Ok(())
    }

    pub async fn process_data(&mut self, _records: &[MBusRecord]) -> Result<(), MBusError> {
        // TODO: Implement data processing logic
        self.state = MBusProtocolState::Idle;
        Ok(())
    }

    pub fn handle_error(&mut self, error: MBusError) -> Result<(), MBusError> {
        // TODO: Implement error handling logic
        self.state = MBusProtocolState::Error;
        Err(error)
    }

    pub fn reset(&mut self) {
        self.state = MBusProtocolState::Idle;
    }
}

/// Represents the M-Bus protocol implementation.
pub struct MBusProtocol {
    state_machine: StateMachine,
    frame_handler: FrameHandler,
    discovery_manager: DeviceDiscoveryManager,
    data_retrieval_manager: DataRetrievalManager,
}

impl MBusProtocol {
    /// Creates a new instance of the M-Bus protocol implementation.
    pub fn new() -> Self {
        MBusProtocol {
            state_machine: StateMachine::new(),
            frame_handler: FrameHandler::new(),
            discovery_manager: DeviceDiscoveryManager::new(),
            data_retrieval_manager: DataRetrievalManager::new(),
        }
    }

    /// Sends a request to an M-Bus device and collects the responses.
    pub async fn send_request(&mut self, address: u8) -> Result<Vec<MBusRecord>, MBusError> {
        self.state_machine.select_device(address).await?;
        self.state_machine.request_data().await?;

        let records = self.data_retrieval_manager.retrieve_data(address).await?;

        self.state_machine.receive_data().await?;
        self.state_machine.process_data(&records).await?;

        Ok(records)
    }

    /// Scans for available M-Bus devices and returns their addresses.
    pub async fn scan_devices(&mut self) -> Result<Vec<u8>, MBusError> {
        self.state_machine.select_device(0).await?;
        let addresses = self.discovery_manager.scan_secondary_addresses().await?;
        self.state_machine.receive_data().await?;
        self.state_machine.process_data(&[]).await?;
        Ok(addresses)
    }

    /// Disconnects from all connected M-Bus devices.
    pub async fn disconnect_all(&mut self) -> Result<(), MBusError> {
        self.state_machine.select_device(0).await?;
        self.frame_handler.disconnect_all().await?;
        self.state_machine.reset();
        Ok(())
    }
}

/// Handles the processing of M-Bus frames.
pub struct FrameHandler {
    frame_cache: Vec<MBusFrame>,
}

impl FrameHandler {
    /// Creates a new instance of the FrameHandler.
    pub fn new() -> Self {
        FrameHandler {
            frame_cache: Vec::new(),
        }
    }

    /// Parses an M-Bus frame from the input data.
    pub fn parse_frame(&mut self, input: &[u8]) -> Result<Option<MBusFrame>, MBusError> {
        match crate::mbus::frame::parse_frame(input) {
            Ok((remaining, frame)) => {
                if remaining.is_empty() {
                    Ok(Some(frame))
                } else {
                    self.frame_cache.push(frame);
                    Ok(None)
                }
            }
            Err(nom::Err::Incomplete(_)) => {
                // Need more data to parse the frame
                Ok(None)
            }
            Err(err) => Err(MBusError::FrameParseError(err.to_string())),
        }
    }

    /// Packs an M-Bus frame for transmission.
    pub fn pack_frame(&self, frame: &MBusFrame) -> Vec<u8> {
        crate::mbus::frame::pack_frame(frame)
    }

    /// Verifies the integrity of an M-Bus frame.
    pub fn verify_frame(&self, frame: &MBusFrame) -> Result<(), MBusError> {
        crate::mbus::frame::verify_frame(frame)
    }

    /// Disconnects from all connected M-Bus devices.
    pub async fn disconnect_all(&mut self) -> Result<(), MBusError> {
        // Implement the logic for disconnecting from all devices
        Ok(())
    }

    /// Sends a frame (stub).
    pub async fn send_frame(&mut self, _frame: &MBusFrame) -> Result<(), MBusError> {
        Ok(())
    }

    /// Receives a frame (stub).
    pub async fn receive_frame(&mut self) -> Result<MBusFrame, MBusError> {
        // Return a dummy frame
        Ok(MBusFrame {
            frame_type: MBusFrameType::Short,
            control: 0,
            address: 0,
            control_information: 0,
            data: vec![],
            checksum: 0,
            more_records_follow: false,
        })
    }
}

/// Manages the discovery of M-Bus devices.
pub struct DeviceDiscoveryManager {
    frame_handler: FrameHandler,
}

impl DeviceDiscoveryManager {
    /// Creates a new instance of the DeviceDiscoveryManager.
    pub fn new() -> Self {
        DeviceDiscoveryManager {
            frame_handler: FrameHandler::new(),
        }
    }

    /// Scans for available M-Bus devices using the secondary address selection mechanism.
    pub async fn scan_secondary_addresses(&mut self) -> Result<Vec<u8>, MBusError> {
        // Depth-first mask probing: start with all wildcards and split on first 'F'.
        let mut found = Vec::new();
        let mut stack = vec!["FFFFFFFFFFFFFFFF".to_string()];

        while let Some(mask) = stack.pop() {
            match self.select_secondary_address(&mask).await? {
                ProbeResult::Single(addr) => found.push(addr),
                ProbeResult::Nothing => continue,
                ProbeResult::Collision => {
                    // Split on first 'F' into 16 submasks
                    if let Some(pos) = mask.find('F') {
                        for nib in [
                            '0','1','2','3','4','5','6','7','8','9','A','B','C','D','E','F'
                        ] {
                            let mut sub = mask.clone();
                            sub.replace_range(pos..=pos, &nib.to_string());
                            stack.push(sub);
                        }
                    }
                }
            }
        }

        Ok(found)
    }

    /// Selects a secondary address for an M-Bus device.
    async fn select_secondary_address(&mut self, mask: &str) -> Result<ProbeResult, MBusError> {
        // Send select frame targeting secondary address mask.
        let frame = self.create_select_frame(mask)?;
        self.frame_handler.send_frame(&frame).await?;

        // Expect ACK on success; timeout or short means nothing, long indicates collision/noise.
        let resp1 = self.frame_handler.receive_frame().await;
        let resp1 = match resp1 {
            Ok(f) => f,
            Err(_) => return Ok(ProbeResult::Nothing),
        };
        if resp1.frame_type != MBusFrameType::Ack {
            return Ok(ProbeResult::Collision);
        }

        // Send REQ_UD2 to network-layer address to fetch one response,
        // and treat a single long response as a single device match.
        let req = frame::MBusFrame {
            frame_type: MBusFrameType::Short,
            control: crate::constants::MBUS_CONTROL_MASK_REQ_UD2,
            address: crate::constants::MBUS_ADDRESS_NETWORK_LAYER,
            control_information: 0,
            data: vec![],
            checksum: (crate::constants::MBUS_CONTROL_MASK_REQ_UD2)
                .wrapping_add(crate::constants::MBUS_ADDRESS_NETWORK_LAYER),
            more_records_follow: false,
        };
        self.frame_handler.send_frame(&req).await?;
        let resp2 = self.frame_handler.receive_frame().await;
        let resp2 = match resp2 {
            Ok(f) => f,
            Err(_) => return Ok(ProbeResult::Collision),
        };
        if matches!(resp2.frame_type, MBusFrameType::Long | MBusFrameType::Control) {
            return Ok(ProbeResult::Single(resp2.address));
        }
        Ok(ProbeResult::Collision)
    }

    /// Creates a select frame for the given secondary address mask.
    fn create_select_frame(&self, mask: &str) -> Result<MBusFrame, MBusError> {
        let mut frame = MBusFrame {
            frame_type: MBusFrameType::Long,
            control: 0,
            address: 0,
            control_information: 0,
            data: vec![],
            checksum: 0,
            more_records_follow: false,
        };
        crate::mbus::frame::pack_select_frame(&mut frame, mask)?;
        Ok(frame)
    }
}

enum ProbeResult {
    Single(u8),
    Collision,
    Nothing,
}


/// Manages the retrieval of data from M-Bus devices.
pub struct DataRetrievalManager {
    frame_handler: FrameHandler,
    record_parser: RecordParser,
}

impl DataRetrievalManager {
    /// Creates a new instance of the DataRetrievalManager.
    pub fn new() -> Self {
        DataRetrievalManager {
            frame_handler: FrameHandler::new(),
            record_parser: RecordParser::new(),
        }
    }

    /// Retrieves data from an M-Bus device.
    pub async fn retrieve_data(&mut self, address: u8) -> Result<Vec<MBusRecord>, MBusError> {
        let request_frame = self.create_request_frame(address)?;
        self.frame_handler.send_frame(&request_frame).await?;

        let mut records = Vec::new();
        loop {
            let response_frame = self.frame_handler.receive_frame().await?;
            let new_records = self.record_parser.parse_records(&response_frame)?;
            records.extend(new_records);

            if !response_frame.more_records_follow {
                break;
            }
        }

        Ok(records)
    }

    /// Creates a request frame for the given M-Bus device address.
    fn create_request_frame(&self, address: u8) -> Result<MBusFrame, MBusError> {
        use crate::constants::MBUS_CONTROL_MASK_REQ_UD2;
        // Short request frame: 0x10 | control | address | checksum | 0x16
        let mut frame = MBusFrame {
            frame_type: MBusFrameType::Short,
            control: MBUS_CONTROL_MASK_REQ_UD2,
            address,
            control_information: 0,
            data: vec![],
            checksum: 0,
            more_records_follow: false,
        };
        // checksum for short frames = control + address
        frame.checksum = frame.control.wrapping_add(frame.address);
        Ok(frame)
    }
}

/// Parses M-Bus data records from the received frames.
pub struct RecordParser {
    // Implementation omitted for brevity
}

impl RecordParser {
    pub fn new() -> Self {
        RecordParser {}
    }
    /// Parses M-Bus data records from the given frame.
    pub fn parse_records(&mut self, frame: &MBusFrame) -> Result<Vec<MBusRecord>, MBusError> {
        use crate::constants::{
            MBUS_CONTROL_INFO_RESP_FIXED, MBUS_CONTROL_INFO_RESP_VARIABLE,
        };

        let mut out = Vec::new();
        match frame.control_information {
            MBUS_CONTROL_INFO_RESP_VARIABLE => {
                // Parse variable format from frame.data
                let mut remaining = &frame.data[..];
                while !remaining.is_empty() {
                    if remaining[0] == crate::constants::MBUS_DIB_DIF_IDLE_FILLER {
                        remaining = &remaining[1..];
                        continue;
                    }
                    match crate::payload::record::parse_variable_record(remaining) {
                        Ok(record) => {
                            out.push(record);
                            // For simplicity, assume one record per frame or handle length properly
                            // In full implementation, update remaining based on parsed length
                            break; // Placeholder: parse until end properly
                        }
                        Err(e) => {
                            crate::logging::log_error(&format!("Error parsing variable record: {:?}", e));
                            break;
                        }
                    }
                }
            }
            MBUS_CONTROL_INFO_RESP_FIXED => {
                // Parse fixed format from frame.data
                let record = crate::payload::record::parse_fixed_record(&frame.data)?;
                out.push(record);
            }
            _ => {}
        }
        Ok(out)
    }
}
