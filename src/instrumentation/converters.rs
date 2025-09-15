//! Converters from specific device types to unified instrumentation

use super::{
    UnifiedInstrumentation, ProtocolType, Reading, ReadingQuality,
    RadioMetrics, MeteringReport, validate_reading,
};
use crate::mbus::frame::MBusFrame;
use crate::mbus::secondary_addressing::SecondaryAddress;
use crate::payload::record::{MBusDataRecordHeader, MBusRecord, MBusRecordValue, MBusDataInformationBlock, MBusValueInformationBlock};

impl Default for MBusDataRecordHeader {
    fn default() -> Self {
        Self {
            dib: MBusDataInformationBlock {
                dif: 0,
                ndife: 0,
                dife: [0; 10],
            },
            vib: MBusValueInformationBlock {
                vif: 0,
                nvife: 0,
                vife: [0; 10],
                custom_vif: String::new(),
            },
        }
    }
}
use crate::vendors::{VendorDeviceInfo, manufacturer_id_to_string};
use crate::wmbus::frame::WMBusFrame;
use crate::wmbus::radio::lora::decoder::MeteringData;
use std::time::SystemTime;

/// Convert M-Bus frame and records to unified instrumentation
/// If split_readings is true, bad readings will be separated into bad_readings field
/// If instrumentation_only is true, good readings will be excluded (only diagnostics)
pub fn from_mbus_frame_with_split(
    frame: &MBusFrame,
    records: &[MBusRecord],
    secondary_addr: Option<&SecondaryAddress>,
    split_readings: bool,
    instrumentation_only: bool,
) -> UnifiedInstrumentation {
    let mut inst = if let Some(addr) = secondary_addr {
        UnifiedInstrumentation::new(
            format!("{:08X}", addr.device_id),
            manufacturer_id_to_string(addr.manufacturer),
            ProtocolType::MBusWired,
        )
    } else {
        UnifiedInstrumentation::new(
            format!("{}", frame.address),
            "Unknown".to_string(),
            ProtocolType::MBusWired,
        )
    };

    // Set device type from secondary address if available
    if let Some(addr) = secondary_addr {
        inst.set_device_type_from_medium(addr.device_type);
        inst.version = Some(format!("{}", addr.version));
    }

    // Convert records to readings
    let mut good_readings = Vec::new();
    let mut bad_readings = Vec::new();

    for record in records {
        let value = match &record.value {
            MBusRecordValue::Numeric(n) => *n,
            MBusRecordValue::String(s) => {
                // Try to parse string as number, otherwise skip
                match s.parse::<f64>() {
                    Ok(v) => v,
                    Err(_) => continue,
                }
            }
        };

        let mut reading = Reading {
            name: record.quantity.clone(),
            value,
            unit: record.unit.clone(),
            timestamp: record.timestamp,
            tariff: if record.tariff >= 0 { Some(record.tariff as u32) } else { None },
            storage_number: Some(record.storage_number),
            quality: ReadingQuality::Good,
        };

        // Check for error conditions in quantity
        if record.quantity.contains("ERROR") || record.quantity.contains("ALARM") {
            inst.device_status.alarm = true;
            reading.quality = ReadingQuality::Invalid;
        }

        if split_readings {
            // Validate and separate good/bad readings
            if validate_reading(&reading).is_ok() {
                good_readings.push(reading);
            } else {
                bad_readings.push(reading);
            }
        } else {
            // Legacy mode - all readings go to readings field
            inst.readings.push(reading);
        }
    }

    if split_readings {
        // Determine overall reading quality
        if bad_readings.is_empty() {
            inst.reading_quality = ReadingQuality::Good;
        } else if good_readings.is_empty() {
            inst.reading_quality = ReadingQuality::Invalid;
        } else {
            inst.reading_quality = ReadingQuality::Substitute; // Partial data
        }

        // For instrumentation-only mode, exclude good readings
        if instrumentation_only {
            inst.readings = Vec::new(); // Clear good readings for pure instrumentation
            if !bad_readings.is_empty() {
                inst.bad_readings = Some(bad_readings);
            }
        } else {
            // Legacy mode - include good readings
            inst.readings = good_readings;
            if !bad_readings.is_empty() {
                inst.bad_readings = Some(bad_readings);
            }
        }
    }

    // Set frame statistics
    inst.frame_statistics.frames_received = 1;
    inst.frame_statistics.frames_valid = 1;
    inst.frame_statistics.last_frame_time = Some(SystemTime::now());

    inst
}

/// Legacy converter for backward compatibility
pub fn from_mbus_frame(
    frame: &MBusFrame,
    records: &[MBusRecord],
    secondary_addr: Option<&SecondaryAddress>,
) -> UnifiedInstrumentation {
    from_mbus_frame_with_split(frame, records, secondary_addr, false, false)
}

/// Convert M-Bus frame to metering report (good readings only)
pub fn from_mbus_metering(
    frame: &MBusFrame,
    records: &[MBusRecord],
    secondary_addr: Option<&SecondaryAddress>,
) -> MeteringReport {
    let inst = from_mbus_frame_with_split(frame, records, secondary_addr, true, false);
    MeteringReport::from_unified(&inst)
}

/// Convert M-Bus frame to instrumentation report (diagnostics only, no good readings)
pub fn from_mbus_instrumentation(
    frame: &MBusFrame,
    records: &[MBusRecord],
    secondary_addr: Option<&SecondaryAddress>,
) -> UnifiedInstrumentation {
    from_mbus_frame_with_split(frame, records, secondary_addr, true, true)
}

/// Convert wM-Bus frame to unified instrumentation
pub fn from_wmbus_frame(
    frame: &WMBusFrame,
    rssi: Option<i16>,
    packet_stats: Option<(u64, u64, u64)>,
) -> UnifiedInstrumentation {
    let mut inst = UnifiedInstrumentation::new(
        format!("{:08X}", frame.device_address),
        manufacturer_id_to_string(frame.manufacturer_id),
        ProtocolType::WMBusMode("Unknown".to_string()),
    );

    inst.set_device_type_from_medium(frame.device_type);
    inst.version = Some(format!("{}", frame.version));

    // Set radio metrics if available
    if let Some(rssi_dbm) = rssi {
        inst.set_radio_metrics(rssi_dbm, None);
    }

    // Set frame statistics if available
    if let Some((received, valid, errors)) = packet_stats {
        inst.frame_statistics.frames_received = received;
        inst.frame_statistics.frames_valid = valid;
        inst.frame_statistics.crc_errors = errors;
    }

    inst.frame_statistics.last_frame_time = Some(SystemTime::now());
    inst.raw_payload = Some(frame.payload.clone());

    inst
}

/// Convert LoRa metering data to unified instrumentation
/// If split_readings is true, bad readings will be separated into bad_readings field
/// If instrumentation_only is true, good readings will be excluded (only diagnostics)
pub fn from_lora_metering_data_with_split(
    data: &MeteringData,
    rssi: Option<i16>,
    snr: Option<f32>,
    split_readings: bool,
    instrumentation_only: bool,
) -> UnifiedInstrumentation {
    // LoRa data doesn't have device_id/manufacturer_id, use defaults
    let mut inst = UnifiedInstrumentation::new(
        "lora_device".to_string(),
        "Unknown".to_string(),
        ProtocolType::LoRa,
    );

    // Set radio metrics
    if rssi.is_some() || snr.is_some() {
        inst.radio_metrics = Some(RadioMetrics {
            rssi_dbm: rssi,
            snr_db: snr,
            frequency_hz: None,
            spreading_factor: None,
            bandwidth_khz: None,
            packet_counter: None,
        });
    }

    // Convert battery status
    if let Some(battery) = &data.battery {
        inst.battery_status = Some(super::BatteryStatus {
            voltage: battery.voltage,
            percentage: battery.percentage,
            low_battery: battery.low_battery,
            estimated_days_remaining: None,
        });
    }

    // Convert device status
    inst.device_status = super::DeviceStatus {
        alarm: data.status.alarm,
        tamper: data.status.tamper,
        leak_detected: data.status.leak,
        reverse_flow: data.status.reverse_flow,
        burst_detected: false,
        dry_running: false,
        error_code: data.status.error_code,
        error_description: None,
        additional_flags: Default::default(),
    };

    // Convert readings
    let mut good_readings = Vec::new();
    let mut bad_readings = Vec::new();

    for reading in &data.readings {
        // Convert MBusRecordValue to f64
        let value = match &reading.value {
            MBusRecordValue::Numeric(n) => *n,
            MBusRecordValue::String(_) => 0.0, // Skip string values for now
        };

        let reading_struct = Reading {
            name: reading.quantity.clone(),
            value,
            unit: reading.unit.clone(),
            timestamp: data.timestamp,
            tariff: None,
            storage_number: None,
            quality: ReadingQuality::Good,
        };

        if split_readings {
            // Validate and separate good/bad readings
            if validate_reading(&reading_struct).is_ok() {
                good_readings.push(reading_struct);
            } else {
                bad_readings.push(reading_struct);
            }
        } else {
            // Legacy mode - all readings go to readings field
            inst.readings.push(reading_struct);
        }
    }

    if split_readings {
        // Determine overall reading quality
        if bad_readings.is_empty() {
            inst.reading_quality = ReadingQuality::Good;
        } else if good_readings.is_empty() {
            inst.reading_quality = ReadingQuality::Invalid;
        } else {
            inst.reading_quality = ReadingQuality::Substitute; // Partial data
        }

        // For instrumentation-only mode, exclude good readings
        if instrumentation_only {
            inst.readings = Vec::new(); // Clear good readings for pure instrumentation
            if !bad_readings.is_empty() {
                inst.bad_readings = Some(bad_readings);
            }
        } else {
            // Legacy mode - include good readings
            inst.readings = good_readings;
            if !bad_readings.is_empty() {
                inst.bad_readings = Some(bad_readings);
            }
        }
    }

    inst.timestamp = data.timestamp;
    inst.raw_payload = Some(data.raw_payload.clone());

    inst
}

/// Legacy converter for backward compatibility
pub fn from_lora_metering_data(
    data: &MeteringData,
    rssi: Option<i16>,
    snr: Option<f32>,
) -> UnifiedInstrumentation {
    from_lora_metering_data_with_split(data, rssi, snr, false, false)
}

/// Convert LoRa metering data to metering report (good readings only)
pub fn from_lora_metering(
    data: &MeteringData,
    rssi: Option<i16>,
    snr: Option<f32>,
) -> MeteringReport {
    let inst = from_lora_metering_data_with_split(data, rssi, snr, true, false);
    MeteringReport::from_unified(&inst)
}

/// Convert LoRa metering data to instrumentation report (diagnostics only, no good readings)
pub fn from_lora_instrumentation(
    data: &MeteringData,
    rssi: Option<i16>,
    snr: Option<f32>,
) -> UnifiedInstrumentation {
    from_lora_metering_data_with_split(data, rssi, snr, true, true)
}

/// Convert vendor device info to unified instrumentation
pub fn from_vendor_device_info(
    info: &VendorDeviceInfo,
    protocol: ProtocolType,
) -> UnifiedInstrumentation {
    let mut inst = UnifiedInstrumentation::new(
        format!("{:08X}", info.device_id),
        manufacturer_id_to_string(info.manufacturer_id),
        protocol,
    );

    inst.set_device_type_from_medium(info.device_type);
    inst.version = Some(format!("{}", info.version));
    inst.model = info.model.clone();

    // Convert additional info to vendor_data
    if !info.additional_info.is_empty() {
        inst.vendor_data = serde_json::to_value(&info.additional_info).ok();
    }

    inst
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::mbus::frame::MBusFrameType;

    #[test]
    fn test_mbus_frame_conversion() {
        let frame = MBusFrame {
            frame_type: MBusFrameType::Long,
            control: 0x08,
            address: 1,
            control_information: 0x72,
            data: vec![],
            checksum: 0,
            more_records_follow: false,
        };

        let records = vec![
            MBusRecord {
                timestamp: SystemTime::now(),
                storage_number: 0,
                tariff: -1,
                device: -1,
                is_numeric: true,
                value: MBusRecordValue::Numeric(123.45),
                unit: "m³".to_string(),
                function_medium: String::new(),
                quantity: "Volume".to_string(),
                drh: Default::default(),
                data_len: 0,
                data: [0; 256],
                more_records_follow: 0,
            },
        ];

        let inst = from_mbus_frame(&frame, &records, None);

        assert_eq!(inst.device_id, "1");
        assert_eq!(inst.readings.len(), 1);
        assert_eq!(inst.readings[0].value, 123.45);
        assert_eq!(inst.readings[0].unit, "m³");
    }

    #[test]
    fn test_wmbus_frame_conversion() {
        let frame = WMBusFrame {
            length: 0x44,
            control_field: 0x44,
            manufacturer_id: 0x2C2D, // KAM
            device_address: 0x12345678,
            version: 0x01,
            device_type: 0x07, // Water
            control_info: 0x72,
            payload: vec![1, 2, 3],
            crc: 0x1234,
            encrypted: false,
        };

        let inst = from_wmbus_frame(&frame, Some(-75), Some((100, 95, 5)));

        assert_eq!(inst.device_id, "12345678");
        assert_eq!(inst.manufacturer, "KAM");
        assert!(inst.radio_metrics.is_some());
        assert_eq!(inst.radio_metrics.as_ref().unwrap().rssi_dbm, Some(-75));
        assert_eq!(inst.frame_statistics.frames_received, 100);
        assert_eq!(inst.frame_statistics.crc_errors, 5);
    }

    #[test]
    fn test_lora_converter() {
        use crate::wmbus::radio::lora::decoder::{
            MeteringData, DeviceStatus as LoRaDeviceStatus,
            BatteryStatus as LoRaBatteryStatus, Reading as LoRaReading
        };

        let data = MeteringData {
            timestamp: SystemTime::now(),
            readings: vec![
                LoRaReading {
                    value: MBusRecordValue::Numeric(23.5),
                    unit: "°C".to_string(),
                    quantity: "Temperature".to_string(),
                    tariff: None,
                    storage_number: None,
                    description: Some("Sensor 1".to_string()),
                },
                LoRaReading {
                    value: MBusRecordValue::Numeric(65.0),
                    unit: "%".to_string(),
                    quantity: "Humidity".to_string(),
                    tariff: None,
                    storage_number: None,
                    description: Some("Sensor 2".to_string()),
                },
            ],
            battery: Some(LoRaBatteryStatus {
                voltage: Some(3.3),
                percentage: Some(85),
                low_battery: false,
            }),
            status: LoRaDeviceStatus {
                alarm: false,
                tamper: false,
                leak: false,
                reverse_flow: false,
                error_code: None,
                flags: 0,
            },
            raw_payload: vec![0x01, 0x67, 0x00, 0xEB, 0x02, 0x68, 0x82],
            decoder_type: "CayenneLPP".to_string(),
        };

        let inst = from_lora_metering_data(&data, Some(-85), Some(7.5));

        // Verify basic conversion
        assert_eq!(inst.device_id, "lora_device");
        assert_eq!(inst.manufacturer, "Unknown");
        assert!(matches!(inst.protocol, ProtocolType::LoRa));

        // Verify radio metrics
        assert!(inst.radio_metrics.is_some());
        let metrics = inst.radio_metrics.as_ref().unwrap();
        assert_eq!(metrics.rssi_dbm, Some(-85));
        assert_eq!(metrics.snr_db, Some(7.5));

        // Verify battery status
        assert!(inst.battery_status.is_some());
        let battery = inst.battery_status.as_ref().unwrap();
        assert_eq!(battery.voltage, Some(3.3));
        assert_eq!(battery.percentage, Some(85));
        assert!(!battery.low_battery);

        // Verify readings
        assert_eq!(inst.readings.len(), 2);
        assert_eq!(inst.readings[0].name, "Temperature");
        assert_eq!(inst.readings[0].value, 23.5);
        assert_eq!(inst.readings[0].unit, "°C");
        assert_eq!(inst.readings[1].name, "Humidity");
        assert_eq!(inst.readings[1].value, 65.0);
        assert_eq!(inst.readings[1].unit, "%");

        // Verify raw payload
        assert_eq!(inst.raw_payload, Some(vec![0x01, 0x67, 0x00, 0xEB, 0x02, 0x68, 0x82]));
    }
}