# LoRa Parameters and Trade-offs

Based on Semtech AN1200.22 LoRa Modulation Basics

## Overview

This document describes the LoRa parameters supported by `mbus-rs` and their trade-offs based on the Semtech AN1200.22 reference documentation. Understanding these trade-offs is crucial for optimizing range, data rate, power consumption, and regulatory compliance.

## Key Parameters

### 1. Spreading Factor (SF)

The spreading factor determines the number of chips per symbol (2^SF chips/symbol). Higher SF provides better sensitivity but lower data rate.

| SF | Sensitivity (125kHz) | Min SNR | Data Rate | Time on Air | Use Case |
|----|---------------------|---------|-----------|-------------|----------|
| SF5 | -124 dBm | -5.0 dB | Highest | Shortest | Very short range, high data |
| SF6 | -127 dBm | -7.5 dB | High | Short | Short range, high data |
| SF7 | -130 dBm | -7.5 dB | High | Short | Urban, moderate range |
| SF8 | -133 dBm | -10.0 dB | Medium | Medium | Suburban |
| SF9 | -136 dBm | -12.5 dB | Medium | Medium | Mixed urban/rural |
| SF10 | -139 dBm | -15.0 dB | Low | Long | Rural, long range |
| SF11 | -141 dBm | -17.5 dB | Low | Long | Long range (LDRO required) |
| SF12 | -144 dBm | -20.0 dB | Lowest | Longest | Maximum range (LDRO required) |

**Key Points:**
- SF values are orthogonal - a receiver on SF7 cannot decode SF8 transmissions
- Each SF increment adds ~2.5dB sensitivity but doubles transmission time
- SF11/SF12 require LDRO when BW ≤ 125kHz

### 2. Bandwidth (BW)

The signal bandwidth affects both data rate and sensitivity. Wider bandwidth = higher data rate but worse sensitivity.

| Bandwidth | Data Rate Factor | Sensitivity Impact | Typical Use |
|-----------|-----------------|-------------------|-------------|
| 7.8 kHz | 0.0625x | +6 dB | Ultra long range |
| 10.4 kHz | 0.083x | +5 dB | Very long range |
| 15.6 kHz | 0.125x | +4 dB | Long range |
| 20.8 kHz | 0.167x | +3 dB | Long range |
| 31.2 kHz | 0.25x | +2 dB | Extended range |
| 41.7 kHz | 0.33x | +1 dB | Extended range |
| 62.5 kHz | 0.5x | +1 dB | Moderate range |
| 125 kHz | 1x (reference) | 0 dB | Standard (EU868 default) |
| 250 kHz | 2x | -3 dB | Higher data rate |
| 500 kHz | 4x | -6 dB | Maximum data rate |

**Trade-offs:**
- Doubling bandwidth doubles data rate but costs 3dB sensitivity
- Lower bandwidths provide better sensitivity but longer time on air
- Regional regulations may limit bandwidth choices

### 3. Coding Rate (CR)

Forward Error Correction ratio affects robustness vs overhead.

| Coding Rate | Overhead | Error Correction | Use Case |
|------------|----------|-----------------|----------|
| 4/5 | 25% | Minimal | Good channel conditions |
| 4/6 | 50% | Moderate | Standard conditions |
| 4/7 | 75% | Good | Noisy environment |
| 4/8 | 100% | Maximum | Very noisy environment |

**Trade-offs:**
- Higher CR improves error correction but increases time on air
- CR is included in packet header for automatic detection
- CR 4/5 is typically sufficient for most applications

### 4. Low Data Rate Optimization (LDRO)

LDRO compensates for clock drift in long symbol times.

**When Required:**
- SF11 or SF12 AND BW ≤ 125kHz
- Automatically enabled by the driver

**Impact:**
- Prevents demodulation errors at high SF
- Slightly increases time on air
- Essential for reliable long-range communication

## Time on Air Calculation

The time on air determines duty cycle compliance and battery life:

```
T_packet = T_preamble + T_payload

Where:
- T_preamble = (n_preamble + 4.25) * T_symbol
- T_payload = n_payload * T_symbol
- T_symbol = 2^SF / BW
```

### Example Calculations (50-byte payload)

| Configuration | Time on Air | Max Packets/Hour (1% duty cycle) |
|--------------|-------------|----------------------------------|
| SF7, BW125, CR4/5 | 61 ms | 590 |
| SF9, BW125, CR4/5 | 185 ms | 194 |
| SF10, BW125, CR4/5 | 371 ms | 97 |
| SF12, BW125, CR4/8 | 2466 ms | 14 |

## Optimization Strategies

### For Maximum Range
```rust
// Configuration for maximum range
let params = LoRaModParams {
    sf: SpreadingFactor::SF12,
    bw: LoRaBandwidth::BW125,  // Or lower for even more range
    cr: CodingRate::CR4_8,
    low_data_rate_optimize: true,  // Auto-enabled
};
```

### For Maximum Data Rate
```rust
// Configuration for highest data rate
let params = LoRaModParams {
    sf: SpreadingFactor::SF7,
    bw: LoRaBandwidth::BW500,
    cr: CodingRate::CR4_5,
    low_data_rate_optimize: false,
};
```

### For Balanced Performance
```rust
// Typical EU868 configuration
let params = LoRaModParams {
    sf: SpreadingFactor::SF9,
    bw: LoRaBandwidth::BW125,
    cr: CodingRate::CR4_5,
    low_data_rate_optimize: false,
};
```

## Adaptive Data Rate (ADR)

The ADR controller automatically adjusts SF based on link quality:

```rust
use mbus_rs::wmbus::radio::lora::adr::AdrController;

let mut adr = AdrController::new(
    SpreadingFactor::SF7,  // Start with fast rate
    SpreadingFactor::SF12, // Allow up to SF12 for range
);

// ADR will automatically adjust based on SNR/RSSI
if let Some(new_sf) = adr.process_metrics(rssi, snr) {
    driver.configure_for_lora(freq, new_sf, bw, cr, power)?;
}
```

### ADR Algorithm

1. **Good Link (SNR > threshold + margin)**: Decrease SF (faster data rate)
2. **Poor Link (SNR < threshold)**: Increase SF (better sensitivity)
3. **Packet Loss**: Immediately increase SF

SNR thresholds are based on the sensitivity table above.

## Regional Considerations

### EU868
- Default: SF7-SF12, BW125
- Duty cycle: 1% (36s/hour)
- Channels: 868.1, 868.3, 868.5 MHz

### US915
- Default: SF7-SF10, BW125/500
- No duty cycle limit
- 64 uplink channels

### AS923
- Default: SF7-SF12, BW125
- Duty cycle varies by country
- Channels vary by country

## Power Consumption

Time on air directly affects battery life:

| Configuration | Current Draw | Battery Life (2000mAh) |
|--------------|-------------|----------------------|
| SF7, 1 msg/hour | ~10 µA avg | >2 years |
| SF10, 1 msg/hour | ~30 µA avg | ~1 year |
| SF12, 1 msg/hour | ~200 µA avg | ~3 months |

## Best Practices

1. **Start with lower SF**: Begin with SF7-SF9 and increase only if needed
2. **Use ADR**: Let the controller optimize SF automatically
3. **Monitor duty cycle**: Ensure compliance with regional regulations
4. **Consider bandwidth**: Use 125kHz as default, adjust for specific needs
5. **Enable LDRO**: Always enabled automatically for SF11/SF12
6. **Set appropriate preamble**: 8 symbols is standard, increase for wake-on-radio

## API Reference

### Key Functions

```rust
// Get sensitivity for given parameters
let sensitivity = get_lora_sensitivity_dbm(sf, bw);

// Check minimum SNR requirement
let min_snr = get_min_snr_db(sf);

// Determine if LDRO needed
let ldro = requires_ldro(sf, bw);

// Calculate data rate
let bitrate = lora_bitrate_hz(sf, bw, cr);
```

### Configuration Example

```rust
use mbus_rs::wmbus::radio::driver::Sx126xDriver;
use mbus_rs::wmbus::radio::lora::{SpreadingFactor, LoRaBandwidth, CodingRate};

// Configure for long range
driver.configure_for_lora(
    868_100_000,  // Frequency in Hz
    SpreadingFactor::SF10,
    LoRaBandwidth::BW125,
    CodingRate::CR4_5,
    14,  // TX power in dBm
)?;
```

## Troubleshooting

### Poor Range
- Increase SF (up to SF12)
- Decrease BW (down to 62.5kHz or lower)
- Increase TX power
- Check antenna matching

### Packet Loss at High SF
- Ensure LDRO is enabled (automatic for SF11/SF12)
- Check clock accuracy (±20ppm recommended)
- Verify bandwidth setting

### Duty Cycle Violations
- Reduce SF to decrease time on air
- Increase packet interval
- Use channel hopping

### High Power Consumption
- Reduce SF
- Increase BW
- Optimize packet size
- Use sleep modes between transmissions

## Enhanced Features (SX126x Application Notes)

Based on Semtech application notes (AN1200.37, AN1200.48, AN1200.94), `mbus-rs` now includes enhanced features for improved performance and ease of use.

### Feature Selection Guide

| Feature | Power Cost | Benefit | When to Use |
|---------|------------|---------|-------------|
| **RX Boost** | +20mA | +6dB sensitivity | Urban/noisy environments, SF≥10 |
| **CAD LBT** | Minimal | 50-80% fewer collisions | Dense networks, regulatory requirement |
| **DC-DC Mode** | Saves power | 50% less drift | TX >+15dBm or packets >100ms |
| **TCXO** | +2mA | ±2ppm stability | Outdoor (-40°C to +85°C) |
| **Defaults** | None | Quick start | Prototyping, testing |

### RX Boost Mode

From SX126x Development Kit User Guide: Provides +6dB sensitivity improvement at +20mA current cost.

```rust
// Enable RX boost for better sensitivity
driver.set_rx_boosted_gain(true)?;

// Or use auto-optimization with enhanced configuration
driver.configure_for_lora_enhanced(freq, sf, bw, cr, power, true)?;
```

**Benefits**: 20-30% improved range in noisy urban environments, essential for metering in dense areas.

### CAD for Listen Before Talk

From AN1200.48: Channel Activity Detection provides 50-80% better accuracy than RSSI-based LBT with ~1ms detection time.

```rust
use mbus_rs::wmbus::radio::lora::LoRaCadParams;

// Use optimal CAD parameters from AN1200.48
let params = LoRaCadParams::optimal(SF10, BW125);
driver.set_cad_params(&params)?;

// Perform CAD-based LBT
if driver.cad_lbt(SF10, BW125, 3)? {
    // Channel clear, safe to transmit
    driver.transmit(&packet)?;
}
```

**Benefits**: Reduces packet collisions by 50-80%, ensures regulatory compliance, faster than RSSI scanning.

### Temperature Stability with TCXO

From AN1200.37: Essential for outdoor deployments with temperature variations.

```rust
// Configure 3.3V TCXO with 1ms startup
driver.configure_tcxo(3300, 1000)?;

// Enable DC-DC for high power efficiency
driver.set_regulator_mode(true)?;
```

**Benefits**: ±2ppm frequency stability from -40°C to +85°C, 30% improved link reliability in extreme conditions.

## Single-Channel Networks (AN1200.94)

For private or fixed-configuration networks (common in metering):

### Configuration Requirements

1. **Disable ADR** on network server to prevent SF mismatches
2. **Fix SF/BW** on all devices to match gateway
3. **Use private sync word** (0x1424) for network isolation
4. **Consider CAD** for collision avoidance

### Example: Single-Channel Gateway

```rust
use mbus_rs::wmbus::radio::lora::{LoRaModParams, SyncWords};

// Gateway configuration for EU868 channel 3
let params = LoRaModParams {
    sf: SpreadingFactor::SF10,  // Fixed SF for all devices
    bw: LoRaBandwidth::BW125,   // EU868 standard
    cr: CodingRate::CR4_5,
    low_data_rate_optimize: false,
};

// Configure with auto-optimizations
driver.configure_for_lora_enhanced(
    868_500_000,  // Fixed frequency
    params.sf,
    params.bw,
    params.cr,
    14,           // EU868 max power
    true,         // Auto-enable RX boost for SF10
)?;

// Use private network sync word
driver.set_sync_word(&SyncWords::PRIVATE)?;

// Optional: Enable CAD for collision avoidance
let cad_params = LoRaCadParams::optimal(params.sf, params.bw);
driver.set_cad_params(&cad_params)?;
```

### Throughput Expectations

With 1% duty cycle (EU868):
- SF7/BW125: ~370 packets/hour
- SF10/BW125: ~60 packets/hour
- SF12/BW125: ~10 packets/hour

Without duty cycle (US915):
- SF7/BW500: ~37,000 packets/hour
- SF10/BW125: ~6,000 packets/hour

### Running the Example

```bash
# Build and run single-channel gateway example
cargo run --example single_channel_gateway

# With debug logging
RUST_LOG=debug cargo run --example single_channel_gateway
```

## Quick Start with Defaults

The new Default implementations provide tested configurations from the SX126x User Guide:

```rust
use mbus_rs::wmbus::radio::lora::{LoRaModParams, LoRaPacketParams};

// Quick start with defaults (SF7, BW500, CR4/5)
let mod_params = LoRaModParams::default();
let packet_params = LoRaPacketParams::default();

// Or use regional defaults
let eu_params = LoRaModParams::eu868_defaults();  // SF9, BW125
let us_params = LoRaModParams::us915_defaults();  // SF7, BW500

// Validate parameters
mod_params.validate()?;  // Checks for incompatible combinations
```

## Performance Comparison

| Configuration | Range | Data Rate | Power | Use Case |
|---------------|-------|-----------|-------|----------|
| SF7/BW500 (Default) | 2km | 21.8kbps | Low | Testing, high-speed |
| SF9/BW125 (EU868) | 5km | 1.76kbps | Medium | Urban metering |
| SF10/BW125 + RxBoost | 8km | 980bps | High | Rural metering |
| SF12/BW125 + CAD | 15km | 293bps | High | Maximum range |

## Migration Guide

For existing code, the enhancements are backward-compatible:

```rust
// Old way still works
driver.set_modulation_params(&mod_params)?;
driver.set_packet_params(&packet_params)?;

// New enhanced way with auto-optimizations
driver.configure_for_lora_enhanced(
    freq, sf, bw, cr, power,
    true,  // Enable auto-optimizations
)?;
```

The auto-optimization feature:
- Enables RX boost for SF≥10
- Enables DC-DC regulator for TX >15dBm
- Sets LDRO automatically for SF11/SF12 with BW≤125kHz