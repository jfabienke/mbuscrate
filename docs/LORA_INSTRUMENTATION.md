# LoRa Device Instrumentation Information

This document describes the device instrumentation metadata available through LoRa transmissions in the mbus-rs crate.

## Overview

The crate provides comprehensive instrumentation data from LoRa devices at multiple levels:
1. **Radio-level metadata** - Physical layer information (RSSI, SNR, packet statistics)
2. **Device health metrics** - Battery status, tamper detection, alarms
3. **Environmental data** - Temperature, humidity, pressure from sensors
4. **Network statistics** - Packet counters, error rates, timestamps

## 1. Radio-Level Metadata

### LoRaPacketStatus
Located in `src/wmbus/radio/lora/params.rs` and `src/wmbus/radio/modulation.rs`

```rust
pub struct LoRaPacketStatus {
    pub rssi_pkt_dbm: i16,        // Received Signal Strength Indicator in dBm
    pub snr_pkt_db: f32,           // Signal-to-Noise Ratio in dB
    pub signal_rssi_pkt_dbm: i16, // Signal RSSI (processed) in dBm
}
```

**Typical values:**
- RSSI: -30 dBm (strong) to -120 dBm (weak)
- SNR: -20 dB (poor) to +10 dB (excellent)
- Signal RSSI: Adjusted RSSI accounting for noise floor

### RadioPacketInfo
Located in `src/wmbus/radio/radio_driver.rs`

```rust
pub struct RadioPacketInfo {
    pub rssi_dbm: i16,              // RSSI at packet reception
    pub freq_error_hz: Option<i32>, // Frequency error in Hz
    pub lqi: Option<u8>,             // Link Quality Indicator (0-255)
    pub timestamp: SystemTime,       // Packet reception timestamp
}
```

### RadioStats
Located in `src/wmbus/radio/driver.rs` and `src/wmbus/radio/radio_driver.rs`

```rust
pub struct RadioStats {
    pub packets_received: u32,     // Total packets received
    pub packets_crc_valid: u32,    // Packets with valid CRC
    pub packets_crc_error: u32,    // Packets with CRC errors
    pub packets_transmitted: u32,  // Total packets sent
    pub last_rssi_dbm: i16,       // Most recent RSSI reading
}
```

### Link Budget Calculation
The link budget can be estimated from radio parameters:
- **Path loss** = Tx Power - RSSI
- **Fade margin** = RSSI - Receiver Sensitivity
- **Link quality** = SNR indicates demodulation margin

## 2. Device Health Metrics

### BatteryStatus
Located in `src/wmbus/radio/lora/decoder.rs`

```rust
pub struct BatteryStatus {
    pub voltage: Option<f32>,      // Battery voltage in volts
    pub percentage: Option<u8>,    // Battery level 0-100%
    pub low_battery: bool,         // Low battery warning flag
}
```

**Interpretation:**
- Voltage < 2.5V typically indicates low battery for lithium cells
- Percentage < 20% triggers low battery warnings
- Some devices only report percentage, others only voltage

### DeviceStatus
Located in `src/wmbus/radio/lora/decoder.rs`

```rust
pub struct DeviceStatus {
    pub alarm: bool,              // General alarm condition
    pub tamper: bool,             // Physical tamper detected
    pub leak: bool,               // Water leak detected (water meters)
    pub reverse_flow: bool,       // Reverse flow detected
    pub error_code: Option<u16>,  // Manufacturer error codes
    pub flags: u32,               // Additional status bits
}
```

**Common error codes:**
- 0x0001: Communication error
- 0x0002: Sensor failure
- 0x0004: Memory error
- 0x0008: Configuration error
- 0x0010: Calibration needed

## 3. Environmental Data from Decoders

### Temperature Sensors
Available from multiple decoder types:

**Cayenne LPP** (`decoders/nom/cayenne_lpp.rs`):
- Temperature: -327.68°C to +327.67°C (0.1°C resolution)
- Barometric pressure: 0 to 655.35 hPa
- Humidity: 0-100% (0.5% resolution)

**Dragino** (`decoders/dragino.rs`):
- Internal temperature from device
- External temperature probes
- Temperature compensation for measurements

**Sensative** (`decoders/sensative.rs`):
- Built-in temperature sensor
- Door/window state detection
- Light intensity measurements

### Metering Data
Located in `src/wmbus/radio/lora/decoder.rs`

```rust
pub struct MeteringData {
    pub device_id: String,
    pub timestamp: SystemTime,
    pub readings: Vec<Reading>,
    pub battery: Option<BatteryStatus>,
    pub status: DeviceStatus,
    pub raw_payload: Vec<u8>,
    pub decoder_type: String,
}

pub struct Reading {
    pub name: String,
    pub value: f64,
    pub unit: String,
}
```

**Common readings by meter type:**
- **Water meters**: Volume (m³), flow rate (m³/h), temperature (°C)
- **Electricity meters**: Energy (kWh), power (kW), voltage (V), current (A)
- **Gas meters**: Volume (m³), flow rate, pressure (bar)
- **Heat meters**: Energy (MWh), flow, supply/return temperatures

## 4. Network Statistics

### Channel Assessment
Located in `src/wmbus/radio/driver.rs`

```rust
pub struct LbtConfig {
    pub rssi_threshold_dbm: i16,  // Channel clear threshold
    pub listen_duration_ms: u32,  // Listen before talk duration
    pub max_retries: u8,          // Retry attempts if busy
}
```

**Regulatory compliance:**
- EU: -85 dBm threshold, 5ms listen time
- Provides channel occupancy information
- Helps optimize transmission timing

### Packet Reception Quality

```rust
// From RadioPacketInfo
let reception_quality = match rssi_dbm {
    r if r > -50 => "Excellent",
    r if r > -70 => "Good",
    r if r > -90 => "Fair",
    r if r > -110 => "Poor",
    _ => "Very Poor"
};

let snr_quality = match snr_db {
    s if s > 10.0 => "Excellent",
    s if s > 0.0 => "Good",
    s if s > -10.0 => "Fair",
    _ => "Poor"
};
```

## 5. Decoder-Specific Instrumentation

### OMS Decoder
- Manufacturer ID
- Device version and generation
- Medium type (water, electricity, gas, heat)
- Access number (transmission counter)
- Status byte with alarm flags

### DLMS/COSEM Decoder
- OBIS codes for standardized data points
- Security counters
- Authentication status
- Association level

### Manufacturer-Specific

**Elvaco CMi4110** (Water/Heat):
- Multiple temperature sensors
- Pressure measurements
- Extended status with 16 different alarm types

**Elvaco CMe3100** (Electricity):
- Three-phase measurements
- Power quality indicators
- Tariff information

**Dragino**:
- GPS coordinates (for mobile devices)
- Motion detection
- Configurable thresholds and alarms

**Decentlab**:
- Multi-sensor support (up to 64 sensors)
- Sensor health diagnostics
- Calibration coefficients

## 6. Usage Examples

### Accessing Radio Metadata

```rust
use mbus::wmbus::radio::lora::{LoRaDeviceManager, LoRaPacketStatus};

// After receiving a packet
let packet_status = LoRaPacketStatus {
    rssi_pkt_dbm: -75,
    snr_pkt_db: 8.5,
    signal_rssi_pkt_dbm: -73,
};

// Evaluate link quality
if packet_status.rssi_pkt_dbm > -90 && packet_status.snr_pkt_db > 0.0 {
    println!("Good link quality");
}
```

### Monitoring Device Health

```rust
use mbus::wmbus::radio::lora::decoder::{MeteringData, DeviceStatus};

fn check_device_health(data: &MeteringData) {
    // Check battery
    if let Some(battery) = &data.battery {
        if battery.low_battery {
            println!("WARNING: Low battery on device {}", data.device_id);
        }
        if let Some(voltage) = battery.voltage {
            println!("Battery voltage: {:.2}V", voltage);
        }
    }
    
    // Check alarms
    if data.status.tamper {
        println!("ALERT: Tamper detected on device {}", data.device_id);
    }
    if data.status.leak {
        println!("ALERT: Leak detected on device {}", data.device_id);
    }
}
```

### Network Quality Monitoring

```rust
use mbus::wmbus::radio::driver::RadioStats;

fn analyze_network_quality(stats: &RadioStats) {
    let packet_loss = if stats.packets_received > 0 {
        (stats.packets_crc_error as f32 / stats.packets_received as f32) * 100.0
    } else {
        0.0
    };
    
    println!("Packet loss rate: {:.1}%", packet_loss);
    println!("Last RSSI: {} dBm", stats.last_rssi_dbm);
    
    if packet_loss > 5.0 {
        println!("WARNING: High packet loss detected");
    }
    if stats.last_rssi_dbm < -100 {
        println!("WARNING: Weak signal strength");
    }
}
```

## 7. Best Practices

### Signal Quality Thresholds
- **Excellent**: RSSI > -70 dBm, SNR > 10 dB
- **Good**: RSSI > -85 dBm, SNR > 0 dB
- **Acceptable**: RSSI > -95 dBm, SNR > -5 dB
- **Poor**: RSSI < -95 dBm or SNR < -5 dB

### Battery Monitoring
- Alert at 20% remaining capacity
- Critical at 10% remaining capacity
- Voltage thresholds depend on battery chemistry:
  - Lithium: < 2.5V is low
  - Alkaline: < 1.2V per cell is low

### Alarm Prioritization
1. **Critical**: Tamper, leak, reverse flow
2. **High**: Low battery, sensor failure
3. **Medium**: Communication errors, missed readings
4. **Low**: Configuration issues, time sync needed

### Data Logging Recommendations
Always log:
- Device ID and timestamp
- RSSI and SNR for each packet
- Battery status changes
- All alarm conditions
- Decoder type used
- Raw payload for debugging

## 8. Troubleshooting Guide

### Weak Signal Issues
**Symptoms**: RSSI < -100 dBm, high packet loss
**Solutions**:
- Check antenna placement and orientation
- Reduce distance or add repeaters
- Increase transmission power (if allowed)
- Use lower data rate (higher spreading factor)

### Battery Drain Issues
**Symptoms**: Rapid battery percentage decrease
**Solutions**:
- Reduce transmission frequency
- Optimize payload size
- Check for excessive retransmissions
- Verify sleep mode operation

### Decoding Failures
**Symptoms**: Unknown payload format, partial decoding
**Solutions**:
- Check FormatDetector confidence scores
- Verify device configuration matches decoder
- Use SmartDecoderV2 for automatic fallback
- Examine raw payload for patterns

### Interference Problems
**Symptoms**: Variable RSSI, SNR < -10 dB
**Solutions**:
- Use LBT (Listen Before Talk)
- Change frequency channel
- Implement frequency hopping
- Add time diversity (retransmissions)

## Summary

The mbus-rs crate provides comprehensive instrumentation data for LoRa metering devices:

1. **Physical layer**: RSSI, SNR, packet statistics for link quality assessment
2. **Device health**: Battery, tamper, alarms for maintenance planning
3. **Application data**: Meter readings with proper units and timestamps
4. **Network performance**: Error rates, channel utilization for optimization

This instrumentation enables:
- Predictive maintenance through battery and sensor monitoring
- Network optimization through signal quality analysis
- Security monitoring through tamper and alarm detection
- Compliance verification through regulatory parameter tracking

All instrumentation data is accessible through the unified `MeteringData` structure, making it easy to build monitoring and alerting systems on top of the LoRa metering infrastructure.