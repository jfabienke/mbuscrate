// Demonstration of dual-path instrumentation with realistic scenarios
use mbus_rs::instrumentation::{
    UnifiedInstrumentation, MeteringReport, Reading, ReadingQuality,
    DeviceType, ProtocolType, DeviceStatus, BatteryStatus, RadioMetrics,
    FrameStatistics,
};
use std::time::SystemTime;
use std::collections::HashMap;

fn main() {
    println!("=== Dual-Path Instrumentation Demo ===\n");

    // Scenario 1: Normal operation - all readings good
    scenario_normal_operation();

    // Scenario 2: Sensor failure - some bad readings
    scenario_sensor_failure();

    // Scenario 3: Data corruption - mixed quality
    scenario_data_corruption();

    // Scenario 4: Battery low with degraded readings
    scenario_battery_low();

    // Show data flow diagram
    demonstrate_data_flow();
}

fn scenario_normal_operation() {
    println!("📊 Scenario 1: Normal Operation");
    println!("{}", "-".repeat(40));

    let mut inst = UnifiedInstrumentation::new(
        "12345678".to_string(),
        "KAM".to_string(),
        ProtocolType::WMBusMode("T1".to_string()),
    );

    inst.device_type = DeviceType::WaterMeter;
    inst.model = Some("MULTICAL 21".to_string());
    inst.version = Some("1.5".to_string());

    // All good readings
    inst.readings = vec![
        Reading {
            name: "Volume".to_string(),
            value: 1234.567,
            unit: "m³".to_string(),
            timestamp: SystemTime::now(),
            tariff: None,
            storage_number: Some(0),
            quality: ReadingQuality::Good,
        },
        Reading {
            name: "Temperature".to_string(),
            value: 18.5,
            unit: "°C".to_string(),
            timestamp: SystemTime::now(),
            tariff: None,
            storage_number: Some(0),
            quality: ReadingQuality::Good,
        },
    ];

    inst.bad_readings = None; // No bad readings

    inst.battery_status = Some(BatteryStatus {
        voltage: Some(3.0),
        percentage: Some(75),
        low_battery: false,
        estimated_days_remaining: Some(1825),
    });

    let metering = MeteringReport::from_unified(&inst);

    println!("✅ Status: All systems operational");
    println!("   Metering: {} good readings", metering.readings.len());
    println!("   Instrumentation: No errors detected");
    println!("   Battery: 75% (1825 days remaining)");
    println!();
}

fn scenario_sensor_failure() {
    println!("⚠️ Scenario 2: Sensor Failure");
    println!("{}", "-".repeat(40));

    let mut inst = UnifiedInstrumentation::new(
        "87654321".to_string(),
        "ELS".to_string(),
        ProtocolType::MBusWired,
    );

    inst.device_type = DeviceType::HeatMeter;
    inst.model = Some("SensoStar".to_string());

    // Mix of good and bad readings
    inst.readings = vec![
        Reading {
            name: "Energy".to_string(),
            value: 15678.9,
            unit: "kWh".to_string(),
            timestamp: SystemTime::now(),
            tariff: Some(1),
            storage_number: Some(0),
            quality: ReadingQuality::Good,
        },
        Reading {
            name: "Flow Temperature".to_string(),
            value: 65.2,
            unit: "°C".to_string(),
            timestamp: SystemTime::now(),
            tariff: None,
            storage_number: Some(0),
            quality: ReadingQuality::Good,
        },
    ];

    // Bad readings from failed sensor
    inst.bad_readings = Some(vec![
        Reading {
            name: "Return Temperature".to_string(),
            value: 250.0, // Out of bounds
            unit: "°C".to_string(),
            timestamp: SystemTime::now(),
            tariff: None,
            storage_number: Some(0),
            quality: ReadingQuality::Invalid,
        },
        Reading {
            name: "Flow Rate".to_string(),
            value: -5.0, // Negative flow
            unit: "m³/h".to_string(),
            timestamp: SystemTime::now(),
            tariff: None,
            storage_number: Some(0),
            quality: ReadingQuality::Invalid,
        },
    ]);

    inst.device_status = DeviceStatus {
        alarm: true,
        tamper: false,
        leak_detected: false,
        reverse_flow: false,
        burst_detected: false,
        dry_running: false,
        error_code: Some(0x02),
        error_description: Some("Temperature sensor failure".to_string()),
        additional_flags: HashMap::new(),
    };

    let metering = MeteringReport::from_unified(&inst);

    println!("⚠️ Status: Sensor failure detected");
    println!("   Metering: {} good readings (partial data)", metering.readings.len());
    println!("   Instrumentation: {} bad readings detected",
        inst.bad_readings.as_ref().map(|b| b.len()).unwrap_or(0));
    println!("   Error: Temperature sensor failure (code 0x02)");
    println!("   Action: Service required");
    println!();
}

fn scenario_data_corruption() {
    println!("❌ Scenario 3: Data Corruption");
    println!("{}", "-".repeat(40));

    let mut inst = UnifiedInstrumentation::new(
        "98765432".to_string(),
        "ELV".to_string(),
        ProtocolType::WMBusMode("C1".to_string()),
    );

    inst.device_type = DeviceType::ElectricityMeter;
    inst.model = Some("AS3000".to_string());

    // Some good readings
    inst.readings = vec![
        Reading {
            name: "Active Energy Import".to_string(),
            value: 45678.123,
            unit: "kWh".to_string(),
            timestamp: SystemTime::now(),
            tariff: Some(1),
            storage_number: Some(0),
            quality: ReadingQuality::Good,
        },
    ];

    // Corrupted data
    inst.bad_readings = Some(vec![
        Reading {
            name: "Active Energy Export".to_string(),
            value: -1234.567, // Negative energy
            unit: "kWh".to_string(),
            timestamp: SystemTime::now(),
            tariff: Some(1),
            storage_number: Some(0),
            quality: ReadingQuality::Invalid,
        },
        Reading {
            name: "Frequency".to_string(),
            value: 500.0, // Should be ~50Hz
            unit: "Hz".to_string(),
            timestamp: SystemTime::now(),
            tariff: None,
            storage_number: Some(0),
            quality: ReadingQuality::Invalid,
        },
        Reading {
            name: "Current L1".to_string(),
            value: f64::NAN, // NaN value
            unit: "A".to_string(),
            timestamp: SystemTime::now(),
            tariff: None,
            storage_number: Some(0),
            quality: ReadingQuality::Invalid,
        },
    ]);

    inst.frame_statistics = FrameStatistics {
        frames_received: 23456,
        frames_valid: 23450,
        crc_errors: 6,
        decryption_errors: 3,
        parsing_errors: 5,
        last_frame_time: Some(SystemTime::now()),
    };

    inst.device_status.error_code = Some(0x10);
    inst.device_status.error_description = Some("Data integrity error".to_string());

    let metering = MeteringReport::from_unified(&inst);

    println!("❌ Status: Data corruption detected");
    println!("   Metering: {} readings salvaged", metering.readings.len());
    println!("   Instrumentation: {} corrupted readings",
        inst.bad_readings.as_ref().map(|b| b.len()).unwrap_or(0));
    println!("   Frame errors: {} CRC, {} decrypt, {} parse",
        inst.frame_statistics.crc_errors,
        inst.frame_statistics.decryption_errors,
        inst.frame_statistics.parsing_errors);
    println!("   Action: Check communication link");
    println!();
}

fn scenario_battery_low() {
    println!("🔋 Scenario 4: Low Battery with Degraded Readings");
    println!("{}", "-".repeat(40));

    let mut inst = UnifiedInstrumentation::new(
        "lora_env_42".to_string(),
        "Unknown".to_string(),
        ProtocolType::LoRa,
    );

    inst.device_type = DeviceType::TemperatureSensor;

    // Some readings degraded due to low battery
    inst.readings = vec![
        Reading {
            name: "Temperature".to_string(),
            value: 22.5,
            unit: "°C".to_string(),
            timestamp: SystemTime::now(),
            tariff: None,
            storage_number: None,
            quality: ReadingQuality::Good,
        },
        Reading {
            name: "Humidity".to_string(),
            value: 65.0,
            unit: "%".to_string(),
            timestamp: SystemTime::now(),
            tariff: None,
            storage_number: None,
            quality: ReadingQuality::Estimated, // Degraded but acceptable
        },
    ];

    inst.bad_readings = Some(vec![
        Reading {
            name: "Pressure".to_string(),
            value: 0.0, // Sensor shutdown due to low power
            unit: "hPa".to_string(),
            timestamp: SystemTime::now(),
            tariff: None,
            storage_number: None,
            quality: ReadingQuality::Invalid,
        },
    ]);

    inst.battery_status = Some(BatteryStatus {
        voltage: Some(2.1),
        percentage: Some(5),
        low_battery: true,
        estimated_days_remaining: Some(7),
    });

    inst.radio_metrics = Some(RadioMetrics {
        rssi_dbm: Some(-95), // Weak signal due to low power
        snr_db: Some(3.0),
        frequency_hz: Some(868300000),
        spreading_factor: Some(12), // Max SF to save power
        bandwidth_khz: Some(125),
        packet_counter: Some(8901),
    });

    inst.device_status.alarm = true;
    inst.device_status.error_description = Some("Low battery - replace soon".to_string());

    let metering = MeteringReport::from_unified(&inst);

    println!("🔋 Status: Low battery affecting operations");
    println!("   Battery: 5% (⚠️ 7 days remaining)");
    println!("   Metering: {} readings (partial)", metering.readings.len());
    println!("   Instrumentation: {} sensors offline",
        inst.bad_readings.as_ref().map(|b| b.len()).unwrap_or(0));
    println!("   Radio: RSSI {} dBm (weak signal)",
        inst.radio_metrics.as_ref().and_then(|r| r.rssi_dbm).unwrap_or(0));
    println!("   Action: Replace battery immediately");
    println!();
}

// Helper to show data flow
fn demonstrate_data_flow() {
    println!("\n📈 Data Flow Summary:");
    println!("{}", "=".repeat(50));
    println!();
    println!("  [Device] ──┬──> [Validation] ──┬──> [Metering Path]");
    println!("             │                   │     └─> Clean data only");
    println!("             │                   │     └─> CSV/JSON export");
    println!("             │                   │     └─> Analytics/Billing");
    println!("             │                   │");
    println!("             └───────────────────┴──> [Instrumentation Path]");
    println!("                                       └─> All data + diagnostics");
    println!("                                       └─> Bad readings preserved");
    println!("                                       └─> Device health metrics");
    println!("                                       └─> Troubleshooting/Alerts");
}