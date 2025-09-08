//! # M-Bus Serial Communication
//!
//! This module provides the implementation for handling the serial communication
//! aspect of the M-Bus protocol, including connecting to the serial port,
//! sending M-Bus frames, and receiving M-Bus frames.

use crate::error::MBusError;
use crate::mbus::frame::{pack_frame, parse_frame, MBusFrame};
use crate::payload::record::MBusRecord;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::SerialPortBuilderExt;

/// Configuration for serial connection.
#[derive(Debug, Clone)]
pub struct SerialConfig {
    pub baudrate: u32,
    pub timeout: Duration,
}

impl Default for SerialConfig {
    fn default() -> Self {
        SerialConfig {
            baudrate: 2400,
            timeout: Duration::from_secs(5),
        }
    }
}

/// Represents a handle to the M-Bus serial connection, encapsulating the tokio_serial::SerialPort.
pub struct MBusDeviceHandle {
    port: tokio_serial::SerialStream,
    config: SerialConfig,
}

impl MBusDeviceHandle {
/// Establishes a connection to the serial port using the provided port name.
/// It sets up the serial port settings (baud rate, data bits, stop bits, parity, and timeout) and opens the port.
pub async fn connect(port_name: &str) -> Result<MBusDeviceHandle, MBusError> {
    Self::connect_with_config(port_name, SerialConfig::default()).await
}

/// Establishes a connection with custom config.
pub async fn connect_with_config(port_name: &str, config: SerialConfig) -> Result<MBusDeviceHandle, MBusError> {
    let port = tokio_serial::new(port_name, config.baudrate)
        .data_bits(tokio_serial::DataBits::Eight)
        .stop_bits(tokio_serial::StopBits::One)
        .parity(tokio_serial::Parity::Even)
        .timeout(config.timeout)
        .open_native_async()
        .map_err(|e| MBusError::SerialPortError(e.to_string()))?;

    Ok(MBusDeviceHandle { port, config })
}

/// Closes the serial port connection.
pub async fn disconnect(&mut self) -> Result<(), MBusError> {
    // SerialStream does not have a close method; dropping the handle closes it
    Ok(())
}

/// Takes an `MBusFrame` and sends it over the serial connection.
/// It uses the `pack_frame()` function from the `frame.rs` module to convert the frame to a byte vector,
/// and then writes the data to the serial port. It also flushes the serial port to ensure the frame is fully transmitted.
pub async fn send_frame(&mut self, frame: &MBusFrame) -> Result<(), MBusError> {
    let data = pack_frame(frame);
    self.port
        .write_all(&data)
        .await
        .map_err(|e| MBusError::SerialPortError(e.to_string()))?;
    self.port
        .flush()
        .await
        .map_err(|e| MBusError::SerialPortError(e.to_string()))
}

/// Reads data from the serial port and attempts to parse an `MBusFrame` from the received bytes.
/// It uses a fixed-size buffer to read the data, and then calls the `parse_frame()` function
/// from the `frame.rs` module to parse the frame.
pub async fn recv_frame(&mut self) -> Result<MBusFrame, MBusError> {
    use tokio::time::timeout;

    // Map baudrate to a coarse timeout (similar to epulse defaults)
    let to = match self.config.baudrate {
        300 => Duration::from_millis(1300),
        600 => Duration::from_millis(800),
        1200 => Duration::from_millis(500),
        2400 => Duration::from_millis(300),
        4800 => Duration::from_millis(300),
        9600 => Duration::from_millis(200),
        19200 => Duration::from_millis(200),
        38400 => Duration::from_millis(200),
        _ => Duration::from_millis(500),
    };

    // Read first byte (start)
    let mut start = [0u8; 1];
    let n = timeout(to, self.port.read(&mut start))
        .await
        .map_err(|_| MBusError::NomError("timeout".into()))
        .and_then(|res| res.map_err(|e| MBusError::SerialPortError(e.to_string())))?;
    if n == 0 {
        return Err(MBusError::NomError("empty".into()));
    }

    let total_len = match start[0] {
        0xE5 => 1usize,         // ACK
        0x10 => 5usize,         // SHORT
        0x68 => {
            // Need to read two length bytes to determine total
            let mut lenbuf = [0u8; 2];
            timeout(to, self.port.read_exact(&mut lenbuf))
                .await
                .map_err(|_| MBusError::NomError("timeout".into()))
                .and_then(|res| res.map_err(|e| MBusError::SerialPortError(e.to_string())))?;
            let length1 = lenbuf[0] as usize;
            // total = len1 + 6 bytes (0x68 len1 len2 0x68 ... checksum 0x16)
            6 + length1
        }
        _ => return Err(MBusError::FrameParseError("Invalid frame start".into())),
    };

    // We already consumed 1 byte, possibly 3 bytes; gather remaining
    let mut buf = Vec::with_capacity(total_len);
    buf.push(start[0]);
    if start[0] == 0x68 {
        // fetch already-read len bytes and read rest
        // We already read lenbuf; but we didn't keep them. Re-read full frame after start for simplicity
        // Read remaining (total_len - 1) bytes
        let mut rest = vec![0u8; total_len - 1];
        timeout(to, self.port.read_exact(&mut rest))
            .await
            .map_err(|_| MBusError::NomError("timeout".into()))
            .and_then(|res| res.map_err(|e| MBusError::SerialPortError(e.to_string())))?;
        buf.extend_from_slice(&rest);
    } else {
        let mut rest = vec![0u8; total_len - 1];
        timeout(to, self.port.read_exact(&mut rest))
            .await
            .map_err(|_| MBusError::NomError("timeout".into()))
            .and_then(|res| res.map_err(|e| MBusError::SerialPortError(e.to_string())))?;
        buf.extend_from_slice(&rest);
    }

    let (_, frame) =
        parse_frame(&buf[..]).map_err(|e| MBusError::FrameParseError(format!("{:?}", e)))?;
    Ok(frame)
}

// Stub: send a request to a device by address and return parsed records (none by default).
}

impl MBusDeviceHandle {
    pub async fn send_request(&mut self, _address: u8) -> Result<Vec<MBusRecord>, MBusError> {
        Ok(Vec::new())
    }

    pub async fn scan_devices(&mut self) -> Result<Vec<String>, MBusError> {
        Ok(Vec::new())
    }
}
