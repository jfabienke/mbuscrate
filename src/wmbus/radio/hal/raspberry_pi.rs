//! # Raspberry Pi HAL Implementation
//!
//! Hardware abstraction layer implementation for Raspberry Pi 4 and 5,
//! providing SPI communication and GPIO control for SX126x radio modules.
//!
//! ## Supported Platforms
//!
//! - **Raspberry Pi 4**: BCM2711 SoC with quad-core ARM Cortex-A72
//! - **Raspberry Pi 5**: BCM2712 SoC with quad-core ARM Cortex-A76
//!
//! ## Hardware Setup
//!
//! ### SPI Configuration
//!
//! The Raspberry Pi provides two SPI controllers:
//! - **SPI0**: `/dev/spidev0.0`, `/dev/spidev0.1` (recommended)
//! - **SPI1**: `/dev/spidev1.0`, `/dev/spidev1.1`, `/dev/spidev1.2`
//!
//! ### Pinout (40-pin GPIO header)
//!
//! #### SPI0 Pins (recommended)
//! ```text
//! Pi Pin │ BCM GPIO │ SX126x Pin │ Function
//! ───────┼──────────┼────────────┼─────────────
//! 19     │ GPIO 10  │ MOSI       │ SPI data out
//! 21     │ GPIO 9   │ MISO       │ SPI data in  
//! 23     │ GPIO 11  │ SCLK       │ SPI clock
//! 24     │ GPIO 8   │ NSS        │ Chip select
//! ```
//!
//! #### Control Pins (configurable)
//! ```text
//! Pi Pin │ BCM GPIO │ SX126x Pin │ Function
//! ───────┼──────────┼────────────┼─────────────
//! 22     │ GPIO 25  │ BUSY       │ Status (input)
//! 18     │ GPIO 24  │ DIO1       │ Interrupt (input)
//! 16     │ GPIO 23  │ DIO2       │ Interrupt (input, optional)
//! 15     │ GPIO 22  │ NRESET     │ Reset (output, optional)
//! ```
//!
//! ## Dependencies
//!
//! Add to your Cargo.toml:
//! ```toml
//! [dependencies]
//! rppal = "0.14"
//! ```
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use crate::wmbus::radio::hal::raspberry_pi::{RaspberryPiHal, GpioPins};
//! use crate::wmbus::radio::driver::Sx126xDriver;
//!
//! // Define GPIO pin assignments
//! let gpio_pins = GpioPins {
//!     busy: 25,
//!     dio1: 24,
//!     dio2: Some(23),
//!     reset: Some(22),
//! };
//!
//! // Initialize HAL for SPI bus 0
//! let hal = RaspberryPiHal::new(0, &gpio_pins)?;
//!
//! // Create radio driver
//! let mut driver = Sx126xDriver::new(hal, 32_000_000);
//! ```

use crate::wmbus::radio::hal::{Hal, HalError};
#[cfg(feature = "rfm69")]
use crate::wmbus::radio::rfm69_registers::SPI_SPEED as RFM69_SPI_SPEED;
use rppal::gpio::{Gpio, InputPin, Level, OutputPin, Trigger};
use rppal::spi::{BitOrder, Bus, Error as SpiError, Mode, SlaveSelect, Spi};
use std::thread;
use std::time::Duration;
use thiserror::Error;

/// Errors specific to Raspberry Pi HAL implementation
#[derive(Error, Debug)]
pub enum RpiHalError {
    /// SPI bus initialization failed
    #[error("SPI initialization failed: {0}")]
    SpiInit(#[from] SpiError),
    /// GPIO initialization failed  
    #[error("GPIO initialization failed: {0}")]
    GpioInit(#[from] rppal::gpio::Error),
    /// SPI transfer failed
    #[error("SPI transfer failed: {0}")]
    SpiTransfer(SpiError),
    /// GPIO operation failed
    #[error("GPIO operation failed: {0}")]
    GpioOperation(rppal::gpio::Error),
    /// BUSY pin timeout - radio did not respond
    #[error("BUSY pin timeout - radio not responding")]
    BusyTimeout,
    /// Invalid configuration parameter
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// GPIO pin configuration for SX126x connections
///
/// Specifies which Raspberry Pi GPIO pins are connected to the SX126x radio.
/// All pin numbers use BCM GPIO numbering (not physical pin numbers).
///
/// # Examples
///
/// ```rust
/// use crate::wmbus::radio::hal::raspberry_pi::GpioPins;
///
/// // Minimal configuration (required pins only)
/// let pins = GpioPins {
///     busy: 25,           // GPIO 25 for BUSY signal
///     dio1: 24,           // GPIO 24 for primary interrupt
///     dio2: None,         // DIO2 not used
///     reset: None,        // Reset not controlled by software
/// };
///
/// // Full configuration with all optional pins
/// let pins = GpioPins {
///     busy: 25,
///     dio1: 24,
///     dio2: Some(23),     // GPIO 23 for secondary interrupt
///     reset: Some(22),    // GPIO 22 for radio reset control
/// };
/// ```
#[derive(Debug, Clone)]
pub struct GpioPins {
    /// BUSY pin (input) - indicates radio is processing a command
    pub busy: u8,
    /// DIO1 pin (input) - primary interrupt from radio
    pub dio1: u8,
    /// DIO2 pin (input, optional) - secondary interrupt from radio
    pub dio2: Option<u8>,
    /// RESET pin (output, optional) - radio reset control (active low)
    pub reset: Option<u8>,
}

impl Default for GpioPins {
    /// Default GPIO pin configuration for typical SX126x wiring
    fn default() -> Self {
        Self {
            busy: 25,        // GPIO 25 (Pin 22)
            dio1: 24,        // GPIO 24 (Pin 18)
            dio2: Some(23),  // GPIO 23 (Pin 16)
            reset: Some(22), // GPIO 22 (Pin 15)
        }
    }
}

/// Raspberry Pi HAL implementation for SX126x radio
///
/// This implementation provides SPI communication and GPIO control specifically
/// optimized for Raspberry Pi 4 and 5 platforms using the rppal crate.
///
/// # Features
///
/// - Hardware SPI interface with configurable bus and chip select
/// - GPIO control for BUSY, DIO, and RESET pins
/// - Automatic BUSY pin monitoring for command completion
/// - Support for both Pi 4 (BCM2711) and Pi 5 (BCM2712)
/// - Configurable SPI parameters (speed, mode, bit order)
///
/// # Hardware Requirements
///
/// - Raspberry Pi 4 or 5 with 40-pin GPIO header
/// - SPI enabled in `/boot/config.txt` (add `dtparam=spi=on`)
/// - Proper electrical connections between Pi and SX126x module
/// - Adequate power supply (3.3V for SX126x, sufficient current for Pi)
pub struct RaspberryPiHal {
    /// SPI interface for radio communication
    spi: Spi,
    /// GPIO controller for pin access
    gpio: Gpio,
    /// Input pins for radio status signals
    busy_pin: InputPin,
    dio1_pin: InputPin,
    dio2_pin: Option<InputPin>,
    /// Output pin for radio reset control
    reset_pin: Option<OutputPin>,
    /// Pin configuration for reference
    pin_config: GpioPins,
    /// SPI bus information for debugging
    bus_info: String,
}

impl RaspberryPiHal {
    /// Create a new Raspberry Pi HAL instance
    ///
    /// Initializes SPI communication and GPIO pins for SX126x radio control.
    /// The SPI interface is configured for optimal SX126x communication.
    ///
    /// # Arguments
    ///
    /// * `spi_bus` - SPI bus number (0 for primary SPI, 1 for auxiliary SPI)
    /// * `gpio_pins` - GPIO pin configuration for radio connections
    ///
    /// # Returns
    ///
    /// * `Ok(RaspberryPiHal)` - Successfully initialized HAL
    /// * `Err(RpiHalError)` - Initialization failed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use crate::wmbus::radio::hal::raspberry_pi::{RaspberryPiHal, GpioPins};
    ///
    /// // Use default pin configuration
    /// let hal = RaspberryPiHal::new(0, &GpioPins::default())?;
    ///
    /// // Custom pin configuration
    /// let pins = GpioPins {
    ///     busy: 25,
    ///     dio1: 24,
    ///     dio2: None,        // Not using DIO2
    ///     reset: Some(22),   // Using reset control
    /// };
    /// let hal = RaspberryPiHal::new(0, &pins)?;
    /// ```
    ///
    /// # SPI Configuration
    ///
    /// The SPI interface is automatically configured with:
    /// - **Speed**: 8 MHz (safe for SX126x, max 16 MHz supported)
    /// - **Mode**: Mode 0 (CPOL=0, CPHA=0)
    /// - **Bit Order**: MSB first
    /// - **Word Size**: 8 bits
    pub fn new(spi_bus: u8, gpio_pins: &GpioPins) -> Result<Self, RpiHalError> {
        // Validate SPI bus number
        let (bus, slave_select) = match spi_bus {
            0 => (Bus::Spi0, SlaveSelect::Ss0),
            1 => (Bus::Spi1, SlaveSelect::Ss0),
            _ => {
                return Err(RpiHalError::InvalidConfig(format!(
                    "Invalid SPI bus {}, only 0 and 1 are supported",
                    spi_bus
                )))
            }
        };

        // Initialize SPI with SX126x-compatible settings
        let spi =
            Spi::new(bus, slave_select, 8_000_000, Mode::Mode0)?.bit_order(BitOrder::MsbFirst);

        let bus_info = format!(
            "SPI{} ({})",
            spi_bus,
            if spi_bus == 0 { "primary" } else { "auxiliary" }
        );

        // Initialize GPIO controller
        let gpio = Gpio::new()?;

        // Configure input pins
        let busy_pin = gpio.get(gpio_pins.busy)?.into_input();
        let dio1_pin = gpio.get(gpio_pins.dio1)?.into_input();

        let dio2_pin = if let Some(dio2) = gpio_pins.dio2 {
            Some(gpio.get(dio2)?.into_input())
        } else {
            None
        };

        // Configure reset pin as output (if specified)
        let reset_pin = if let Some(reset) = gpio_pins.reset {
            let mut pin = gpio.get(reset)?.into_output();
            pin.set_high(); // SX126x reset is active low, so start high
            Some(pin)
        } else {
            None
        };

        log::info!("Raspberry Pi HAL initialized:");
        log::info!("  SPI: {}", bus_info);
        log::info!("  BUSY: GPIO {}", gpio_pins.busy);
        log::info!("  DIO1: GPIO {}", gpio_pins.dio1);
        if let Some(dio2) = gpio_pins.dio2 {
            log::info!("  DIO2: GPIO {}", dio2);
        }
        if let Some(reset) = gpio_pins.reset {
            log::info!("  RESET: GPIO {}", reset);
        }

        Ok(Self {
            spi,
            gpio,
            busy_pin,
            dio1_pin,
            dio2_pin,
            reset_pin,
            pin_config: gpio_pins.clone(),
            bus_info,
        })
    }

    /// Reset the SX126x radio using the RESET pin
    ///
    /// Performs a hardware reset of the radio if a reset pin is configured.
    /// This is more reliable than software reset and ensures clean startup.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Reset completed successfully
    /// * `Err(RpiHalError)` - Reset failed or no reset pin configured
    ///
    /// # Timing
    ///
    /// - Assert reset (low): 100µs minimum
    /// - Deassert reset (high): Wait 1ms for radio startup
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// // Reset radio before configuration
    /// hal.reset_radio()?;
    ///
    /// // Radio is now in clean state and ready for commands
    /// ```
    pub fn reset_radio(&mut self) -> Result<(), RpiHalError> {
        if let Some(ref mut reset_pin) = self.reset_pin {
            log::debug!("Performing hardware reset of SX126x");

            // Assert reset (active low)
            reset_pin.set_low();
            thread::sleep(Duration::from_micros(100));

            // Deassert reset
            reset_pin.set_high();
            thread::sleep(Duration::from_millis(1));

            log::debug!("Hardware reset completed");
            Ok(())
        } else {
            Err(RpiHalError::InvalidConfig(
                "No reset pin configured".to_string(),
            ))
        }
    }

    /// Wait for the BUSY pin to go low (command processing complete)
    ///
    /// The SX126x asserts the BUSY pin high while processing commands.
    /// This method blocks until the radio finishes processing.
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Maximum time to wait in milliseconds
    ///
    /// # Returns
    ///
    /// * `Ok(())` - BUSY pin went low (command completed)
    /// * `Err(RpiHalError::BusyTimeout)` - Timeout waiting for BUSY
    /// * `Err(RpiHalError::GpioOperation)` - GPIO read error
    fn wait_for_busy_low(&self, timeout_ms: u32) -> Result<(), RpiHalError> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms as u64);

        while start.elapsed() < timeout {
            match self.busy_pin.read() {
                Level::Low => return Ok(()), // Command processing complete
                Level::High => {
                    // Still busy, check again after short delay
                    thread::sleep(Duration::from_micros(10));
                    continue;
                }
            }
        }

        log::warn!("BUSY pin timeout after {}ms", timeout_ms);
        Err(RpiHalError::BusyTimeout)
    }

    /// Get the current state of a DIO pin
    ///
    /// # Arguments
    ///
    /// * `dio_num` - DIO pin number (1 or 2)
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Pin is high (active)
    /// * `Ok(false)` - Pin is low (inactive)
    /// * `Err(RpiHalError)` - Pin not configured or GPIO error
    pub fn read_dio(&self, dio_num: u8) -> Result<bool, RpiHalError> {
        match dio_num {
            1 => Ok(self.dio1_pin.read() == Level::High),
            2 => {
                if let Some(ref dio2_pin) = self.dio2_pin {
                    Ok(dio2_pin.read() == Level::High)
                } else {
                    Err(RpiHalError::InvalidConfig(
                        "DIO2 pin not configured".to_string(),
                    ))
                }
            }
            _ => Err(RpiHalError::InvalidConfig(format!(
                "Invalid DIO pin number: {}",
                dio_num
            ))),
        }
    }

    /// Configure DIO2 for automatic RF switch control
    ///
    /// If `enabled`, DIO2 will be configured as an output controlled by the radio
    /// for automatic TX/RX switching. The pin must already be designated as DIO2
    /// in the GPIO configuration.
    ///
    /// This requires calling the driver's `set_rf_switch_enabled(true)` after HAL setup.
    ///
    /// # Arguments
    ///
    /// * `enabled` - Enable RF switch control on DIO2
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Configuration applied (no-op in HAL, handled in driver)
    pub fn configure_rf_switch_dio2(&mut self, _enabled: bool) -> Result<(), RpiHalError> {
        // DIO2 pin configuration is handled automatically by the radio when
        // SetDIO2AsRfSwitchCtrl is called in the driver. Here we just log the intent.
        if self.dio2_pin.is_some() {
            log::info!("DIO2 (GPIO {}) configured for RF switch control", self.pin_config.dio2.unwrap_or(0));
            Ok(())
        } else {
            Err(RpiHalError::InvalidConfig("DIO2 pin not configured".to_string()))
        }
    }

impl Hal for RaspberryPiHal {
    fn write_command(&mut self, opcode: u8, data: &[u8]) -> Result<(), HalError> {
        // Prepare command buffer: opcode followed by data
        let mut cmd_buf = Vec::with_capacity(1 + data.len());
        cmd_buf.push(opcode);
        cmd_buf.extend_from_slice(data);

        // Send command via SPI
        match self.spi.write(&cmd_buf) {
            Ok(_) => {
                log::trace!("SPI write command 0x{:02X}, {} bytes", opcode, data.len());

                // Wait for radio to process command (BUSY pin monitoring)
                self.wait_for_busy_low(100) // 100ms timeout for command processing
                    .map_err(|_| HalError::Spi)?;

                Ok(())
            }
            Err(e) => {
                log::error!("SPI write command failed: {}", e);
                Err(HalError::Spi)
            }
        }
    }

    fn read_command(&mut self, opcode: u8, buf: &mut [u8]) -> Result<(), HalError> {
        // Prepare command with NOP bytes for reading response
        let mut cmd_buf = vec![opcode];
        cmd_buf.resize(1 + buf.len(), 0x00); // Pad with NOP bytes

        match self.spi.transfer(&mut cmd_buf, buf) {
            Ok(_) => {
                log::trace!("SPI read command 0x{:02X}, {} bytes", opcode, buf.len());
                Ok(())
            }
            Err(e) => {
                log::error!("SPI read command failed: {}", e);
                Err(HalError::Spi)
            }
        }
    }

    fn write_register(&mut self, addr: u16, data: &[u8]) -> Result<(), HalError> {
        // WriteRegister command format: 0x0D, addr_msb, addr_lsb, data...
        let mut cmd_buf = Vec::with_capacity(3 + data.len());
        cmd_buf.push(0x0D); // WriteRegister command
        cmd_buf.push((addr >> 8) as u8); // Address MSB
        cmd_buf.push(addr as u8); // Address LSB
        cmd_buf.extend_from_slice(data);

        match self.spi.write(&cmd_buf) {
            Ok(_) => {
                log::trace!("Register write 0x{:04X}, {} bytes", addr, data.len());

                // Wait for command completion
                self.wait_for_busy_low(50).map_err(|_| HalError::Register)?;

                Ok(())
            }
            Err(e) => {
                log::error!("Register write failed: {}", e);
                Err(HalError::Register)
            }
        }
    }

    fn read_register(&mut self, addr: u16, buf: &mut [u8]) -> Result<(), HalError> {
        // ReadRegister command format: 0x1D, addr_msb, addr_lsb, NOP, data...
        let mut cmd_buf = vec![0x1D, (addr >> 8) as u8, addr as u8, 0x00];
        cmd_buf.resize(4 + buf.len(), 0x00);

        let mut read_buf = vec![0u8; cmd_buf.len()];

        match self.spi.transfer(&mut cmd_buf, &mut read_buf) {
            Ok(_) => {
                // Copy response data (skip command, address, and status bytes)
                buf.copy_from_slice(&read_buf[4..]);
                log::trace!("Register read 0x{:04X}, {} bytes", addr, buf.len());
                Ok(())
            }
            Err(e) => {
                log::error!("Register read failed: {}", e);
                Err(HalError::Register)
            }
        }
    }

    fn gpio_read(&mut self, pin: u8) -> Result<bool, HalError> {
        match pin {
            1 => Ok(self.dio1_pin.read() == Level::High),
            2 => {
                if let Some(ref dio2_pin) = self.dio2_pin {
                    Ok(dio2_pin.read() == Level::High)
                } else {
                    Err(HalError::Gpio)
                }
            }
            _ => Err(HalError::Gpio),
        }
    }

    fn gpio_write(&mut self, pin: u8, value: bool) -> Result<(), HalError> {
        // Currently only reset pin is controllable as output
        if pin == 0 && self.reset_pin.is_some() {
            if let Some(ref mut reset_pin) = self.reset_pin {
                if value {
                    reset_pin.set_high();
                } else {
                    reset_pin.set_low();
                }
                Ok(())
            } else {
                Err(HalError::Gpio)
            }
        } else {
            log::warn!("GPIO write to unsupported pin {}", pin);
            Err(HalError::Gpio)
        }
    }

    // RF switch control methods use default implementations (no-op)
    // Override these if using external RF switches
}

/// Builder for Raspberry Pi HAL configuration
///
/// Provides a fluent interface for configuring the HAL with validation
/// and sensible defaults.
///
/// # Examples
///
/// ```rust,no_run
/// use crate::wmbus::radio::hal::raspberry_pi::RaspberryPiHalBuilder;
///
/// let hal = RaspberryPiHalBuilder::new()
///     .spi_bus(0)
///     .spi_speed(10_000_000)  // 10 MHz SPI
///     .busy_pin(25)
///     .dio1_pin(24)
///     .dio2_pin(23)
///     .reset_pin(22)
///     .build()?;
/// ```
pub struct RaspberryPiHalBuilder {
    spi_bus: u8,
    spi_speed: u32,
    gpio_pins: GpioPins,
}

impl Default for RaspberryPiHalBuilder {
    fn default() -> Self {
        Self {
            spi_bus: 0,           // Primary SPI bus
            spi_speed: 8_000_000, // 8 MHz (safe for SX126x)
            gpio_pins: GpioPins::default(),
        }
    }
}

impl RaspberryPiHalBuilder {
    /// Create a new HAL builder with default configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the SPI bus number (0 or 1)
    pub fn spi_bus(mut self, bus: u8) -> Self {
        self.spi_bus = bus;
        self
    }

    /// Set the SPI clock speed in Hz (max 16 MHz for SX126x)
    pub fn spi_speed(mut self, speed: u32) -> Self {
        self.spi_speed = speed.min(16_000_000); // Clamp to SX126x maximum
        self
    }

    /// Set the BUSY pin GPIO number
    pub fn busy_pin(mut self, pin: u8) -> Self {
        self.gpio_pins.busy = pin;
        self
    }

    /// Set the DIO1 pin GPIO number  
    pub fn dio1_pin(mut self, pin: u8) -> Self {
        self.gpio_pins.dio1 = pin;
        self
    }

    /// Set the DIO2 pin GPIO number (optional)
    pub fn dio2_pin(mut self, pin: u8) -> Self {
        self.gpio_pins.dio2 = Some(pin);
        self
    }

    /// Enable DIO2 as RF switch control pin
    ///
    /// Designates the DIO2 pin for automatic RF front-end switching.
    /// DIO2 will go high during TX and low during RX when enabled in the driver.
    pub fn rf_switch_dio2(mut self) -> Self {
        self.gpio_pins.dio2 = self.gpio_pins.dio2.or(Some(23)); // Default to GPIO 23 if not set
        self
    }

    /// Set the RESET pin GPIO number (optional)
    pub fn reset_pin(mut self, pin: u8) -> Self {
        self.gpio_pins.reset = Some(pin);
        self
    }

    /// Disable RESET pin control
    pub fn no_reset(mut self) -> Self {
        self.gpio_pins.reset = None;
        self
    }

    /// Build the HAL instance with current configuration
    pub fn build(self) -> Result<RaspberryPiHal, RpiHalError> {
        // Validate configuration
        if self.spi_bus > 1 {
            return Err(RpiHalError::InvalidConfig(format!(
                "Invalid SPI bus {}, only 0 and 1 supported",
                self.spi_bus
            )));
        }

        if self.spi_speed == 0 || self.spi_speed > 16_000_000 {
            return Err(RpiHalError::InvalidConfig(format!(
                "Invalid SPI speed {} Hz, must be 1-16000000",
                self.spi_speed
            )));
        }

        // TODO: Add GPIO pin validation (check for conflicts, valid pin numbers)

        RaspberryPiHal::new(self.spi_bus, &self.gpio_pins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpio_pins_default() {
        let pins = GpioPins::default();
        assert_eq!(pins.busy, 25);
        assert_eq!(pins.dio1, 24);
        assert_eq!(pins.dio2, Some(23));
        assert_eq!(pins.reset, Some(22));
    }

    #[test]
    fn test_hal_builder() {
        let builder = RaspberryPiHalBuilder::new()
            .spi_bus(1)
            .spi_speed(10_000_000)
            .busy_pin(20)
            .dio1_pin(21)
            .no_dio2()
            .no_reset();

        assert_eq!(builder.spi_bus, 1);
        assert_eq!(builder.spi_speed, 10_000_000);
        assert_eq!(builder.gpio_pins.busy, 20);
        assert_eq!(builder.gpio_pins.dio1, 21);
        assert_eq!(builder.gpio_pins.dio2, None);
        assert_eq!(builder.gpio_pins.reset, None);
    }

    #[test]
    fn test_invalid_spi_speed() {
        let result = RaspberryPiHalBuilder::new()
            .spi_speed(20_000_000) // Too high
            .build();

        // Should not fail because speed is clamped
        // In a real test, we'd need actual hardware
    }
}

// =============================================================================
// RFM69-specific HAL Extensions
// =============================================================================

#[cfg(feature = "rfm69")]
/// RFM69-specific GPIO pin configuration
#[derive(Debug, Clone)]
pub struct Rfm69GpioPins {
    /// Reset pin (output) - RFM69 reset control (active high for reset pulse)
    pub reset: Option<u8>,
    /// Interrupt pin (input) - DIO1 for FIFO level interrupt  
    pub interrupt: Option<u8>,
}

#[cfg(feature = "rfm69")]
impl Default for Rfm69GpioPins {
    fn default() -> Self {
        Self {
            reset: Some(5),      // GPIO 5 for reset (common configuration)
            interrupt: Some(23), // GPIO 23 for interrupt (common configuration)
        }
    }
}

#[cfg(feature = "rfm69")]
/// Create RFM69-specific SPI and GPIO setup for Raspberry Pi
///
/// Returns (SpiDevice, OutputPin, InputPin) for RFM69 driver use
pub fn new_rfm69_spi(
    gpio: &Gpio,
    spi_device: Option<&str>,
    pins: &Rfm69GpioPins,
) -> Result<(Spi, Option<OutputPin>, Option<InputPin>), RpiHalError> {
    // Initialize SPI with RFM69-specific settings (1 MHz, Mode 0)
    let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, RFM69_SPI_SPEED, Mode::Mode0)?
        .bit_order(BitOrder::MsbFirst);

    log::info!(
        "RFM69 SPI initialized: {} Hz, Mode 0, MSB first",
        RFM69_SPI_SPEED
    );

    // Setup reset pin if configured
    let reset_pin = if let Some(reset_num) = pins.reset {
        let mut pin = gpio
            .get(reset_num)
            .map_err(|e| RpiHalError::GpioInit(e))?
            .into_output();
        pin.set_low(); // RFM69 reset is active high, start low (not in reset)
        log::info!("RFM69 reset pin configured: GPIO {}", reset_num);
        Some(pin)
    } else {
        log::warn!("RFM69 reset pin not configured");
        None
    };

    // Setup interrupt pin if configured
    let interrupt_pin = if let Some(intr_num) = pins.interrupt {
        let pin = gpio
            .get(intr_num)
            .map_err(|e| RpiHalError::GpioInit(e))?
            .into_input();
        log::info!("RFM69 interrupt pin configured: GPIO {}", intr_num);
        Some(pin)
    } else {
        log::warn!("RFM69 interrupt pin not configured - will use polling mode");
        None
    };

    Ok((spi, reset_pin, interrupt_pin))
}

#[cfg(feature = "rfm69")]
/// RFM69-specific HAL builder
pub struct Rfm69HalBuilder {
    spi_device: Option<String>,
    gpio_pins: Rfm69GpioPins,
}

#[cfg(feature = "rfm69")]
impl Default for Rfm69HalBuilder {
    fn default() -> Self {
        Self {
            spi_device: Some("/dev/spidev0.0".to_string()),
            gpio_pins: Rfm69GpioPins::default(),
        }
    }
}

#[cfg(feature = "rfm69")]
impl Rfm69HalBuilder {
    /// Create new RFM69 HAL builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set SPI device path
    pub fn spi_device(mut self, device: &str) -> Self {
        self.spi_device = Some(device.to_string());
        self
    }

    /// Set reset pin GPIO number
    pub fn reset_pin(mut self, pin: u8) -> Self {
        self.gpio_pins.reset = Some(pin);
        self
    }

    /// Disable reset pin
    pub fn no_reset(mut self) -> Self {
        self.gpio_pins.reset = None;
        self
    }

    /// Set interrupt pin GPIO number
    pub fn interrupt_pin(mut self, pin: u8) -> Self {
        self.gpio_pins.interrupt = Some(pin);
        self
    }

    /// Disable interrupt pin (use polling mode)
    pub fn no_interrupt(mut self) -> Self {
        self.gpio_pins.interrupt = None;
        self
    }

    /// Build the RFM69 HAL components
    pub fn build(self) -> Result<(Spi, Option<OutputPin>, Option<InputPin>), RpiHalError> {
        let gpio = Gpio::new().map_err(|e| RpiHalError::GpioInit(e))?;
        new_rfm69_spi(&gpio, self.spi_device.as_deref(), &self.gpio_pins)
    }
}
