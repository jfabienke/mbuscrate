# API Reference

## Table of Contents
- [Public API Functions](#public-api-functions)
- [Wireless M-Bus API](#wireless-m-bus-api)
- [Core Types](#core-types)
- [Frame Processing](#frame-processing)
- [Protocol Operations](#protocol-operations)
- [Data Structures](#data-structures)
- [Error Types](#error-types)
- [Serial Communication](#serial-communication)
- [Radio Driver API](#radio-driver-api)
- [Utility Functions](#utility-functions)

## Public API Functions

The main library entry point (`lib.rs`) provides high-level functions for M-Bus operations:

### Connection Management

#### `connect(port: &str) -> Result<MBusDeviceHandle, MBusError>`
Connect to an M-Bus device via serial port.

**Parameters:**
- `port`: Serial port path (e.g., "/dev/ttyUSB0", "COM1")

**Returns:**
- `Ok(MBusDeviceHandle)`: Connected device handle
- `Err(MBusError)`: Connection error

**Example:**
```rust
let handle = connect("/dev/ttyUSB0").await?;
```

---

#### `disconnect(handle: &mut MBusDeviceHandle) -> Result<(), MBusError>`
Disconnect from M-Bus device.

**Parameters:**
- `handle`: Device handle to disconnect

**Returns:**
- `Ok(())`: Successfully disconnected
- `Err(MBusError)`: Disconnection error

---

### Data Operations

#### `send_request(handle: &mut MBusDeviceHandle, address: u8) -> Result<Vec<MBusRecord>, MBusError>`
Send data request to device and return parsed records.

**Parameters:**
- `handle`: Connected device handle
- `address`: Device primary address (1-250)

**Returns:**
- `Ok(Vec<MBusRecord>)`: Vector of data records
- `Err(MBusError)`: Communication or parsing error

**Example:**
```rust
let records = send_request(&mut handle, 0x01).await?;
for record in records {
    println!("Value: {:?}, Unit: {}", record.value, record.unit);
}
```

---

#### `scan_devices(handle: &mut MBusDeviceHandle) -> Result<Vec<u8>, MBusError>`
Scan for all responding devices on the bus.

**Parameters:**
- `handle`: Connected device handle

**Returns:**
- `Ok(Vec<u8>)`: Vector of responding device addresses
- `Err(MBusError)`: Scanning error

**Example:**
```rust
let addresses = scan_devices(&mut handle).await?;
println!("Found devices: {:?}", addresses);
```

## Wireless M-Bus API

The wireless M-Bus module (`wmbus/`) provides comprehensive support for SX126x radio-based wireless M-Bus communication.

### Device Manager

#### `MBusDeviceManager::new() -> Self`
Create a new device manager for handling both wired and wireless M-Bus devices.

**Example:**
```rust
use mbus_rs::MBusDeviceManager;
let mut manager = MBusDeviceManager::new();
```

### Radio Driver

#### `Sx126xDriver::new(hal: H, xtal_freq: u32) -> Self`
Initialize SX126x radio driver with hardware abstraction layer.

**Parameters:**
- `hal`: Platform-specific HAL implementation (e.g., RaspberryPiHal)
- `xtal_freq`: Crystal frequency in Hz (typically 32_000_000)

#### `configure_for_wmbus(frequency: u32, datarate: u32) -> Result<(), RadioError>`
Configure radio for wireless M-Bus operation.

**Parameters:**
- `frequency`: Center frequency in Hz (868_950_000 for EU S-mode)
- `datarate`: Data rate in bps (100_000 for S-mode)

**Example:**
```rust
use mbus_rs::wmbus::radio::{driver::Sx126xDriver, hal::RaspberryPiHal};

let hal = RaspberryPiHal::new(0, &Default::default())?;
let mut driver = Sx126xDriver::new(hal, 32_000_000);
driver.configure_for_wmbus(868_950_000, 100_000)?;
```

### Hardware Abstraction Layer

#### `RaspberryPiHal::new(spi_bus: u8, pins: &GpioPins) -> Result<Self, HalError>`
Initialize Raspberry Pi HAL for SPI and GPIO communication.

**Parameters:**
- `spi_bus`: SPI bus number (0 or 1)
- `pins`: GPIO pin configuration

#### `RaspberryPiHalBuilder`
Builder pattern for flexible HAL configuration:

```rust
use mbus_rs::wmbus::radio::hal::{RaspberryPiHalBuilder, GpioPins};

let hal = RaspberryPiHalBuilder::new()
    .spi_bus(0)
    .spi_speed(8_000_000)
    .busy_pin(25)
    .dio1_pin(24)
    .dio2_pin(23)
    .reset_pin(22)
    .build()?;
```

---

## Core Types

### `MBusFrame`
Main frame structure representing all M-Bus frame types.

```rust
pub struct MBusFrame {
    pub frame_type: MBusFrameType,
    pub control: u8,
    pub address: u8,
    pub control_information: u8,
    pub data: Vec<u8>,
    pub checksum: u8,
    pub more_records_follow: bool,
}
```

**Fields:**
- `frame_type`: Type of frame (Ack, Short, Control, Long)
- `control`: Control field byte
- `address`: Device address
- `control_information`: CI field for long frames
- `data`: Frame payload data
- `checksum`: Calculated checksum
- `more_records_follow`: Multi-telegram flag

---

### `MBusFrameType`
Frame type enumeration.

```rust
pub enum MBusFrameType {
    Ack,      // Single byte acknowledgment (0xE5)
    Short,    // 5-byte control frame
    Control,  // 9-byte extended control frame
    Long,     // Variable length data frame
}
```

---

### `MBusRecord`
Parsed data record containing measurement value and metadata.

```rust
pub struct MBusRecord {
    pub timestamp: SystemTime,
    pub storage_number: u32,
    pub tariff: i32,
    pub device: i32,
    pub is_numeric: bool,
    pub value: MBusRecordValue,
    pub unit: String,
    pub function_medium: String,
    pub quantity: String,
    pub drh: MBusDataRecordHeader,
    pub data_len: usize,
    pub data: [u8; 256],
}
```

**Fields:**
- `timestamp`: Record creation time
- `storage_number`: Storage location index
- `tariff`: Tariff information
- `device`: Device unit number
- `is_numeric`: Whether value is numeric
- `value`: The actual measurement value
- `unit`: Physical unit (e.g., "kWh", "m³")
- `function_medium`: Function and medium type
- `quantity`: Quantity description
- `drh`: Data record header (DIB/VIB)
- `data_len`: Raw data length
- `data`: Raw data bytes

---

### `MBusRecordValue`
Enumeration of possible record values.

```rust
pub enum MBusRecordValue {
    None,
    Long(i64),
    Double(f64),
    String(String),
    Date(SystemTime),
    Bcd(u32),
}
```

**Variants:**
- `None`: No data present
- `Long(i64)`: Integer value
- `Double(f64)`: Floating point value
- `String(String)`: Text value
- `Date(SystemTime)`: Date/time value
- `Bcd(u32)`: BCD encoded value

## Frame Processing

### `parse_frame(input: &[u8]) -> IResult<&[u8], MBusFrame>`
Parse byte array into M-Bus frame structure.

**Parameters:**
- `input`: Raw frame bytes

**Returns:**
- `Ok((remaining, frame))`: Parsed frame and remaining bytes
- `Err(nom::Err)`: Parsing error

**Example:**
```rust
let bytes = vec![0xE5];
let (_, frame) = parse_frame(&bytes).unwrap();
assert_eq!(frame.frame_type, MBusFrameType::Ack);
```

---

### `pack_frame(frame: &MBusFrame) -> Vec<u8>`
Serialize frame structure to byte array.

**Parameters:**
- `frame`: Frame to serialize

**Returns:**
- `Vec<u8>`: Serialized frame bytes

**Example:**
```rust
let frame = MBusFrame {
    frame_type: MBusFrameType::Short,
    control: 0x40,
    address: 0x01,
    ..Default::default()
};
let bytes = pack_frame(&frame);
```

---

### `verify_frame(frame: &MBusFrame) -> Result<(), MBusError>`
Validate frame checksum and structure.

**Parameters:**
- `frame`: Frame to validate

**Returns:**
- `Ok(())`: Frame is valid
- `Err(MBusError)`: Validation error

## Protocol Operations

### `DataRetrievalManager`
High-level protocol manager for device communication.

```rust
pub struct DataRetrievalManager {
    fcb: bool,
    scanner: PrimaryAddressScanner,
    requestor: DataRequestor,
    parser: ResponseParser,
}
```

#### Methods

##### `initialize_device(&mut self, handle: &mut MBusDeviceHandle, address: u8) -> Result<(), MBusError>`
Initialize device communication with SND_NKE.

**Parameters:**
- `handle`: Device handle
- `address`: Target device address

**Returns:**
- `Ok(())`: Device initialized
- `Err(MBusError)`: Initialization failed

---

##### `request_data(&mut self, handle: &mut MBusDeviceHandle, address: u8) -> Result<Vec<MBusRecord>, MBusError>`
Request and parse data from device.

**Parameters:**
- `handle`: Device handle
- `address`: Target device address

**Returns:**
- `Ok(Vec<MBusRecord>)`: Parsed data records
- `Err(MBusError)`: Request or parsing failed

---

##### `scan_primary_addresses(&mut self, handle: &mut MBusDeviceHandle) -> Result<Vec<u8>, MBusError>`
Scan all primary addresses (1-250) for responding devices.

**Parameters:**
- `handle`: Device handle

**Returns:**
- `Ok(Vec<u8>)`: List of responding addresses
- `Err(MBusError)`: Scanning failed

## Data Structures

### `MBusDataRecordHeader`
Container for DIB and VIB structures.

```rust
pub struct MBusDataRecordHeader {
    pub dib: MBusDataInformationBlock,
    pub vib: MBusValueInformationBlock,
}
```

---

### `MBusDataInformationBlock`
Data Information Block (DIB) containing data format information.

```rust
pub struct MBusDataInformationBlock {
    pub dif: Vec<u8>,           // Data Information Field
    pub dife: Vec<u8>,          // DIF Extension
    pub data_field: u8,         // Data field type (0x0-0xF)
    pub function_field: u8,     // Function field
    pub storage_number: u32,    // Storage number
    pub tariff: i32,            // Tariff
    pub device: i32,            // Device
}
```

---

### `MBusValueInformationBlock`
Value Information Block (VIB) containing unit and scale information.

```rust
pub struct MBusValueInformationBlock {
    pub vif: Vec<u8>,           // Value Information Field
    pub vife: Vec<u8>,          // VIF Extension
    pub custom_vif: String,     // Custom VIF string
}
```

## Error Types

### `MBusError`
Comprehensive error enumeration for all M-Bus operations.

```rust
#[derive(Debug, thiserror::Error)]
pub enum MBusError {
    #[error("Serial port error: {0}")]
    SerialPortError(String),
    
    #[error("Frame parse error: {0}")]
    FrameParseError(String),
    
    #[error("Invalid checksum: expected {expected:02X}, calculated {calculated:02X}")]
    InvalidChecksum { expected: u8, calculated: u8 },
    
    #[error("Unknown VIF: {0:02X}")]
    UnknownVif(u8),
    
    #[error("Unknown DIF: {0:02X}")]
    UnknownDif(u8),
    
    #[error("Premature end at data")]
    PrematureEndAtData,
    
    #[error("Invalid manufacturer")]
    InvalidManufacturer,
    
    #[error("Timeout occurred")]
    Timeout,
    
    #[error("Device not responding")]
    DeviceNotResponding,
    
    #[error("Invalid frame length")]
    InvalidFrameLength,
    
    #[error("Invalid secondary address")]
    InvalidSecondaryAddress,
    
    #[error("Integer encode error: {0}")]
    IntegerEncodeError(String),
    
    #[error("Time decode error: {0}")]
    TimeDecodeError(String),
}
```

**Common Error Handling:**
```rust
match result {
    Ok(data) => println!("Success: {:?}", data),
    Err(MBusError::Timeout) => println!("Device timeout"),
    Err(MBusError::InvalidChecksum { expected, calculated }) => {
        println!("Checksum error: expected {:02X}, got {:02X}", expected, calculated);
    }
    Err(e) => println!("Error: {}", e),
}
```

## Serial Communication

### `MBusDeviceHandle`
Serial port handle for M-Bus communication.

```rust
pub struct MBusDeviceHandle {
    port: tokio_serial::SerialStream,
    config: SerialConfig,
}
```

#### Methods

##### `connect(port_name: &str) -> Result<MBusDeviceHandle, MBusError>`
Connect to serial port with default configuration.

**Parameters:**
- `port_name`: Serial port path

**Returns:**
- `Ok(MBusDeviceHandle)`: Connected handle
- `Err(MBusError)`: Connection failed

**Default Configuration:**
- Baud rate: 2400
- Data bits: 8
- Parity: Even
- Stop bits: 1
- Timeout: 300ms

---

##### `connect_with_config(port_name: &str, config: SerialConfig) -> Result<MBusDeviceHandle, MBusError>`
Connect with custom configuration.

**Parameters:**
- `port_name`: Serial port path
- `config`: Serial configuration

**Returns:**
- `Ok(MBusDeviceHandle)`: Connected handle
- `Err(MBusError)`: Connection failed

---

##### `send_frame(&mut self, frame: &MBusFrame) -> Result<(), MBusError>`
Send frame to device.

**Parameters:**
- `frame`: Frame to send

**Returns:**
- `Ok(())`: Frame sent successfully
- `Err(MBusError)`: Send failed

---

##### `recv_frame(&mut self) -> Result<MBusFrame, MBusError>`
Receive frame from device with timeout.

**Returns:**
- `Ok(MBusFrame)`: Received frame
- `Err(MBusError)`: Receive failed or timeout

### `SerialConfig`
Serial port configuration structure.

```rust
pub struct SerialConfig {
    pub baudrate: u32,      // 300, 600, 1200, 2400, 4800, 9600, 19200, 38400
    pub timeout: Duration,  // Read timeout
}
```

**Standard Configurations:**
```rust
// Low speed (utility meters)
let config = SerialConfig {
    baudrate: 300,
    timeout: Duration::from_millis(1300),
};

// Standard speed
let config = SerialConfig {
    baudrate: 2400,
    timeout: Duration::from_millis(300),
};

// High speed
let config = SerialConfig {
    baudrate: 9600,
    timeout: Duration::from_millis(200),
};
```

## Utility Functions

### Data Encoding

#### `decode_bcd(input: &[u8]) -> IResult<&[u8], u32>`
Decode BCD (Binary Coded Decimal) value.

**Parameters:**
- `input`: BCD encoded bytes

**Returns:**
- `Ok((remaining, value))`: Decoded decimal value
- `Err(nom::Err)`: Decoding error

**Example:**
```rust
let bcd = vec![0x12, 0x34];  // 3412 in BCD
let (_, value) = decode_bcd(&bcd).unwrap();
assert_eq!(value, 3412);
```

---

#### `encode_bcd(value: u32) -> Vec<u8>`
Encode decimal value as BCD.

**Parameters:**
- `value`: Decimal value to encode

**Returns:**
- `Vec<u8>`: BCD encoded bytes

---

#### `decode_int(input: &[u8], size: usize) -> IResult<&[u8], i32>`
Decode little-endian integer.

**Parameters:**
- `input`: Integer bytes
- `size`: Number of bytes (1, 2, 3, 4)

**Returns:**
- `Ok((remaining, value))`: Decoded integer
- `Err(nom::Err)`: Decoding error

---

#### `decode_float(input: &[u8]) -> IResult<&[u8], f32>`
Decode IEEE 754 32-bit float.

**Parameters:**
- `input`: Float bytes (4 bytes, little-endian)

**Returns:**
- `Ok((remaining, value))`: Decoded float
- `Err(nom::Err)`: Decoding error

### VIF Processing

#### `normalize_vib(vib: &MBusValueInformationBlock) -> (String, f64, String)`
Convert VIB to human-readable format.

**Parameters:**
- `vib`: Value Information Block

**Returns:**
- Tuple of (unit, scale_factor, quantity_description)

**Example:**
```rust
let vib = MBusValueInformationBlock {
    vif: vec![0x13],  // Volume 10^-3 m³
    vife: vec![],
    custom_vif: String::new(),
};
let (unit, scale, quantity) = normalize_vib(&vib);
// Returns: ("m³", 0.001, "Volume")
```

---

#### `parse_vif(vif: u8) -> Result<VifInfo, MBusError>`
Parse VIF byte into unit information.

**Parameters:**
- `vif`: VIF byte

**Returns:**
- `Ok(VifInfo)`: Unit information
- `Err(MBusError::UnknownVif)`: Unknown VIF code

### Manufacturer Encoding

#### `mbus_decode_manufacturer(byte1: u8, byte2: u8) -> String`
Decode manufacturer ID from 2 bytes.

**Parameters:**
- `byte1`, `byte2`: Manufacturer code bytes

**Returns:**
- `String`: 3-letter manufacturer code

**Example:**
```rust
let manufacturer = mbus_decode_manufacturer(0x01, 0xA7);  // "AAA"
```

---

#### `mbus_data_manufacturer_encode(manufacturer: &str) -> Result<[u8; 2], MBusError>`
Encode 3-letter manufacturer code to bytes.

**Parameters:**
- `manufacturer`: 3-letter code ("AAA" to "ZZZ")

**Returns:**
- `Ok([u8; 2])`: Encoded bytes
- `Err(MBusError)`: Invalid manufacturer code

### Logging

#### `init_logger()`
Initialize logging subsystem with environment configuration.

Uses `RUST_LOG` environment variable for level control:
```bash
export RUST_LOG=debug    # Enable debug logging
export RUST_LOG=info     # Enable info logging (default)
export RUST_LOG=warn     # Enable warning logging only
```

#### `log_info(message: &str)`, `log_error(message: &str)`, `log_debug(message: &str)`
Convenience logging functions.

**Parameters:**
- `message`: Message to log

**Example:**
```rust
init_logger();
log_info("Starting M-Bus communication");
log_debug("Sending frame to address 0x01");
```

## Constants

Key protocol constants defined in `constants.rs`:

```rust
// Frame delimiters
pub const MBUS_FRAME_ACK: u8 = 0xE5;
pub const MBUS_FRAME_SHORT_START: u8 = 0x10;
pub const MBUS_FRAME_LONG_START: u8 = 0x68;
pub const MBUS_FRAME_STOP: u8 = 0x16;

// Control field masks
pub const MBUS_CONTROL_MASK_SND_NKE: u8 = 0x40;
pub const MBUS_CONTROL_MASK_SND_UD: u8 = 0x53;
pub const MBUS_CONTROL_MASK_REQ_UD2: u8 = 0x5B;
pub const MBUS_CONTROL_MASK_REQ_UD1: u8 = 0x5A;
pub const MBUS_CONTROL_MASK_RSP_UD: u8 = 0x08;
pub const MBUS_CONTROL_MASK_FCB: u8 = 0x20;

// DIB/VIB masks
pub const MBUS_DIB_DIF_EXTENSION_BIT: u8 = 0x80;
pub const MBUS_DIB_VIF_EXTENSION_BIT: u8 = 0x80;
pub const MBUS_DATA_RECORD_DIF_MASK_DATA: u8 = 0x0F;
```

## Type Aliases

Common type aliases for convenience:

```rust
pub type Result<T> = std::result::Result<T, MBusError>;
pub type IResult<I, O> = nom::IResult<I, O, nom::error::Error<I>>;
```

## Feature Flags

The crate supports optional features:

```toml
[dependencies]
mbus-rs = { version = "0.1.0", features = ["async", "serde"] }
```

**Available Features:**
- `async`: Enable async/await support (default)
- `serde`: Enable serialization support for data structures
- `mock`: Enable mock serial port for testing

## Thread Safety

Most types are `Send` but not `Sync`:
- `MBusDeviceHandle`: `Send` (can move between threads)
- `MBusFrame`, `MBusRecord`: `Send + Sync` (immutable data)
- `DataRetrievalManager`: `Send` (contains mutable state)

For multi-threaded usage, wrap in `Arc<Mutex<T>>`:

```rust
use std::sync::{Arc, Mutex};

let handle = Arc::new(Mutex::new(
    MBusDeviceHandle::connect("/dev/ttyUSB0").await?
));

let handle_clone = handle.clone();
tokio::spawn(async move {
    let mut h = handle_clone.lock().unwrap();
    // Use handle...
});
```

## Radio Driver API

Advanced radio driver operations for direct SX126x control.

### Core Operations

#### `set_rx(timeout_ms: u32) -> Result<(), RadioError>`
Set radio in receive mode with timeout.

#### `set_tx() -> Result<(), RadioError>`
Set radio in transmit mode.

#### `process_irqs() -> Result<Option<Vec<u8>>, RadioError>`
Process pending interrupts and return received data if available.

#### `transmit(data: &[u8]) -> Result<(), RadioError>`
Transmit data packet.

### Configuration

#### `set_frequency(freq_hz: u32) -> Result<(), RadioError>`
Set center frequency.

#### `set_tx_power(power_dbm: i8) -> Result<(), RadioError>`
Set transmit power (-17 to +22 dBm).

#### `calibrate_image(freq_hz: u32) -> Result<(), RadioError>`
Perform image calibration for specified frequency.

### Status and Control

#### `get_irq_status() -> Result<IrqStatus, RadioError>`
Get current interrupt status.

#### `clear_irq_status(mask: u16) -> Result<(), RadioError>`
Clear specific interrupt flags.

#### `gpio_read(pin: u8) -> Result<bool, RadioError>`
Read GPIO pin state (DIO1, DIO2).

**Example:**
```rust
// Low-level radio control
let mut driver = Sx126xDriver::new(hal, 32_000_000);

// Configure radio manually
driver.set_frequency(868_950_000)?;
driver.set_tx_power(14)?;
driver.calibrate_image(868_000_000)?;

// Receive mode
driver.set_rx(5000)?; // 5 second timeout

// Check for received data
if let Some(data) = driver.process_irqs()? {
    println!("Received: {:?}", data);
}
```