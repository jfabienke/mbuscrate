//! Mock serial port implementation for testing
//!
//! This module provides a mock serial port that can be used to test
//! the M-Bus serial communication without requiring actual hardware.

use std::collections::VecDeque;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Mock serial port that simulates bidirectional communication
#[derive(Clone)]
pub struct MockSerialPort {
    /// Data written to the port (outgoing)
    pub tx_buffer: Arc<Mutex<Vec<u8>>>,
    /// Data to be read from the port (incoming)
    pub rx_buffer: Arc<Mutex<VecDeque<u8>>>,
    /// Simulated errors
    pub next_error: Arc<Mutex<Option<io::Error>>>,
    /// Read delay in milliseconds (to simulate timing)
    pub read_delay_ms: Arc<Mutex<u64>>,
}

impl Default for MockSerialPort {
    fn default() -> Self {
        Self::new()
    }
}

impl MockSerialPort {
    pub fn new() -> Self {
        MockSerialPort {
            tx_buffer: Arc::new(Mutex::new(Vec::new())),
            rx_buffer: Arc::new(Mutex::new(VecDeque::new())),
            next_error: Arc::new(Mutex::new(None)),
            read_delay_ms: Arc::new(Mutex::new(0)),
        }
    }

    /// Queue data to be read from the port
    pub fn queue_rx_data(&self, data: &[u8]) {
        let mut rx = self.rx_buffer.lock().unwrap();
        rx.extend(data);
    }

    /// Get data that was written to the port
    pub fn get_tx_data(&self) -> Vec<u8> {
        self.tx_buffer.lock().unwrap().clone()
    }

    /// Clear all buffers
    pub fn clear(&self) {
        self.tx_buffer.lock().unwrap().clear();
        self.rx_buffer.lock().unwrap().clear();
    }

    /// Set an error to be returned on the next operation
    pub fn set_next_error(&self, error: io::Error) {
        *self.next_error.lock().unwrap() = Some(error);
    }

    /// Queue an M-Bus frame response
    pub fn queue_frame_response(&self, frame_type: FrameType, _data: Option<Vec<u8>>) {
        let response = match frame_type {
            FrameType::Ack => vec![0xE5],
            FrameType::Short {
                control,
                address,
                checksum,
            } => {
                vec![0x10, control, address, checksum, 0x16]
            }
            FrameType::Long {
                control,
                address,
                ci,
                data: frame_data,
            } => {
                let data_bytes = frame_data.unwrap_or_default();
                let len = (3 + data_bytes.len()) as u8;
                let mut frame = vec![0x68, len, len, 0x68, control, address, ci];
                frame.extend(&data_bytes);

                // Calculate checksum
                let mut checksum = control.wrapping_add(address).wrapping_add(ci);
                for byte in &data_bytes {
                    checksum = checksum.wrapping_add(*byte);
                }
                frame.push(checksum);
                frame.push(0x16);
                frame
            }
            FrameType::Control {
                control,
                address,
                ci,
                checksum,
            } => {
                vec![0x68, 0x03, 0x03, 0x68, control, address, ci, checksum, 0x16]
            }
            FrameType::Invalid => vec![0xFF, 0xFF, 0xFF],
        };
        self.queue_rx_data(&response);
    }
}

pub enum FrameType {
    Ack,
    Short {
        control: u8,
        address: u8,
        checksum: u8,
    },
    Long {
        control: u8,
        address: u8,
        ci: u8,
        data: Option<Vec<u8>>,
    },
    Control {
        control: u8,
        address: u8,
        ci: u8,
        checksum: u8,
    },
    Invalid,
}

// Implement AsyncRead for MockSerialPort
impl AsyncRead for MockSerialPort {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // Check for simulated error
        if let Some(error) = self.next_error.lock().unwrap().take() {
            return Poll::Ready(Err(error));
        }

        // Simulate read delay if configured
        if *self.read_delay_ms.lock().unwrap() > 0 {
            // In real implementation, would use tokio::time::sleep
            // For testing, we'll just proceed
        }

        let mut rx = self.rx_buffer.lock().unwrap();
        let available = rx.len().min(buf.remaining());

        if available > 0 {
            let data: Vec<u8> = rx.drain(..available).collect();
            buf.put_slice(&data);
        }

        Poll::Ready(Ok(()))
    }
}

// Implement AsyncWrite for MockSerialPort
impl AsyncWrite for MockSerialPort {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // Check for simulated error
        if let Some(error) = self.next_error.lock().unwrap().take() {
            return Poll::Ready(Err(error));
        }

        let mut tx = self.tx_buffer.lock().unwrap();
        tx.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_serial_port_creation() {
        let port = MockSerialPort::new();
        assert_eq!(port.get_tx_data().len(), 0);
    }

    #[test]
    fn test_queue_and_read_data() {
        let port = MockSerialPort::new();
        let test_data = vec![0x01, 0x02, 0x03];
        port.queue_rx_data(&test_data);

        let rx = port.rx_buffer.lock().unwrap();
        assert_eq!(rx.len(), 3);
    }

    #[test]
    fn test_queue_ack_frame() {
        let port = MockSerialPort::new();
        port.queue_frame_response(FrameType::Ack, None);

        let rx = port.rx_buffer.lock().unwrap();
        assert_eq!(*rx, vec![0xE5]);
    }

    #[test]
    fn test_queue_short_frame() {
        let port = MockSerialPort::new();
        port.queue_frame_response(
            FrameType::Short {
                control: 0x53,
                address: 0x01,
                checksum: 0x54,
            },
            None,
        );

        let rx = port.rx_buffer.lock().unwrap();
        assert_eq!(*rx, vec![0x10, 0x53, 0x01, 0x54, 0x16]);
    }

    #[test]
    fn test_queue_long_frame() {
        let port = MockSerialPort::new();
        port.queue_frame_response(
            FrameType::Long {
                control: 0x53,
                address: 0x01,
                ci: 0x72,
                data: Some(vec![0x01, 0x02, 0x03]),
            },
            None,
        );

        let rx = port.rx_buffer.lock().unwrap();
        // 0x68, len=6, len=6, 0x68, control, address, ci, data[3], checksum, 0x16
        assert_eq!(rx[0], 0x68);
        assert_eq!(rx[1], 0x06); // length = 3 + 3 data bytes
        assert_eq!(rx[2], 0x06);
        assert_eq!(rx[3], 0x68);
        assert_eq!(rx[rx.len() - 1], 0x16); // stop byte
    }

    #[test]
    fn test_clear_buffers() {
        let port = MockSerialPort::new();
        port.queue_rx_data(&[1, 2, 3]);
        port.clear();

        let rx = port.rx_buffer.lock().unwrap();
        assert_eq!(rx.len(), 0);
    }
}
