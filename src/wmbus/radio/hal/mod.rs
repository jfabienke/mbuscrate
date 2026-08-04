//! # Hardware Abstraction Layer for Radio Hardware
//!
//! This module defines the HAL trait and provides platform-specific implementations
//! for radio hardware control, including enhanced GPIO operations and interrupt-driven
//! processing for optimal performance.

use thiserror::Error;

/// Errors that can occur during HAL operations
#[derive(Debug, Error)]
pub enum HalError {
    #[error("SPI communication error")]
    Spi,

    #[error("GPIO operation error")]
    Gpio,

    #[error("Register access error")]
    Register,

    #[error("Timeout waiting for operation")]
    Timeout,

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Hardware Abstraction Layer trait for SX126x radio control
pub trait Hal {
    /// Write a command with optional data to the radio
    fn write_command(&mut self, opcode: u8, data: &[u8]) -> Result<(), HalError>;

    /// Read a command response from the radio
    fn read_command(&mut self, opcode: u8, buf: &mut [u8]) -> Result<(), HalError>;

    /// Write data to a radio register
    fn write_register(&mut self, addr: u16, data: &[u8]) -> Result<(), HalError>;

    /// Read data from a radio register
    fn read_register(&mut self, addr: u16, buf: &mut [u8]) -> Result<(), HalError>;

    /// Read the state of a GPIO pin
    fn gpio_read(&mut self, pin: u8) -> Result<bool, HalError>;

    /// Write to a GPIO pin (if supported)
    fn gpio_write(&mut self, pin: u8, value: bool) -> Result<(), HalError>;
}

/// A no-op [`Hal`] implementation for tests, examples, and documentation.
///
/// Every write succeeds and is discarded; every read succeeds and yields zeroes.
/// This lets driver logic be exercised without radio hardware attached. It models
/// no device behaviour, so it cannot be used to assert on radio state — use it to
/// check that call sequences type-check and run, not that they do the right thing.
///
/// # Examples
///
/// ```rust
/// use mbus_rs::wmbus::radio::hal::MockHal;
/// use mbus_rs::wmbus::radio::driver::Sx126xDriver;
///
/// let driver = Sx126xDriver::new(MockHal::new(), 32_000_000);
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct MockHal;

impl MockHal {
    /// Create a new mock HAL.
    pub fn new() -> Self {
        Self
    }
}

impl Hal for MockHal {
    fn write_command(&mut self, _opcode: u8, _data: &[u8]) -> Result<(), HalError> {
        Ok(())
    }

    fn read_command(&mut self, _opcode: u8, buf: &mut [u8]) -> Result<(), HalError> {
        buf.fill(0);
        Ok(())
    }

    fn write_register(&mut self, _addr: u16, _data: &[u8]) -> Result<(), HalError> {
        Ok(())
    }

    fn read_register(&mut self, _addr: u16, buf: &mut [u8]) -> Result<(), HalError> {
        buf.fill(0);
        Ok(())
    }

    fn gpio_read(&mut self, _pin: u8) -> Result<bool, HalError> {
        Ok(false)
    }

    fn gpio_write(&mut self, _pin: u8, _value: bool) -> Result<(), HalError> {
        Ok(())
    }
}

// Enhanced GPIO abstraction
pub mod enhanced_gpio;

// Platform implementations
#[cfg(feature = "raspberry-pi")]
pub mod raspberry_pi;

// Re-export enhanced GPIO types
pub use enhanced_gpio::{
    EdgeType, EnhancedGpio, EnhancedGpioError, GpioConfig, GpioEvent, GpioEventType, GpioStats,
};

// Re-export platform implementations for convenience
#[cfg(feature = "raspberry-pi")]
pub use raspberry_pi::{GpioPins, RaspberryPiHal, RaspberryPiHalBuilder};

/// A test HAL that records every command/register write in order and answers `GetStatus`
/// (0xC0) with a chip mode derived from the last mode command, so state transitions
/// (`set_standby`/`set_rx` → `wait_for_state`) resolve without real hardware.
///
/// Shared by the driver and scheduler tests. The recording lives behind an `Arc<Mutex<_>>`
/// so a test can clone a probe handle *before* moving the HAL into the driver and inspect
/// the command stream afterwards. `fail_on` injects a one-shot fault to exercise error paths.
#[cfg(test)]
#[derive(Clone, Default)]
pub struct RecordingHal {
    inner: std::sync::Arc<std::sync::Mutex<Recording>>,
}

#[cfg(test)]
#[derive(Default)]
struct Recording {
    commands: Vec<(u8, Vec<u8>)>,
    reg_writes: Vec<(u16, Vec<u8>)>,
    /// Chip-mode nibble reported in `GetStatus` bits [6:4]; 0 = Sleep at start.
    mode_bits: u8,
    /// One-shot fault: the first `write_command` matching `(opcode, data)` fails, then clears.
    fail_on: Option<(u8, Vec<u8>)>,
    /// Persistent fault: every `write_command` with this opcode fails (any data).
    fail_every: Option<u8>,
    /// A payload queued to be delivered on the next `process_irqs` poll: it makes GetIrqStatus
    /// report RxDone, GetRxBufferStatus report the length, and ReadBuffer return the bytes
    /// (consumed once read). Lets an end-to-end test drive real reception with no hardware.
    pending_rx: Option<Vec<u8>>,
    /// Raw 3 bytes returned by GetPacketStatus (0x14): [RssiPkt, SnrPkt, SignalRssiPkt].
    packet_status: [u8; 3],
}

#[cfg(test)]
impl RecordingHal {
    /// A recording HAL with no injected faults.
    pub fn new() -> Self {
        Self::default()
    }

    /// A recording HAL that fails the first `write_command(opcode, data)` it sees (once),
    /// to exercise error/restore paths.
    pub fn fail_on(opcode: u8, data: &[u8]) -> Self {
        let hal = Self::new();
        hal.inner.lock().unwrap().fail_on = Some((opcode, data.to_vec()));
        hal
    }

    /// A recording HAL that fails *every* `write_command` with `opcode` (any data), to
    /// exercise the case where recovery itself cannot complete.
    pub fn fail_every(opcode: u8) -> Self {
        let hal = Self::new();
        hal.inner.lock().unwrap().fail_every = Some(opcode);
        hal
    }

    /// Queue `payload` to be delivered on the next `process_irqs` poll (one packet).
    pub fn queue_rx(&self, payload: Vec<u8>) {
        self.inner.lock().unwrap().pending_rx = Some(payload);
    }

    /// Set the raw bytes GetPacketStatus (0x14) returns: `[RssiPkt, SnrPkt, SignalRssiPkt]`.
    pub fn set_packet_status(&self, bytes: [u8; 3]) {
        self.inner.lock().unwrap().packet_status = bytes;
    }

    /// Snapshot of the recorded `(opcode, data)` command stream, in order.
    pub fn commands(&self) -> Vec<(u8, Vec<u8>)> {
        self.inner.lock().unwrap().commands.clone()
    }

    /// Index of the first recorded write of `opcode`, if any.
    pub fn first_cmd(&self, opcode: u8) -> Option<usize> {
        self.inner
            .lock()
            .unwrap()
            .commands
            .iter()
            .position(|(op, _)| *op == opcode)
    }

    /// True if `(opcode, data)` was recorded exactly.
    pub fn has_cmd(&self, opcode: u8, data: &[u8]) -> bool {
        self.inner
            .lock()
            .unwrap()
            .commands
            .iter()
            .any(|(op, d)| *op == opcode && d.as_slice() == data)
    }
}

#[cfg(test)]
impl Hal for RecordingHal {
    fn write_command(&mut self, opcode: u8, data: &[u8]) -> Result<(), HalError> {
        let mut g = self.inner.lock().unwrap();
        if g.fail_every == Some(opcode) {
            return Err(HalError::Spi); // persistent
        }
        if g.fail_on
            .as_ref()
            .is_some_and(|(op, d)| *op == opcode && d.as_slice() == data)
        {
            g.fail_on = None; // one-shot
            return Err(HalError::Spi);
        }
        match opcode {
            0x80 => g.mode_bits = if data.first() == Some(&0x01) { 3 } else { 2 }, // SetStandby XOSC/RC
            0x82 => g.mode_bits = 5,                                               // SetRx
            0x83 => g.mode_bits = 6,                                               // SetTx
            0x84 => g.mode_bits = 0,                                               // SetSleep
            0xC1 => g.mode_bits = 4,                                               // SetFs
            _ => {}
        }
        g.commands.push((opcode, data.to_vec()));
        Ok(())
    }

    fn read_command(&mut self, opcode: u8, buf: &mut [u8]) -> Result<(), HalError> {
        buf.fill(0);
        let mut g = self.inner.lock().unwrap();
        match opcode {
            // GetStatus: chip mode in bits [6:4].
            0xC0 => {
                if !buf.is_empty() {
                    buf[0] = g.mode_bits << 4;
                }
            }
            // GetIrqStatus (u16, big-endian): report RxDone (bit 1) when a packet is queued.
            0x12 => {
                if g.pending_rx.is_some() && buf.len() >= 2 {
                    buf[1] = 0x02;
                }
            }
            // GetRxBufferStatus: byte 0 is the payload length.
            0x13 => {
                if let (Some(p), false) = (g.pending_rx.as_ref(), buf.is_empty()) {
                    buf[0] = p.len() as u8;
                }
            }
            // ReadBuffer: deliver (and consume) the queued payload.
            0x1E => {
                if let Some(p) = g.pending_rx.take() {
                    let n = buf.len().min(p.len());
                    buf[..n].copy_from_slice(&p[..n]);
                }
            }
            // GetPacketStatus: [RssiPkt, SnrPkt, SignalRssiPkt].
            0x14 => {
                let n = buf.len().min(3);
                buf[..n].copy_from_slice(&g.packet_status[..n]);
            }
            _ => {}
        }
        Ok(())
    }

    fn write_register(&mut self, addr: u16, data: &[u8]) -> Result<(), HalError> {
        self.inner
            .lock()
            .unwrap()
            .reg_writes
            .push((addr, data.to_vec()));
        Ok(())
    }

    fn read_register(&mut self, _addr: u16, buf: &mut [u8]) -> Result<(), HalError> {
        buf.fill(0);
        Ok(())
    }

    fn gpio_read(&mut self, _pin: u8) -> Result<bool, HalError> {
        Ok(false)
    }

    fn gpio_write(&mut self, _pin: u8, _value: bool) -> Result<(), HalError> {
        Ok(())
    }
}
