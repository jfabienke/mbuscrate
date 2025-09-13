# M-Bus and wM-Bus Instrumentation Support

This document describes the instrumentation metadata available for wired M-Bus and wireless M-Bus (wM-Bus) interfaces in the mbus-rs crate.

## Overview

The crate provides instrumentation data for M-Bus/wM-Bus devices at multiple levels:
1. **Physical layer** - Signal strength (wM-Bus only), error rates
2. **Protocol layer** - Frame statistics, CRC errors, protocol states
3. **Device identification** - Manufacturer, version, device type
4. **Application data** - Meter readings with units, tariffs, storage numbers

## 1. Wired M-Bus Instrumentation

### Frame Structure (`src/mbus/frame.rs`)

```rust
pub struct MBusFrame {
    pub frame_type: MBusFrameType,    // Ack, Short, Control, Long
    pub control: u8,                  // Control field
    pub address: u8,                  // Primary address (1-250)
    pub control_information: u8,      // CI field
    pub data: Vec<u8>,                // Payload data
    pub checksum: u8,                 // Frame checksum
    pub more_records_follow: bool,    // Multi-frame indicator
}
```

### Device Identification (`src/mbus/secondary_addressing.rs`)

```rust
pub struct SecondaryAddress {
    pub device_id: u32,        // 8-digit BCD identification number
    pub manufacturer: u16,     // 3-letter manufacturer code
    pub version: u8,           // Device generation/version
    pub device_type: u8,       // Medium/device type code
}
```

**Device type codes:**
- 0x00: Other
- 0x01: Oil
- 0x02: Electricity
- 0x03: Gas
- 0x04: Heat
- 0x05: Steam
- 0x06: Hot Water
- 0x07: Water
- 0x08: Heat Cost Allocator
- 0x0C: Heat (Volume)
- 0x0D: Compressed Air
- 0x15: Cold Water
- 0x16: Dual Water

### Data Records (`src/payload/record.rs`)

```rust
pub struct MBusRecord {
    pub timestamp: SystemTime,
    pub storage_number: u32,      // Historical data index
    pub tariff: i32,              // Tariff number
    pub device: i32,              // Sub-device number
    pub value: MBusRecordValue,   // Numeric or String
    pub unit: String,             // Physical unit
    pub function_medium: String,  // Function descriptor
    pub quantity: String,         // Quantity type
}
```

### Protocol State Machine (`src/mbus/mbus_protocol.rs`)

The protocol provides:
- **request_user_data()** - REQ_UD2 for current values
- **request_alarm_data()** - REQ_UD1 for alarm/priority data
- **send_control_frame()** - Various control commands
- **scan_secondary_addresses()** - Device discovery

### What's Actually Implemented for Wired M-Bus:

✅ **Fully Supported:**
- Frame parsing/packing with nom
- Primary addressing (1-250)
- Secondary addressing with wildcards
- Data record decoding (DIF/VIF/data)
- Unit conversion and scaling
- Multi-frame support
- Protocol state machine
- Serial communication via tokio-serial

⚠️ **Partially Supported:**
- Error recovery (basic retry logic)
- Baud rate adaptation (structure exists, not fully implemented)

❌ **Not Supported:**
- Device status bytes (alarm flags, battery status)
- Manufacturer-specific VIF extensions
- Encryption (Mode 5, Mode 7)
- Application reset/user data clear
- Time synchronization

## 2. Wireless M-Bus (wM-Bus) Instrumentation

### Radio Packet Info (`src/wmbus/radio/radio_driver.rs`)

```rust
pub struct RadioPacketInfo {
    pub rssi_dbm: i16,              // Received Signal Strength
    pub freq_error_hz: Option<i32>, // Frequency error
    pub lqi: Option<u8>,            // Link Quality Indicator
    pub timestamp: SystemTime,       // Reception timestamp
}
```

### Radio Statistics (`src/wmbus/radio/driver.rs`)

```rust
pub struct RadioStats {
    pub packets_received: u32,
    pub packets_crc_valid: u32,
    pub packets_crc_error: u32,
    pub packets_transmitted: u32,
    pub last_rssi_dbm: i16,
}
```

### Device Discovery (`src/wmbus/handle.rs`)

```rust
pub struct DeviceInfo {
    pub address: u32,
    pub manufacturer: u16,
    pub version: u8,
    pub device_type: u8,
    pub rssi_dbm: i16,          // Signal strength
    pub last_seen: Instant,     // Last communication time
}
```

### Network Topology (`src/wmbus/network.rs`)

```rust
pub struct NetworkTopology {
    pub total_devices: usize,
    pub scan_duration: Duration,
    pub frequencies_scanned: Vec<u32>,
    pub average_rssi: f64,
    pub rssi_distribution: HashMap<String, usize>,  // Signal quality bins
    pub device_type_distribution: HashMap<DeviceCategory, usize>,
    pub manufacturer_distribution: HashMap<u16, usize>,
}
```

**RSSI Quality Bins:**
- Excellent: -40 to 0 dBm
- Good: -70 to -41 dBm
- Fair: -85 to -71 dBm
- Poor: < -85 dBm

### Frame Decoder Statistics (`src/wmbus/frame_decode.rs`)

```rust
pub struct DecodeStats {
    pub frames_processed: u64,
    pub frames_valid: u64,
    pub crc_errors: u64,
    pub header_errors: u64,
    pub encryption_detected: u64,
    pub manufacturer_specific: u64,
}
```

### Enhanced Packet Processing (`src/wmbus/radio/rfm69_packet.rs`)

```rust
pub struct PacketStatistics {
    pub packets_received: u64,
    pub packets_valid: u64,
    pub packets_crc_error: u64,
    pub packets_invalid_header: u64,
    pub packets_encrypted: u64,
    pub fifo_overruns: u64,
}
```

### What's Actually Implemented for wM-Bus:

✅ **Fully Supported:**
- Frame decoding with CRC validation
- RSSI measurement and tracking
- Device discovery and registry
- Network topology analysis
- Packet statistics and error tracking
- Multi-frequency scanning
- Frame type detection (A, B, C modes)

⚠️ **Partially Supported:**
- Radio drivers (SX126x structure exists, RFM69 FSK only)
- Encryption detection (identified but not decrypted)
- LBT (Listen Before Talk) - defined but not active

❌ **Not Supported:**
- Actual LoRa modulation (FSK only)
- Encryption/decryption (AES-128 structure exists, not implemented)
- Frequency hopping
- Repeater mode
- Installation mode

## 3. Error Tracking and Recovery

### Frame Processing Errors

Both M-Bus and wM-Bus track:
- CRC errors with expected vs calculated values
- Header validation failures
- Length mismatches
- Timeout errors
- Serial communication errors

### Error Recovery Strategies

**Wired M-Bus:**
- Automatic retry with configurable attempts
- Timeout adjustment
- State machine reset on persistent errors

**Wireless M-Bus:**
- Packet buffering for burst errors
- RSSI-based filtering
- Automatic frequency switching
- Channel quality assessment

## 4. Practical Usage Examples

### Wired M-Bus Device Scanning

```rust
use mbus::mbus::MBusProtocol;

let mut protocol = MBusProtocol::new(serial_port);

// Scan for all devices
let devices = protocol.scan_primary_addresses(1..=250).await?;

for device in devices {
    println!("Found device at address {}", device);
    
    // Get device data
    let frame = protocol.request_user_data().await?;
    
    // Parse records
    let records = protocol.parse_user_data(&frame)?;
    for record in records {
        println!("{}: {} {}", record.quantity, record.value, record.unit);
    }
}
```

### Wireless M-Bus Network Discovery

```rust
use mbus::wmbus::{WMBusHandle, NetworkConfig};

let config = NetworkConfig {
    scan_duration: Duration::from_secs(30),
    rssi_threshold: -90,  // Minimum signal strength
    max_devices: 100,
};

let mut handle = WMBusHandle::new(hal, config);
let topology = handle.discover_network().await?;

println!("Found {} devices", topology.total_devices);
println!("Average RSSI: {:.1} dBm", topology.average_rssi);

// Get device details
for (address, device) in handle.get_devices() {
    println!("Device {}: RSSI {} dBm, Type: {}", 
             address, device.rssi_dbm, device.device_type);
}
```

## 5. Instrumentation Limitations

### Missing Device Health Metrics

Unlike the LoRa decoders, M-Bus/wM-Bus don't extract:
- Battery status (even though it's in the standard)
- Alarm flags from status bytes
- Tamper detection
- Device error codes
- Operating temperature

### Missing Protocol Features

- No VIF extensions for manufacturer-specific data
- No parsing of status field in data headers
- No support for special VIF codes (0xFD, 0xFB extensions)
- No application-specific error codes

### Radio Limitations

- RFM69: Hardcoded -80 dBm RSSI (not reading actual value)
- SX126x: Driver exists but incomplete
- No SNR measurement
- No real frequency error tracking
- LQI not implemented

## 6. Summary

The crate provides **solid basic instrumentation** for M-Bus/wM-Bus:

**Strengths:**
- Complete frame parsing and data extraction
- Good error tracking and statistics
- Device identification and discovery
- Network topology analysis (wM-Bus)
- Signal strength tracking (wM-Bus)

**Weaknesses:**
- No device health metrics (battery, alarms)
- Limited radio metadata (hardcoded values)
- Missing encryption support
- No status byte interpretation
- Incomplete radio driver implementation

The instrumentation is sufficient for basic meter reading and network management but lacks the comprehensive device health monitoring available in the LoRa decoder implementation.