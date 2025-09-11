//! # M-Bus Protocol Implementation
//!
//! This module provides the implementation of the M-Bus protocol, including the state machine,
//! command handling, device discovery, and data retrieval functionality.

use crate::error::MBusError;
use crate::mbus::frame;
use crate::mbus::frame::{MBusFrame, MBusFrameType};
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
    pub state: MBusProtocolState,
    /// Frame Count Bit (FCB) for frame sequencing in multi-frame communications
    pub fcb: bool,
    /// Current device address for communication
    pub current_address: u8,
}

impl Default for StateMachine {
    fn default() -> Self {
        StateMachine {
            state: MBusProtocolState::Idle,
            fcb: false,
            current_address: 0,
        }
    }
}

impl StateMachine {
    /// Creates a new instance of the M-Bus protocol state machine.
    pub fn new() -> Self {
        StateMachine {
            state: MBusProtocolState::Idle,
            fcb: false,
            current_address: 0,
        }
    }

    /// Selects a device using primary addressing (direct) or secondary addressing (selection sequence).
    /// 
    /// # Arguments
    /// * `address` - For primary addressing (1-250): the device's primary address
    ///               For secondary addressing: use 253, requires prior secondary address selection
    /// 
    /// # Returns
    /// * `Ok(())` - Device successfully selected or primary address validated
    /// * `Err(MBusError)` - Invalid address range or selection failed
    pub async fn select_device(&mut self, address: u8) -> Result<(), MBusError> {
        self.state = MBusProtocolState::Selecting;
        
        // Validate address range according to M-Bus specification
        match address {
            1..=250 => {
                // Primary addressing - direct communication
                // No selection frame needed, device responds to its primary address
                self.current_address = address;
                self.fcb = false; // Reset FCB for new device
                Ok(())
            }
            253 => {
                // Selected device address - used after secondary addressing selection
                // This would have been set by a prior select_device_by_secondary_address call
                self.current_address = address;
                Ok(())
            }
            0 => {
                // Address 0 is for unconfigured slaves - not recommended for normal operation
                Err(MBusError::Other("Address 0 is for unconfigured devices".to_string()))
            }
            254 => {
                // Test address - causes all devices to respond (collision)
                Err(MBusError::Other("Address 254 is test address, causes collisions".to_string()))
            }
            255 => {
                // Broadcast address - no device replies
                Err(MBusError::Other("Address 255 is broadcast, no replies expected".to_string()))
            }
            _ => {
                // Invalid address
                Err(MBusError::Other("Invalid M-Bus address".to_string()))
            }
        }
    }

    /// Selects a device using secondary addressing (8-byte unique identifier).
    /// After successful selection, the device will respond to primary address 253 (0xFD).
    /// 
    /// # Arguments
    /// * `secondary_address` - 8-byte unique identifier (Manufacturer ID + Device ID + Version + Medium)
    /// 
    /// # Returns
    /// * `Ok(())` - Device successfully selected, now responds to address 253
    /// * `Err(MBusError)` - Selection failed or invalid secondary address
    pub async fn select_device_by_secondary_address(&mut self, secondary_address: &[u8; 8]) -> Result<(), MBusError> {
        self.state = MBusProtocolState::Selecting;
        
        if secondary_address.len() != 8 {
            return Err(MBusError::Other("Secondary address must be exactly 8 bytes".to_string()));
        }

        // Create SND_UD frame for device selection
        // Frame structure: [68h] [0Bh] [0Bh] [68h] [53h] [FDh] [52h] [Secondary Address...] [CS] [16h]
        let selection_frame = MBusFrame {
            frame_type: MBusFrameType::Long,
            control: 0x53,           // SND_UD control field
            address: 0xFD,           // Selection address (253)
            control_information: 0x52, // CI for Mode 1 selection
            data: secondary_address.to_vec(), // 8-byte secondary address
            checksum: 0,             // Will be calculated by pack_frame
            more_records_follow: false,
        };

        // TODO: This will be connected to actual serial transmission in integration
        // For now, we just validate the frame can be created correctly
        let _frame_bytes = frame::pack_frame(&selection_frame);
        
        // After successful selection, device responds to address 253
        // The serial layer would need to:
        // 1. Send the selection frame
        // 2. Wait for E5h acknowledgment
        // 3. If ACK received, device is selected
        
        Ok(())
    }

    /// Requests Class 2 data from the currently selected device using REQ_UD2.
    /// This is the standard request for meter readings.
    /// 
    /// # Returns
    /// * `Ok(MBusFrame)` - The constructed REQ_UD2 request frame ready for transmission
    /// * `Err(MBusError)` - No device selected or invalid state
    pub async fn request_data(&mut self) -> Result<MBusFrame, MBusError> {
        if self.current_address == 0 {
            return Err(MBusError::Other("No device selected, call select_device first".to_string()));
        }

        self.state = MBusProtocolState::Requesting;
        
        // Construct REQ_UD2 frame (Request for Class 2 Data)
        // Uses Short Frame format: [10h] [C-Field] [A-Field] [Checksum] [16h]
        
        // Control field: 0x5B (REQ_UD2) or 0x7B (REQ_UD2 with FCB set)
        let control_field = if self.fcb {
            0x7B  // REQ_UD2 with Frame Count Bit set
        } else {
            0x5B  // REQ_UD2 without FCB
        };

        let request_frame = MBusFrame {
            frame_type: MBusFrameType::Short,
            control: control_field,
            address: self.current_address,
            control_information: 0,  // Not used in short frames
            data: Vec::new(),        // No data in REQ_UD2
            checksum: 0,             // Will be calculated by pack_frame
            more_records_follow: false,
        };

        // Validate frame construction
        let _frame_bytes = frame::pack_frame(&request_frame);
        
        Ok(request_frame)
    }

    /// Requests Class 1 data (alarm data) from the currently selected device using REQ_UD1.
    /// This is for high-priority alarm information.
    /// 
    /// # Returns
    /// * `Ok(MBusFrame)` - The constructed REQ_UD1 request frame ready for transmission
    /// * `Err(MBusError)` - No device selected or invalid state
    pub async fn request_alarm_data(&mut self) -> Result<MBusFrame, MBusError> {
        if self.current_address == 0 {
            return Err(MBusError::Other("No device selected, call select_device first".to_string()));
        }

        self.state = MBusProtocolState::Requesting;
        
        // Control field: 0x5A (REQ_UD1) or 0x7A (REQ_UD1 with FCB set)
        let control_field = if self.fcb {
            0x7A  // REQ_UD1 with Frame Count Bit set
        } else {
            0x5A  // REQ_UD1 without FCB
        };

        let request_frame = MBusFrame {
            frame_type: MBusFrameType::Short,
            control: control_field,
            address: self.current_address,
            control_information: 0,  // Not used in short frames
            data: Vec::new(),        // No data in REQ_UD1
            checksum: 0,             // Will be calculated by pack_frame
            more_records_follow: false,
        };

        Ok(request_frame)
    }

    /// Toggles the Frame Count Bit for multi-frame communication sequences.
    /// This must be called between frames when handling multi-frame responses.
    pub fn toggle_fcb(&mut self) {
        self.fcb = !self.fcb;
    }

    /// Resets the Frame Count Bit to false. Used when starting communication with a new device.
    pub fn reset_fcb(&mut self) {
        self.fcb = false;
    }

    /// Validates and processes a received RSP_UD (Response with User Data) frame.
    /// Performs all necessary frame validation according to M-Bus specification.
    /// 
    /// # Arguments
    /// * `received_frame` - The frame received from the device
    /// 
    /// # Returns
    /// * `Ok((Vec<u8>, bool))` - Tuple of (payload data, more_frames_follow)
    /// * `Err(MBusError)` - Frame validation failed or unexpected frame type
    pub async fn receive_data(&mut self, received_frame: &MBusFrame) -> Result<(Vec<u8>, bool), MBusError> {
        self.state = MBusProtocolState::Receiving;
        
        // Validate frame type - expect Long frame for RSP_UD
        match received_frame.frame_type {
            MBusFrameType::Long => {
                // Expected frame type for RSP_UD
            }
            MBusFrameType::Ack => {
                // Single character acknowledgment (E5h) - not data response
                return Err(MBusError::FrameParseError("Received ACK instead of data response".to_string()));
            }
            _ => {
                return Err(MBusError::FrameParseError("Expected Long frame for RSP_UD".to_string()));
            }
        }
        
        // Validate control field - expect 0x08 for RSP_UD
        if received_frame.control != 0x08 {
            return Err(MBusError::FrameParseError(
                format!("Expected control field 0x08 for RSP_UD, got 0x{:02X}", received_frame.control)
            ));
        }
        
        // Validate address matches our current device
        if received_frame.address != self.current_address {
            return Err(MBusError::FrameParseError(
                format!("Address mismatch: expected 0x{:02X}, got 0x{:02X}", 
                    self.current_address, received_frame.address)
            ));
        }
        
        // Verify frame checksum using existing verification function
        frame::verify_frame(received_frame)?;
        
        // Check for multi-frame indication
        // DIF code 0x1F in the data indicates more frames will follow
        let more_frames = self.check_multi_frame_indication(&received_frame.data);
        
        // Extract payload data (remove any multi-frame indicators)
        let payload_data = if more_frames {
            // Remove the 0x1F DIF code from the data
            self.extract_payload_without_multi_frame_dif(&received_frame.data)
        } else {
            received_frame.data.clone()
        };
        
        Ok((payload_data, more_frames))
    }
    
    /// Checks if the received data contains multi-frame indication (DIF code 0x1F).
    /// 
    /// # Arguments
    /// * `data` - The data payload from the received frame
    /// 
    /// # Returns
    /// * `bool` - true if more frames follow, false if this is the last frame
    fn check_multi_frame_indication(&self, data: &[u8]) -> bool {
        // Look for DIF code 0x1F which indicates more data blocks follow
        // This can appear at various positions in the data structure
        for &byte in data {
            if byte == 0x1F {
                return true;
            }
        }
        false
    }
    
    /// Extracts payload data while removing multi-frame DIF indicators.
    /// 
    /// # Arguments
    /// * `data` - The raw data payload from the frame
    /// 
    /// # Returns
    /// * `Vec<u8>` - Cleaned payload data without DIF 0x1F indicators
    fn extract_payload_without_multi_frame_dif(&self, data: &[u8]) -> Vec<u8> {
        // Simple implementation: remove all 0x1F bytes
        // In a more sophisticated implementation, this would properly parse
        // the data structure and only remove DIF bytes, not data bytes that happen to be 0x1F
        data.iter().filter(|&&byte| byte != 0x1F).copied().collect()
    }
    
    /// Validates that a received frame is a proper acknowledgment (E5h).
    /// Used after sending selection frames or other commands that expect ACK.
    /// 
    /// # Arguments
    /// * `received_frame` - The frame received from the device
    /// 
    /// # Returns
    /// * `Ok(())` - Valid acknowledgment received
    /// * `Err(MBusError)` - Not an acknowledgment or invalid
    pub async fn receive_acknowledgment(&mut self, received_frame: &MBusFrame) -> Result<(), MBusError> {
        match received_frame.frame_type {
            MBusFrameType::Ack => {
                // Valid acknowledgment received
                Ok(())
            }
            _ => {
                Err(MBusError::FrameParseError("Expected acknowledgment (E5h) frame".to_string()))
            }
        }
    }

    /// Processes raw payload data by extracting and parsing M-Bus data records.
    /// Converts the raw bytes into structured MBusRecord objects containing values, units, and metadata.
    /// 
    /// # Arguments
    /// * `payload_data` - Raw payload bytes from one or more RSP_UD frames
    /// 
    /// # Returns
    /// * `Ok(Vec<MBusRecord>)` - Successfully parsed data records
    /// * `Err(MBusError)` - Data parsing failed or invalid record format
    pub async fn process_data(&mut self, payload_data: &[u8]) -> Result<Vec<MBusRecord>, MBusError> {
        // Note: parse_variable_record and parse_fixed_record are used in helper methods
        
        self.state = MBusProtocolState::Idle;
        
        if payload_data.is_empty() {
            return Ok(Vec::new());
        }
        
        let mut records = Vec::new();
        let mut remaining_data = payload_data;
        
        // Parse data records until all data is consumed
        while !remaining_data.is_empty() {
            // Try parsing as variable record first (most common format)
            match self.try_parse_variable_record(remaining_data) {
                Ok((record, consumed_bytes)) => {
                    records.push(record);
                    if consumed_bytes >= remaining_data.len() {
                        break;
                    }
                    remaining_data = &remaining_data[consumed_bytes..];
                }
                Err(_) => {
                    // Try parsing as fixed record
                    match self.try_parse_fixed_record(remaining_data) {
                        Ok((record, consumed_bytes)) => {
                            records.push(record);
                            if consumed_bytes >= remaining_data.len() {
                                break;
                            }
                            remaining_data = &remaining_data[consumed_bytes..];
                        }
                        Err(_) => {
                            // Skip this byte and try parsing from the next position
                            // This handles cases where data might be partially corrupted
                            // or contain padding/unknown structures
                            remaining_data = &remaining_data[1..];
                        }
                    }
                }
            }
            
            // Safety check to prevent infinite loops
            if remaining_data.len() >= payload_data.len() {
                break;
            }
        }
        
        // Post-processing: validate and normalize records
        for record in &mut records {
            self.validate_and_normalize_record(record)?;
        }
        
        Ok(records)
    }
    
    /// Attempts to parse data as a variable-length M-Bus record.
    /// 
    /// # Arguments
    /// * `data` - Raw data bytes to parse
    /// 
    /// # Returns
    /// * `Ok((MBusRecord, usize))` - Parsed record and bytes consumed
    /// * `Err(MBusError)` - Parsing failed
    fn try_parse_variable_record(&self, data: &[u8]) -> Result<(MBusRecord, usize), MBusError> {
        use crate::payload::record::parse_variable_record;
        
        let _initial_len = data.len();
        
        // Parse variable record - this function already handles the complex DIF/VIF parsing
        let record = parse_variable_record(data)?;
        
        // Calculate consumed bytes (this is a simplification - in practice, we'd need
        // more sophisticated tracking of how many bytes were consumed)
        let consumed_bytes = self.calculate_record_size(&record);
        
        Ok((record, consumed_bytes))
    }
    
    /// Attempts to parse data as a fixed-length M-Bus record.
    /// 
    /// # Arguments
    /// * `data` - Raw data bytes to parse
    /// 
    /// # Returns
    /// * `Ok((MBusRecord, usize))` - Parsed record and bytes consumed  
    /// * `Err(MBusError)` - Parsing failed
    fn try_parse_fixed_record(&self, data: &[u8]) -> Result<(MBusRecord, usize), MBusError> {
        use crate::payload::record::parse_fixed_record;
        
        let record = parse_fixed_record(data)?;
        let consumed_bytes = self.calculate_record_size(&record);
        
        Ok((record, consumed_bytes))
    }
    
    /// Estimates the size of a parsed record in bytes.
    /// This is used to advance the parser position in the data.
    /// 
    /// # Arguments
    /// * `record` - The parsed M-Bus record
    /// 
    /// # Returns
    /// * `usize` - Estimated size in bytes
    fn calculate_record_size(&self, record: &MBusRecord) -> usize {
        use crate::payload::record::mbus_dif_datalength_lookup;
        
        // Base size: DIF (1 byte) + VIF (at least 1 byte) + data
        let mut size = 1 + 1; // DIF + VIF minimum
        
        // Add data length based on DIF
        size += mbus_dif_datalength_lookup(record.drh.dib.dif);
        
        // Add extended VIF bytes if present
        if record.drh.vib.vif > 0x7F {
            size += 1; // VIFE
        }
        
        // Minimum record size is 3 bytes, maximum reasonable size is 255
        size.clamp(3, 255)
    }
    
    /// Validates and normalizes a parsed M-Bus record.
    /// Checks for reasonable values and applies any necessary corrections.
    /// 
    /// # Arguments
    /// * `record` - Mutable reference to the record to validate
    /// 
    /// # Returns
    /// * `Ok(())` - Record is valid
    /// * `Err(MBusError)` - Record validation failed
    fn validate_and_normalize_record(&self, record: &mut MBusRecord) -> Result<(), MBusError> {
        // Check for error indicators in VIF codes
        // Some VIF codes indicate device errors or invalid data
        if record.drh.vib.vif == 0xFF {
            return Err(MBusError::Other("Record indicates device error (VIF=0xFF)".to_string()));
        }
        
        // Validate data length is reasonable
        if record.data_len > 255 {
            return Err(MBusError::Other("Record data length exceeds maximum".to_string()));
        }
        
        // Validate value is reasonable (not NaN, not infinite) for numeric values
        match &mut record.value {
            crate::payload::record::MBusRecordValue::Numeric(value) => {
                if value.is_nan() || value.is_infinite() {
                    // Set to 0 and log the issue rather than failing
                    *value = 0.0;
                }
            }
            _ => {
                // String values don't need this validation
            }
        }
        
        Ok(())
    }

    /// Handles errors during M-Bus communication and implements recovery strategies.
    /// Provides retry logic, timeout handling, and state recovery according to M-Bus specification.
    /// 
    /// # Arguments
    /// * `error` - The error that occurred during communication
    /// 
    /// # Returns
    /// * `Ok(())` - Error handled, retry communication
    /// * `Err(MBusError)` - Fatal error, communication should be aborted
    pub fn handle_error(&mut self, error: MBusError) -> Result<(), MBusError> {
        self.state = MBusProtocolState::Error;
        
        match error {
            MBusError::SerialPortError(_) => {
                // Serial port communication error - potentially recoverable
                Err(error) // Let higher level handle hardware issues
            }
            MBusError::FrameParseError(_) => {
                // Frame parsing error - could be corruption, try reset
                self.reset_fcb(); // Reset frame count bit
                Ok(()) // Indicate retry is possible
            }
            MBusError::InvalidChecksum { .. } => {
                // Checksum error - retry with same frame
                Ok(()) // Indicate retry is possible  
            }
            MBusError::NomError(_) => {
                // Parsing error - potentially recoverable
                Ok(()) // Indicate retry is possible
            }
            _ => {
                // Other errors are generally fatal for this transaction
                Err(error)
            }
        }
    }

    /// Calculates the appropriate timeout for a given baud rate according to M-Bus specification.
    /// Master timeout should be (330 bit periods + 50ms).
    /// 
    /// # Arguments
    /// * `baud_rate` - The current serial communication baud rate
    /// 
    /// # Returns
    /// * `Duration` - Calculated timeout duration
    pub fn calculate_timeout(baud_rate: u32) -> std::time::Duration {
        // M-Bus spec: slave response time is 11 to (330 bit times + 50ms)
        // Master timeout should be slightly longer than maximum slave response time
        let bit_time_ms = 1000.0 / baud_rate as f64;
        let bit_period_timeout_ms = 330.0 * bit_time_ms;
        let total_timeout_ms = bit_period_timeout_ms + 50.0;
        
        // Add some safety margin for the master timeout
        let master_timeout_ms = total_timeout_ms + 20.0;
        
        std::time::Duration::from_millis(master_timeout_ms as u64)
    }

    /// Calculates inter-frame delay according to M-Bus specification.
    /// Minimum delay between frames is 11 bit times.
    /// 
    /// # Arguments  
    /// * `baud_rate` - The current serial communication baud rate
    /// 
    /// # Returns
    /// * `Duration` - Minimum delay duration between frames
    pub fn calculate_inter_frame_delay(baud_rate: u32) -> std::time::Duration {
        // M-Bus spec: minimum 11 bit times between frames
        let bit_time_ms = 1000.0 / baud_rate as f64;
        let delay_ms = 11.0 * bit_time_ms;
        
        std::time::Duration::from_millis(delay_ms.ceil() as u64)
    }

    /// Resets the state machine to idle state and clears all communication state.
    /// Used to recover from errors or start fresh communication.
    pub fn reset(&mut self) {
        self.state = MBusProtocolState::Idle;
        self.fcb = false;
        self.current_address = 0;
    }
}

/// Represents the M-Bus protocol implementation.
#[derive(Default)]
pub struct MBusProtocol {
    pub state_machine: StateMachine,
    pub frame_handler: FrameHandler,
    pub discovery_manager: DeviceDiscoveryManager,
    pub data_retrieval_manager: DataRetrievalManager,
}

impl MBusProtocol {
    /// Creates a new instance of the M-Bus protocol implementation.
    pub fn new() -> Self {
        MBusProtocol::default()
    }

}

/// Handles the processing of M-Bus frames.
#[derive(Default)]
pub struct FrameHandler {
    frame_cache: Vec<MBusFrame>,
}

impl FrameHandler {
    /// Creates a new instance of the FrameHandler.
    pub fn new() -> Self {
        FrameHandler::default()
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
        // Return a dummy short frame for stubs
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
#[derive(Default)]
pub struct DeviceDiscoveryManager {
    #[allow(dead_code)]
    frame_handler: FrameHandler,
}

impl DeviceDiscoveryManager {
    /// Creates a new instance of the DeviceDiscoveryManager.
    pub fn new() -> Self {
        DeviceDiscoveryManager::default()
    }

    /// Scans for available M-Bus devices using the secondary address selection mechanism.
    pub async fn scan_secondary_addresses(&mut self) -> Result<Vec<u8>, MBusError> {
        // For stub implementation, just return empty immediately
        // This avoids infinite loops with the stub receive_frame
        Ok(vec![])
    }

    /// Selects a secondary address for an M-Bus device.
    #[allow(dead_code)]
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
        if matches!(
            resp2.frame_type,
            MBusFrameType::Long | MBusFrameType::Control
        ) {
            return Ok(ProbeResult::Single(resp2.address));
        }
        Ok(ProbeResult::Collision)
    }

    /// Creates a select frame for the given secondary address mask.
    #[allow(dead_code)]
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

#[allow(dead_code)]
enum ProbeResult {
    Single(u8),
    Collision,
    Nothing,
}

/// Manages the retrieval of data from M-Bus devices.
#[derive(Default)]
pub struct DataRetrievalManager {
    frame_handler: FrameHandler,
    record_parser: RecordParser,
}

impl DataRetrievalManager {
    /// Creates a new instance of the DataRetrievalManager.
    pub fn new() -> Self {
        DataRetrievalManager::default()
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
#[derive(Default)]
pub struct RecordParser {
    // Implementation omitted for brevity
}

impl RecordParser {
    pub fn new() -> Self {
        RecordParser::default()
    }
    /// Parses M-Bus data records from the given frame.
    pub fn parse_records(&mut self, frame: &MBusFrame) -> Result<Vec<MBusRecord>, MBusError> {
        use crate::constants::{MBUS_CONTROL_INFO_RESP_FIXED, MBUS_CONTROL_INFO_RESP_VARIABLE};

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
                            crate::logging::log_error(&format!(
                                "Error parsing variable record: {e:?}"
                            ));
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
