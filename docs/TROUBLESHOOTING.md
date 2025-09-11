# Troubleshooting Guide

This guide covers common issues when working with M-Bus devices and the mbus-rs crate.

## Table of Contents
- [Connection Issues](#connection-issues)
- [Communication Problems](#communication-problems)
- [Data Parsing Errors](#data-parsing-errors)
- [Performance Issues](#performance-issues)
- [Platform-Specific Issues](#platform-specific-issues)
- [Debugging Techniques](#debugging-techniques)
- [FAQ](#frequently-asked-questions)

## Connection Issues

### Problem: "Permission denied" when opening serial port

**Linux/macOS:**
```bash
# Check port permissions
ls -l /dev/ttyUSB0

# Add user to dialout group (Linux)
sudo usermod -a -G dialout $USER
# Log out and back in for changes to take effect

# Or use sudo (not recommended for production)
sudo cargo run
```

**Windows:**
- Run application as Administrator
- Check if port is already in use by another application

### Problem: "No such file or directory" for serial port

**Verify port exists:**
```bash
# Linux
ls /dev/tty*

# macOS
ls /dev/tty.*

# Windows (in PowerShell)
[System.IO.Ports.SerialPort]::GetPortNames()
```

**Common port names:**
- Linux: `/dev/ttyUSB0`, `/dev/ttyS0`, `/dev/ttyACM0`
- macOS: `/dev/tty.usbserial-*`, `/dev/tty.usbmodem*`
- Windows: `COM1`, `COM2`, `COM3`, etc.

### Problem: Serial port exists but won't connect

**Check if port is in use:**
```bash
# Linux/macOS
lsof | grep ttyUSB0
fuser /dev/ttyUSB0

# Kill process using port
sudo fuser -k /dev/ttyUSB0
```

## Communication Problems

### Problem: Device not responding (timeout errors)

**1. Check baud rate:**
```rust
// Common M-Bus baud rates
let baud_rates = [300, 600, 1200, 2400, 4800, 9600, 19200, 38400];

for baud in baud_rates {
    let config = SerialConfig {
        baudrate: baud,
        timeout: Duration::from_millis(1500),
    };
    
    match MBusDeviceHandle::connect_with_config(port, config).await {
        Ok(mut handle) => {
            println!("Connected at {} baud", baud);
            // Try communication
            if manager.initialize_device(&mut handle, address).await.is_ok() {
                println!("Device responds at {} baud", baud);
                break;
            }
        }
        Err(_) => continue,
    }
}
```

**2. Verify wiring:**
- Check M-Bus to serial converter power supply
- Verify TX/RX connections
- Ensure proper grounding
- Check cable length (max ~1000m for M-Bus)

**3. Test with known-good device:**
```rust
// Send SND_NKE to initialize
let init_frame = MBusFrame {
    frame_type: MBusFrameType::Short,
    control: 0x40,  // SND_NKE
    address: 0xFE,  // Broadcast
    ..Default::default()
};
handle.send_frame(&init_frame).await?;
```

### Problem: Checksum errors

**Enable debug logging:**
```bash
RUST_LOG=debug cargo run
```

**Common causes:**
- Electrical interference
- Incorrect baud rate
- Damaged cable
- Too many devices on bus (max 250)

**Diagnostic code:**
```rust
use mbus_rs::mbus::frame::{parse_frame, verify_frame};

// Capture raw bytes
let mut raw_buffer = vec![0u8; 256];
let bytes_read = handle.port.read(&mut raw_buffer).await?;
let frame_bytes = &raw_buffer[..bytes_read];

// Parse and verify
match parse_frame(frame_bytes) {
    Ok((_, frame)) => {
        match verify_frame(&frame) {
            Ok(()) => println!("Frame valid"),
            Err(MBusError::InvalidChecksum { expected, calculated }) => {
                println!("Checksum mismatch: expected {:02X}, got {:02X}", 
                    expected, calculated);
                println!("Raw bytes: {:02X?}", frame_bytes);
            }
            Err(e) => println!("Verification error: {}", e),
        }
    }
    Err(e) => println!("Parse error: {:?}", e),
}
```

### Problem: Partial or corrupted frames

**Increase timeout for slow devices:**
```rust
let config = SerialConfig {
    baudrate: 2400,
    timeout: Duration::from_millis(2000),  // Increase from default
};
```

**Handle multi-telegram responses:**
```rust
let mut all_records = Vec::new();
loop {
    let response = handle.recv_frame().await?;
    all_records.extend(parse_records(&response)?);
    
    if !response.more_records_follow {
        break;
    }
    
    // Send ACK for next telegram
    let ack = MBusFrame {
        frame_type: MBusFrameType::Ack,
        ..Default::default()
    };
    handle.send_frame(&ack).await?;
}
```

## Data Parsing Errors

### Problem: Unknown VIF codes

**Handle manufacturer-specific VIFs:**
```rust
match parse_vif(vif_byte) {
    Ok(info) => println!("Standard VIF: {}", info.unit),
    Err(MBusError::UnknownVif(vif)) if vif >= 0x7C && vif <= 0x7F => {
        // Manufacturer specific
        println!("Manufacturer VIF: {:02X}", vif);
        // Check manufacturer documentation
    }
    Err(e) => println!("VIF error: {}", e),
}
```

### Problem: Incorrect data values

**Verify data encoding:**
```rust
use mbus_rs::payload::data_encoding::{decode_bcd, decode_int};

// BCD vs Binary confusion
let bytes = vec![0x12, 0x34];

// As BCD: 3412
let (_, bcd_value) = decode_bcd(&bytes).unwrap();
println!("BCD: {}", bcd_value);  // 3412

// As integer: 0x3412
let (_, int_value) = decode_int(&bytes, 2).unwrap();
println!("Integer: {}", int_value);  // 13330
```

### Problem: Missing or incomplete records

**Check device configuration:**
- Some devices need configuration to send all data
- Use manufacturer software to verify device setup
- Check if device requires special initialization sequence

## Performance Issues

### Problem: Slow device scanning

**Optimize scan with parallel operations:**
```rust
use futures::future::join_all;

async fn fast_scan(port: &str) -> Vec<u8> {
    let mut handles = Vec::new();
    
    // Create multiple connections (if supported)
    for _ in 0..4 {
        handles.push(MBusDeviceHandle::connect(port).await?);
    }
    
    // Scan in parallel batches
    let futures: Vec<_> = (1..=250)
        .collect::<Vec<_>>()
        .chunks(handles.len())
        .map(|chunk| {
            let handle = &mut handles[i % handles.len()];
            async move {
                // Scan chunk
            }
        })
        .collect();
    
    join_all(futures).await
}
```

### Problem: High CPU usage

**Profile the application:**
```bash
# Install flamegraph
cargo install flamegraph

# Run with profiling
cargo flamegraph --bin mbus-rs

# View flamegraph.svg in browser
```

**Common causes:**
- Polling too frequently
- Not using async properly
- Parsing same data repeatedly

## Platform-Specific Issues

### Linux

**USB device disconnection:**
```bash
# Check kernel messages
dmesg | tail -20

# Disable USB autosuspend
echo -1 | sudo tee /sys/module/usbcore/parameters/autosuspend

# Or for specific device
echo on | sudo tee /sys/bus/usb/devices/*/power/control
```

### macOS

**Serial port naming:**
```bash
# Find USB serial devices
ls /dev/tty.* | grep -E "(usbserial|usbmodem)"

# Get device info
system_profiler SPUSBDataType
```

### Windows

**Driver issues:**
1. Check Device Manager for driver errors
2. Install FTDI/CH340/CP210x drivers as needed
3. Use Zadig for driver replacement if necessary

**COM port numbering:**
```powershell
# List COM ports
Get-WmiObject Win32_SerialPort | Select Name, DeviceID, Description

# Change COM port number
# Device Manager → Ports → Properties → Port Settings → Advanced
```

## Debugging Techniques

### Enable verbose logging

```bash
# Maximum verbosity
RUST_LOG=trace cargo run

# Module-specific logging
RUST_LOG=mbus_rs::mbus::serial=debug cargo run

# With timestamps
RUST_LOG=debug RUST_LOG_STYLE=always cargo run 2>&1 | ts
```

### Capture raw serial traffic

**Linux/macOS - Using socat:**
```bash
# Create virtual serial port pair
socat -d -d pty,raw,echo=0 pty,raw,echo=0

# Monitor traffic
socat -x -v /dev/ttyUSB0,raw,echo=0 PTY,link=/tmp/ttyV0,raw,echo=0
```

**Using interceptty:**
```bash
interceptty /dev/ttyUSB0 /tmp/ttyDUMP
```

### Protocol analysis

**Save frames for analysis:**
```rust
use std::fs::File;
use std::io::Write;

// Save raw frames
let mut dump = File::create("frames.hex")?;
writeln!(dump, "{:02X?}", frame_bytes)?;

// Save parsed frames
let mut log = File::create("frames.log")?;
writeln!(log, "{:#?}", parsed_frame)?;
```

**Analyze with external tools:**
- Wireshark (with M-Bus dissector)
- SerialPCAP for serial capture
- Logic analyzer for electrical signals

### Mock testing

**Test without hardware:**
```rust
use mbus_rs::mbus::serial_mock::MockSerialPort;

#[tokio::test]
async fn test_device_behavior() {
    let mock = MockSerialPort::new();
    
    // Queue expected response
    mock.queue_frame_response(
        FrameType::Long {
            control: 0x08,
            address: 0x01,
            ci: 0x72,
            data: Some(test_data),
        },
        None
    );
    
    // Test your code
    let mut handle = TestableDeviceHandle::new(mock, 2400, Duration::from_secs(1));
    // ... run tests
}
```

## Frequently Asked Questions

### Q: What baud rate should I use?

**A:** Most M-Bus devices default to 2400 baud. Try this sequence:
1. 2400 (most common)
2. 300 (old devices)
3. 9600 (newer devices)
4. Auto-detect using scan

### Q: How many devices can I connect?

**A:** 
- Standard M-Bus: Up to 250 devices (addresses 1-250)
- Depends on power supply capacity
- Cable length affects maximum count

### Q: Why do I get timeouts even with correct settings?

**A:** Common reasons:
- Device needs initialization (SND_NKE)
- Device is in sleep mode
- Response time > timeout setting
- Electrical issues (voltage drop, noise)

### Q: Can I use RS-485 instead of RS-232?

**A:** Yes, M-Bus typically uses:
- RS-232 for PC to M-Bus converter
- RS-485 for longer distances
- Ensure proper termination resistors

### Q: How do I handle encrypted wireless M-Bus?

**A:** Wireless M-Bus encryption:
```rust
// When implemented
use mbus_rs::wmbus::encryption::decrypt_frame;

let key = [0x00; 16];  // 128-bit AES key
let decrypted = decrypt_frame(&encrypted_frame, &key)?;
```

### Q: What's the maximum cable length?

**A:**
- Standard M-Bus: 1000m @ 2400 baud
- With repeaters: 5000m
- Lower baud rates allow longer cables

### Q: How do I read from multiple devices efficiently?

**A:** Use async concurrency:
```rust
use futures::future::try_join_all;

let futures = addresses.iter().map(|addr| {
    read_device(&mut handle, *addr)
});

let results = try_join_all(futures).await?;
```

## Getting Help

If you encounter issues not covered here:

1. **Check logs** with `RUST_LOG=debug`
2. **Search existing issues** on GitHub
3. **Create minimal reproduction** example
4. **Open an issue** with:
   - Error messages
   - Debug logs
   - Code snippet
   - Hardware setup
   - OS and Rust version

## Related Documentation

- [Hardware Guide](HARDWARE.md) - Compatible devices and setup
- [API Reference](API.md) - Complete API documentation
- [Examples](EXAMPLES.md) - Code examples and patterns
- [Protocol Reference](PROTOCOL.md) - M-Bus protocol details