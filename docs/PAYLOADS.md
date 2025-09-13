# LoRa Payload Formats and Decoders

This document describes the payload formats supported by the mbus-rs crate for LoRa metering devices, including both implemented decoders and industry standards roadmap.

## Table of Contents
- [Overview](#overview)
- [Implemented Decoders](#implemented-decoders)
- [Industry Standards](#industry-standards)
- [Usage Guide](#usage-guide)
- [Payload Format Reference](#payload-format-reference)
- [Migration Path](#migration-path)

## Overview

The crate provides a flexible decoder framework that supports both standardized metering protocols and manufacturer-specific formats. The system is designed to handle the European metering market's transition from proprietary formats to open standards like OMS and DLMS/COSEM.

### Architecture

```rust
LoRaDeviceManager
├── Standards-based decoders (OMS, DLMS, Cayenne)
├── Manufacturer-specific decoders (Dragino, Decentlab, etc.)
└── Fallback decoder (Raw binary passthrough)
```

## Implemented Decoders

### 1. EN 13757-3 Compact Frame Decoder
**Status**: ✅ Implemented  
**Use Case**: Foundation for OMS, standard wM-Bus frames over LoRa  
**Supported Meters**: Any EN 13757-3 compliant device

```rust
let decoder = CompactFrameDecoder::default();
```

**Payload Structure**:
```
Standard Compact Frame:
[Length:1][C:1][ManufID:2][DeviceID:4][Version:1][Type:1][CI:1][Data:N][CRC:2]

Simplified Format (also supported):
[DeviceID:4][Counter:4][Status:2][Battery:1][Extensions:N]
```

### 2. Generic Counter/Pulse Decoder
**Status**: ✅ Implemented  
**Use Case**: Retrofit pulse counters, simple meters  
**Supported Devices**: Generic pulse output meters

```rust
// Configure for water meter (10 pulses per liter)
let decoder = GenericCounterDecoder::water_meter(10.0);
```

**Payload Formats**:
```
Basic Format:
[Counter:4][Delta:2][Status:1][Battery:1]

With Timestamp:
[Timestamp:4][Counter:4][Delta:2][Status:1][Battery:1]
```

**Configuration Options**:
- `counter_size`: 1-8 bytes
- `big_endian`: true/false
- `scale_factor`: Pulses to units conversion
- `has_timestamp`: Include Unix timestamp
- `has_battery`: Include battery status

### 3. Decentlab Decoder
**Status**: ✅ Implemented  
**Use Case**: Environmental sensors, pressure/temperature monitoring  
**Supported Models**: DL-PR26, DL-TRS12, DL-PAR, custom configurations

```rust
let decoder = DecentlabDecoder::dl_pr26(); // Pressure + Temperature
```

**Payload Format**:
```
[Protocol:1][DeviceID:2][SensorFlags:1][SensorData:N*2][Battery:2]

Protocol: 0x02 (version 2)
SensorFlags: Bitmask indicating which sensors have data
SensorData: 16-bit big-endian values per active sensor
Battery: Voltage in millivolts
```

### 4. Dragino Decoder
**Status**: ✅ Implemented  
**Use Case**: Water flow sensors, leak detectors  
**Supported Models**: SW3L (flow), LWL03A (leak)

```rust
let decoder = DraginoDecoder::new(DraginoModel::SW3L);
```

**SW3L Payload Format**:
```
[DeviceID:2][Status:1][FlowRate:2][TotalVolume:4][Temperature:2][Battery:2]

FlowRate: L/h * 10
TotalVolume: Liters * 1000
Temperature: °C * 100
Battery: mV
```

**LWL03A Payload Format**:
```
[DeviceID:2][LeakStatus:1][LeakTimes:2][LeakDuration:2][Battery:2]
```

### 5. Sensative Strips Decoder
**Status**: ✅ Implemented  
**Use Case**: Multi-sensor environmental monitoring  
**Format**: TLV (Type-Length-Value)

```rust
let decoder = SensativeDecoder::new();
```

**TLV Format**:
```
[Type:1][Length:1][Value:N]...

Types:
0x01: Temperature (2 bytes, 0.01°C)
0x02: Humidity (1 byte, 0.5%)
0x03: Light (2 bytes, lux)
0x04: Door/Window (1 byte, 0/1)
0x05: Presence (1 byte, 0/1)
```

### 6. Raw Binary Decoder
**Status**: ✅ Implemented  
**Use Case**: Unknown devices, debugging, passthrough  
**Output**: Hex-encoded string

```rust
let decoder = RawBinaryDecoder;
```

## Industry Standards

### Priority 1: OMS (Open Metering System)
**Status**: 🚧 Planned (builds on existing wM-Bus)  
**Coverage**: Multi-utility (water, gas, heat, electricity)  
**Adoption**: Widespread in Europe, LoRa Alliance endorsed

OMS is a profile of Wireless M-Bus (EN 13757) with specific:
- Manufacturer IDs (officially registered)
- Data structures for each medium type
- Security modes (AES-128)
- Interoperability certification

**Implementation Plan**:
```rust
pub struct OmsDecoder {
    version: OmsVersion,      // 3.0, 4.0
    medium: MediumType,       // Water, Gas, Heat, Electricity
    manufacturer_id: u16,     // Official OMS code
    encryption: Option<AesKey>,
}
```

### Priority 2: Cayenne LPP
**Status**: 📋 Planned  
**Coverage**: General IoT, prototyping  
**Adoption**: Wide platform support (TTN, Chirpstack)

**Format Specification**:
```
[Channel:1][Type:1][Data:N]...

Common Types:
0x00: Digital Input (1 byte)
0x01: Digital Output (1 byte)
0x02: Analog Input (2 bytes, 0.01 signed)
0x03: Analog Output (2 bytes, 0.01 signed)
0x65: Illuminance (2 bytes, 1 lux)
0x66: Presence (1 byte)
0x67: Temperature (2 bytes, 0.1°C)
0x68: Humidity (1 byte, 0.5%)
0x71: Accelerometer (6 bytes, 0.001G)
0x73: Barometer (2 bytes, 0.1 hPa)
0x86: GPS (9 bytes: lat/lon/alt)
```

### Priority 3: DLMS/COSEM Light
**Status**: 📋 Planned  
**Coverage**: Electricity (expanding to gas/water)  
**Complexity**: High (requires OBIS codes, SCHC compression)

**Key Components**:
- OBIS codes (Object Identification System)
- SCHC compression for LoRaWAN (RFC 9011)
- Simplified profile for LPWAN

**Example OBIS Codes**:
```
1.0.1.8.0.255 - Active energy import (+A)
1.0.2.8.0.255 - Active energy export (-A)
1.0.3.8.0.255 - Reactive energy import (+R)
1.0.15.8.0.255 - Active energy total
7.0.0.3.1.255 - Gas volume
8.0.0.3.1.255 - Water volume
```

### Priority 4: Wize (169 MHz)
**Status**: 📋 Planned  
**Coverage**: Gas (GRDF Gazpar), water  
**Region**: France (millions of meters)

Wize is essentially Wireless M-Bus at 169 MHz with:
- Extended range (up to 20km)
- Lower data rates
- Specific frame formats

### NB-IoT/LTE-M Formats
**Status**: 🔮 Future  
**Protocols**: LwM2M, CoAP, native DLMS  
**Trend**: Rapid adoption for national rollouts

## Usage Guide

### Basic Usage

```rust
use mbuscrate::wmbus::radio::lora::{
    LoRaDeviceManager, 
    GenericCounterDecoder,
    DraginoModel,
};

// Create device manager
let mut manager = LoRaDeviceManager::new();

// Register device-specific decoders
manager.register_device(
    "00112233",
    Box::new(GenericCounterDecoder::water_meter(10.0))
);

manager.register_device(
    "AABBCCDD",
    Box::new(DraginoDecoder::new(DraginoModel::SW3L))
);

// Decode payload
let payload = vec![...];
match manager.decode_payload(device_addr, &payload, f_port) {
    Ok(data) => {
        for reading in data.readings {
            println!("{}: {} {}", 
                reading.quantity, 
                reading.value, 
                reading.unit
            );
        }
    }
    Err(e) => {
        // Handle as raw binary
    }
}
```

### Auto-Detection

```rust
// Let the manager detect the format
if let Some(decoder_type) = manager.auto_detect_decoder(&payload, f_port) {
    println!("Detected format: {}", decoder_type);
}
```

### Custom Decoder Implementation

```rust
use mbuscrate::wmbus::radio::lora::{
    LoRaPayloadDecoder, 
    MeteringData, 
    LoRaDecodeError,
};

struct MyCustomDecoder;

impl LoRaPayloadDecoder for MyCustomDecoder {
    fn decode(&self, payload: &[u8], f_port: u8) -> Result<MeteringData, LoRaDecodeError> {
        // Custom parsing logic
        Ok(MeteringData {
            timestamp: SystemTime::now(),
            readings: vec![...],
            battery: None,
            status: DeviceStatus::default(),
            raw_payload: payload.to_vec(),
            decoder_type: "MyCustom".to_string(),
        })
    }
    
    fn decoder_type(&self) -> &str {
        "MyCustom"
    }
}
```

## Payload Format Reference

### Status Byte Format (Common)
```
Bit 0: Alarm
Bit 1: Tamper
Bit 2: Leak detected
Bit 3: Reverse flow
Bit 4-7: Reserved/Manufacturer specific
```

### Battery Formats
- **Percentage**: 0-100 (direct)
- **Voltage**: ADC value or millivolts
- **Low Battery Flag**: Single bit

### Time Formats
- **Unix Timestamp**: 4 bytes, seconds since epoch
- **Relative**: 2-4 bytes, seconds/minutes since last transmission
- **Scheduled**: Encoded transmission schedule

### Data Type Encodings
- **Integer**: Little-endian (default) or big-endian
- **Float**: IEEE 754 (rare in LoRa due to size)
- **BCD**: Binary-coded decimal (meter IDs)
- **ASCII**: Text strings (reversed byte order in M-Bus)

## Migration Path

### From Proprietary to Standards

1. **Current State**: Manufacturer-specific decoders
   ```rust
   manager.register_device(addr, Box::new(ProprietaryDecoder));
   ```

2. **Transition**: Dual-mode operation
   ```rust
   // Try OMS first, fallback to proprietary
   manager.set_fallback_chain(vec![
       PayloadStandard::OMS,
       PayloadStandard::Proprietary(old_decoder),
   ]);
   ```

3. **Target State**: Standards-only
   ```rust
   manager.set_standard(PayloadStandard::OMS);
   ```

### Validation Tools

```rust
// Verify decoder compatibility
let test_payload = vec![...];
let old_result = old_decoder.decode(&test_payload, f_port)?;
let new_result = oms_decoder.decode(&test_payload, f_port)?;
assert_eq!(old_result.readings, new_result.readings);
```

## Testing

### Unit Tests
Each decoder includes comprehensive tests:
```bash
cargo test lora_decoder_tests
```

### Integration Example
```bash
cargo run --example lora_decoder_demo
```

### Test Vectors
Test payloads are available in `tests/fixtures/lora_payloads/`:
- `oms_water_meter.hex`
- `dragino_sw3l.hex`
- `decentlab_pr26.hex`
- `cayenne_multisensor.hex`

## Performance Considerations

- **Parsing Speed**: ~1-10 μs per payload (depending on complexity)
- **Memory Usage**: Minimal allocations, stack-based parsing where possible
- **Batch Processing**: Manager supports concurrent decoding
- **Caching**: Device decoder mappings are cached

## Security Notes

1. **Payload Validation**: All decoders validate length and structure
2. **Encryption**: OMS/DLMS support AES-128 (keys must be provided)
3. **Authentication**: MIC/CRC validation where applicable
4. **Tamper Detection**: Status flags indicate physical tampering

## Future Enhancements

- [ ] OMS 4.0 full compliance
- [ ] DLMS/COSEM with SCHC compression
- [ ] Cayenne LPP encoder/decoder
- [ ] Wize protocol support
- [ ] NB-IoT/LwM2M integration
- [ ] Payload signature verification
- [ ] Cloud decoder services integration
- [ ] Machine learning-based format detection

## Contributing

To add a new decoder:

1. Implement the `LoRaPayloadDecoder` trait
2. Add tests in `tests/lora_decoder_tests.rs`
3. Document the format in this file
4. Provide example payloads
5. Submit a pull request

## References

- [OMS Specification v4.0](https://oms-group.org/specifications/)
- [EN 13757-3:2018](https://www.en-standard.eu/csn-en-13757-3-communication-systems-for-meters-part-3-dedicated-application-layer/)
- [Cayenne LPP](https://developers.mydevices.com/cayenne/docs/lora/#lora-cayenne-low-power-payload)
- [DLMS/COSEM Green Book](https://www.dlms.com/greenbookedition10)
- [LoRaWAN Payload Codec API](https://resources.lora-alliance.org/home/lorawan-payload-codec-api)
- [Wize Alliance Specifications](https://www.wize-alliance.com/specifications/)