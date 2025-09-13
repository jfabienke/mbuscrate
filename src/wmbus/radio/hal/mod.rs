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
