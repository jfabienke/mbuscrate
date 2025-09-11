# mbus-rs

The `mbus-rs` crate offers a comprehensive Rust implementation of the M-Bus (Meter-Bus) protocol. This protocol is a European standard for the remote reading of gas or electricity meters. Whether you're developing applications for utility metering, data collection, or monitoring systems, `mbus-rs` provides the tools you need to communicate with M-Bus devices efficiently.

## Features

- **Serial Connection**: Easily connect to M-Bus devices using a serial port.
- **Wireless M-Bus Support**: Complete SX126x radio driver with GFSK modulation for wireless M-Bus communication.
- **Raspberry Pi Platform**: Native hardware support for Pi 4/5 with SPI and GPIO control.
- **Data Communication**: Send requests to M-Bus devices and process their responses.
- **Network Scanning**: Discover available M-Bus devices on your network.
- **Data Parsing**: Parse both fixed-length and variable-length M-Bus data records.
- **Data Normalization**: Utilize `VIF` (Value Information Field) and `VIB` (Value Information Block) to normalize data values.
- **High-level API**: Engage with M-Bus devices through a straightforward and intuitive API.
- **Cross-compilation**: Build for ARM targets with dedicated tooling and scripts.
- **Logging and Error Handling**: Leverage built-in support for comprehensive logging and robust error handling.

## Standards Compliance

`mbus-rs` achieves **100% compliance** with EN 13757-3 M-Bus standards for RF and serial transport.

**📋 Full compliance details available in [COMPLIANCE.md](COMPLIANCE.md)**

### Summary
- ✅ **Wired M-Bus (EN 13757-2/3)**: 100% compliant
- ✅ **Wireless M-Bus (EN 13757-4)**: 100% compliant for RF modes
- ✅ **OMS v4.0.4**: Full support for Modes 5/7/9 and compact frames
- ✅ **ETSI EN 300 220**: Complete regulatory compliance with LBT and duty cycle
- ✅ **Security**: AES-128 CTR/CBC/GCM with Mode 9 (OMS 7.3.6) fully implemented

For detailed standards mapping, test coverage, and implementation specifics, see [COMPLIANCE.md](COMPLIANCE.md).

## Getting Started

To integrate `mbus-rs` into your project, add it as a dependency in your `Cargo.toml` file:

```toml
[dependencies]
mbus-rs = "0.1.0"
```

Then, you can start using mbus-rs by importing it into your Rust code:

```rust
use mbus_rs::{
    connect, disconnect, send_request, scan_devices,
    MBusRecord, MBusRecordValue, MBusError, init_logger, log_info,
};
```

## Usage Examples

### Connecting to an M-Bus Device

```rust
let mut handle = connect("/dev/ttyUSB0").await?;
```

### Sending a Request and Receiving a Response

```rust
let records = send_request(&mut handle, 0x01).await?;
for record in records {
    println!("Value: {:?}, Unit: {}, Quantity: {}", record.value, record.unit, record.quantity);
}
```

### Scanning for Devices

```rust
let addresses = scan_devices(&mut handle).await?;
for address in addresses {
    println!("Found device: {}", address);
}
```

### Disconnecting from a Device

```rust
disconnect(&mut handle).await?;
```

### Error Handling

mbus-rs uses the `MBusError` enum to represent various error conditions. You can handle these using the ? operator or by matching against the error variants.

### Logging

To enable logging, use the `init_logger()` function at the start of your application. This crate uses the `log` and `env_logger` crates for logging purposes.

## Contributing

We welcome contributions to `mbus-rs`! Please feel free to submit issues or pull requests on GitHub.

## Documentation

Comprehensive documentation is available:

- **[Architecture Overview](ARCHITECTURE.md)** - System design, components, and data flow
- **[API Reference](docs/API.md)** - Complete API documentation with examples
- **[Module Documentation](docs/MODULES.md)** - Detailed module descriptions and interfaces
- **[Protocol Reference](docs/PROTOCOL.md)** - M-Bus protocol specification and frame formats
- **[Testing Guide](docs/TESTING.md)** - Testing strategies, coverage analysis, and mock infrastructure
- **[Examples](docs/EXAMPLES.md)** - Usage examples and common patterns

## Architecture

The crate follows a layered architecture:

- **Application Layer**: `main.rs` (CLI), `lib.rs` (Public API), `mbus_device_manager.rs` (Device Management)
- **Protocol Layer**: `mbus/mbus_protocol.rs` (DataRetrievalManager), `wmbus/` (Wireless M-Bus)
- **Radio Layer**: `wmbus/radio/` (SX126x driver with HAL abstraction)
- **Data Layer**: `payload/` (Record parsing, VIF/DIF handling, data encoding)
- **Frame Layer**: `mbus/frame.rs` (Wired frames), `wmbus/frame.rs` (Wireless frames)
- **Transport Layer**: `mbus/serial.rs` (Serial), `wmbus/radio/hal/` (SPI/GPIO)

Reference: EN 13757-3 for M-Bus physical and link layers, EN 13757-4 for wireless M-Bus.

## Platform Support

### Raspberry Pi (New! 🎉)

mbus-rs now includes native support for Raspberry Pi 4 and 5 platforms with SX126x radio modules for wireless M-Bus communication:

```rust
use mbus_rs::wmbus::radio::hal::{RaspberryPiHal, GpioPins};
use mbus_rs::wmbus::radio::driver::Sx126xDriver;

// Initialize Raspberry Pi HAL
let hal = RaspberryPiHal::new(0, &GpioPins::default())?;

// Create radio driver
let mut driver = Sx126xDriver::new(hal, 32_000_000);

// Configure for EU wM-Bus S-mode
driver.configure_for_wmbus(868_950_000, 100_000)?;

// Start listening for wM-Bus frames
driver.set_rx_continuous()?;
loop {
    if let Some(frame) = driver.process_irqs()? {
        println!("Received wM-Bus frame: {} bytes", frame.len());
    }
}
```

**Supported Platforms:**
- Raspberry Pi 5 (ARM Cortex-A76, 64-bit)
- Raspberry Pi 4 (ARM Cortex-A72, 64-bit/32-bit)

**Features:**
- Hardware SPI interface with configurable pins
- GPIO control for BUSY, DIO, and RESET signals  
- Cross-compilation support from development machines
- Complete examples and documentation
- Production-ready systemd service configurations

**Getting Started:**
1. See [Raspberry Pi Setup Guide](docs/RASPBERRY_PI_SETUP.md)
2. Run examples: `cargo run --example raspberry_pi_wmbus --features raspberry-pi`
3. Cross-compile: `./scripts/build_pi.sh pi5`

For complete platform documentation, see [RASPBERRY_PI_PLATFORMS.md](RASPBERRY_PI_PLATFORMS.md).

### Advanced Examples

For advanced usage, see the `examples/` directory:

```rust
// Example: Parsing a full frame with records
let frame_bytes = hex::decode("68 0A 0A 68 53 01 78 02 01 00 00 00 00 00 00 00 00 54 16").unwrap();
let (_, frame) = mbus_rs::mbus::frame::parse_frame(&frame_bytes).unwrap();
let records = mbus_rs::mbus::mbus_protocol::DataRetrievalManager::default().parse_records(&frame).unwrap();
for record in records {
    println!("{:?}", record);
}
```

## Acknowledgments

`mbus-rs` is an original Rust implementation of the M-Bus protocol, developed from scratch based on international standards. We acknowledge the contributions of the M-Bus community to the collective knowledge that makes robust implementations possible.

For detailed attribution and acknowledgments, please see [CREDITS.md](CREDITS.md).
