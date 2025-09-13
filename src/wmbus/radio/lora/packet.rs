use crate::wmbus::radio::modulation::LoRaPacketStatus;
use thiserror::Error;
use ciborium::de::from_reader;
use serde::{Serialize, Deserialize};
use std::io::Cursor;

/// Errors for LoRa packet parsing and handling.
#[derive(Error, Debug)]
pub enum LoRaError {
    #[error("Invalid MHDR: {0:#X} (expected 0x00 for JoinReq or 0x20/0x80 for DataUp)")]
    InvalidMhdr(u8),
    #[error("CRC failure")]
    CrcFail,
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Device not found")]
    DeviceNotFound,
    #[error("CBOR decode error: {0}")]
    Cbor(#[from] ciborium::de::Error<std::io::Error>),
    #[error("CBOR encode error: {0}")]
    CborSer(#[from] ciborium::ser::Error<std::io::Error>),
}

/// LoRa payload structure after MHDR parsing.
#[derive(Debug)]
pub struct LoRaPayload {
    pub mhdr: u8, // Message Header (0x00 JoinReq, 0x20 UnconfDataUp, 0x80 ConfDataUp)
    pub dev_addr: [u8; 4], // Device Address (for ABP)
    pub fctrl: u8, // Frame Control
    pub fport: u8, // FPort (custom 0xFF for triggers)
    pub frm_payload: Vec<u8>, // Meter data + schedule (CBOR)
}

/// Parse LoRa packet (basic non-LoRaWAN)
pub fn decode_lora_packet(payload: &[u8], _status: LoRaPacketStatus) -> Result<LoRaPayload, LoRaError> {
    if payload.is_empty() {
        return Err(LoRaError::Parse("Empty payload".to_string()));
    }

    let mhdr = payload[0];
    match mhdr {
        0x00 => { // JoinReq (OTAA)
            if payload.len() < 18 { // Min: MHDR + DevEUI(8) + AppEUI(8) + DevNonce(2)
                return Err(LoRaError::Parse("Too short for JoinReq".to_string()));
            }
            let _dev_eui = &payload[1..9]; // 8B
            let _app_eui = &payload[9..17]; // 8B
            let _dev_nonce = u16::from_le_bytes([payload[17], payload[18]]); // 2B
            let schedule_start = 19; // Custom CBOR after nonce
            let frm_payload = payload[schedule_start..].to_vec();

            // Parse custom schedule from CBOR
            let _schedule: ScheduleInfo = from_reader(Cursor::new(&frm_payload))?;
            Ok(LoRaPayload {
                mhdr,
                dev_addr: [0; 4], // Not in JoinReq
                fctrl: 0,
                fport: 0,
                frm_payload, // Includes schedule
            })
        }
        0x20 | 0x80 => { // Unconf/Conf DataUp (ABP)
            if payload.len() < 13 { // Min: MHDR + DevAddr(4) + FCtrl(1) + FPort(1) + MIC(4)
                return Err(LoRaError::Parse("Too short for DataUp".to_string()));
            }
            let dev_addr = payload[1..5].try_into().unwrap(); // 4B
            let fctrl = payload[5];
            let fport = payload[6];
            let mic_start = payload.len() - 4; // MIC last 4B
            let frm_payload = payload[7..mic_start].to_vec(); // Data between FPort and MIC

            // Parse custom schedule from FRMPayload CBOR (if present)
            let _schedule: Option<ScheduleInfo> = if !frm_payload.is_empty() {
                Some(from_reader(Cursor::new(&frm_payload))?)
            } else {
                None
            };

            Ok(LoRaPayload {
                mhdr,
                dev_addr,
                fctrl,
                fport,
                frm_payload,
            })
        }
        _ => Err(LoRaError::InvalidMhdr(mhdr)),
    }
}

/// Parse OTAA Join Request
pub fn parse_otaa_join(payload: &[u8]) -> Result<JoinRequest, LoRaError> {
    let decoded = decode_lora_packet(payload, LoRaPacketStatus::default())?;
    if decoded.mhdr != 0x00 {
        return Err(LoRaError::InvalidMhdr(decoded.mhdr));
    }

    // Extract from frm_payload (custom after nonce)
    let dev_eui = hex::encode(&decoded.frm_payload[0..8]); // Assume first 8B DevEUI
    let app_eui = hex::encode(&decoded.frm_payload[8..16]); // Next 8B AppEUI
    let dev_nonce = u16::from_le_bytes(
        decoded.frm_payload[16..18]
            .try_into()
            .map_err(|_| LoRaError::Parse("Invalid dev_nonce bytes".to_string()))?
    );
    let schedule_start = 18;
    let schedule_info: ScheduleInfo = if decoded.frm_payload.len() > schedule_start {
        from_reader(Cursor::new(&decoded.frm_payload[schedule_start..]))?
    } else {
        ScheduleInfo::default() // No schedule reported
    };

    Ok(JoinRequest {
        dev_eui,
        app_eui,
        dev_nonce,
        schedule_info,
    })
}

/// Parse ABP Data Up
pub fn parse_abp_data(payload: &[u8]) -> Result<DataPayload, LoRaError> {
    let decoded = decode_lora_packet(payload, LoRaPacketStatus::default())?;
    if decoded.mhdr != 0x20 && decoded.mhdr != 0x80 {
        return Err(LoRaError::InvalidMhdr(decoded.mhdr));
    }

    let dev_addr = hex::encode(decoded.dev_addr);
    let fport = decoded.fport;
    let meter_data = decoded.frm_payload.clone(); // Raw meter data
    let schedule_info: Option<ScheduleInfo> = if !decoded.frm_payload.is_empty() {
        Some(from_reader(Cursor::new(&decoded.frm_payload))?)
    } else {
        None
    };

    Ok(DataPayload {
        dev_addr,
        fport,
        meter_data,
        schedule_info,
    })
}

/// Build trigger downlink frame for Class A
pub fn build_trigger_frame(device_addr: u32, payload: &[u8]) -> Result<Vec<u8>, LoRaError> {
    let mut frame = Vec::new();
    frame.push(0x40); // MHDR: Unconfirmed Data Down
    frame.extend_from_slice(&device_addr.to_le_bytes()); // DevAddr (4B)
    frame.push(0x00); // FCtrl: Unconfirmed, no ACK
    frame.push(0xFF); // FPort: Custom triggers
    frame.extend_from_slice(payload); // CBOR command (e.g., { "cmd": "tx_now" })
    // MIC omitted for simplicity (add if needed)

    Ok(frame)
}

/// Calculate cumulative delta for missed packets (tolerance for cumulative meters)
pub fn calc_cumulative_delta(new_value: f64, last: Option<f64>) -> f64 {
    match last {
        Some(last) => new_value - last,
        None => new_value, // First reading
    }
}

/// Custom schedule info from FRMPayload CBOR
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScheduleInfo {
    pub tx_interval_min: Option<u32>,
    pub class: Option<LoRaClass>,
    pub freq_hz: Option<u32>, // For steering
    pub duty_pct: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LoRaClass {
    A,
    B,
    C,
}

/// Join Request (OTAA)
#[derive(Debug, Clone)]
pub struct JoinRequest {
    pub dev_eui: String, // Hex string
    pub app_eui: String, // Hex string
    pub dev_nonce: u16,
    pub schedule_info: ScheduleInfo,
}

/// Data Payload (ABP)
#[derive(Debug, Clone)]
pub struct DataPayload {
    pub dev_addr: String, // Hex string
    pub fport: u8,
    pub meter_data: Vec<u8>, // Raw binary (wM-Bus-like records)
    pub schedule_info: Option<ScheduleInfo>,
}