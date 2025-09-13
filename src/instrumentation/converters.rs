//! Converters from specific device types to unified instrumentation

use super::{
    UnifiedInstrumentation, ProtocolType, Reading, ReadingQuality,
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
// use crate::wmbus::radio::lora::decoder::MeteringData; // LoRa module disabled
use std::time::SystemTime;

/// Convert M-Bus frame and records to unified instrumentation
pub fn from_mbus_frame(
    frame: &MBusFrame,
    records: &[MBusRecord],
    secondary_addr: Option<&SecondaryAddress>,
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

        inst.readings.push(reading);
    }

    // Set frame statistics
    inst.frame_statistics.frames_received = 1;
    inst.frame_statistics.frames_valid = 1;
    inst.frame_statistics.last_frame_time = Some(SystemTime::now());

    inst
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

/* LoRa module disabled - requires additional fixes
/// Convert LoRa metering data to unified instrumentation
pub fn from_lora_metering_data(
    data: &MeteringData,
    rssi: Option<i16>,
    snr: Option<f32>,
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
        inst.battery_status = Some(BatteryStatus {
            voltage: battery.voltage,
            percentage: battery.percentage,
            low_battery: battery.low_battery,
            estimated_days_remaining: None,
        });
    }

    // Convert device status
    inst.device_status = DeviceStatus {
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
    for reading in &data.readings {
        // Convert MBusRecordValue to f64
        let value = match &reading.value {
            MBusRecordValue::Numeric(n) => *n,
            MBusRecordValue::String(_) => 0.0, // Skip string values for now
        };

        inst.readings.push(Reading {
            name: reading.quantity.clone(),
            value,
            unit: reading.unit.clone(),
            timestamp: data.timestamp,
            tariff: None,
            storage_number: None,
            quality: ReadingQuality::Good,
        });
    }

    inst.timestamp = data.timestamp;
    inst.raw_payload = Some(data.raw_payload.clone());

    inst
}
*/

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
            manufacturer_id: 0x2D2C, // KAM
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
}