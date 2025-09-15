//! Dual-Path Gateway Example
//!
//! Demonstrates separate metering and instrumentation data paths.
//!
//! - Metering Path: Clean data for analytics/billing (valid readings only)
//! - Instrumentation Path: Full diagnostics including bad readings
//!
//! Example simulates receiving data from various sources and routing
//! it to appropriate channels based on validation.

use mbus_rs::instrumentation::{
    MeteringReport, UnifiedInstrumentation, Reading, ReadingQuality,
    converters::{from_mbus_metering, from_lora_metering, from_mbus_instrumentation, from_lora_instrumentation},
};
use mbus_rs::mbus::frame::{MBusFrame, MBusFrameType};
use mbus_rs::payload::record::{MBusRecord, MBusRecordValue};
use mbus_rs::wmbus::radio::lora::decoder::{
    MeteringData, DeviceStatus, BatteryStatus, Reading as LoRaReading,
};
use std::time::SystemTime;

fn main() {
    println!("=== Dual-Path Gateway Demo ===\n");

    // Simulate M-Bus data with mixed good/bad readings
    demo_mbus_dual_path();

    println!("\n{}\n", "=".repeat(50));

    // Simulate LoRa data with mixed good/bad readings
    demo_lora_dual_path();
}

fn demo_mbus_dual_path() {
    println!("📊 M-Bus Device Data Processing");
    println!("{}", "-".repeat(30));

    let frame = MBusFrame {
        frame_type: MBusFrameType::Long,
        control: 0x08,
        address: 42,
        control_information: 0x72,
        data: vec![],
        checksum: 0,
        more_records_follow: false,
    };

    // Create records with mix of good and bad data
    let records = vec![
        // Good reading - valid volume
        MBusRecord {
            timestamp: SystemTime::now(),
            storage_number: 0,
            tariff: -1,
            device: -1,
            is_numeric: true,
            value: MBusRecordValue::Numeric(1234.567),
            unit: "m³".to_string(),
            function_medium: String::new(),
            quantity: "Volume".to_string(),
            drh: Default::default(),
            data_len: 0,
            data: [0; 256],
            more_records_follow: 0,
        },
        // Bad reading - negative energy (invalid)
        MBusRecord {
            timestamp: SystemTime::now(),
            storage_number: 0,
            tariff: -1,
            device: -1,
            is_numeric: true,
            value: MBusRecordValue::Numeric(-50.0),
            unit: "kWh".to_string(),
            function_medium: String::new(),
            quantity: "Energy".to_string(),
            drh: Default::default(),
            data_len: 0,
            data: [0; 256],
            more_records_follow: 0,
        },
        // Good reading - valid temperature
        MBusRecord {
            timestamp: SystemTime::now(),
            storage_number: 0,
            tariff: -1,
            device: -1,
            is_numeric: true,
            value: MBusRecordValue::Numeric(22.5),
            unit: "°C".to_string(),
            function_medium: String::new(),
            quantity: "Temperature".to_string(),
            drh: Default::default(),
            data_len: 0,
            data: [0; 256],
            more_records_follow: 0,
        },
        // Bad reading - out of bounds temperature
        MBusRecord {
            timestamp: SystemTime::now(),
            storage_number: 0,
            tariff: -1,
            device: -1,
            is_numeric: true,
            value: MBusRecordValue::Numeric(150.0),
            unit: "°C".to_string(),
            function_medium: String::new(),
            quantity: "Temperature Sensor 2".to_string(),
            drh: Default::default(),
            data_len: 0,
            data: [0; 256],
            more_records_follow: 0,
        },
    ];

    // Process for METERING path (clean data only)
    println!("\n🔹 Metering Path (Clean Data):");
    let metering_report = from_mbus_metering(&frame, &records, None);

    println!("  Device ID: {}", metering_report.device_id);
    println!("  Valid Readings: {}", metering_report.readings.len());
    for reading in &metering_report.readings {
        println!("    ✓ {}: {} {}", reading.name, reading.value, reading.unit);
    }

    // Export as JSON
    if let Ok(json) = metering_report.to_json() {
        println!("\n  JSON Output (truncated):");
        let lines: Vec<&str> = json.lines().take(10).collect();
        for line in lines {
            println!("    {}", line);
        }
        if json.lines().count() > 10 {
            println!("    ...");
        }
    }

    // Export as CSV
    let csv = metering_report.to_csv();
    println!("\n  CSV Output:");
    for line in csv.lines() {
        println!("    {}", line);
    }

    // Process for INSTRUMENTATION path (diagnostics only, no good readings)
    println!("\n🔹 Instrumentation Path (Diagnostics Only):");
    let inst = from_mbus_instrumentation(&frame, &records, None);

    println!("  Device ID: {}", inst.device_id);
    println!("  Reading Quality: {:?}", inst.reading_quality);

    if let Some(bad_readings) = &inst.bad_readings {
        println!("  Bad Readings: {}", bad_readings.len());
        for reading in bad_readings {
            println!("    ✗ {}: {} {} (Invalid/Out of bounds)",
                reading.name, reading.value, reading.unit);
        }
    } else {
        println!("  Bad Readings: None (would show Good quality if no errors)");
    }

    // Show instrumentation JSON with bad readings
    if let Ok(json) = inst.to_instrumentation_json() {
        println!("\n  Instrumentation JSON (showing bad_readings):");
        // Find and show bad_readings section
        for line in json.lines() {
            if line.contains("bad_readings") || line.contains("Temperature Sensor 2") || line.contains("Energy") {
                println!("    {}", line);
            }
        }
    }
}

fn demo_lora_dual_path() {
    println!("📡 LoRa Device Data Processing");
    println!("{}", "-".repeat(30));

    // Create LoRa data with mixed good/bad readings
    let data = MeteringData {
        timestamp: SystemTime::now(),
        readings: vec![
            // Good reading
            LoRaReading {
                value: MBusRecordValue::Numeric(65.0),
                unit: "%".to_string(),
                quantity: "Humidity".to_string(),
                tariff: None,
                storage_number: None,
                description: Some("Room sensor".to_string()),
            },
            // Bad reading - out of bounds
            LoRaReading {
                value: MBusRecordValue::Numeric(150.0),
                unit: "%".to_string(),
                quantity: "Humidity Sensor 2".to_string(),
                tariff: None,
                storage_number: None,
                description: Some("Faulty sensor".to_string()),
            },
            // Good reading
            LoRaReading {
                value: MBusRecordValue::Numeric(1013.25),
                unit: "hPa".to_string(),
                quantity: "Pressure".to_string(),
                tariff: None,
                storage_number: None,
                description: Some("Atmospheric pressure".to_string()),
            },
            // Bad reading - negative counter
            LoRaReading {
                value: MBusRecordValue::Numeric(-5.0),
                unit: "count".to_string(),
                quantity: "Counter".to_string(),
                tariff: None,
                storage_number: None,
                description: Some("Event counter".to_string()),
            },
        ],
        battery: Some(BatteryStatus {
            voltage: Some(3.3),
            percentage: Some(75),
            low_battery: false,
        }),
        status: DeviceStatus {
            alarm: false,
            tamper: false,
            leak: false,
            reverse_flow: false,
            error_code: None,
            flags: 0,
        },
        raw_payload: vec![0x01, 0x68, 0x96, 0x02, 0x73, 0x03, 0xF5, 0x27],
        decoder_type: "CayenneLPP".to_string(),
    };

    let rssi = Some(-82i16);
    let snr = Some(9.5f32);

    // Process for METERING path
    println!("\n🔹 Metering Path (Clean Data):");
    let metering_report = from_lora_metering(&data, rssi, snr);

    println!("  Device ID: {}", metering_report.device_id);
    println!("  Valid Readings: {}", metering_report.readings.len());
    for reading in &metering_report.readings {
        println!("    ✓ {}: {} {}", reading.name, reading.value, reading.unit);
    }

    // Process for INSTRUMENTATION path (diagnostics only)
    println!("\n🔹 Instrumentation Path (Diagnostics Only):");
    let inst = from_lora_instrumentation(&data, rssi, snr);

    println!("  Device ID: {}", inst.device_id);
    println!("  Protocol: {:?}", inst.protocol);
    println!("  Reading Quality: {:?}", inst.reading_quality);

    if let Some(radio) = &inst.radio_metrics {
        println!("  Radio Metrics:");
        if let Some(rssi) = radio.rssi_dbm {
            println!("    RSSI: {} dBm", rssi);
        }
        if let Some(snr) = radio.snr_db {
            println!("    SNR: {} dB", snr);
        }
    }

    if let Some(battery) = &inst.battery_status {
        println!("  Battery Status:");
        if let Some(v) = battery.voltage {
            println!("    Voltage: {} V", v);
        }
        if let Some(p) = battery.percentage {
            println!("    Level: {}%", p);
        }
    }

    if let Some(bad_readings) = &inst.bad_readings {
        println!("  Bad Readings: {}", bad_readings.len());
        for reading in bad_readings {
            println!("    ✗ {}: {} {} (Invalid/Out of bounds)",
                reading.name, reading.value, reading.unit);
        }
    } else {
        println!("  Bad Readings: None - all sensors operating normally");
    }

    println!("\n📈 Summary:");
    println!("  - Metering path: Clean, validated data ONLY for business use");
    println!("  - Instrumentation path: Diagnostics ONLY (no duplication of good data)");
    println!("  - Reading Quality field indicates overall health: Good/Substitute/Invalid");
    println!("  - This separation prevents both data duplication and noisy analytics");
}