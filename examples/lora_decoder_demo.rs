//! Example demonstrating LoRa payload decoder usage with device manager
//!
//! This example shows how to:
//! - Register different decoders for specific devices
//! - Decode payloads from various meter types
//! - Handle unknown devices with fallback decoders

use mbus_rs::wmbus::radio::lora::decoder::{
    DecentlabConfig, DecoderType, DraginoModel, GenericCounterConfig, LoRaDeviceManager,
};

fn main() {
    // Create a device manager
    let mut manager = LoRaDeviceManager::new();

    // Register specific decoders for known devices

    // Water meter using generic counter format
    let water_meter_config = GenericCounterConfig {
        big_endian: false,
        counter_size: 4,
        unit: "L".to_string(),
        scale_factor: 1.0 / 10.0, // 10 pulses per liter
        has_timestamp: false,
        has_battery: true,
    };
    manager.register_device("00112233".to_string(), DecoderType::GenericCounter(water_meter_config));

    // Dragino SW3L flow sensor
    manager.register_device("AABBCCDD".to_string(), DecoderType::Dragino(DraginoModel::SW3L));

    // Decentlab pressure sensor
    let decentlab_config = DecentlabConfig {
        protocol_version: 2,
        channels: vec![], // Will be auto-detected
    };
    manager.register_device("12345678".to_string(), DecoderType::Decentlab(decentlab_config));

    // EN 13757-3 Compact frame decoder as default
    manager.set_default_decoder(DecoderType::En13757Compact);

    // Example 1: Decode water meter data
    println!("=== Water Meter Example ===");
    let water_payload = vec![
        0x10, 0x27, 0x00, 0x00, // Counter = 10000 pulses = 1000L
        0x64, 0x00, // Delta = 100 pulses = 10L
        0x00, // Status OK
        85,   // Battery 85%
    ];

    match manager.decode_payload("00112233", &water_payload, 1) {
        Ok(data) => {
            println!("Decoder: {}", data.decoder_type);
            for reading in &data.readings {
                println!(
                    "  {} = {:?} {}",
                    reading.quantity, reading.value, reading.unit
                );
            }
            if let Some(battery) = &data.battery {
                println!("  Battery: {}%", battery.percentage.unwrap_or(0));
            }
        }
        Err(e) => println!("Decode error: {}", e),
    }

    // Example 2: Decode Dragino SW3L
    println!("\n=== Dragino SW3L Example ===");
    let dragino_payload = vec![
        0x12, 0x34, // Device ID
        0x00, // Status OK
        0xE8, 0x03, // Flow rate = 100 L/h
        0x10, 0x27, 0x00, 0x00, // Total = 10L
        0x10, 0x09, // Temperature = 23.20°C
        0xE4, 0x0C, // Battery = 3300mV
    ];

    match manager.decode_payload("AABBCCDD", &dragino_payload, 1) {
        Ok(data) => {
            println!("Decoder: {}", data.decoder_type);
            for reading in &data.readings {
                if let Some(desc) = &reading.description {
                    println!(
                        "  {} ({}) = {:?} {}",
                        desc, reading.quantity, reading.value, reading.unit
                    );
                }
            }
        }
        Err(e) => println!("Decode error: {}", e),
    }

    // Example 3: Unknown device falls back to default decoder
    println!("\n=== Unknown Device Example ===");
    let unknown_payload = vec![
        0x78, 0x56, 0x34, 0x12, // Device ID
        0xE8, 0x03, 0x00, 0x00, // Counter
        0x01, 0x00, // Status
        85,   // Battery
    ];

    match manager.decode_payload("UNKNOWN", &unknown_payload, 1) {
        Ok(data) => {
            println!("Decoder: {} (fallback)", data.decoder_type);
            println!("  Decoded {} readings", data.readings.len());
        }
        Err(e) => println!("Decode error: {}", e),
    }

    // Example 4: Auto-detect decoder
    println!("\n=== Auto-detect Example ===");
    if let Some(detected) = manager.auto_detect_decoder(&dragino_payload, 1) {
        println!("Auto-detected decoder: {}", detected);
    } else {
        println!("Could not auto-detect decoder");
    }

    // Example 5: Raw binary fallback
    println!("\n=== Raw Binary Example ===");
    let raw_payload = vec![0x01, 0x02, 0x03, 0x04, 0x05];

    // Create a manager with only raw binary decoder
    let raw_manager = LoRaDeviceManager::new();

    match raw_manager.decode_payload("ANY", &raw_payload, 1) {
        Ok(data) => {
            println!("Decoder: {}", data.decoder_type);
            for reading in &data.readings {
                println!("  {} = {:?}", reading.quantity, reading.value);
            }
        }
        Err(e) => println!("Decode error: {}", e),
    }
}
