//! EN 13757-3 Compact Frame decoder using nom parser combinators
//!
//! This is a nom-based version that directly reuses the existing M-Bus frame parsers.

use crate::payload::record::MBusRecordValue;
use crate::wmbus::radio::lora::decoder::{
    BatteryStatus, DeviceStatus, LoRaDecodeError, LoRaPayloadDecoder, MeteringData, Reading,
};
use nom::{
    combinator::opt,
    number::complete::{le_i16, le_u16, le_u32, u8 as parse_u8},
    IResult,
};
use std::time::SystemTime;

/// Simple compact frame structure (non-standard but common)
#[derive(Debug)]
pub struct SimpleCompactFrame {
    pub device_id: u32,
    pub counter: u32,
    pub status: u16,
    pub battery: u8,
    pub temperature: Option<i16>,
    pub pressure: Option<u16>,
}

/// Parse simple compact frame format
pub fn parse_simple_compact(input: &[u8]) -> IResult<&[u8], SimpleCompactFrame> {
    let (input, device_id) = le_u32(input)?;
    let (input, counter) = le_u32(input)?;
    let (input, status) = le_u16(input)?;
    let (input, battery) = parse_u8(input)?;

    // Optional temperature (2 bytes)
    let (input, temperature) = opt(le_i16)(input)?;

    // Optional pressure (2 bytes)
    let (input, pressure) = opt(le_u16)(input)?;

    Ok((
        input,
        SimpleCompactFrame {
            device_id,
            counter,
            status,
            battery,
            temperature,
            pressure,
        },
    ))
}

/// Convert simple compact frame to MeteringData
pub fn simple_compact_to_metering_data(
    frame: SimpleCompactFrame,
    f_port: u8,
    raw_payload: &[u8],
) -> MeteringData {
    // Determine unit based on fPort
    let (unit, quantity) = match f_port {
        1 => ("m³", "Volume"),
        2 => ("kWh", "Energy"),
        3 => ("L", "Volume"),
        4 => ("MWh", "Energy"),
        5 => ("kg", "Mass"),
        6 => ("t", "Mass"),
        _ => ("units", "Count"),
    };

    let mut readings = vec![Reading {
        value: MBusRecordValue::Numeric(frame.counter as f64),
        unit: unit.to_string(),
        quantity: quantity.to_string(),
        tariff: None,
        storage_number: Some(0),
        description: Some(format!("Device {:#010X}", frame.device_id)),
    }];

    // Add temperature if present
    if let Some(temp_raw) = frame.temperature {
        readings.push(Reading {
            value: MBusRecordValue::Numeric(temp_raw as f64 / 10.0),
            unit: "°C".to_string(),
            quantity: "Temperature".to_string(),
            tariff: None,
            storage_number: Some(1),
            description: Some("Ambient temperature".to_string()),
        });
    }

    // Add pressure if present
    if let Some(pressure_raw) = frame.pressure {
        readings.push(Reading {
            value: MBusRecordValue::Numeric(pressure_raw as f64 / 10.0),
            unit: "hPa".to_string(),
            quantity: "Pressure".to_string(),
            tariff: None,
            storage_number: Some(2),
            description: Some("Atmospheric pressure".to_string()),
        });
    }

    MeteringData {
        timestamp: SystemTime::now(),
        readings,
        battery: Some(BatteryStatus {
            voltage: None,
            percentage: Some(frame.battery),
            low_battery: frame.battery < 20,
        }),
        status: DeviceStatus {
            alarm: (frame.status & 0x01) != 0,
            tamper: (frame.status & 0x02) != 0,
            leak: (frame.status & 0x04) != 0,
            reverse_flow: (frame.status & 0x08) != 0,
            error_code: if (frame.status & 0xFF00) != 0 {
                Some((frame.status >> 8) & 0xFF)
            } else {
                None
            },
            flags: frame.status as u32,
        },
        raw_payload: raw_payload.to_vec(),
        decoder_type: "CompactFrame".to_string(),
    }
}

// TODO: Fix this function - WMBusFrame doesn't have data_records field
/*
/// Parse wM-Bus frame and convert to MeteringData
pub fn parse_wmbus_to_metering_data(input: &[u8]) -> IResult<&[u8], MeteringData> {
    // Try to parse as wM-Bus frame
    let (remaining, frame) = parse_wmbus_frame(input)
        .map_err(|_| nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Tag)))?;

    let mut readings = Vec::new();
    let mut battery = None;
    let mut status = DeviceStatus::default();

    // Convert data records to readings
    for record in frame.data_records {
        readings.push(Reading {
            value: record.value.clone(),
            unit: record.unit,
            quantity: record.quantity,
            tariff: Some(record.tariff as u8),
            storage_number: Some(record.storage_number),
            description: record.description,
        });
    }

    // Extract device status
    if let Some(dev_status) = frame.device_status {
        status.alarm = dev_status.alarm;
        status.tamper = dev_status.tamper;
        status.leak = dev_status.leak;
        status.reverse_flow = dev_status.reverse_flow;
        status.error_code = dev_status.error_code;
        status.flags = dev_status.flags;
    }

    // Extract battery status
    if let Some(batt) = frame.battery_status {
        battery = Some(BatteryStatus {
            voltage: batt.voltage,
            percentage: batt.percentage,
            low_battery: batt.low_battery,
        });
    }

    Ok((
        remaining,
        MeteringData {
            timestamp: frame.timestamp.unwrap_or_else(SystemTime::now),
            readings,
            battery,
            status,
            raw_payload: input.to_vec(),
            decoder_type: "WMBusCompact".to_string(),
        },
    ))
}
*/

/// Compact frame decoder using nom
#[derive(Debug, Clone)]
pub struct CompactFrameNomDecoder {
    pub manufacturer_id: Option<u16>,
    pub parse_extended: bool,
}

impl Default for CompactFrameNomDecoder {
    fn default() -> Self {
        Self {
            manufacturer_id: None,
            parse_extended: true,
        }
    }
}

impl LoRaPayloadDecoder for CompactFrameNomDecoder {
    fn decode(&self, payload: &[u8], f_port: u8) -> Result<MeteringData, LoRaDecodeError> {
        // TODO: Fix parse_wmbus_to_metering_data function
        // First try to parse as standard wM-Bus frame
        // if let Ok((_, data)) = parse_wmbus_to_metering_data(payload) {
        //     return Ok(data);
        // }

        // Fall back to simple compact format
        match parse_simple_compact(payload) {
            Ok((_, frame)) => Ok(simple_compact_to_metering_data(frame, f_port, payload)),
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => {
                Err(LoRaDecodeError::InvalidData {
                    offset: payload.len() - e.input.len(),
                    reason: format!("Compact frame parse error: {:?}", e.code),
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
        "CompactFrameNom"
    }

    fn clone_box(&self) -> Box<dyn LoRaPayloadDecoder> {
        Box::new(self.clone())
    }

    fn can_decode(&self, payload: &[u8], f_port: u8) -> bool {
        // Check for compact frame markers
        if payload.len() < 11 || f_port == 0 {
            return false;
        }

        // Try parsing
        parse_simple_compact(payload).is_ok() // || parse_wmbus_to_metering_data(payload).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_compact() {
        let payload = vec![
            0x78, 0x56, 0x34, 0x12, // Device ID
            0xE8, 0x03, 0x00, 0x00, // Counter = 1000
            0x01, 0x00, // Status
            85,   // Battery
            0x10, 0x01, // Temperature = 272 (27.2°C)
            0xE8, 0x03, // Pressure = 1000 (100.0 hPa)
        ];

        let (remaining, frame) = parse_simple_compact(&payload).unwrap();

        assert!(remaining.is_empty());
        assert_eq!(frame.device_id, 0x12345678);
        assert_eq!(frame.counter, 1000);
        assert_eq!(frame.battery, 85);
        assert_eq!(frame.temperature, Some(272));
        assert_eq!(frame.pressure, Some(1000));
    }

    #[test]
    fn test_compact_frame_nom_decoder() {
        let decoder = CompactFrameNomDecoder::default();

        let payload = vec![
            0x78, 0x56, 0x34, 0x12, // Device ID
            0x64, 0x00, 0x00, 0x00, // Counter = 100
            0x00, 0x00, // Status
            75,   // Battery
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        assert_eq!(result.readings.len(), 1);
        match &result.readings[0].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 100.0),
            _ => panic!("Expected numeric value"),
        }
        assert_eq!(result.readings[0].unit, "m³");
        assert_eq!(result.battery.as_ref().unwrap().percentage, Some(75));
    }

    #[test]
    fn test_f_port_unit_mapping() {
        let decoder = CompactFrameNomDecoder::default();

        let payload = vec![
            0x00, 0x00, 0x00, 0x00, // Device ID
            0x0A, 0x00, 0x00, 0x00, // Counter = 10
            0x00, 0x00, // Status
            100,  // Battery
        ];

        // Test different fPort values
        let result_energy = decoder.decode(&payload, 2).unwrap();
        assert_eq!(result_energy.readings[0].unit, "kWh");

        let result_water = decoder.decode(&payload, 3).unwrap();
        assert_eq!(result_water.readings[0].unit, "L");
    }
}
