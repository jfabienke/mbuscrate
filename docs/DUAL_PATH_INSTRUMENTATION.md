# Dual-Path Instrumentation System

## Overview

The dual-path instrumentation system provides clean separation between metering data (for business use) and instrumentation data (for diagnostics). This prevents noisy or corrupted data from affecting production analytics while preserving all data for troubleshooting.

## Architecture

### Data Flow

```
[Device] ──┬──> [Validation] ──┬──> [Metering Path]
           │                   │     └─> Good readings ONLY
           │                   │     └─> CSV/JSON export
           │                   │     └─> Analytics/Billing
           │                   │
           └───────────────────┴──> [Instrumentation Path]
                                     └─> Diagnostics ONLY (no good data)
                                     └─> Bad readings if any
                                     └─> Reading quality indicator
                                     └─> Device health metrics
                                     └─> Troubleshooting/Alerts
```

## Key Components

### 1. MeteringReport

Clean data structure for business analytics:

```rust
pub struct MeteringReport {
    pub device_id: String,
    pub manufacturer: String,
    pub device_type: DeviceType,
    pub readings: Vec<Reading>,  // Only validated readings
    pub timestamp: SystemTime,
}
```

### 2. UnifiedInstrumentation

Diagnostics-only structure (no duplication of good data):

```rust
pub struct UnifiedInstrumentation {
    // ... device identification ...
    pub reading_quality: ReadingQuality,     // Overall indicator: Good/Substitute/Invalid
    pub bad_readings: Option<Vec<Reading>>,  // Only populated if issues exist
    pub readings: Vec<Reading>,              // Legacy field (prefer MeteringReport)
    // ... device status, metrics, etc ...
}
```

## Validation Rules

| Measurement Type | Valid Range    | Notes               |
|------------------|----------------|--------------------|
| Volume/Energy    | ≥ 0            | No negative values |
| Temperature      | -50 to 100°C   | Physical limits    |
| Humidity         | 0 to 100%      | Percentage bounds  |
| Battery          | 0 to 100%      | Percentage bounds  |
| Pressure         | 0 to 2000 hPa  | Atmospheric range  |
| Quality          | Must be "Good" | For metering path  |

## Usage

### Basic Usage

```rust
use mbus_rs::instrumentation::converters::{
    from_mbus_metering,
    from_mbus_frame_with_split,
};

// For clean metering data
let metering = from_mbus_metering(&frame, &records, None);
publish_to_analytics(metering.to_json()?);

// For full diagnostics
let inst = from_mbus_frame_with_split(&frame, &records, None, true);
publish_to_monitoring(inst.to_instrumentation_json()?);
```

### Export Formats

#### JSON Export

```rust
// Clean metering data
let json = metering.to_json()?;

// Full instrumentation with bad readings
let json = inst.to_instrumentation_json()?;

// Instrumentation without empty bad_readings field
let json = inst.to_clean_instrumentation_json()?;
```

#### CSV Export

```rust
// Time-series format for databases
let csv = metering.to_csv();
// Output: timestamp,device_id,manufacturer,reading_name,value,unit
```

## Examples

### Normal Operation

All readings pass validation:

```json
{
  "metering": {
    "readings": [
      {"name": "Volume", "value": 1234.567, "unit": "m³"},
      {"name": "Temperature", "value": 18.5, "unit": "°C"}
    ]
  },
  "instrumentation": {
    "reading_quality": "Good",
    "bad_readings": null,
    "device_status": {"alarm": false},
    "battery_status": {"percentage": 75}
  }
}
```

### Sensor Failure

Some readings invalid:

```json
{
  "metering": {
    "readings": [
      {"name": "Energy", "value": 15678.9, "unit": "kWh"}
    ]
  },
  "instrumentation": {
    "reading_quality": "Substitute",  // Partial data available
    "bad_readings": [
      {"name": "Temperature", "value": 250.0, "unit": "°C", "quality": "Invalid"}
    ],
    "device_status": {
      "error_code": "0x02",
      "error_description": "Temperature sensor failure"
    }
  }
}
```

## Converter Functions

### M-Bus Converters

- `from_mbus_frame()` - Legacy, backward compatible (includes all data)
- `from_mbus_metering()` - Metering path only (good readings)
- `from_mbus_instrumentation()` - Instrumentation only (diagnostics, no good data)

### wM-Bus Converters

- `from_wmbus_frame()` - Legacy, backward compatible
- `from_wmbus_metering()` - Metering path only
- `from_wmbus_instrumentation()` - Instrumentation only

### LoRa Converters

- `from_lora_metering_data()` - Legacy, backward compatible
- `from_lora_metering()` - Metering path only
- `from_lora_instrumentation()` - Instrumentation only

## Running Examples

### Dual-Path Gateway

Demonstrates separation of clean and diagnostic data:

```bash
cargo run --example dual_path_gateway
```

### Instrumentation Demo

Shows real-world scenarios with various error conditions:

```bash
cargo run --example instrumentation_demo
```

## Sample Data

Complete sample data is available in `samples/instrumentation_samples.json`, including:

- 6 different device types (water, heat, gas, electricity meters, etc.)
- Various error conditions (sensor failures, data corruption, low battery)
- Both clean metering reports and full instrumentation with bad readings

## Benefits

1. **Clean Analytics Pipeline** - No corrupted data in business reports
2. **Comprehensive Diagnostics** - All issues preserved for troubleshooting
3. **Backward Compatible** - Existing code continues working
4. **Flexible Validation** - Customizable per device/measurement type
5. **Multiple Export Formats** - CSV for time-series, JSON for APIs

## Migration Guide

### Existing Code

No changes required - existing code continues working:

```rust
// This still works
let inst = from_mbus_frame(&frame, &records, None);
```

### New Code

Use split functions for dual-path benefits:

```rust
// Metering path
let metering = from_mbus_metering(&frame, &records, None);

// Full instrumentation with bad readings
let inst = from_mbus_frame_with_split(&frame, &records, None, true);
```

## Customizing Validation

Edit the `validate_reading()` function in `src/instrumentation/mod.rs` to add custom rules:

```rust
pub fn validate_reading(reading: &Reading) -> Result<(), &'static str> {
    // Add custom validation logic here
    if reading.name.contains("CustomSensor") {
        if reading.value < 10.0 || reading.value > 90.0 {
            return Err("Custom sensor out of range");
        }
    }
    // ... existing validation ...
}
```

## See Also

- [Instrumentation Module](../src/instrumentation/mod.rs)
- [Converters](../src/instrumentation/converters.rs)
- [Sample Data](../samples/instrumentation_samples.json)
- [Examples](../examples/)
