# Sample Data

This directory contains sample data demonstrating various aspects of the M-Bus/wM-Bus/LoRa protocol implementation.

## Files

### `instrumentation_samples.json`

Comprehensive instrumentation data samples showing:

- **Metering Reports**: Clean, validated data for business analytics
- **Instrumentation Reports**: Full diagnostics including bad readings and device health

#### Device Types Included

1. **Kamstrup MULTICAL 21** - Water meter via wM-Bus (normal operation)
2. **Engelmann SensoStar** - Heat meter with sensor failure
3. **Dragino LWL03A** - LoRa leak sensor with active leak detection
4. **Elster AS3000** - Electricity meter with data corruption
5. **SENSUS 620** - Gas meter with tamper detection
6. **Generic Environmental Sensor** - LoRa sensor with calibration issues

#### Data Categories

- **Good Readings**: Valid measurements passing all validation rules
- **Bad Readings**: Invalid data preserved for diagnostics
  - Out of bounds values (e.g., temperature > 100°C)
  - Negative counters/volumes
  - NaN/Inf values
  - Poor quality indicators

#### Use Cases

- Testing data validation logic
- Demonstrating dual-path separation
- Example JSON structures for API development
- Reference for device integration

## Format Examples

### Clean Metering Data

```json
{
  "device_id": "12345678",
  "manufacturer": "KAM",
  "device_type": "WaterMeter",
  "readings": [
    {"name": "Volume", "value": 1234.567, "unit": "m³"},
    {"name": "Temperature", "value": 18.5, "unit": "°C"}
  ]
}
```

### Full Instrumentation

```json
{
  "device_id": "87654321",
  "readings": [/* good readings */],
  "bad_readings": [
    {"name": "Sensor2", "value": 250.0, "unit": "°C", "quality": "Invalid"}
  ],
  "device_status": {
    "alarm": true,
    "error_code": "0x02"
  }
}
```

## Related Documentation

- [Dual-Path Instrumentation](../docs/DUAL_PATH_INSTRUMENTATION.md)
- [Running Examples](../examples/)

## Generating More Samples

Use the example programs to generate additional sample data:

```bash
# Generate dual-path samples
cargo run --example dual_path_gateway

# Generate scenario-based samples
cargo run --example instrumentation_demo
```