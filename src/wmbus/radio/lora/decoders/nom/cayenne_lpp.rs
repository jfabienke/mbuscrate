//! Cayenne LPP (Low Power Payload) decoder using nom
//!
//! Cayenne LPP is a popular TLV format for LoRaWAN devices,
//! widely supported by platforms like TTN and ChirpStack.

use crate::payload::record::MBusRecordValue;
use crate::wmbus::radio::lora::decoder::{
    BatteryStatus, DeviceStatus, LoRaDecodeError, LoRaPayloadDecoder, MeteringData, Reading,
};
use nom::{
    bytes::complete::take,
    multi::many0,
    number::complete::u8 as parse_u8,
    IResult,
};
use std::time::SystemTime;

/// Cayenne LPP data types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CayenneType {
    DigitalInput = 0x00,
    DigitalOutput = 0x01,
    AnalogInput = 0x02,
    AnalogOutput = 0x03,
    Illuminance = 0x65,
    Presence = 0x66,
    Temperature = 0x67,
    Humidity = 0x68,
    Accelerometer = 0x71,
    Barometer = 0x73,
    Gyrometer = 0x86,
    GpsLocation = 0x88,
    // Extended types
    Battery = 0x77,
    Voltage = 0x74,
    Current = 0x75,
    Frequency = 0x76,
    Percentage = 0x78,
    Altitude = 0x79,
    Power = 0x80,
    Energy = 0x83,
    Direction = 0x84,
    UnixTime = 0x85,
}

impl CayenneType {
    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x00 => Some(Self::DigitalInput),
            0x01 => Some(Self::DigitalOutput),
            0x02 => Some(Self::AnalogInput),
            0x03 => Some(Self::AnalogOutput),
            0x65 => Some(Self::Illuminance),
            0x66 => Some(Self::Presence),
            0x67 => Some(Self::Temperature),
            0x68 => Some(Self::Humidity),
            0x71 => Some(Self::Accelerometer),
            0x73 => Some(Self::Barometer),
            0x77 => Some(Self::Battery),
            0x74 => Some(Self::Voltage),
            0x75 => Some(Self::Current),
            0x76 => Some(Self::Frequency),
            0x78 => Some(Self::Percentage),
            0x79 => Some(Self::Altitude),
            0x80 => Some(Self::Power),
            0x83 => Some(Self::Energy),
            0x84 => Some(Self::Direction),
            0x85 => Some(Self::UnixTime),
            0x86 => Some(Self::Gyrometer),
            0x88 => Some(Self::GpsLocation),
            _ => None,
        }
    }

    fn data_size(&self) -> usize {
        match self {
            Self::DigitalInput | Self::DigitalOutput | Self::Presence => 1,
            Self::AnalogInput
            | Self::AnalogOutput
            | Self::Temperature
            | Self::Humidity
            | Self::Illuminance
            | Self::Direction => 2,
            Self::Barometer | Self::Altitude => 2,
            Self::Battery | Self::Voltage | Self::Current | Self::Percentage => 2,
            Self::Frequency | Self::Power => 2,
            Self::Energy | Self::UnixTime => 4,
            Self::Accelerometer | Self::Gyrometer => 6,
            Self::GpsLocation => 9,
        }
    }
}

/// Parsed Cayenne LPP value
#[derive(Debug, Clone)]
pub enum CayenneValue {
    Digital(bool),
    Analog(f32),
    Temperature(f32),
    Humidity(f32),
    Illuminance(u16),
    Presence(bool),
    Pressure(f32),
    Accelerometer {
        x: f32,
        y: f32,
        z: f32,
    },
    Gyrometer {
        x: f32,
        y: f32,
        z: f32,
    },
    Gps {
        latitude: f32,
        longitude: f32,
        altitude: f32,
    },
    Battery(f32),
    Voltage(f32),
    Current(f32),
    Frequency(f32),
    Percentage(f32),
    Altitude(f32),
    Power(f32),
    Energy(f32),
    Direction(u16),
    UnixTime(u32),
}

impl CayenneValue {
    fn to_reading(&self, channel: u8) -> Reading {
        let (value, unit, quantity, description) = match self {
            Self::Digital(v) => (
                MBusRecordValue::Numeric(if *v { 1.0 } else { 0.0 }),
                "".to_string(),
                "Digital".to_string(),
                format!("Channel {channel} Digital"),
            ),
            Self::Analog(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "".to_string(),
                "Analog".to_string(),
                format!("Channel {channel} Analog"),
            ),
            Self::Temperature(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "°C".to_string(),
                "Temperature".to_string(),
                format!("Channel {channel} Temperature"),
            ),
            Self::Humidity(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "%".to_string(),
                "Humidity".to_string(),
                format!("Channel {channel} Humidity"),
            ),
            Self::Illuminance(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "lux".to_string(),
                "Illuminance".to_string(),
                format!("Channel {channel} Light"),
            ),
            Self::Presence(v) => (
                MBusRecordValue::Numeric(if *v { 1.0 } else { 0.0 }),
                "".to_string(),
                "Presence".to_string(),
                format!("Channel {channel} Presence"),
            ),
            Self::Pressure(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "hPa".to_string(),
                "Pressure".to_string(),
                format!("Channel {channel} Pressure"),
            ),
            Self::Battery(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "V".to_string(),
                "Battery".to_string(),
                format!("Channel {channel} Battery"),
            ),
            Self::Voltage(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "V".to_string(),
                "Voltage".to_string(),
                format!("Channel {channel} Voltage"),
            ),
            Self::Current(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "A".to_string(),
                "Current".to_string(),
                format!("Channel {channel} Current"),
            ),
            Self::Power(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "W".to_string(),
                "Power".to_string(),
                format!("Channel {channel} Power"),
            ),
            Self::Energy(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "Wh".to_string(),
                "Energy".to_string(),
                format!("Channel {channel} Energy"),
            ),
            Self::Frequency(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "Hz".to_string(),
                "Frequency".to_string(),
                format!("Channel {channel} Frequency"),
            ),
            Self::Percentage(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "%".to_string(),
                "Percentage".to_string(),
                format!("Channel {channel} Percentage"),
            ),
            Self::Altitude(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "m".to_string(),
                "Altitude".to_string(),
                format!("Channel {channel} Altitude"),
            ),
            Self::Direction(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "°".to_string(),
                "Direction".to_string(),
                format!("Channel {channel} Direction"),
            ),
            Self::UnixTime(v) => (
                MBusRecordValue::Numeric(*v as f64),
                "s".to_string(),
                "Timestamp".to_string(),
                format!("Channel {channel} Unix Time"),
            ),
            Self::Accelerometer { x, y, z } => (
                MBusRecordValue::String(format!("X:{x:.3} Y:{y:.3} Z:{z:.3}")),
                "g".to_string(),
                "Accelerometer".to_string(),
                format!("Channel {channel} Accelerometer"),
            ),
            Self::Gyrometer { x, y, z } => (
                MBusRecordValue::String(format!("X:{x:.1} Y:{y:.1} Z:{z:.1}")),
                "°/s".to_string(),
                "Gyrometer".to_string(),
                format!("Channel {channel} Gyrometer"),
            ),
            Self::Gps {
                latitude,
                longitude,
                altitude,
            } => (
                MBusRecordValue::String(format!("{latitude}°, {longitude}°, {altitude}m")),
                "".to_string(),
                "GPS".to_string(),
                format!("Channel {channel} GPS Location"),
            ),
        };

        Reading {
            value,
            unit,
            quantity,
            tariff: None,
            storage_number: Some(channel as u32),
            description: Some(description),
        }
    }
}

/// Parse a single Cayenne LPP TLV entry
pub fn parse_cayenne_tlv(input: &[u8]) -> IResult<&[u8], (u8, CayenneValue)> {
    let (input, channel) = parse_u8(input)?;
    let (input, type_byte) = parse_u8(input)?;

    let cayenne_type = CayenneType::from_byte(type_byte).ok_or_else(|| {
        nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Tag))
    })?;

    let data_size = cayenne_type.data_size();
    let (input, data) = take(data_size)(input)?;

    let value = match cayenne_type {
        CayenneType::DigitalInput | CayenneType::DigitalOutput => {
            CayenneValue::Digital(data[0] != 0)
        }
        CayenneType::Presence => CayenneValue::Presence(data[0] != 0),
        CayenneType::AnalogInput | CayenneType::AnalogOutput => {
            let raw = i16::from_be_bytes([data[0], data[1]]);
            CayenneValue::Analog(raw as f32 / 100.0)
        }
        CayenneType::Temperature => {
            let raw = i16::from_be_bytes([data[0], data[1]]);
            CayenneValue::Temperature(raw as f32 / 10.0)
        }
        CayenneType::Humidity => CayenneValue::Humidity(data[0] as f32 / 2.0),
        CayenneType::Illuminance => {
            let raw = u16::from_be_bytes([data[0], data[1]]);
            CayenneValue::Illuminance(raw)
        }
        CayenneType::Barometer => {
            let raw = u16::from_be_bytes([data[0], data[1]]);
            CayenneValue::Pressure(raw as f32 / 10.0)
        }
        CayenneType::Battery | CayenneType::Voltage => {
            let raw = u16::from_be_bytes([data[0], data[1]]);
            let voltage = raw as f32 / 100.0;
            if cayenne_type == CayenneType::Battery {
                CayenneValue::Battery(voltage)
            } else {
                CayenneValue::Voltage(voltage)
            }
        }
        CayenneType::Current => {
            let raw = u16::from_be_bytes([data[0], data[1]]);
            CayenneValue::Current(raw as f32 / 1000.0)
        }
        CayenneType::Frequency => {
            let raw = u16::from_be_bytes([data[0], data[1]]);
            CayenneValue::Frequency(raw as f32)
        }
        CayenneType::Percentage => CayenneValue::Percentage(data[0] as f32),
        CayenneType::Altitude => {
            let raw = i16::from_be_bytes([data[0], data[1]]);
            CayenneValue::Altitude(raw as f32)
        }
        CayenneType::Power => {
            let raw = u16::from_be_bytes([data[0], data[1]]);
            CayenneValue::Power(raw as f32 / 10.0)
        }
        CayenneType::Energy => {
            let raw = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
            CayenneValue::Energy(raw as f32 / 1000.0)
        }
        CayenneType::Direction => {
            let raw = u16::from_be_bytes([data[0], data[1]]);
            CayenneValue::Direction(raw)
        }
        CayenneType::UnixTime => {
            let raw = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
            CayenneValue::UnixTime(raw)
        }
        CayenneType::Accelerometer => {
            let x = i16::from_be_bytes([data[0], data[1]]) as f32 / 1000.0;
            let y = i16::from_be_bytes([data[2], data[3]]) as f32 / 1000.0;
            let z = i16::from_be_bytes([data[4], data[5]]) as f32 / 1000.0;
            CayenneValue::Accelerometer { x, y, z }
        }
        CayenneType::Gyrometer => {
            let x = i16::from_be_bytes([data[0], data[1]]) as f32 / 100.0;
            let y = i16::from_be_bytes([data[2], data[3]]) as f32 / 100.0;
            let z = i16::from_be_bytes([data[4], data[5]]) as f32 / 100.0;
            CayenneValue::Gyrometer { x, y, z }
        }
        CayenneType::GpsLocation => {
            let lat_raw = i32::from_be_bytes([0, data[0], data[1], data[2]]);
            let lon_raw = i32::from_be_bytes([0, data[3], data[4], data[5]]);
            let alt_raw = i32::from_be_bytes([0, data[6], data[7], data[8]]);

            let latitude = (lat_raw as f32) / 10000.0;
            let longitude = (lon_raw as f32) / 10000.0;
            let altitude = (alt_raw as f32) / 100.0;

            CayenneValue::Gps {
                latitude,
                longitude,
                altitude,
            }
        }
    };

    Ok((input, (channel, value)))
}

/// Parse complete Cayenne LPP payload
pub fn parse_cayenne_lpp(input: &[u8]) -> IResult<&[u8], Vec<(u8, CayenneValue)>> {
    many0(parse_cayenne_tlv)(input)
}

/// Convert Cayenne values to MeteringData
pub fn cayenne_to_metering_data(
    values: Vec<(u8, CayenneValue)>,
    raw_payload: &[u8],
) -> MeteringData {
    let mut readings = Vec::new();
    let mut battery = None;
    let mut timestamp = SystemTime::now();

    for (channel, value) in values {
        // Special handling for battery and timestamp
        match &value {
            CayenneValue::Battery(voltage) => {
                battery = Some(BatteryStatus {
                    voltage: Some(*voltage),
                    percentage: None,
                    low_battery: *voltage < 2.5,
                });
            }
            CayenneValue::UnixTime(unix_time) => {
                timestamp =
                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(*unix_time as u64);
            }
            _ => {}
        }

        readings.push(value.to_reading(channel));
    }

    MeteringData {
        timestamp,
        readings,
        battery,
        status: DeviceStatus::default(),
        raw_payload: raw_payload.to_vec(),
        decoder_type: "CayenneLPP".to_string(),
    }
}

/// Cayenne LPP decoder implementation
#[derive(Debug, Clone)]
pub struct CayenneLppDecoder;

impl Default for CayenneLppDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl CayenneLppDecoder {
    pub fn new() -> Self {
        Self
    }
}

impl LoRaPayloadDecoder for CayenneLppDecoder {
    fn decode(&self, payload: &[u8], _f_port: u8) -> Result<MeteringData, LoRaDecodeError> {
        match parse_cayenne_lpp(payload) {
            Ok((_, values)) => Ok(cayenne_to_metering_data(values, payload)),
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => {
                Err(LoRaDecodeError::InvalidData {
                    offset: payload.len() - e.input.len(),
                    reason: format!("Cayenne LPP parse error: {:?}", e.code),
                })
            }
            Err(nom::Err::Incomplete(_)) => Err(LoRaDecodeError::InvalidLength {
                expected: payload.len() + 1,
                actual: payload.len(),
            }),
        }
    }

    fn decoder_type(&self) -> &str {
        "CayenneLPP"
    }

    fn clone_box(&self) -> Box<dyn LoRaPayloadDecoder> {
        Box::new(self.clone())
    }

    fn can_decode(&self, payload: &[u8], _f_port: u8) -> bool {
        // Try to parse and see if we get valid Cayenne types
        if payload.len() < 3 {
            return false;
        }

        // Check if first bytes look like valid Cayenne
        if payload.len() >= 2 {
            if let Some(typ) = CayenneType::from_byte(payload[1]) {
                let expected_size = 2 + typ.data_size(); // channel + type + data
                return payload.len() >= expected_size;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_temperature() {
        // Channel 1, Temperature type, 23.5°C (235 / 10)
        let payload = vec![0x01, 0x67, 0x00, 0xEB];

        let (remaining, values) = parse_cayenne_lpp(&payload).unwrap();
        assert!(remaining.is_empty());
        assert_eq!(values.len(), 1);

        let (channel, value) = &values[0];
        assert_eq!(*channel, 1);
        match value {
            CayenneValue::Temperature(t) => assert_eq!(*t, 23.5),
            _ => panic!("Expected temperature value"),
        }
    }

    #[test]
    fn test_parse_multiple_sensors() {
        let payload = vec![
            0x01, 0x67, 0x00, 0xEB, // Ch1: Temperature 23.5°C
            0x02, 0x68, 0x64, // Ch2: Humidity 50%
            0x03, 0x73, 0x27, 0x10, // Ch3: Pressure 1000.0 hPa
        ];

        let (_, values) = parse_cayenne_lpp(&payload).unwrap();
        assert_eq!(values.len(), 3);

        // Check humidity
        match &values[1].1 {
            CayenneValue::Humidity(h) => assert_eq!(*h, 50.0),
            _ => panic!("Expected humidity value"),
        }

        // Check pressure
        match &values[2].1 {
            CayenneValue::Pressure(p) => assert_eq!(*p, 1000.0),
            _ => panic!("Expected pressure value"),
        }
    }

    #[test]
    fn test_cayenne_decoder() {
        let decoder = CayenneLppDecoder::new();

        let payload = vec![
            0x01, 0x67, 0x01, 0x10, // Temperature: 27.2°C
            0x02, 0x77, 0x0B, 0xB8, // Battery: 3.0V
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        assert_eq!(result.readings.len(), 2);
        assert_eq!(result.readings[0].quantity, "Temperature");

        // Check battery was extracted
        assert!(result.battery.is_some());
        assert_eq!(result.battery.as_ref().unwrap().voltage, Some(3.0));
    }
}
