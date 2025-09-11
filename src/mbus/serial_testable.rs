//! Testable serial implementation with dependency injection
//!
//! This module provides a testable version of the M-Bus serial communication
//! that can work with either real serial ports or mock implementations.

use crate::error::MBusError;
use crate::mbus::frame::{pack_frame, parse_frame, MBusFrame};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Trait for serial port operations
#[async_trait::async_trait]
pub trait SerialPort: AsyncReadExt + AsyncWriteExt + Unpin + Send {
    async fn flush(&mut self) -> Result<(), std::io::Error>;
}

// Implement SerialPort for tokio_serial::SerialStream
#[async_trait::async_trait]
impl SerialPort for tokio_serial::SerialStream {
    async fn flush(&mut self) -> Result<(), std::io::Error> {
        AsyncWriteExt::flush(self).await
    }
}

// Implement SerialPort for our MockSerialPort
#[cfg(test)]
#[async_trait::async_trait]
impl SerialPort for crate::mbus::serial_mock::MockSerialPort {
    async fn flush(&mut self) -> Result<(), std::io::Error> {
        Ok(())
    }
}

/// Generic M-Bus device handle that works with any SerialPort implementation
pub struct TestableDeviceHandle<P: SerialPort> {
    port: P,
    baudrate: u32,
    #[allow(dead_code)]
    timeout: Duration,
}

impl<P: SerialPort> TestableDeviceHandle<P> {
    /// Create a new handle with the given port
    pub fn new(port: P, baudrate: u32, timeout: Duration) -> Self {
        TestableDeviceHandle {
            port,
            baudrate,
            timeout,
        }
    }

    /// Send an M-Bus frame
    pub async fn send_frame(&mut self, frame: &MBusFrame) -> Result<(), MBusError> {
        let data = pack_frame(frame);
        self.port
            .write_all(&data)
            .await
            .map_err(|e| MBusError::SerialPortError(e.to_string()))?;
        SerialPort::flush(&mut self.port)
            .await
            .map_err(|e| MBusError::SerialPortError(e.to_string()))
    }

    /// Receive an M-Bus frame
    pub async fn recv_frame(&mut self) -> Result<MBusFrame, MBusError> {
        use tokio::time::timeout;

        // Map baudrate to timeout
        let to = match self.baudrate {
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

        // Read first byte
        let mut start = [0u8; 1];
        let n = timeout(to, self.port.read(&mut start))
            .await
            .map_err(|_| MBusError::NomError("timeout".into()))
            .and_then(|res| res.map_err(|e| MBusError::SerialPortError(e.to_string())))?;

        if n == 0 {
            return Err(MBusError::NomError("empty".into()));
        }

        let (total_len, mut buf) = match start[0] {
            0xE5 => (1usize, vec![0xE5]), // ACK
            0x10 => (5usize, vec![0x10]), // SHORT
            0x68 => {
                // Read length bytes
                let mut lenbuf = [0u8; 3]; // len1, len2, second 0x68
                timeout(to, self.port.read_exact(&mut lenbuf))
                    .await
                    .map_err(|_| MBusError::NomError("timeout".into()))
                    .and_then(|res| res.map_err(|e| MBusError::SerialPortError(e.to_string())))?;

                if lenbuf[0] != lenbuf[1] || lenbuf[2] != 0x68 {
                    return Err(MBusError::FrameParseError(
                        "Invalid long frame header".into(),
                    ));
                }

                let length1 = lenbuf[0] as usize;
                let total = 6 + length1; // 0x68 len len 0x68 [data] checksum 0x16

                // Start building the frame buffer
                let frame_buf = vec![0x68, lenbuf[0], lenbuf[1], lenbuf[2]];
                (total, frame_buf)
            }
            _ => return Err(MBusError::FrameParseError("Invalid frame start".into())),
        };

        // Read remaining bytes based on frame type
        if start[0] == 0x68 {
            // For long frames, read the remaining data (control, address, CI, data, checksum, stop)
            let remaining = total_len - buf.len();
            let mut rest = vec![0u8; remaining];
            timeout(to, self.port.read_exact(&mut rest))
                .await
                .map_err(|_| MBusError::NomError("timeout".into()))
                .and_then(|res| res.map_err(|e| MBusError::SerialPortError(e.to_string())))?;
            buf.extend_from_slice(&rest);
        } else if total_len > 1 {
            let mut rest = vec![0u8; total_len - 1];
            timeout(to, self.port.read_exact(&mut rest))
                .await
                .map_err(|_| MBusError::NomError("timeout".into()))
                .and_then(|res| res.map_err(|e| MBusError::SerialPortError(e.to_string())))?;
            buf.extend_from_slice(&rest);
        }

        let (_, frame) =
            parse_frame(&buf[..]).map_err(|e| MBusError::FrameParseError(format!("{e:?}")))?;
        Ok(frame)
    }

    /// Send a request and wait for response
    pub async fn request_response(&mut self, request: &MBusFrame) -> Result<MBusFrame, MBusError> {
        self.send_frame(request).await?;
        self.recv_frame().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mbus::frame::MBusFrameType;
    use crate::mbus::serial_mock::{FrameType, MockSerialPort};

    #[tokio::test]
    async fn test_send_frame_ack() {
        let mock = MockSerialPort::new();
        let mut handle = TestableDeviceHandle::new(mock.clone(), 2400, Duration::from_secs(1));

        let frame = MBusFrame {
            frame_type: MBusFrameType::Ack,
            control: 0,
            address: 0,
            control_information: 0,
            data: vec![],
            checksum: 0,
            more_records_follow: false,
        };

        let result = handle.send_frame(&frame).await;
        assert!(result.is_ok());

        let tx_data = mock.get_tx_data();
        assert_eq!(tx_data, vec![0xE5]);
    }

    #[tokio::test]
    async fn test_send_frame_short() {
        let mock = MockSerialPort::new();
        let mut handle = TestableDeviceHandle::new(mock.clone(), 2400, Duration::from_secs(1));

        let frame = MBusFrame {
            frame_type: MBusFrameType::Short,
            control: 0x53,
            address: 0x01,
            control_information: 0,
            data: vec![],
            checksum: 0x54,
            more_records_follow: false,
        };

        let result = handle.send_frame(&frame).await;
        assert!(result.is_ok());

        let tx_data = mock.get_tx_data();
        assert_eq!(tx_data, vec![0x10, 0x53, 0x01, 0x54, 0x16]);
    }

    #[tokio::test]
    async fn test_recv_frame_ack() {
        let mock = MockSerialPort::new();
        mock.queue_frame_response(FrameType::Ack, None);

        let mut handle = TestableDeviceHandle::new(mock.clone(), 2400, Duration::from_secs(1));

        let frame = handle.recv_frame().await.unwrap();
        assert_eq!(frame.frame_type, MBusFrameType::Ack);
    }

    #[tokio::test]
    async fn test_recv_frame_short() {
        let mock = MockSerialPort::new();
        mock.queue_frame_response(
            FrameType::Short {
                control: 0x53,
                address: 0x01,
                checksum: 0x54,
            },
            None,
        );

        let mut handle = TestableDeviceHandle::new(mock.clone(), 2400, Duration::from_secs(1));

        let frame = handle.recv_frame().await.unwrap();
        assert_eq!(frame.frame_type, MBusFrameType::Short);
        assert_eq!(frame.control, 0x53);
        assert_eq!(frame.address, 0x01);
    }

    #[tokio::test]
    async fn test_recv_frame_long() {
        let mock = MockSerialPort::new();
        mock.queue_frame_response(
            FrameType::Long {
                control: 0x08,
                address: 0x01,
                ci: 0x72,
                data: Some(vec![0x01, 0x02, 0x03]),
            },
            None,
        );

        // Debug: Check what was actually queued
        let queued_data = {
            let rx = mock.rx_buffer.lock().unwrap();
            rx.iter().cloned().collect::<Vec<u8>>()
        };
        eprintln!("Queued data: {:?} (len={})", queued_data, queued_data.len());

        let mut handle = TestableDeviceHandle::new(mock.clone(), 2400, Duration::from_secs(1));

        let frame = handle.recv_frame().await.unwrap();
        assert_eq!(frame.frame_type, MBusFrameType::Long);
        assert_eq!(frame.data.len(), 3);
        assert_eq!(frame.data, vec![0x01, 0x02, 0x03]);
    }

    #[tokio::test]
    async fn test_recv_frame_timeout() {
        let mock = MockSerialPort::new();
        // Don't queue any data

        let mut handle = TestableDeviceHandle::new(mock.clone(), 38400, Duration::from_millis(10));

        let result = handle.recv_frame().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_recv_frame_invalid_start() {
        let mock = MockSerialPort::new();
        mock.queue_rx_data(&[0xFF, 0xFF, 0xFF]);

        let mut handle = TestableDeviceHandle::new(mock.clone(), 2400, Duration::from_secs(1));

        let result = handle.recv_frame().await;
        assert!(result.is_err());
        assert!(matches!(result, Err(MBusError::FrameParseError(_))));
    }

    #[tokio::test]
    async fn test_request_response() {
        let mock = MockSerialPort::new();

        // Queue an ACK response
        mock.queue_frame_response(FrameType::Ack, None);

        let mut handle = TestableDeviceHandle::new(mock.clone(), 2400, Duration::from_secs(1));

        let request = MBusFrame {
            frame_type: MBusFrameType::Short,
            control: 0x53,
            address: 0x01,
            control_information: 0,
            data: vec![],
            checksum: 0x54,
            more_records_follow: false,
        };

        let response = handle.request_response(&request).await.unwrap();
        assert_eq!(response.frame_type, MBusFrameType::Ack);

        // Verify request was sent
        let tx_data = mock.get_tx_data();
        assert_eq!(tx_data[0], 0x10); // Short frame start
    }

    #[tokio::test]
    async fn test_baudrate_timeout_mapping() {
        let baudrates = vec![300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 115200];

        for baudrate in baudrates {
            let mock = MockSerialPort::new();
            mock.queue_frame_response(FrameType::Ack, None);

            let mut handle =
                TestableDeviceHandle::new(mock.clone(), baudrate, Duration::from_secs(5));

            // This should succeed with proper timeout for each baudrate
            let result = handle.recv_frame().await;
            assert!(result.is_ok(), "Failed for baudrate {baudrate}");
        }
    }

    #[tokio::test]
    async fn test_error_propagation() {
        let mock = MockSerialPort::new();
        mock.set_next_error(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "Test error",
        ));

        let mut handle = TestableDeviceHandle::new(mock.clone(), 2400, Duration::from_secs(1));

        let frame = MBusFrame {
            frame_type: MBusFrameType::Ack,
            control: 0,
            address: 0,
            control_information: 0,
            data: vec![],
            checksum: 0,
            more_records_follow: false,
        };

        let result = handle.send_frame(&frame).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(MBusError::SerialPortError(_))));
    }
}
