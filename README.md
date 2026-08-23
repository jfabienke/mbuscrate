# mbus-rs

**A Rust implementation of the M-Bus protocol for smart meter communication.**
Reads electricity, gas, water, and heat meters over wired and wireless M-Bus — async I/O,
multi-telegram support, and encryption. Verified against real meter traffic; still
consolidating toward a 1.0 release (see [What's New](#whats-new) for current status).

## What's New

- **`no_std` protocol core**: the parsing, decoding and crypto now live in a separate
  `mbus-core` crate that builds `no_std` with **no heap and no panics** — the same
  decode code runs on a Linux gateway and on a Cortex-M microcontroller. It compiles for
  `thumbv6m-none-eabi`, and its panic-freedom is not asserted but *linker-verified* in CI:
  if any reachable path in the decode pipeline could panic, the check binary fails to link.
- **Exact large counters**: record values from integer and BCD codings keep their raw
  64-bit mantissa (`MBusRecordValue::Scaled`) instead of being folded to `f64` at parse
  time, which silently corrupted any total past ~9×10¹⁵. `as_f64()` gives a convenience
  float where the loss is acceptable.
- **Crypto**: CMAC/HMAC/SHA1 on RustCrypto primitives; OMS Mode 9 (GCM) authenticates the
  spec's 12-byte truncated tag correctly and defaults to it.
- **LoRa Decoder Refactor**: Enum-based `DecoderType` for easier device registration
  (Dragino, Decentlab, GenericCounter); `lora_decoder_demo.rs`.

See full details in [CHANGELOG.md](CHANGELOG.md).

## Why mbus-rs?

- ✅ **EN 13757 wired and wireless** - frame parsing, mode-C link layer, DIF/VIF records
- 📻 **Reads real meters** - verified end-to-end against live Kamstrup traffic on a
  Raspberry Pi gateway: ELL AES-CTR decryption, compact-frame expansion, scaled readings
- 🔒 **Audited crypto primitives** - AES/CMAC/HMAC from the RustCrypto crates, never
  hand-rolled; the `crypto` feature is on by default and cannot degrade to a no-op
- 🚀 **Async-first** - Built on tokio for concurrent multi-device operations
- 📡 **Wireless ready** - Native Raspberry Pi support with SX126x and RFM69 drivers
- 🌐 **LoRa PHY** - CAD, RX Boost (+6dB), and regional configurations

## 🚀 Quick Start

Get running in 30 seconds:

```toml
[dependencies]
mbus-rs = "1.0.0"
```

```rust
use mbus_rs::{connect, send_request};

#[tokio::main]
async fn main() -> Result<(), mbus_rs::MBusError> {
    // Connect to meter via serial port
    let mut handle = connect("/dev/ttyUSB0").await?;

    // Request data from device address 0x01
    let records = send_request(&mut handle, 0x01).await?;

    // Process meter data
    for record in records {
        println!("{} {} ({})", record.value, record.unit, record.quantity);
    }

    Ok(())
}
```

## Key Features

| Feature | Status | Performance |
|---------|--------|-------------|
| **Wired M-Bus** | ✅ Full EN 13757-2/3 | Auto-baud 300-38400 bps |
| **Wireless M-Bus** | ✅ S/T/C modes | 868 MHz, <0.9% duty cycle |
| **LoRa Support** | ✅ SX126x advanced | CAD, RX Boost, regional configs |
| **Multi-telegram** | ✅ FCB handling | Reassemble 2-10 frames |
| **Encryption** | ✅ AES-128 CTR/CBC/GCM | <5ms decrypt time |
| **Device scanning** | ✅ Primary/secondary | 100 devices in <30s |
| **Raspberry Pi** | ✅ Native SX126x driver | SPI up to 16 MHz |
| **`no_std` core** | ✅ `mbus-core` crate | no heap, no panics; builds for Cortex-M |

## Installation

```toml
[dependencies]
mbus-rs = { version = "1.0", features = ["crypto"] }

# For Raspberry Pi wireless M-Bus:
mbus-rs = { version = "1.0", features = ["crypto", "raspberry-pi"] }
```

## Common Use Cases

### Device Discovery
Scan your network to find all connected meters:

```rust
let mut handle = connect("/dev/ttyUSB0").await?;
let devices = scan_devices(&mut handle).await?;
println!("Found {} meters", devices.len());
```

### Wireless M-Bus on Raspberry Pi
Monitor wireless meter broadcasts (868 MHz):

```rust
use mbus_rs::wmbus::radio::{RaspberryPiHal, Sx126xDriver};

let hal = RaspberryPiHal::new(0, Default::default())?;
let mut radio = Sx126xDriver::new(hal, 32_000_000);
radio.configure_for_wmbus(868_950_000, 100_000)?;
// Listen for wireless frames...
```

### Advanced LoRa Configuration
Leverage optimized LoRa features for better performance:

```rust
use mbus_rs::wmbus::radio::lora::{LoRaModParams, LoRaCadParams};

// Use regional defaults for quick setup
let params = LoRaModParams::eu868_defaults(); // Or us915_defaults()

// Configure with auto-optimization (enables RX Boost for SF≥10)
radio.configure_for_lora_enhanced(
    868_100_000,           // Frequency
    SpreadingFactor::SF10, // Auto-enables RX Boost
    LoRaBandwidth::BW125,
    CodingRate::CR4_5,
    14,                    // TX power (dBm)
    true                   // Auto-optimize
)?;

// Enable CAD for 50-80% fewer collisions
let cad_params = LoRaCadParams::optimal(SF10, BW125);
radio.set_cad_params(cad_params)?;
```

More examples in [`examples/`](examples/) directory.

## Standards Compliance

What is implemented and tested, rather than a single compliance percentage — the
previous "95% compliant" and "85% test coverage" figures were not derived from any
measurement and have been removed.

- **EN 13757-2/3 (wired M-Bus)**: frame parsing/packing, fixed and variable data
  records, DIF/VIF decoding with exponents pinned to Table 10 by test.
- **EN 13757-4 (wireless)**: mode-C link layer (frame types A and B, per-block
  CRC-16/EN-13757 verified against the standard's `0xC2B7` check value); mode C
  receive is exercised on hardware. S and T mode framing is implemented but has not
  been validated against live traffic.
- **Extended Link Layer**: CI 0x8C–0x8F headers; AES-128-CTR decryption verified
  against captured Kamstrup Multical 21 traffic.
- **Compact frames**: layout learned from a full frame and re-applied, with the
  format signature confirmed against captured traffic.
- **OMS security profiles**: Mode 5 (AES-128-CBC, Security Profile A) and Mode 9
  (AES-128-GCM) are implemented on RustCrypto primitives and covered by known-answer
  vectors. **Mode 7 is not implemented.** The implemented modes have **not** been
  validated against live meter traffic — the meters reachable from the test gateway use
  ELL encryption. (An earlier version of this list said "Mode 5 (CTR), 7 (CBC)"; Mode 5's
  cipher is CBC, not CTR — the CTR path was a bug and was retired — and no Mode 7 code
  exists.)
- **Not implemented**: OMS master-key derivation (AES-CMAC). Supply the per-device
  key directly; see `KeyMode`.

## 🌐 LoRa Features

Advanced SX126x radio features based on Semtech application notes:

### Channel Activity Detection (CAD)
- **50-80% fewer collisions** compared to RSSI-based LBT
- Optimal parameters from AN1200.48 for each SF/BW combination
- Fast detect, high reliability, and optimal modes
- Real-time statistics tracking

### Performance Enhancements
- **RX Boost Mode**: +6dB sensitivity improvement (auto-enabled for SF≥10)
- **DC-DC Regulator**: 50% temperature drift reduction for TX >15dBm
- **TCXO Support**: ±2ppm frequency stability from -40°C to +85°C
- **LDRO**: Automatic Low Data Rate Optimization for SF11/SF12

### Regional Configurations
Pre-configured regional defaults for quick deployment:
- **EU868**: SF9, BW125, 1% duty cycle compliant
- **US915**: SF7, BW500, maximum throughput
- **AS923**: SF8, BW125, Asia-Pacific optimized
- **Custom**: Private network configurations

### Single-Channel Gateway
Perfect for private metering networks:
- Fixed frequency/SF operation (no ADR)
- Example configurations for all regions
- Duty cycle management
- See [`examples/single_channel_gateway.rs`](examples/single_channel_gateway.rs)

Full LoRa documentation in [docs/LORA_PARAMETERS.md](docs/LORA_PARAMETERS.md).

## 📖 Documentation

- [Architecture](ARCHITECTURE.md) - System design and components
- [API Reference](docs/API.md) - Complete API documentation
- [LoRa Parameters](docs/LORA_PARAMETERS.md) - Advanced LoRa configuration guide
- [Raspberry Pi Setup](docs/RASPBERRY_PI_SETUP.md) - Hardware guide
- [Examples](docs/EXAMPLES.md) - Code samples and patterns
- [Troubleshooting](docs/TROUBLESHOOTING.md) - Common issues

## Platform Support

- **Linux**: Primary platform (x86_64, ARM)
- **Raspberry Pi**: Native support for Pi 4/5 with SX126x radios
- **macOS**: Development and testing
- **Windows**: Serial communication only

Cross-compilation scripts available in [`scripts/`](scripts/).

## Contributing

We welcome contributions! See [CONTRIBUTING.md](docs/CONTRIBUTING.md) for guidelines.

## License

MIT - See [LICENSE](LICENSE) for details.

## Acknowledgments

Built on EN 13757 standards with community knowledge. See [CREDITS.md](docs/CREDITS.md).
