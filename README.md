# mbus-rs

**Production-ready Rust implementation of the M-Bus protocol for smart meter communication.**
Connect to electricity, gas, water, and heat meters with confidence—featuring async I/O, multi-telegram support, and encryption.

## Why mbus-rs?

- ✅ **95% EN 13757 compliant** - Battle-tested with real meters from Kamstrup, Landis+Gyr, and more
- ⚡ **Blazing fast** - Parse frames in <1ms with zero-copy nom parsers
- 🔒 **Secure** - AES-128 encryption (Modes 5/7/9) with proven OMS compliance
- 🚀 **Async-first** - Built on tokio for concurrent multi-device operations
- 📡 **Wireless ready** - Native Raspberry Pi support with SX126x radio drivers
- 🛠️ **Production proven** - 85% test coverage with extensive real-world testing

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
| **Multi-telegram** | ✅ FCB handling | Reassemble 2-10 frames |
| **Encryption** | ✅ AES-128 CTR/CBC/GCM | <5ms decrypt time |
| **Device scanning** | ✅ Primary/secondary | 100 devices in <30s |
| **Raspberry Pi** | ✅ Native SX126x driver | SPI up to 16 MHz |

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

More examples in [`examples/`](examples/) directory.

## Standards Compliance

**95% compliant** with EN 13757 standards. Full details in [COMPLIANCE.md](COMPLIANCE.md).

- ✅ EN 13757-2/3 (Wired M-Bus): 100% compliant
- ✅ EN 13757-4 (Wireless): S/T/C modes with LBT
- ✅ OMS v4.0.4: Modes 5/7/9 encryption
- ✅ Multi-telegram: FCB handling and frame reassembly

## 📖 Documentation

- [Architecture](ARCHITECTURE.md) - System design and components
- [API Reference](docs/API.md) - Complete API documentation
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
