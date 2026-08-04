use crate::wmbus::radio::modulation::LoRaPacketStatus;
use ciborium::de::from_reader;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use thiserror::Error;

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
pub fn decode_lora_packet(
    payload: &[u8],
    _status: LoRaPacketStatus,
) -> Result<LoRaPayload, LoRaError> {
    if payload.is_empty() {
        return Err(LoRaError::Parse("Empty payload".to_string()));
    }

    let mhdr = payload[0];
    match mhdr {
        0x00 => {
            // JoinReq (OTAA): MHDR(1) + DevEUI(8) + AppEUI(8) + DevNonce(2) = 19 bytes min,
            // so byte 18 (the DevNonce high byte) is always in bounds.
            if payload.len() < 19 {
                return Err(LoRaError::Parse("Too short for JoinReq".to_string()));
            }
            let _dev_eui = &payload[1..9]; // 8B
            let _app_eui = &payload[9..17]; // 8B
            let _dev_nonce = u16::from_le_bytes([payload[17], payload[18]]); // 2B
            let frm_payload = payload[19..].to_vec(); // custom schedule CBOR (may be empty)

            // Parse custom schedule from CBOR only if present.
            if !frm_payload.is_empty() {
                let _schedule: ScheduleInfo = from_reader(Cursor::new(&frm_payload))?;
            }
            Ok(LoRaPayload {
                mhdr,
                dev_addr: [0; 4], // Not in JoinReq
                fctrl: 0,
                fport: 0,
                frm_payload, // schedule only (EUI/nonce stay in the original packet)
            })
        }
        0x20 | 0x80 => {
            // Unconf/Conf DataUp (ABP)
            if payload.len() < 13 {
                // Min: MHDR + DevAddr(4) + FCtrl(1) + FPort(1) + MIC(4)
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
    if payload.is_empty() || payload[0] != 0x00 {
        return Err(LoRaError::InvalidMhdr(
            payload.first().copied().unwrap_or(0),
        ));
    }
    if payload.len() < 19 {
        return Err(LoRaError::Parse("Too short for JoinReq".to_string()));
    }

    // EUI/nonce come from the JoinReq header itself, not the schedule payload:
    // MHDR(1) | DevEUI(8) | AppEUI(8) | DevNonce(2) | schedule CBOR...
    let dev_eui = hex::encode(&payload[1..9]);
    let app_eui = hex::encode(&payload[9..17]);
    let dev_nonce = u16::from_le_bytes([payload[17], payload[18]]);
    let schedule_info: ScheduleInfo = if payload.len() > 19 {
        from_reader(Cursor::new(&payload[19..]))?
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

#[cfg(test)]
mod join_parse_tests {
    use super::*;

    #[test]
    fn join_18_bytes_errors_without_panicking() {
        // 18 bytes is one short of the 19-byte minimum; must error, not index-panic on byte 18.
        let payload = [0x00u8; 18];
        assert!(decode_lora_packet(&payload, LoRaPacketStatus::default()).is_err());
        assert!(parse_otaa_join(&payload).is_err());
    }

    #[test]
    fn parse_otaa_join_reads_fields_from_header_not_schedule() {
        // MHDR(0x00) + DevEUI(8) + AppEUI(8) + DevNonce(2 LE), no schedule.
        let mut p = vec![0x00u8];
        p.extend_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]); // DevEUI
        p.extend_from_slice(&[0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8]); // AppEUI
        p.extend_from_slice(&[0xCD, 0xAB]); // DevNonce LE = 0xABCD
        let jr = parse_otaa_join(&p).expect("valid 19-byte join");
        assert_eq!(jr.dev_eui, "1122334455667788");
        assert_eq!(jr.app_eui, "a1a2a3a4a5a6a7a8");
        assert_eq!(jr.dev_nonce, 0xABCD);
    }
}
