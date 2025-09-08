# mbus-rs

The `mbus-rs` crate offers a comprehensive Rust implementation of the M-Bus (Meter-Bus) protocol. This protocol is a European standard for the remote reading of gas or electricity meters. Whether you're developing applications for utility metering, data collection, or monitoring systems, `mbus-rs` provides the tools you need to communicate with M-Bus devices efficiently.

## Features

- **Serial Connection**: Easily connect to M-Bus devices using a serial port.
- **Data Communication**: Send requests to M-Bus devices and process their responses.
- **Network Scanning**: Discover available M-Bus devices on your network.
- **Data Parsing**: Parse both fixed-length and variable-length M-Bus data records.
- **Data Normalization**: Utilize `VIF` (Value Information Field) and `VIB` (Value Information Block) to normalize data values.
- **High-level API**: Engage with M-Bus devices through a straightforward and intuitive API.
- **Logging and Error Handling**: Leverage built-in support for comprehensive logging and robust error handling.

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

## Architecture

The crate is structured as follows:

- `src/lib.rs`: Public API and re-exports.
- `src/mbus/`: M-Bus protocol implementation (frames, serial).
- `src/payload/`: Data record parsing and VIF handling.
- `src/wmbus/`: Wireless M-Bus support.
- `src/error.rs`: Custom error types.
- `src/logging.rs`: Logging helpers.

Reference: EN 13757-3 for M-Bus physical and link layers.

### Advanced Examples

For advanced usage, see `examples/` directory (to be added).

```rust
// Example: Parsing a full frame with records
let frame_bytes = hex::decode("68 0A 0A 68 53 01 78 02 01 00 00 00 00 00 00 00 00 54 16").unwrap();
let (_, frame) = mbus_rs::mbus::frame::parse_frame(&frame_bytes).unwrap();
let records = mbus_rs::mbus::mbus_protocol::DataRetrievalManager::default().parse_records(&frame).unwrap();
for record in records {
    println!("{:?}", record);
}
```
