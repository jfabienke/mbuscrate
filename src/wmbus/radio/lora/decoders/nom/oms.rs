//! OMS (Open Metering System) decoder using nom
//!
//! OMS is a profile of Wireless M-Bus (EN 13757), so this decoder
//! heavily reuses the existing M-Bus parsing infrastructure.

use crate::constants::*;
use crate::payload::data::parse_enhanced_variable_data_record;
use crate::payload::record::MBusRecordValue;
use crate::wmbus::radio::lora::decoder::{
    BatteryStatus, DeviceStatus, LoRaDecodeError, LoRaPayloadDecoder, MeteringData, Reading,
};
use nom::{
    number::complete::{le_u16, le_u32, u8 as parse_u8},
    IResult,
};
use std::time::SystemTime;

/// Type alias for OMS header parser result
type OmsHeaderResult<'a> = IResult<&'a [u8], (u8, u8, u16, u32, u8, OmsMedium, u8, u8)>;

/// OMS version
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OmsVersion {
    V3_0,
    V4_0,
    V4_1,
}

/// Medium type as per OMS specification
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OmsMedium {
    Other = 0x00,
    Oil = 0x01,
    Electricity = 0x02,
    Gas = 0x03,
    Heat = 0x04,
    Steam = 0x05,
    HotWater = 0x06,
    Water = 0x07,
    HeatCostAllocator = 0x08,
    CoolingLoad = 0x0A,
    CoolingReturn = 0x0B,
    HeatFlow = 0x0C,
    HeatReturn = 0x0D,
    Reserved = 0x0F,
}

impl OmsMedium {
    fn from_byte(byte: u8) -> Self {
        match byte {
            0x00 => Self::Other,
            0x01 => Self::Oil,
            0x02 => Self::Electricity,
            0x03 => Self::Gas,
            0x04 => Self::Heat,
            0x05 => Self::Steam,
            0x06 => Self::HotWater,
            0x07 => Self::Water,
            0x08 => Self::HeatCostAllocator,
            0x0A => Self::CoolingLoad,
            0x0B => Self::CoolingReturn,
            0x0C => Self::HeatFlow,
            0x0D => Self::HeatReturn,
            _ => Self::Reserved,
        }
    }
}

/// OMS frame structure
#[derive(Debug)]
pub struct OmsFrame {
    pub length: u8,
    pub c_field: u8,
    pub manufacturer: u16,
    pub device_id: u32,
    pub version: u8,
    pub medium: OmsMedium,
    pub access_no: u8,
    pub status: u8,
    pub signature: Option<u16>,
    pub data_records: Vec<OmsDataRecord>,
}

/// OMS data record
#[derive(Debug)]
pub struct OmsDataRecord {
    pub dif: u8,
    pub dife: Vec<u8>,
    pub vif: u8,
    pub vife: Vec<u8>,
    pub data: Vec<u8>,
    pub value: MBusRecordValue,
    pub unit: String,
    pub quantity: String,
}

/// Parse OMS frame header
pub fn parse_oms_header(input: &[u8]) -> OmsHeaderResult<'_> {
    let (input, length) = parse_u8(input)?;
    let (input, c_field) = parse_u8(input)?;
    let (input, manufacturer) = le_u16(input)?;
    let (input, device_id) = le_u32(input)?;
    let (input, version) = parse_u8(input)?;
    let (input, medium_byte) = parse_u8(input)?;
    let (input, access_no) = parse_u8(input)?;
    let (input, status) = parse_u8(input)?;

    let medium = OmsMedium::from_byte(medium_byte);

    Ok((
        input,
        (
            length,
            c_field,
            manufacturer,
            device_id,
            version,
            medium,
            access_no,
            status,
        ),
    ))
}

/// Parse OMS data records using existing M-Bus parsers
pub fn parse_oms_data_records(input: &[u8]) -> IResult<&[u8], Vec<OmsDataRecord>> {
    let mut records = Vec::new();
    let mut remaining = input;

    while !remaining.is_empty() {
        // Check for idle filler or end of data
        if remaining[0] == MBUS_DIB_DIF_IDLE_FILLER || remaining[0] == 0x16 {
            break;
        }

        // Try to parse using enhanced M-Bus parser
        match parse_enhanced_variable_data_record(remaining) {
            Ok((new_remaining, record)) => {
                // Convert M-Bus record to OMS record
                // Convert data::MBusRecordValue to record::MBusRecordValue
                let value = match record.value {
                    crate::payload::data::MBusRecordValue::Numeric(n) => MBusRecordValue::Numeric(n),
                    crate::payload::data::MBusRecordValue::String(s) => MBusRecordValue::String(s),
                };
                let oms_record = OmsDataRecord {
                    dif: record.dif_chain.first().copied().unwrap_or(0),
                    dife: record.dif_chain[1..].to_vec(),
                    vif: record.vif_chain.first().copied().unwrap_or(0),
                    vife: record.vif_chain[1..].to_vec(),
                    data: vec![], // Already processed into value
                    value,
                    unit: record.unit,
                    quantity: record.quantity,
                };
                records.push(oms_record);
                remaining = new_remaining;
            }
            Err(_) => {
                // Skip this byte and try again
                remaining = &remaining[1..];
            }
        }
    }

    Ok((remaining, records))
}

/// Parse complete OMS frame
pub fn parse_oms_frame(input: &[u8]) -> IResult<&[u8], OmsFrame> {
    let (input, (length, c_field, manufacturer, device_id, version, medium, access_no, status)) =
        parse_oms_header(input)?;

    // Optional signature field (2 bytes)
    let (input, signature) = if input.len() >= 2 && (status & 0x30) != 0 {
        let (i, sig) = le_u16(input)?;
        (i, Some(sig))
    } else {
        (input, None)
    };

    // Parse data records
    let (input, data_records) = parse_oms_data_records(input)?;

    Ok((
        input,
        OmsFrame {
            length,
            c_field,
            manufacturer,
            device_id,
            version,
            medium,
            access_no,
            status,
            signature,
            data_records,
        },
    ))
}

/// Convert OMS frame to MeteringData
pub fn oms_frame_to_metering_data(frame: OmsFrame, raw_payload: &[u8]) -> MeteringData {
    let mut readings = Vec::new();
    let mut battery = None;

    // Process data records
    for (idx, record) in frame.data_records.into_iter().enumerate() {
        // Check for battery-related VIF codes
        if record.vif == 0xFD && record.vife.first() == Some(&0x74) {
            // Battery voltage
            if let MBusRecordValue::Numeric(voltage) = &record.value {
                battery = Some(BatteryStatus {
                    voltage: Some(*voltage as f32),
                    percentage: None,
                    low_battery: *voltage < 2.5,
                });
            }
        } else {
            readings.push(Reading {
                value: record.value,
                unit: record.unit,
                quantity: record.quantity.clone(),
                tariff: None,
                storage_number: Some(idx as u32),
                description: Some(format!("OMS {}", record.quantity)),
            });
        }
    }

    // Parse status byte
    let status = DeviceStatus {
        alarm: (frame.status & 0x01) != 0,
        tamper: (frame.status & 0x02) != 0,
        leak: (frame.status & 0x04) != 0,
        reverse_flow: (frame.status & 0x08) != 0,
        error_code: if (frame.status & 0x80) != 0 {
            Some((frame.status & 0x70) as u16)
        } else {
            None
        },
        flags: frame.status as u32,
    };

    MeteringData {
        timestamp: SystemTime::now(),
        readings,
        battery,
        status,
        raw_payload: raw_payload.to_vec(),
        decoder_type: format!("OMS-{:04X}", frame.manufacturer),
    }
}

/// OMS decoder implementation
#[derive(Debug, Clone)]
pub struct OmsDecoder {
    pub version: OmsVersion,
    pub expected_manufacturer: Option<u16>,
}

impl OmsDecoder {
    pub fn new(version: OmsVersion) -> Self {
        Self {
            version,
            expected_manufacturer: None,
        }
    }

    pub fn with_manufacturer(mut self, manufacturer: u16) -> Self {
        self.expected_manufacturer = Some(manufacturer);
        self
    }
}

impl LoRaPayloadDecoder for OmsDecoder {
    fn decode(&self, payload: &[u8], _f_port: u8) -> Result<MeteringData, LoRaDecodeError> {
        match parse_oms_frame(payload) {
            Ok((_, frame)) => {
                // Validate manufacturer if specified
                if let Some(expected) = self.expected_manufacturer {
                    if frame.manufacturer != expected {
                        return Err(LoRaDecodeError::InvalidData {
                            offset: 2,
                            reason: format!(
                                "Manufacturer mismatch: expected {:04X}, got {:04X}",
                                expected, frame.manufacturer
                            ),
                        });
                    }
                }

                Ok(oms_frame_to_metering_data(frame, payload))
            }
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => {
                Err(LoRaDecodeError::InvalidData {
                    offset: payload.len() - e.input.len(),
                    reason: format!("OMS parse error: {:?}", e.code),
                })
            }
            Err(nom::Err::Incomplete(needed)) => {
                let expected = match needed {
                    nom::Needed::Unknown => payload.len() + 1,
                    nom::Needed::Size(n) => payload.len() + n.get(),
                };
                Err(LoRaDecodeError::InvalidLength {
                    expected,
                    actual: payload.len(),
                })
            }
        }
    }

    fn decoder_type(&self) -> &str {
        "OMS"
    }

    fn clone_box(&self) -> Box<dyn LoRaPayloadDecoder> {
        Box::new(self.clone())
    }

    fn can_decode(&self, payload: &[u8], _f_port: u8) -> bool {
        // Check minimum OMS frame length
        if payload.len() < 12 {
            return false;
        }

        // Check for typical OMS C-field values
        if payload.len() > 1 {
            let c_field = payload[1];
            // 0x44 = SND-NR, 0x46 = SND-IR, 0x08 = RSP-UD
            if c_field == 0x44 || c_field == 0x46 || c_field == 0x08 {
                return true;
            }
        }

        false
    }
}

/// Well-known OMS manufacturer codes
pub mod manufacturers {
    pub const KAMSTRUP: u16 = 0x2C2D;
    pub const DIEHL: u16 = 0x11A5;
    pub const ITRON: u16 = 0x1C08;
    pub const LANDIS_GYR: u16 = 0x32A7;
    pub const ZENNER: u16 = 0x6A50;
    pub const SENSUS: u16 = 0x4CAE;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_oms_header() {
        // Example OMS header
        let payload = vec![
            0x2C, // Length
            0x44, // C-field (SND-NR)
            0x2D, 0x2C, // Manufacturer (Kamstrup)
            0x78, 0x56, 0x34, 0x12, // Device ID
            0x01, // Version
            0x07, // Medium (Water)
            0x00, // Access No
            0x00, // Status
        ];

        let (remaining, (_, c_field, mfr, id, _, medium, _, status)) =
            parse_oms_header(&payload).unwrap();

        assert_eq!(c_field, 0x44);
        assert_eq!(mfr, 0x2C2D); // Kamstrup
        assert_eq!(id, 0x12345678);
        assert_eq!(medium, OmsMedium::Water);
        assert_eq!(status, 0x00);
        assert!(remaining.is_empty());
    }

    #[test]
    fn test_oms_decoder() {
        let decoder = OmsDecoder::new(OmsVersion::V4_0).with_manufacturer(manufacturers::KAMSTRUP);

        // Minimal OMS frame
        let payload = vec![
            0x0C, // Length
            0x44, // C-field
            0x2D, 0x2C, // Kamstrup
            0x00, 0x00, 0x00, 0x00, // Device ID
            0x01, // Version
            0x07, // Water
            0x00, // Access
            0x00, // Status
        ];

        let result = decoder.decode(&payload, 1);
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.decoder_type, "OMS-2C2D");
    }
}
