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
