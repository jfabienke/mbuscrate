//! # Byte transport seam for wired M-Bus
//!
//! `ByteTransport` is the single seam between the M-Bus session logic and the wire: it moves
//! bytes and nothing else (the one acknowledged exception, [`ByteTransport::set_baud_rate`], is a
//! default-`Unsupported` method on the same trait rather than a separate downcast). Framing,
//! buffering, timeouts, retries, and protocol state all live above this layer.
//!
//! Step 1 of the transport refactor (see `docs/design/wired-transport-refactor.md`) introduces the
//! trait plus the production [`SerialTransport`] and reroutes [`MBusDeviceHandle`] through it,
//! byte-for-byte identically. Later steps move the framing/length logic into the codec and add the
//! scripted `VirtualBus` for deterministic tests.
//!
//! [`MBusDeviceHandle`]: crate::mbus::serial::MBusDeviceHandle

use super::serial::MBusBaudRate;
use crate::error::MBusError;
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

/// Errors raised by a [`ByteTransport`]. These are transport-level only — framing, checksum, and
/// timeout classification belong to the session/codec above.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// Underlying I/O failure.
    #[error("transport I/O: {0}")]
    Io(#[from] std::io::Error),
    /// The peer closed the stream (read returned 0 before the expected bytes arrived).
    #[error("transport closed")]
    Closed,
    /// The operation is not supported by this transport (e.g. `set_baud_rate` on a non-serial
    /// transport such as the in-memory/virtual bus).
    #[error("operation not supported by this transport")]
    Unsupported,
}

/// Moves bytes between the M-Bus session and the wire. Implementations MUST NOT impose their own
/// read timeout — the session owns timing. `read` should be cancellation-safe: if the returned
/// future is dropped before completion, no bytes may be consumed-and-lost.
#[async_trait]
pub trait ByteTransport: Send {
    /// Read up to `buf.len()` bytes. `Ok(0)` means end-of-stream (the port/peer closed).
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError>;

    /// Write the whole buffer.
    async fn write_all(&mut self, buf: &[u8]) -> Result<(), TransportError>;

    /// Flush any buffered output.
    async fn flush(&mut self) -> Result<(), TransportError>;

    /// Reconfigure the line baud rate. The one acknowledged serial-specific poke on this seam;
    /// non-serial transports inherit the default `Unsupported`.
    fn set_baud_rate(&mut self, _baud: MBusBaudRate) -> Result<(), TransportError> {
        Err(TransportError::Unsupported)
    }
}

/// Fill `buf` completely by looping [`ByteTransport::read`], erroring on an early close. Mirrors
/// `tokio::io::AsyncReadExt::read_exact` semantics; it does **not** do any framing/length logic —
/// that stays in the codec.
pub async fn fill_exact(t: &mut dyn ByteTransport, buf: &mut [u8]) -> Result<(), TransportError> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = t.read(&mut buf[filled..]).await?;
        if n == 0 {
            return Err(TransportError::Closed);
        }
        filled += n;
    }
    Ok(())
}

/// Production [`ByteTransport`] over a `serial2_tokio::SerialPort`.
pub struct SerialTransport {
    port: serial2_tokio::SerialPort,
    port_name: String,
}

impl SerialTransport {
    /// Open the named port at `baud` with the M-Bus line settings (8E1, per-baud timeout).
    pub fn open(port_name: &str, baud: MBusBaudRate) -> Result<Self, MBusError> {
        let port = build_port(port_name, baud)?;
        Ok(Self {
            port,
            port_name: port_name.to_string(),
        })
    }
}

fn build_port(
    port_name: &str,
    baud: MBusBaudRate,
) -> Result<serial2_tokio::SerialPort, MBusError> {
    // M-Bus line settings: 8 data bits, even parity, 1 stop bit (8E1), no flow control.
    // serial2 configures via a `Settings` closure (vs tokio-serial's builder). The read
    // timeout tokio-serial set here was a no-op on an async port — read deadlines are
    // enforced above this layer (`tokio::time::timeout`), so it isn't reproduced.
    serial2_tokio::SerialPort::open(port_name, |mut settings: serial2::Settings| {
        settings.set_raw();
        settings.set_baud_rate(baud.as_u32())?;
        settings.set_char_size(serial2::CharSize::Bits8);
        settings.set_stop_bits(serial2::StopBits::One);
        settings.set_parity(serial2::Parity::Even);
        settings.set_flow_control(serial2::FlowControl::None);
        Ok(settings)
    })
    .map_err(|e| MBusError::SerialPortError(e.to_string()))
}

#[async_trait]
impl ByteTransport for SerialTransport {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        Ok(self.port.read(buf).await?)
    }

    async fn write_all(&mut self, buf: &[u8]) -> Result<(), TransportError> {
        self.port.write_all(buf).await?;
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), TransportError> {
        self.port.flush().await?;
        Ok(())
    }

    fn set_baud_rate(&mut self, baud: MBusBaudRate) -> Result<(), TransportError> {
        // Rebuilding the port at the new baud closes the old one — the same behavior the handle's
        // former inline `switch_baud_rate` had.
        let port = build_port(&self.port_name, baud)
            .map_err(|e| TransportError::Io(std::io::Error::other(e.to_string())))?;
        self.port = port;
        Ok(())
    }
}
