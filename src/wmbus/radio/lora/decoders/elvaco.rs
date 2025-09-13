//! Elvaco meter decoder for LoRa payloads
//!
//! Handles Elvaco's CMi and CMe series meters which often use M-Bus-like structures.

use crate::payload::record::MBusRecordValue;
use crate::wmbus::radio::lora::decoder::{
    helpers, BatteryStatus, DeviceStatus, ElvacoModel, LoRaDecodeError, LoRaPayloadDecoder,
    MeteringData, Reading,
};
use std::time::SystemTime;

/// Elvaco meter decoder
#[derive(Debug, Clone)]
pub struct ElvacoDecoder {
    pub model: ElvacoModel,
}

impl ElvacoDecoder {
    pub fn new(model: ElvacoModel) -> Self {
        Self { model }
    }
}

impl LoRaPayloadDecoder for ElvacoDecoder {
    fn decode(&self, payload: &[u8], _f_port: u8) -> Result<MeteringData, LoRaDecodeError> {
        match self.model {
            ElvacoModel::CMi4110 => decode_cmi4110(payload),
            ElvacoModel::CMe3100 => decode_cme3100(payload),
            ElvacoModel::Generic => decode_elvaco_generic(payload),
        }
    }

    fn decoder_type(&self) -> &str {
        "Elvaco"
    }

    fn clone_box(&self) -> Box<dyn LoRaPayloadDecoder> {
        Box::new(self.clone())
    }

    fn can_decode(&self, payload: &[u8], _f_port: u8) -> bool {
        // Check for Elvaco-specific markers
        if payload.len() < 10 {
            return false;
        }

        // Elvaco meters typically start with specific patterns
        match self.model {
            ElvacoModel::CMi4110 => {
                // CMi4110 has a specific header structure
                payload.len() >= 20 && (payload[0] == 0x78 || payload[0] == 0x79)
            }
            ElvacoModel::CMe3100 => {
                // CMe3100 electricity meter format
                payload.len() >= 24 && (payload[0] & 0xF0) == 0x40
            }
            ElvacoModel::Generic => {
                // Generic Elvaco format check
                payload.len() >= 12
            }
        }
    }
}

/// Decode CMi4110 Water/Heat meter
fn decode_cmi4110(payload: &[u8]) -> Result<MeteringData, LoRaDecodeError> {
    if payload.len() < 20 {
        return Err(LoRaDecodeError::InvalidLength {
            expected: 20,
            actual: payload.len(),
        });
    }

    // CMi4110 typical format:
    // [0]: Status/Type (0x78 for water, 0x79 for heat)
    // [1-4]: Device ID (4 bytes, BCD)
    // [5-8]: Volume (4 bytes, little-endian, in liters or m³)
    // [9-12]: Flow rate (4 bytes, little-endian, in L/h)
    // [13-14]: Temperature 1 (2 bytes, little-endian, 0.01°C)
    // [15-16]: Temperature 2 (2 bytes, little-endian, 0.01°C) - for heat meters
    // [17-18]: Power (2 bytes, little-endian, in W) - for heat meters
    // [19]: Battery/Status byte

    let medium_type = payload[0];
    let device_id = helpers::read_le_uint(payload, 1, 4)?;
    let volume = helpers::read_le_uint(payload, 5, 4)? as f64;
    let flow_rate = helpers::read_le_uint(payload, 9, 4)? as f64;

    let mut readings = vec![];

    // Determine units based on medium type
    let (volume_unit, volume_scale) = if medium_type == 0x78 {
        // Water meter
        ("m³", 0.001) // Convert from liters to m³
    } else {
        // Heat meter
        ("MWh", 0.001) // Convert from kWh to MWh
    };

    readings.push(Reading {
        value: MBusRecordValue::Numeric(volume * volume_scale),
        unit: volume_unit.to_string(),
        quantity: if medium_type == 0x78 {
            "Volume"
        } else {
            "Energy"
        }
        .to_string(),
        tariff: None,
        storage_number: Some(0),
        description: Some(format!("Device {device_id:08X}")),
    });

    readings.push(Reading {
        value: MBusRecordValue::Numeric(flow_rate),
        unit: "L/h".to_string(),
        quantity: "Flow Rate".to_string(),
        tariff: None,
        storage_number: Some(1),
        description: Some("Current flow rate".to_string()),
    });

    // Temperature readings
    if payload.len() >= 15 {
        let temp1 = helpers::read_le_uint(payload, 13, 2)? as f64 / 100.0;
        readings.push(Reading {
            value: MBusRecordValue::Numeric(temp1),
            unit: "°C".to_string(),
            quantity: if medium_type == 0x78 {
                "Temperature"
            } else {
                "Flow Temperature"
            }
            .to_string(),
            tariff: None,
            storage_number: Some(2),
            description: Some("Temperature sensor 1".to_string()),
        });
    }

    if payload.len() >= 17 && medium_type == 0x79 {
        let temp2 = helpers::read_le_uint(payload, 15, 2)? as f64 / 100.0;
        readings.push(Reading {
            value: MBusRecordValue::Numeric(temp2),
            unit: "°C".to_string(),
            quantity: "Return Temperature".to_string(),
            tariff: None,
            storage_number: Some(3),
            description: Some("Temperature sensor 2".to_string()),
        });
    }

    if payload.len() >= 19 && medium_type == 0x79 {
        let power = helpers::read_le_uint(payload, 17, 2)? as f64;
        readings.push(Reading {
            value: MBusRecordValue::Numeric(power),
            unit: "W".to_string(),
            quantity: "Power".to_string(),
            tariff: None,
            storage_number: Some(4),
            description: Some("Thermal power".to_string()),
        });
    }

    // Battery status
    let battery = if payload.len() > 19 {
        let battery_byte = payload[19];
        Some(BatteryStatus {
            voltage: None,
            percentage: Some(battery_byte & 0x7F), // Lower 7 bits for percentage
            low_battery: (battery_byte & 0x80) != 0, // MSB for low battery flag
        })
    } else {
        None
    };

    Ok(MeteringData {
        timestamp: SystemTime::now(),
        readings,
        battery,
        status: DeviceStatus::default(),
        raw_payload: payload.to_vec(),
        decoder_type: "Elvaco-CMi4110".to_string(),
    })
}

/// Decode CMe3100 Electricity meter
fn decode_cme3100(payload: &[u8]) -> Result<MeteringData, LoRaDecodeError> {
    if payload.len() < 24 {
        return Err(LoRaDecodeError::InvalidLength {
            expected: 24,
            actual: payload.len(),
        });
    }

    // CMe3100 typical format:
    // [0]: Type/Version (0x4x for electricity)
    // [1-4]: Device ID (4 bytes)
    // [5-8]: Active Energy Import (4 bytes, Wh)
    // [9-12]: Active Energy Export (4 bytes, Wh)
    // [13-14]: Active Power (2 bytes, W)
    // [15-16]: Voltage L1 (2 bytes, 0.1V)
    // [17-18]: Current L1 (2 bytes, mA)
    // [19-20]: Power Factor (2 bytes, 0.001)
    // [21-22]: Frequency (2 bytes, 0.01 Hz)
    // [23]: Status byte

    let device_id = helpers::read_le_uint(payload, 1, 4)?;
    let energy_import = helpers::read_le_uint(payload, 5, 4)? as f64 / 1000.0; // Wh to kWh
    let energy_export = helpers::read_le_uint(payload, 9, 4)? as f64 / 1000.0; // Wh to kWh
    let active_power = helpers::read_le_uint(payload, 13, 2)? as f64;
    let voltage = helpers::read_le_uint(payload, 15, 2)? as f64 / 10.0;
    let current = helpers::read_le_uint(payload, 17, 2)? as f64 / 1000.0; // mA to A
    let power_factor = helpers::read_le_uint(payload, 19, 2)? as f64 / 1000.0;
    let frequency = helpers::read_le_uint(payload, 21, 2)? as f64 / 100.0;
    let status_byte = payload[23];

    let readings = vec![
        Reading {
            value: MBusRecordValue::Numeric(energy_import),
            unit: "kWh".to_string(),
            quantity: "Energy Import".to_string(),
            tariff: None,
            storage_number: Some(0),
            description: Some(format!("Device {device_id:08X} - Active energy import")),
        },
        Reading {
            value: MBusRecordValue::Numeric(energy_export),
            unit: "kWh".to_string(),
            quantity: "Energy Export".to_string(),
            tariff: None,
            storage_number: Some(1),
            description: Some("Active energy export".to_string()),
        },
        Reading {
            value: MBusRecordValue::Numeric(active_power),
            unit: "W".to_string(),
            quantity: "Power".to_string(),
            tariff: None,
            storage_number: Some(2),
            description: Some("Active power".to_string()),
        },
        Reading {
            value: MBusRecordValue::Numeric(voltage),
            unit: "V".to_string(),
            quantity: "Voltage".to_string(),
            tariff: None,
            storage_number: Some(3),
            description: Some("Line voltage L1".to_string()),
        },
        Reading {
            value: MBusRecordValue::Numeric(current),
            unit: "A".to_string(),
            quantity: "Current".to_string(),
            tariff: None,
            storage_number: Some(4),
            description: Some("Line current L1".to_string()),
        },
        Reading {
            value: MBusRecordValue::Numeric(power_factor),
            unit: "".to_string(),
            quantity: "Power Factor".to_string(),
            tariff: None,
            storage_number: Some(5),
            description: Some("Power factor".to_string()),
        },
        Reading {
            value: MBusRecordValue::Numeric(frequency),
            unit: "Hz".to_string(),
            quantity: "Frequency".to_string(),
            tariff: None,
            storage_number: Some(6),
            description: Some("Grid frequency".to_string()),
        },
    ];

    let status = DeviceStatus {
        alarm: (status_byte & 0x01) != 0,
        tamper: (status_byte & 0x02) != 0,
        reverse_flow: (status_byte & 0x04) != 0, // Reverse power flow
        flags: status_byte as u32,
        ..Default::default()
    };

    Ok(MeteringData {
        timestamp: SystemTime::now(),
        readings,
        battery: None, // Electricity meters typically don't have batteries
        status,
        raw_payload: payload.to_vec(),
        decoder_type: "Elvaco-CMe3100".to_string(),
    })
}

/// Decode generic Elvaco format
fn decode_elvaco_generic(payload: &[u8]) -> Result<MeteringData, LoRaDecodeError> {
    if payload.len() < 12 {
        return Err(LoRaDecodeError::InvalidLength {
            expected: 12,
            actual: payload.len(),
        });
    }

    // Generic Elvaco format (simplified):
    // [0-3]: Device ID
    // [4-7]: Primary value (depends on meter type)
    // [8-9]: Secondary value
    // [10]: Status
    // [11]: Battery/RSSI

    let device_id = helpers::read_le_uint(payload, 0, 4)?;
    let primary_value = helpers::read_le_uint(payload, 4, 4)? as f64;
    let secondary_value = helpers::read_le_uint(payload, 8, 2)? as f64;
    let status_byte = payload[10];
    let battery_byte = payload[11];

    let readings = vec![
        Reading {
            value: MBusRecordValue::Numeric(primary_value),
            unit: "units".to_string(),
            quantity: "Primary Value".to_string(),
            tariff: None,
            storage_number: Some(0),
            description: Some(format!("Device {device_id:08X} primary reading")),
        },
        Reading {
            value: MBusRecordValue::Numeric(secondary_value),
            unit: "units".to_string(),
            quantity: "Secondary Value".to_string(),
            tariff: None,
            storage_number: Some(1),
            description: Some("Secondary reading".to_string()),
        },
    ];

    let battery = Some(BatteryStatus {
        voltage: None,
        percentage: Some(battery_byte & 0x7F),
        low_battery: (battery_byte & 0x80) != 0,
    });

    let status = DeviceStatus {
        alarm: (status_byte & 0x01) != 0,
        tamper: (status_byte & 0x02) != 0,
        leak: (status_byte & 0x04) != 0,
        reverse_flow: (status_byte & 0x08) != 0,
        flags: status_byte as u32,
        ..Default::default()
    };

    Ok(MeteringData {
        timestamp: SystemTime::now(),
        readings,
        battery,
        status,
        raw_payload: payload.to_vec(),
        decoder_type: "Elvaco-Generic".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmi4110_water_meter() {
        let decoder = ElvacoDecoder::new(ElvacoModel::CMi4110);

        let payload = vec![
            0x78, // Water meter type
            0x12, 0x34, 0x56, 0x78, // Device ID
            0xE8, 0x03, 0x00, 0x00, // Volume: 1000L
            0x64, 0x00, 0x00, 0x00, // Flow: 100 L/h
            0x10, 0x09, // Temp: 23.20°C
            0x00, 0x00, // Unused
            0x00, 0x00, // Unused
            0x55, // Battery: 85%, not low
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        assert_eq!(result.readings.len(), 3); // Volume, Flow, Temperature

        // Check volume (1000L = 1m³)
        match &result.readings[0].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 1.0),
            _ => panic!("Expected numeric volume"),
        }
        assert_eq!(result.readings[0].unit, "m³");

        // Check battery
        assert_eq!(result.battery.as_ref().unwrap().percentage, Some(85));
        assert!(!result.battery.as_ref().unwrap().low_battery);
    }

    #[test]
    fn test_cme3100_electricity_meter() {
        let decoder = ElvacoDecoder::new(ElvacoModel::CMe3100);

        let payload = vec![
            0x42, // Electricity type
            0xAB, 0xCD, 0xEF, 0x01, // Device ID
            0x10, 0x27, 0x00, 0x00, // Import: 10000 Wh = 10 kWh
            0x00, 0x00, 0x00, 0x00, // Export: 0
            0xE8, 0x03, // Power: 1000W
            0x5C, 0x09, // Voltage: 240.0V (2396 * 0.1)
            0xD0, 0x07, // Current: 2000mA = 2A
            0xE8, 0x03, // PF: 1.000
            0x88, 0x13, // Frequency: 50.00 Hz
            0x00, // Status: OK
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        assert_eq!(result.readings.len(), 7); // All electrical parameters

        // Check energy import
        match &result.readings[0].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 10.0),
            _ => panic!("Expected numeric energy"),
        }
        assert_eq!(result.readings[0].unit, "kWh");

        // Check voltage
        match &result.readings[3].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 239.6),
            _ => panic!("Expected numeric voltage"),
        }
        assert_eq!(result.readings[3].unit, "V");
    }
}
