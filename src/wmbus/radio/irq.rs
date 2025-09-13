//! # SX126x Interrupt Handling
//!
//! This module provides interrupt management for the SX126x radio, including interrupt
//! mask configuration and status processing. The SX126x uses a 16-bit interrupt register
//! where each bit represents a specific radio event.
//!
//! ## Interrupt Architecture
//!
//! The SX126x supports multiple interrupt sources that can be routed to different DIO pins:
//! - **DIO1**: Primary interrupt pin (RX/TX done, errors)
//! - **DIO2**: Secondary interrupt pin (optional)
//! - **DIO3**: Tertiary interrupt pin (optional)
//!
//! ## Usage Pattern
//!
//! 1. **Configure interrupt routing** using `SetDioIrqParams` command
//! 2. **Monitor interrupt pins** via GPIO or interrupt handlers
//! 3. **Read interrupt status** when interrupt occurs
//! 4. **Process events** based on active interrupt bits
//! 5. **Clear interrupts** to reset the status register
//!
//! ## Example
//!
//! ```rust,no_run
//! use crate::wmbus::radio::irq::{IrqMaskBit, IrqStatus};
//!
//! // Configure interrupt routing for RX/TX operations
//! let irq_mask = IrqMaskBit::RxDone as u16 | IrqMaskBit::TxDone as u16;
//! driver.set_dio_irq_params(irq_mask, IrqMaskBit::RxDone as u16, 0, 0)?;
//!
//! // Process interrupts in main loop
//! if gpio.read_pin(DIO1_PIN) {
//!     let status = driver.get_irq_status()?;
//!     if status.rx_done() {
//!         println!("Packet received!");
//!     }
//!     driver.clear_irq_status(0xFFFF)?; // Clear all interrupts
//! }
//! ```

/// SX126x interrupt bit definitions
///
/// Each bit in the SX126x interrupt register represents a specific radio event.
/// These can be combined using bitwise OR operations to create interrupt masks.
///
/// # Bit Assignments
///
/// The interrupt register follows this bit layout:
/// ```text
/// Bit 15-10: Reserved
/// Bit 9:  Timeout - Operation timed out
/// Bit 8:  CadDetected - Channel Activity Detection triggered  
/// Bit 7:  CadDone - Channel Activity Detection completed
/// Bit 6:  CrcErr - CRC validation failed
/// Bit 5:  HeaderError - Packet header validation failed
/// Bit 4:  HeaderValid - Valid packet header received
/// Bit 3:  SyncwordValid - Valid sync word detected
/// Bit 2:  PreambleDetected - Preamble pattern detected
/// Bit 1:  RxDone - Packet reception completed
/// Bit 0:  TxDone - Packet transmission completed
/// ```
#[repr(u16)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum IrqMaskBit {
    /// No interrupts enabled
    None = 0x0000,
    /// Transmission completed successfully
    TxDone = 1 << 0,
    /// Reception completed (packet received)
    RxDone = 1 << 1,
    /// Preamble pattern detected during reception
    PreambleDetected = 1 << 2,
    /// Valid sync word detected
    SyncwordValid = 1 << 3,
    /// Valid packet header received (for variable length packets)
    HeaderValid = 1 << 4,
    /// Packet header validation failed
    HeaderError = 1 << 5,
    /// CRC validation failed on received packet
    CrcErr = 1 << 6,
    /// Channel Activity Detection scan completed
    CadDone = 1 << 7,
    /// Channel activity detected during CAD scan
    CadDetected = 1 << 8,
    /// Operation timed out (RX/TX timeout)
    Timeout = 1 << 9,
    /// All interrupt sources enabled
    All = 0xFFFF,
}

/// Interrupt mask for configuring which events generate interrupts
///
/// This structure wraps a 16-bit mask value and provides methods for building
/// interrupt configurations by combining individual interrupt sources.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct IrqMask {
    /// Internal 16-bit mask value
    inner: u16,
}

impl IrqMask {
    /// Create an empty interrupt mask (no interrupts enabled)
    ///
    /// # Examples
    /// ```rust
    /// use crate::wmbus::radio::irq::IrqMask;
    ///
    /// let mask = IrqMask::none();
    /// assert_eq!(u16::from(mask), 0x0000);
    /// ```
    pub const fn none() -> Self {
        Self {
            inner: IrqMaskBit::None as u16,
        }
    }

    /// Create a mask with all interrupts enabled
    ///
    /// # Examples
    /// ```rust
    /// use crate::wmbus::radio::irq::IrqMask;
    ///
    /// let mask = IrqMask::all();
    /// assert_eq!(u16::from(mask), 0xFFFF);
    /// ```
    pub const fn all() -> Self {
        Self {
            inner: IrqMaskBit::All as u16,
        }
    }

    /// Add an interrupt source to the mask
    ///
    /// # Arguments
    /// * `bit` - Interrupt source to add
    ///
    /// # Examples
    /// ```rust
    /// use crate::wmbus::radio::irq::{IrqMask, IrqMaskBit};
    ///
    /// let mask = IrqMask::none()
    ///     .combine(IrqMaskBit::RxDone)
    ///     .combine(IrqMaskBit::TxDone);
    /// ```
    pub fn combine(self, bit: IrqMaskBit) -> Self {
        let inner = self.inner | bit as u16;
        Self { inner }
    }
}

impl From<IrqMask> for u16 {
    fn from(val: IrqMask) -> Self {
        val.inner
    }
}

impl From<u16> for IrqMask {
    fn from(mask: u16) -> Self {
        Self { inner: mask }
    }
}

impl Default for IrqMask {
    fn default() -> Self {
        Self::none()
    }
}

/// Interrupt status register value
///
/// This structure represents the current state of the SX126x interrupt register.
/// Each bit indicates whether a specific radio event has occurred. The status
/// is typically read after an interrupt occurs and then cleared.
///
/// # Usage
///
/// ```rust,no_run
/// // Read interrupt status from radio
/// let status = driver.get_irq_status()?;
///
/// // Check for specific events
/// if status.rx_done() {
///     println!("Packet received");
/// }
/// if status.crc_err() {
///     println!("CRC error in received packet");
/// }
///
/// // Clear all interrupts
/// driver.clear_irq_status(0xFFFF)?;
/// ```
#[derive(Copy, Clone, Debug, PartialEq)]
#[derive(Default)]
pub struct IrqStatus {
    /// Raw 16-bit interrupt status register value
    inner: u16,
}


impl From<u16> for IrqStatus {
    /// Create IrqStatus from raw 16-bit register value
    ///
    /// # Arguments
    /// * `status` - Raw interrupt status register value
    fn from(status: u16) -> Self {
        Self { inner: status }
    }
}

impl From<IrqStatus> for u16 {
    /// Extract raw 16-bit register value from IrqStatus
    fn from(status: IrqStatus) -> Self {
        status.inner
    }
}

impl IrqStatus {
    /// Check if transmission completed successfully
    ///
    /// # Returns
    /// * `true` - Packet transmission finished
    /// * `false` - No transmission completion event
    pub fn tx_done(self) -> bool {
        (self.inner & (IrqMaskBit::TxDone as u16)) != 0
    }

    /// Check if packet reception completed
    ///
    /// # Returns
    /// * `true` - Packet received and available in RX buffer
    /// * `false` - No reception completion event
    pub fn rx_done(self) -> bool {
        (self.inner & (IrqMaskBit::RxDone as u16)) != 0
    }

    /// Check if preamble pattern was detected
    ///
    /// This indicates the start of a potential packet reception.
    ///
    /// # Returns
    /// * `true` - Preamble detected during reception
    /// * `false` - No preamble detection event
    pub fn preamble_detected(self) -> bool {
        (self.inner & (IrqMaskBit::PreambleDetected as u16)) != 0
    }

    /// Check if sync word was successfully detected
    ///
    /// This occurs after preamble detection when the expected sync word
    /// pattern is found in the received data stream.
    ///
    /// # Returns
    /// * `true` - Valid sync word detected
    /// * `false` - No sync word validation event
    pub fn syncword_valid(self) -> bool {
        (self.inner & (IrqMaskBit::SyncwordValid as u16)) != 0
    }

    /// Check if packet header was successfully validated
    ///
    /// Only relevant for variable length packet configurations.
    ///
    /// # Returns
    /// * `true` - Packet header is valid
    /// * `false` - No header validation event
    pub fn header_valid(self) -> bool {
        (self.inner & (IrqMaskBit::HeaderValid as u16)) != 0
    }

    /// Check if packet header validation failed
    ///
    /// Indicates corruption in the packet header for variable length packets.
    ///
    /// # Returns
    /// * `true` - Header validation failed
    /// * `false` - No header error event
    pub fn header_error(self) -> bool {
        (self.inner & (IrqMaskBit::HeaderError as u16)) != 0
    }

    /// Check if CRC validation failed on received packet
    ///
    /// This indicates data corruption in the received packet payload.
    ///
    /// # Returns
    /// * `true` - CRC validation failed
    /// * `false` - No CRC error event
    pub fn crc_err(self) -> bool {
        (self.inner & (IrqMaskBit::CrcErr as u16)) != 0
    }

    /// Check if Channel Activity Detection scan completed
    ///
    /// CAD is used to detect ongoing transmissions before starting transmission.
    ///
    /// # Returns
    /// * `true` - CAD scan completed
    /// * `false` - No CAD completion event
    pub fn cad_done(self) -> bool {
        (self.inner & (IrqMaskBit::CadDone as u16)) != 0
    }

    /// Check if channel activity was detected during CAD scan
    ///
    /// Indicates another transmitter is active on the channel.
    ///
    /// # Returns
    /// * `true` - Channel activity detected
    /// * `false` - No channel activity detected
    pub fn cad_detected(self) -> bool {
        (self.inner & (IrqMaskBit::CadDetected as u16)) != 0
    }

    /// Check if operation timed out
    ///
    /// Can occur during RX timeout, TX timeout, or other timed operations.
    ///
    /// # Returns
    /// * `true` - Operation timed out
    /// * `false` - No timeout event
    pub fn timeout(self) -> bool {
        (self.inner & (IrqMaskBit::Timeout as u16)) != 0
    }

    /// Get the raw interrupt status value
    ///
    /// # Returns
    /// Raw 16-bit interrupt register value
    pub fn raw(self) -> u16 {
        self.inner
    }

    /// Check if any interrupt is active
    ///
    /// # Returns
    /// * `true` - At least one interrupt bit is set
    /// * `false` - No interrupts pending
    pub fn has_any(self) -> bool {
        self.inner != 0
    }
}
