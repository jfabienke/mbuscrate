//! Comprehensive tests for the vendor extension system

use mbus_rs::{
    VendorExtension, VendorRegistry, VendorDataRecord, VendorVariable, VendorDeviceInfo,
    MBusError, MBusFrame, MBusRecord, MBusRecordValue,
    UnifiedInstrumentation, DeviceType, ProtocolType,
    from_mbus_frame, from_wmbus_frame, from_vendor_device_info,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

/// Mock vendor extension implementation for Kamstrup devices
struct KamstrupExtension;

impl VendorExtension for KamstrupExtension {
    fn handle_dif_manufacturer_block(
        &self,
        manufacturer_id: &str,
        dif: u8,
        remaining_payload: &[u8],
    ) -> Result<Option<Vec<VendorDataRecord>>, MBusError> {
        if manufacturer_id != "KAM" {
            return Ok(None);
        }

        match dif {
            0x0F if remaining_payload.len() >= 4 => {
                // Custom Kamstrup data block
                let value = u32::from_le_bytes([
                    remaining_payload[0],
                    remaining_payload[1],
                    remaining_payload[2],
                    remaining_payload[3],
                ]) as f64;

                let record = VendorDataRecord {
                    dif,
                    vif: 0xFF,
                    unit: "Kamstrup Units".to_string(),
                    value: VendorVariable::Numeric(value),
                    quantity: "Kamstrup Proprietary".to_string(),
                };
                Ok(Some(vec![record]))
            }
            _ => Ok(None),
        }
    }

    fn parse_vif_manufacturer_specific(
        &self,
        manufacturer_id: &str,
        vif: u8,
        data: &[u8],
    ) -> Result<Option<(String, i8, String, VendorVariable)>, MBusError> {
        if manufacturer_id != "KAM" || vif != 0xFF {
            return Ok(None);
        }

        // Kamstrup-specific VIF interpretation
        let var = VendorVariable::Numeric(1234.56);
        Ok(Some((
            "Kamstrup Flow Rate".to_string(),
            -3, // Scale factor
            "m³/h".to_string(),
            var,
        )))
    }

    fn handle_ci_manufacturer_range(
        &self,
        manufacturer_id: &str,
        ci: u8,
        payload: &[u8],
    ) -> Result<Option<VendorDataRecord>, MBusError> {
        if manufacturer_id != "KAM" || ci < 0xA0 || ci > 0xB7 {
            return Ok(None);
        }

        // Kamstrup command response
        let record = VendorDataRecord {
            dif: 0x00,
            vif: ci,
            unit: "Command Response".to_string(),
            value: VendorVariable::Binary(payload.to_vec()),
            quantity: format!("Kamstrup Command 0x{:02X}", ci),
        };
        Ok(Some(record))
    }

    fn decode_status_bits(
        &self,
        manufacturer_id: &str,
        status_byte: u8,
    ) -> Result<Option<Vec<VendorVariable>>, MBusError> {
        if manufacturer_id != "KAM" {
            return Ok(None);
        }

        let vendor_bits = (status_byte >> 5) & 0x07;
        let mut vars = Vec::new();

        if vendor_bits & 0x01 != 0 {
            vars.push(VendorVariable::Boolean(true));
        }
        if vendor_bits & 0x02 != 0 {
            vars.push(VendorVariable::Boolean(true));
        }
        if vendor_bits & 0x04 != 0 {
            vars.push(VendorVariable::Boolean(true));
        }

        if vars.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vars))
        }
    }

    fn enrich_device_header(
        &self,
        manufacturer_id: &str,
        mut basic_info: VendorDeviceInfo,
    ) -> Result<Option<VendorDeviceInfo>, MBusError> {
        if manufacturer_id != "KAM" {
            return Ok(None);
        }

        // Enrich with Kamstrup-specific info
        basic_info.model = Some(match basic_info.device_type {
            0x07 => "MULTICAL 21".to_string(),
            0x04 => "MULTICAL 403".to_string(),
            _ => "MULTICAL".to_string(),
        });

        basic_info.firmware_version = Some(format!("v{}.{}",
            basic_info.version >> 4,
            basic_info.version & 0x0F
        ));

        basic_info.additional_info.insert(
            "kamstrup_series".to_string(),
            "MULTICAL".to_string(),
        );

        Ok(Some(basic_info))
    }

    fn provision_key(
        &self,
        manufacturer_id: &str,
        device_info: &VendorDeviceInfo,
        _frame_data: &[u8],
    ) -> Result<Option<[u8; 16]>, MBusError> {
        if manufacturer_id != "KAM" {
            return Ok(None);
        }

        // Generate key based on Kamstrup algorithm (example)
        let mut key = [0u8; 16];
        let device_bytes = device_info.device_id.to_be_bytes();
        let mfr_bytes = device_info.manufacturer_id.to_be_bytes();

        // Simple key derivation for testing
        for i in 0..4 {
            key[i] = device_bytes[i];
            key[i + 4] = device_bytes[i + 4];
            key[i + 8] = mfr_bytes[0];
            key[i + 12] = mfr_bytes[1];
        }

        Ok(Some(key))
    }
}

#[test]
fn test_kamstrup_dif_manufacturer_block() {
    let registry = VendorRegistry::new();
    registry.register("KAM", Arc::new(KamstrupExtension)).unwrap();

    // Test DIF 0x0F with manufacturer data
    let payload = vec![0x12, 0x34, 0x56, 0x78, 0xAB, 0xCD];
    let extension = registry.get("KAM").unwrap();
    let result = extension.handle_dif_manufacturer_block("KAM", 0x0F, &payload).unwrap();

    assert!(result.is_some());
    let records = result.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].quantity, "Kamstrup Proprietary");

    if let VendorVariable::Numeric(value) = records[0].value {
        assert_eq!(value, 0x78563412 as f64); // Little-endian
    } else {
        panic!("Expected numeric value");
    }
}

#[test]
fn test_kamstrup_vif_parsing() {
    let registry = VendorRegistry::new();
    registry.register("KAM", Arc::new(KamstrupExtension)).unwrap();

    let extension = registry.get("KAM").unwrap();
    let result = extension.parse_vif_manufacturer_specific("KAM", 0xFF, &[]).unwrap();

    assert!(result.is_some());
    let (quantity, exp, unit, var) = result.unwrap();
    assert_eq!(quantity, "Kamstrup Flow Rate");
    assert_eq!(exp, -3);
    assert_eq!(unit, "m³/h");

    if let VendorVariable::Numeric(value) = var {
        assert_eq!(value, 1234.56);
    } else {
        panic!("Expected numeric value");
    }
}

#[test]
fn test_kamstrup_ci_commands() {
    let registry = VendorRegistry::new();
    registry.register("KAM", Arc::new(KamstrupExtension)).unwrap();

    let extension = registry.get("KAM").unwrap();
    let payload = vec![0x01, 0x02, 0x03];
    let result = extension.handle_ci_manufacturer_range("KAM", 0xA5, &payload).unwrap();

    assert!(result.is_some());
    let record = result.unwrap();
    assert_eq!(record.quantity, "Kamstrup Command 0xA5");

    if let VendorVariable::Binary(data) = record.value {
        assert_eq!(data, vec![0x01, 0x02, 0x03]);
    } else {
        panic!("Expected binary value");
    }
}

#[test]
fn test_kamstrup_status_bits() {
    let registry = VendorRegistry::new();
    registry.register("KAM", Arc::new(KamstrupExtension)).unwrap();

    let extension = registry.get("KAM").unwrap();

    // Test with all vendor bits set (bits 7:5 = 0b111)
    let status_byte = 0b11100000;
    let result = extension.decode_status_bits("KAM", status_byte).unwrap();

    assert!(result.is_some());
    let vars = result.unwrap();
    assert_eq!(vars.len(), 3);

    // Verify all alerts are present
    let alert_names: Vec<String> = vars.iter().map(|v| {
        if let VendorVariable::Boolean(_) = v {
            "Alert".to_string() // Generic alert name
        } else {
            String::new()
        }
    }).collect();

    assert!(alert_names.contains(&"Kamstrup Alert 1".to_string()));
    assert!(alert_names.contains(&"Kamstrup Alert 2".to_string()));
    assert!(alert_names.contains(&"Kamstrup Alert 3".to_string()));
}

#[test]
fn test_kamstrup_device_enrichment() {
    let registry = VendorRegistry::new();
    registry.register("KAM", Arc::new(KamstrupExtension)).unwrap();

    let extension = registry.get("KAM").unwrap();

    let basic_info = VendorDeviceInfo {
        manufacturer_id: 0x2D2C, // KAM
        device_id: 0x12345678,
        version: 0x15, // Version 1.5
        device_type: 0x07, // Water meter
        model: None,
        serial_number: Some("12345678".to_string()),
        firmware_version: None,
        additional_info: HashMap::new(),
    };

    let result = extension.enrich_device_header("KAM", basic_info).unwrap();
    assert!(result.is_some());

    let enriched = result.unwrap();
    assert_eq!(enriched.model, Some("MULTICAL 21".to_string()));
    assert_eq!(enriched.firmware_version, Some("v1.5".to_string()));
    assert_eq!(
        enriched.additional_info.get("kamstrup_series"),
        Some(&"MULTICAL".to_string())
    );
}

#[test]
fn test_kamstrup_key_provisioning() {
    let registry = VendorRegistry::new();
    registry.register("KAM", Arc::new(KamstrupExtension)).unwrap();

    let extension = registry.get("KAM").unwrap();

    let device_info = VendorDeviceInfo {
        manufacturer_id: 0x2D2C,
        device_id: 0x12345678,
        version: 0x01,
        device_type: 0x07,
        model: None,
        serial_number: Some("12345678".to_string()),
        firmware_version: None,
        additional_info: HashMap::new(),
    };

    let result = extension.provision_key("KAM", &device_info, &[]).unwrap();
    assert!(result.is_some());

    let key = result.unwrap();
    // Verify key structure (first 8 bytes are device ID)
    assert_eq!(&key[0..4], &0x12345678u32.to_be_bytes());
    assert_eq!(&key[4..8], &0x00000000u32.to_be_bytes());
}

#[test]
fn test_unified_instrumentation_from_vendor() {
    let device_info = VendorDeviceInfo {
        manufacturer_id: 0x2D2C,
        device_id: 0x87654321,
        version: 0x10,
        device_type: 0x04, // Heat meter
        model: Some("MULTICAL 403".to_string()),
        serial_number: Some("87654321".to_string()),
        firmware_version: Some("v2.1".to_string()),
        additional_info: HashMap::new(),
    };

    let inst = from_vendor_device_info(&device_info, ProtocolType::WMBusMode("T1".to_string()));

    assert_eq!(inst.device_id, "87654321");
    assert_eq!(inst.manufacturer, "KAM");
    assert!(matches!(inst.device_type, DeviceType::HeatMeter));
    assert_eq!(inst.model, Some("MULTICAL 403".to_string()));
    assert_eq!(inst.version, Some("16".to_string()));
}

#[test]
fn test_registry_thread_safety() {
    use std::thread;
    use std::sync::Arc;

    let registry = Arc::new(VendorRegistry::new());

    // Register from one thread
    let reg_clone = registry.clone();
    let handle1 = thread::spawn(move || {
        reg_clone.register("KAM", Arc::new(KamstrupExtension)).unwrap();
    });

    handle1.join().unwrap();

    // Access from multiple threads
    let mut handles = vec![];
    for i in 0..10 {
        let reg_clone = registry.clone();
        let handle = thread::spawn(move || {
            // Check extension exists
            assert!(reg_clone.has_extension("KAM"));

            // Get extension
            let ext = reg_clone.get("KAM");
            assert!(ext.is_some());

            // Test a hook
            if let Some(extension) = ext {
                let result = extension.decode_status_bits("KAM", 0x20 * i).unwrap();
                // Just verify it doesn't crash
                let _ = result;
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_integration_with_mbus_frame() {
    // Create a mock M-Bus frame
    let frame = MBusFrame {
        frame_type: mbus_rs::MBusFrameType::Long,
        control: 0x08,
        address: 1,
        control_information: 0x72,
        data: vec![],
        checksum: 0,
        more_records_follow: false,
    };

    // Create some records with vendor-specific data
    let records = vec![
        MBusRecord {
            timestamp: SystemTime::now(),
            storage_number: 0,
            tariff: -1,
            device: -1,
            is_numeric: true,
            value: MBusRecordValue::Numeric(1234.56),
            unit: "m³".to_string(),
            function_medium: String::new(),
            quantity: "Volume".to_string(),
            drh: Default::default(),
            data_len: 0,
            data: [0; 256],
            more_records_follow: 0,
        },
    ];

    // Convert to unified instrumentation
    let inst = from_mbus_frame(&frame, &records, None);

    assert_eq!(inst.device_id, "1");
    assert_eq!(inst.readings.len(), 1);
    assert_eq!(inst.readings[0].name, "Volume");
    assert_eq!(inst.readings[0].value, 1234.56);
    assert_eq!(inst.readings[0].unit, "m³");

    // Verify JSON serialization works
    let json = inst.to_json().unwrap();
    assert!(json.contains("\"device_id\":\"1\""));
    assert!(json.contains("\"Volume\""));
}

#[test]
fn test_no_vendor_extension_fallback() {
    let registry = VendorRegistry::new();
    // Don't register any extensions

    // All operations should return None without errors
    let extension = registry.get("UNKNOWN");
    assert!(extension.is_none());

    assert!(!registry.has_extension("UNKNOWN"));

    let manufacturers = registry.registered_manufacturers();
    assert!(manufacturers.is_empty());
}