# M-Bus Examples and Usage Patterns

## Table of Contents
- [Getting Started](#getting-started)
- [Basic Operations](#basic-operations)
- [Frame Handling](#frame-handling)
- [Data Parsing](#data-parsing)
- [Device Communication](#device-communication)
- [Advanced Patterns](#advanced-patterns)
- [Error Handling](#error-handling)
- [Testing Examples](#testing-examples)

## Getting Started

### Project Setup
```toml
# Cargo.toml
[dependencies]
mbus-rs = "0.1.0"
tokio = { version = "1.0", features = ["full"] }
hex = "0.4"
env_logger = "0.11"
log = "0.4"
```

### Basic Initialization
```rust
use mbus_rs::{init_logger, MBusError};

#[tokio::main]
async fn main() -> Result<(), MBusError> {
    // Initialize logging
    init_logger();
    
    // Your M-Bus code here
    Ok(())
}
```

## Basic Operations

### Connecting to a Device
```rust
use mbus_rs::mbus::serial::MBusDeviceHandle;

// Connect to serial port
let mut handle = MBusDeviceHandle::connect("/dev/ttyUSB0").await?;

// With specific baud rate
let mut handle = MBusDeviceHandle::connect_with_baud("/dev/ttyUSB0", 2400).await?;
```

### Simple Data Request
```rust
use mbus_rs::mbus::mbus_protocol::DataRetrievalManager;

let mut manager = DataRetrievalManager::default();

// Initialize device (SND_NKE)
manager.initialize_device(&mut handle, 0x01).await?;

// Request data from device
let records = manager.request_data(&mut handle, 0x01).await?;

for record in records {
    println!("Value: {:?}, Unit: {}", record.value, record.unit);
}
```

### Device Scanning
```rust
// Scan for all devices (primary addresses 1-250)
let mut manager = DataRetrievalManager::default();
let addresses = manager.scan_primary_addresses(&mut handle).await?;

println!("Found devices at addresses: {:?}", addresses);
```

## Frame Handling

### Parsing Raw Frames
```rust
use mbus_rs::mbus::frame::{parse_frame, MBusFrame, MBusFrameType};

// Parse ACK frame
let ack_bytes = vec![0xE5];
let (_, frame) = parse_frame(&ack_bytes).unwrap();
assert_eq!(frame.frame_type, MBusFrameType::Ack);

// Parse short frame
let short_bytes = vec![0x10, 0x5B, 0x01, 0x5C, 0x16];
let (_, frame) = parse_frame(&short_bytes).unwrap();
assert_eq!(frame.frame_type, MBusFrameType::Short);

// Parse long frame with data
let long_bytes = hex::decode("68 0A 0A 68 73 01 78 02 01 00 00 00 00 00 54 16")
    .unwrap()
    .into_iter()
    .filter(|b| *b != 0x20)  // Remove spaces
    .collect::<Vec<u8>>();
let (_, frame) = parse_frame(&long_bytes).unwrap();
assert_eq!(frame.frame_type, MBusFrameType::Long);
```

### Building Frames
```rust
use mbus_rs::mbus::frame::{pack_frame, MBusFrame, MBusFrameType};
use mbus_rs::constants::*;

// Create SND_NKE frame
let init_frame = MBusFrame {
    frame_type: MBusFrameType::Short,
    control: MBUS_CONTROL_MASK_SND_NKE,
    address: 0x01,
    ..Default::default()
};
let bytes = pack_frame(&init_frame);

// Create REQ_UD2 frame
let request_frame = MBusFrame {
    frame_type: MBusFrameType::Short,
    control: MBUS_CONTROL_MASK_REQ_UD2,
    address: 0x01,
    ..Default::default()
};
let bytes = pack_frame(&request_frame);

// Create long frame with data
let data_frame = MBusFrame {
    frame_type: MBusFrameType::Long,
    control: MBUS_CONTROL_MASK_RSP_UD,
    address: 0x01,
    control_information: 0x72,
    data: vec![0x01, 0x02, 0x03, 0x04],
    ..Default::default()
};
let bytes = pack_frame(&data_frame);
```

### Frame Validation
```rust
use mbus_rs::mbus::frame::verify_frame;

let frame = parse_frame(&frame_bytes).unwrap().1;
match verify_frame(&frame) {
    Ok(()) => println!("Frame is valid"),
    Err(MBusError::InvalidChecksum { expected, calculated }) => {
        println!("Checksum error: expected {:02X}, got {:02X}", expected, calculated);
    }
    Err(e) => println!("Frame error: {}", e),
}
```

## Data Parsing

### Parsing Variable Records
```rust
use mbus_rs::payload::record::parse_variable_record;

let data = vec![
    0x04, 0x13, 0x34, 0x12, 0x00, 0x00,  // DIB: 4-byte integer, VIF: Volume
];
let record = parse_variable_record(&data)?;

println!("Storage: {}", record.storage_number);
println!("Unit: {}", record.unit);
println!("Value: {:?}", record.value);
```

### Parsing Fixed Records
```rust
use mbus_rs::payload::record::parse_fixed_record;

// Fixed frame data (16 bytes)
let fixed_data = vec![
    0x78, 0x56, 0x34, 0x12,  // ID
    0xA7, 0x01,              // Manufacturer
    0x05, 0x01,              // Version, Medium
    0x00, 0x00,              // Access No, Status
    0x00, 0x00,              // Signature
    0x12, 0x34, 0x56, 0x78,  // Counter 1
];

let record = parse_fixed_record(&fixed_data)?;
```

### Decoding Data Values
```rust
use mbus_rs::payload::data::mbus_data_record_decode;
use mbus_rs::payload::data_encoding::{decode_bcd, decode_int};

// Decode BCD value
let bcd_bytes = vec![0x12, 0x34];  // 3412 in BCD
let (_, value) = decode_bcd(&bcd_bytes).unwrap();
println!("BCD value: {}", value);  // 3412

// Decode integer
let int_bytes = vec![0x34, 0x12];  // Little-endian
let (_, value) = decode_int(&int_bytes, 2).unwrap();
println!("Integer value: {}", value);  // 0x1234 = 4660

// Decode record with VIF normalization
let (quantity, value, unit) = mbus_data_record_decode(&record)?;
println!("{}: {:?} {}", quantity, value, unit);
```

### Working with VIF/DIF
```rust
use mbus_rs::payload::vif::{normalize_vib, parse_vif};
use mbus_rs::payload::record::{MBusValueInformationBlock, MBusDataInformationBlock};

// Parse VIF to get unit info
let vif_info = parse_vif(0x13)?;  // Volume in m³
println!("Unit: {}, Scale: {}", vif_info.unit, vif_info.scale);

// Normalize VIB for display
let vib = MBusValueInformationBlock {
    vif: vec![0x13],  // Volume 10^-3 m³
    vife: vec![],
    custom_vif: String::new(),
};
let (unit, scale, quantity) = normalize_vib(&vib);
println!("{}: {} (scale: {})", quantity, unit, scale);
```

## Device Communication

### Request-Response Pattern
```rust
use mbus_rs::mbus::frame::{parse_frame, pack_frame};
use std::time::Duration;
use tokio::time::timeout;

async fn request_data(
    handle: &mut MBusDeviceHandle,
    address: u8
) -> Result<Vec<MBusRecord>, MBusError> {
    // Send REQ_UD2
    let request = create_request_frame(address);
    handle.send_frame(&request).await?;
    
    // Receive response with timeout
    let response = timeout(
        Duration::from_secs(2),
        handle.recv_frame()
    ).await
        .map_err(|_| MBusError::Timeout)?
        .map_err(|e| e)?;
    
    // Send ACK
    let ack = MBusFrame {
        frame_type: MBusFrameType::Ack,
        ..Default::default()
    };
    handle.send_frame(&ack).await?;
    
    // Parse records from response
    parse_response_records(&response)
}
```

### Multi-Telegram Handling
```rust
async fn read_multi_telegram(
    handle: &mut MBusDeviceHandle,
    address: u8
) -> Result<Vec<MBusRecord>, MBusError> {
    let mut all_records = Vec::new();
    let mut more_data = true;
    
    while more_data {
        let request = create_request_frame(address);
        handle.send_frame(&request).await?;
        
        let response = handle.recv_frame().await?;
        more_data = response.more_records_follow;
        
        let records = parse_response_records(&response)?;
        all_records.extend(records);
        
        // Send ACK
        handle.send_frame(&create_ack_frame()).await?;
    }
    
    Ok(all_records)
}
```

### Secondary Addressing
```rust
use mbus_rs::mbus::frame::pack_select_frame;

async fn select_secondary_address(
    handle: &mut MBusDeviceHandle,
    secondary_addr: &str  // e.g., "12345678AAAAVVVV"
) -> Result<(), MBusError> {
    // Create selection frame
    let mut select_frame = MBusFrame {
        frame_type: MBusFrameType::Long,
        control: 0x53,
        address: 0xFD,
        control_information: 0x52,
        ..Default::default()
    };
    
    pack_select_frame(&mut select_frame, secondary_addr)?;
    
    // Send selection (no response expected)
    handle.send_frame(&select_frame).await?;
    
    // Small delay for device activation
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Now use address 0xFD for communication
    let request = create_request_frame(0xFD);
    handle.send_frame(&request).await?;
    
    Ok(())
}
```

## Advanced Patterns

### Custom VIF Handling
```rust
use mbus_rs::payload::vif::VifInfo;

fn handle_manufacturer_vif(vif: u8, data: &[u8]) -> Result<VifInfo, MBusError> {
    if vif == 0x7F {
        // Manufacturer specific
        // Parse according to manufacturer documentation
        Ok(VifInfo {
            unit: "custom".to_string(),
            scale: 1.0,
            quantity: "Manufacturer Specific".to_string(),
        })
    } else if vif >= 0x7B && vif <= 0x7E {
        // VIF in following string
        let vif_string = String::from_utf8_lossy(&data[0..2]);
        Ok(VifInfo {
            unit: vif_string.to_string(),
            scale: 1.0,
            quantity: "Extended".to_string(),
        })
    } else {
        Err(MBusError::UnknownVif(vif))
    }
}
```

### Batch Device Reading
```rust
async fn read_all_devices(
    port: &str,
    addresses: Vec<u8>
) -> HashMap<u8, Vec<MBusRecord>> {
    let mut results = HashMap::new();
    let mut handle = MBusDeviceHandle::connect(port).await.unwrap();
    let mut manager = DataRetrievalManager::default();
    
    for address in addresses {
        match manager.request_data(&mut handle, address).await {
            Ok(records) => {
                results.insert(address, records);
            }
            Err(e) => {
                log::warn!("Failed to read device {}: {}", address, e);
            }
        }
    }
    
    results
}
```

### Data Export
```rust
use serde_json::json;

fn export_records_to_json(records: &[MBusRecord]) -> String {
    let json_records: Vec<_> = records.iter().map(|r| {
        json!({
            "timestamp": r.timestamp.duration_since(UNIX_EPOCH).unwrap().as_secs(),
            "value": format!("{:?}", r.value),
            "unit": r.unit,
            "quantity": r.quantity,
            "storage": r.storage_number,
            "tariff": r.tariff,
        })
    }).collect();
    
    serde_json::to_string_pretty(&json_records).unwrap()
}

fn export_records_to_csv(records: &[MBusRecord]) -> String {
    let mut csv = String::from("timestamp,value,unit,quantity,storage,tariff\n");
    
    for record in records {
        csv.push_str(&format!(
            "{},{:?},{},{},{},{}\n",
            record.timestamp.duration_since(UNIX_EPOCH).unwrap().as_secs(),
            record.value,
            record.unit,
            record.quantity,
            record.storage_number,
            record.tariff
        ));
    }
    
    csv
}
```

## Error Handling

### Comprehensive Error Handling
```rust
use mbus_rs::MBusError;

async fn robust_device_read(
    handle: &mut MBusDeviceHandle,
    address: u8,
    max_retries: u32
) -> Result<Vec<MBusRecord>, MBusError> {
    let mut retries = 0;
    
    loop {
        match read_device_with_timeout(handle, address).await {
            Ok(records) => return Ok(records),
            Err(MBusError::Timeout) if retries < max_retries => {
                log::warn!("Timeout reading device {}, retry {}/{}", 
                    address, retries + 1, max_retries);
                retries += 1;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(MBusError::InvalidChecksum { .. }) if retries < max_retries => {
                log::warn!("Checksum error, retry {}/{}", 
                    retries + 1, max_retries);
                retries += 1;
            }
            Err(MBusError::SerialPortError(ref e)) if e.contains("timed out") => {
                return Err(MBusError::DeviceNotResponding);
            }
            Err(e) => return Err(e),
        }
    }
}
```

### Error Recovery
```rust
async fn recover_from_error(
    handle: &mut MBusDeviceHandle,
    address: u8
) -> Result<(), MBusError> {
    // Try to reset device state
    let mut manager = DataRetrievalManager::default();
    
    // Send SND_NKE to initialize
    if let Err(e) = manager.initialize_device(handle, address).await {
        log::error!("Failed to initialize: {}", e);
        
        // Try reconnecting
        drop(handle);
        tokio::time::sleep(Duration::from_secs(1)).await;
        *handle = MBusDeviceHandle::connect("/dev/ttyUSB0").await?;
    }
    
    Ok(())
}
```

## Testing Examples

### Unit Test Example
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_frame_parsing() {
        let frame_bytes = vec![0xE5];
        let result = parse_frame(&frame_bytes);
        assert!(result.is_ok());
        let (_, frame) = result.unwrap();
        assert_eq!(frame.frame_type, MBusFrameType::Ack);
    }
    
    #[test]
    fn test_bcd_decoding() {
        let bcd = vec![0x12, 0x34];
        let (_, value) = decode_bcd(&bcd).unwrap();
        assert_eq!(value, 3412);
    }
}
```

### Integration Test with Mock
```rust
#[cfg(test)]
mod integration_tests {
    use mbus_rs::mbus::serial_mock::MockSerialPort;
    use mbus_rs::mbus::serial_testable::TestableDeviceHandle;
    
    #[tokio::test]
    async fn test_device_communication() {
        // Setup mock
        let mock = MockSerialPort::new();
        mock.queue_frame_response(
            FrameType::Long {
                control: 0x08,
                address: 0x01,
                ci: 0x72,
                data: Some(vec![0x01, 0x02, 0x03])
            },
            None
        );
        
        // Create testable handle
        let mut handle = TestableDeviceHandle::new(
            mock.clone(),
            2400,
            Duration::from_secs(1)
        );
        
        // Test communication
        let request = create_request_frame(0x01);
        handle.send_frame(&request).await.unwrap();
        
        let response = handle.recv_frame().await.unwrap();
        assert_eq!(response.address, 0x01);
        
        // Verify sent data
        let tx_data = mock.get_tx_data();
        assert_eq!(tx_data[0], 0x10);  // Short frame start
    }
}
```

### Property Testing Example
```rust
#[cfg(test)]
mod property_tests {
    use proptest::prelude::*;
    
    proptest! {
        #[test]
        fn test_frame_roundtrip(
            control in 0u8..255,
            address in 0u8..255,
            data in prop::collection::vec(0u8..255, 0..252)
        ) {
            let frame = MBusFrame {
                frame_type: MBusFrameType::Long,
                control,
                address,
                control_information: 0x72,
                data: data.clone(),
                ..Default::default()
            };
            
            let packed = pack_frame(&frame);
            let (_, unpacked) = parse_frame(&packed).unwrap();
            
            assert_eq!(unpacked.control, control);
            assert_eq!(unpacked.address, address);
            assert_eq!(unpacked.data, data);
        }
    }
}
```

## Complete Example Application

```rust
use mbus_rs::{
    init_logger,
    MBusError,
    mbus::serial::MBusDeviceHandle,
    mbus::mbus_protocol::DataRetrievalManager,
};
use std::env;

#[tokio::main]
async fn main() -> Result<(), MBusError> {
    // Initialize logging
    init_logger();
    
    // Get port from command line
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <serial_port>", args[0]);
        std::process::exit(1);
    }
    
    let port = &args[1];
    
    // Connect to device
    log::info!("Connecting to {}...", port);
    let mut handle = MBusDeviceHandle::connect(port).await?;
    
    // Create protocol manager
    let mut manager = DataRetrievalManager::default();
    
    // Scan for devices
    log::info!("Scanning for devices...");
    let addresses = manager.scan_primary_addresses(&mut handle).await?;
    log::info!("Found {} devices: {:?}", addresses.len(), addresses);
    
    // Read from each device
    for address in addresses {
        log::info!("Reading from device {}...", address);
        
        // Initialize device
        if let Err(e) = manager.initialize_device(&mut handle, address).await {
            log::error!("Failed to initialize device {}: {}", address, e);
            continue;
        }
        
        // Request data
        match manager.request_data(&mut handle, address).await {
            Ok(records) => {
                log::info!("Device {} returned {} records:", address, records.len());
                for record in records {
                    log::info!("  {} = {:?} {}", 
                        record.quantity, 
                        record.value, 
                        record.unit
                    );
                }
            }
            Err(e) => {
                log::error!("Failed to read device {}: {}", address, e);
            }
        }
    }
    
    log::info!("Done!");
    Ok(())
}
```

## Resources

- [EN 13757-3 Standard](https://www.en-standard.eu/)
- [M-Bus Protocol Documentation](http://www.m-bus.com/)
- [Example Meter Data](https://github.com/rscada/libmbus/tree/master/test)
- [Rust Async Book](https://rust-lang.github.io/async-book/)
- [nom Parser Combinators](https://docs.rs/nom/)