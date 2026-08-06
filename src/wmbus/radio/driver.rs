//! # SX126x Radio Driver
//!
//! This module provides a high-level driver for the Semtech SX126x family of sub-GHz
//! radio transceivers (including SX1261, SX1262, SX1268). The driver is specifically
//! optimized for wireless M-Bus (wM-Bus) applications but can be used for other
//! sub-GHz protocols.
//!
//! ## Features
//!
//! - Full SX126x command set implementation
//! - GFSK modulation support with configurable parameters
//! - Hardware abstraction layer for different platforms
//! - Interrupt-driven operation
//! - wM-Bus specific configuration profiles
//! - Buffer management for TX/RX operations
//! - Power amplifier control
//! - CRC and sync word configuration
//!
//! ## Architecture
//!
//! The driver follows a layered architecture:
//! ```text
//! ┌─────────────────────────────────┐
//! │        Application Layer        │
//! ├─────────────────────────────────┤
//! │     Sx126xDriver (this file)    │
//! ├─────────────────────────────────┤
//! │      HAL Abstraction Layer      │
//! ├─────────────────────────────────┤
//! │    Platform-specific HAL impl   │
//! └─────────────────────────────────┘
//! ```
//!
//! ## Usage Example
//!
//! ```rust
//! use mbus_rs::wmbus::radio::driver::Sx126xDriver;
//! # use mbus_rs::wmbus::radio::driver::DriverError;
//! // Substitute your own `Hal` implementation for `MockHal` on real hardware.
//! use mbus_rs::wmbus::radio::hal::MockHal;
//!
//! # fn main() -> Result<(), DriverError> {
//! // Initialize with your HAL implementation
//! let hal = MockHal::new();
//! let mut driver = Sx126xDriver::new(hal, 32_000_000); // 32MHz crystal
//!
//! // Configure for wM-Bus operation
//! driver.configure_for_wmbus(868_950_000, 100_000)?; // 868.95 MHz, 100 kbps
//!
//! // Start receiving
//! driver.set_rx_continuous()?;
//!
//! // Poll for received data
//! if let Some(payload) = driver.process_irqs()? {
//!     println!("Received: {:?}", payload);
//! }
//! # Ok(())
//! # }
//! ```

use crate::wmbus::radio::hal::{Hal, HalError};
use crate::wmbus::radio::irq::{IrqMaskBit, IrqStatus};
use crate::wmbus::radio::modulation::{
    CodingRate, CrcType, GfskModParams, HeaderType, LoRaBandwidth, LoRaModParams, LoRaPacketParams,
    ModulationParams, PacketParams, PacketType, SpreadingFactor,
};
use log;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Convert the SX126x LoRa modem frequency-error registers (0x076B..0x076D — a signed
/// 20-bit value, big-endian, only the low nibble of the first byte is significant) into Hz
/// for the given LoRa bandwidth in kHz.
///
/// The scaling follows the widely-used RadioLib SX126x implementation and assumes the 32 MHz
/// reference (matching `Sx126xDriver`'s default `xtal_freq`). The sign handling and the
/// linear scaling with bandwidth are correct by construction; the absolute Hz constant
/// (1.55/1600) still warrants an on-hardware check against a known offset.
fn lora_freq_error_hz(regs: [u8; 3], bw_khz: f64) -> i32 {
    // Assemble the 20-bit value (bit 19 is the sign).
    let raw = (((regs[0] & 0x0F) as u32) << 16) | ((regs[1] as u32) << 8) | regs[2] as u32;
    let efe = if raw & 0x0008_0000 != 0 {
        (raw | 0xFFF0_0000) as i32 // sign-extend negative
    } else {
        raw as i32
    };
    (1.55 * f64::from(efe) / (1600.0 / bw_khz)).round() as i32
}

/// Convert the SX126x LoRa `SnrPkt` byte from GetPacketStatus (0x14) into dB.
///
/// `SnrPkt` is a *signed* two's-complement byte in quarter-dB units, so the SNR is
/// `(byte as i8) / 4`. (Earlier code treated this byte as an RSSI — `-(byte)/2` — which is
/// wrong: it flips the sign and mis-scales, so a real SNR of +5 dB read as roughly -10 dBm.)
fn lora_snr_db(snr_byte: u8) -> f32 {
    (snr_byte as i8) as f32 / 4.0
}

/// Radio operating states based on SX126x chip modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioState {
    /// Device is in sleep mode (lowest power, ~160nA)
    Sleep = 0x0,
    /// Device is in standby mode using RC oscillator (~0.6mA)
    StandbyRc = 0x2,
    /// Device is in standby mode using crystal oscillator (~0.8mA)
    StandbyXosc = 0x3,
    /// Device is in frequency synthesis mode (transitional state)
    FreqSynth = 0x4,
    /// Device is in receive mode
    Rx = 0x5,
    /// Device is in transmit mode
    Tx = 0x6,
}

/// Standby mode options for power management
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandbyMode {
    /// Use 13MHz RC oscillator (faster wake-up, higher power)
    RC = 0x00,
    /// Use 32MHz crystal oscillator (slower wake-up, lower power)
    XOSC = 0x01,
}

/// Sleep configuration options
#[derive(Debug, Clone, Copy)]
pub struct SleepConfig {
    /// Keep configuration in memory for warm start (600nA vs 160nA)
    pub warm_start: bool,
    /// Enable RTC wake-up functionality
    pub rtc_wake: bool,
}

impl Default for SleepConfig {
    fn default() -> Self {
        Self {
            warm_start: true, // Default to warm start for faster wake-up
            rtc_wake: false,  // RTC wake-up disabled by default
        }
    }
}

/// Radio packet statistics returned by GetStats command
#[derive(Debug, Clone, Copy, Default)]
pub struct RadioStats {
    /// Number of packets successfully received
    pub packets_received: u16,
    /// Number of packets with CRC errors
    pub packets_crc_error: u16,
    /// Number of packets with length errors
    pub packets_length_error: u16,
}

/// Comprehensive radio status report for debugging and monitoring
#[derive(Debug)]
pub struct RadioStatusReport {
    /// Current radio state
    pub state: RadioState,
    /// Packet statistics
    pub stats: RadioStats,
    /// Device error flags
    pub device_errors: DeviceErrors,
    /// Current interrupt status
    pub irq_status: IrqStatus,
    /// Timestamp of last state change (if any)
    pub last_state_change: Option<Instant>,
}

/// Listen Before Talk configuration for regulatory compliance
#[derive(Debug, Clone, Copy)]
pub struct LbtConfig {
    /// RSSI threshold in dBm below which channel is considered clear
    pub rssi_threshold_dbm: i16,
    /// Duration to listen before transmitting (milliseconds)
    pub listen_duration_ms: u32,
    /// Maximum number of retry attempts if channel is busy
    pub max_retries: u8,
}

impl Default for LbtConfig {
    fn default() -> Self {
        Self {
            rssi_threshold_dbm: -85, // EU regulatory compliant threshold
            listen_duration_ms: 5,   // 5ms listen time
            max_retries: 3,          // Up to 3 retry attempts
        }
    }
}

/// Device error flags returned by GetDeviceErrors command
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceErrors {
    /// RC64k calibration failed
    pub rc64k_calib_error: bool,
    /// RC13M calibration failed
    pub rc13m_calib_error: bool,
    /// PLL calibration failed
    pub pll_calib_error: bool,
    /// ADC calibration failed
    pub adc_calib_error: bool,
    /// Image calibration failed
    pub img_calib_error: bool,
    /// Crystal oscillator failed to start
    pub xosc_start_error: bool,
    /// PLL lock lost
    pub pll_lock_error: bool,
    /// PA ramping failed
    pub pa_ramp_error: bool,
}

impl DeviceErrors {
    /// Create DeviceErrors from raw error register value
    pub fn from_raw(raw: u16) -> Self {
        Self {
            rc64k_calib_error: (raw & 0x0001) != 0,
            rc13m_calib_error: (raw & 0x0002) != 0,
            pll_calib_error: (raw & 0x0004) != 0,
            adc_calib_error: (raw & 0x0008) != 0,
            img_calib_error: (raw & 0x0010) != 0,
            xosc_start_error: (raw & 0x0020) != 0,
            pll_lock_error: (raw & 0x0040) != 0,
            pa_ramp_error: (raw & 0x0080) != 0,
        }
    }

    /// Check if any errors are present
    pub fn has_errors(&self) -> bool {
        self.rc64k_calib_error
            || self.rc13m_calib_error
            || self.pll_calib_error
            || self.adc_calib_error
            || self.img_calib_error
            || self.xosc_start_error
            || self.pll_lock_error
            || self.pa_ramp_error
    }
}

/// Errors that can occur during radio driver operations
#[derive(Error, Debug)]
pub enum DriverError {
    /// Hardware abstraction layer error (SPI, GPIO, etc.)
    #[error("HAL error: {0}")]
    Hal(HalError),
    /// Invalid configuration parameters provided
    #[error("Invalid params")]
    InvalidParams,
    /// Data checksum verification failed
    #[error("Checksum mismatch")]
    Checksum,
    /// Operation timed out
    #[error("Timeout")]
    Timeout,
    /// Invalid state transition attempted
    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition { from: RadioState, to: RadioState },
    /// Radio is in wrong state for requested operation
    #[error("Wrong state: expected {expected:?}, got {actual:?}")]
    WrongState {
        expected: RadioState,
        actual: RadioState,
    },
    /// Channel is busy (for LBT operations)
    #[error("Channel busy: RSSI {rssi_dbm} dBm above threshold {threshold_dbm} dBm")]
    ChannelBusy { rssi_dbm: i16, threshold_dbm: i16 },
    /// Device hardware errors detected
    #[error("Device errors detected: {0:?}")]
    DeviceErrors(DeviceErrors),
}

impl From<HalError> for DriverError {
    fn from(err: HalError) -> Self {
        DriverError::Hal(err)
    }
}

/// Main driver structure for SX126x radio transceivers
///
/// This structure maintains the radio state and provides high-level operations
/// for configuring and operating the SX126x radio. It uses a hardware abstraction
/// layer (HAL) to interface with the actual hardware.
///
/// ## Type Parameters
///
/// * `H` - Hardware abstraction layer implementation that provides SPI and GPIO access
///
/// ## Fields
///
/// The driver maintains internal state including current modulation parameters,
/// packet configuration, frequency settings, and buffer addresses.
pub struct Sx126xDriver<H: Hal> {
    /// Hardware abstraction layer for SPI/GPIO operations
    hal: H,
    /// Crystal oscillator frequency in Hz (typically 32MHz)
    xtal_freq: u32,
    /// Currently configured modulation parameters
    current_mod_params: Option<ModulationParams>,
    /// Currently configured packet parameters
    current_packet_params: Option<PacketParams>,
    /// Current RF frequency register value
    current_freq: Option<u32>,
    /// Base address in radio buffer for TX operations
    tx_base_addr: u8,
    /// Base address in radio buffer for RX operations
    rx_base_addr: u8,
    /// Current radio state (tracked for validation and power management)
    current_state: RadioState,
    /// Current packet type (GFSK or LoRa). Written by [`Sx126xDriver::set_packet_type`]
    /// and read by [`Sx126xDriver::get_lora_frequency_error`] to know whether the LoRa
    /// frequency-error register is meaningful for the live modem.
    current_packet_type: Option<PacketType>,
    /// Last time state was updated (for timeout detection)
    last_state_change: Option<Instant>,
}

/// A complete, storable description of one radio operating mode.
///
/// A single SX126x runs either a wM-Bus (GFSK) profile or a LoRa profile — never both at
/// once. `RadioProfile` bundles *all* the caller-facing parameters for a mode so the whole
/// configuration can be applied in one atomic step by [`Sx126xDriver::switch_profile`]
/// (standby → packet-type select → modulation → packet params → frequency → sync → IRQ),
/// rather than being reconstructed field-by-field at each switch.
#[derive(Debug, Clone)]
pub enum RadioProfile {
    /// Wireless M-Bus (GFSK) profile.
    Wmbus(WmbusProfile),
    /// LoRa profile.
    LoRa(LoRaProfile),
}

impl RadioProfile {
    /// The SX126x packet (modem) type this profile requires.
    pub fn packet_type(&self) -> PacketType {
        match self {
            RadioProfile::Wmbus(_) => PacketType::Gfsk,
            RadioProfile::LoRa(_) => PacketType::LoRa,
        }
    }
}

/// Parameters for a wireless M-Bus (GFSK) profile.
#[derive(Debug, Clone)]
pub struct WmbusProfile {
    /// RF carrier frequency in Hz (e.g. `868_950_000` for the EU C/T band).
    pub frequency_hz: u32,
    /// On-air bitrate in bits/s (e.g. `100_000` for mode C).
    pub bitrate: u32,
    /// Sync word bytes to match, from the value written to the sync registers.
    /// Mode C normally matches all three of `54 3D 54`, leaving the frame marker
    /// (`0xCD`/`0x3D`) as the first payload byte. Matching only `54 3D` is more
    /// permissive and also catches mode T.
    pub sync_word_len: u8,
    /// Preamble bits required before the sync word is looked for; 0 disables the gate.
    pub preamble_detect_bits: u8,
    /// Bytes written to the sync word registers, zero-padded. Only the first
    /// `sync_word_len` of them are matched.
    pub sync_word: [u8; 8],
}

impl WmbusProfile {
    /// Mode-C style profile: the given carrier and bitrate with the standard wM-Bus GFSK
    /// framing that [`Sx126xDriver::apply_wmbus_profile`] applies.
    pub fn mode_c(frequency_hz: u32, bitrate: u32) -> Self {
        Self {
            frequency_hz,
            bitrate,
            sync_word_len: 3,
            preamble_detect_bits: 8,
            sync_word: [0x54, 0x3D, 0x54, 0, 0, 0, 0, 0],
        }
    }
}

/// Parameters for a LoRa profile.
#[derive(Debug, Clone)]
pub struct LoRaProfile {
    /// RF carrier frequency in Hz.
    pub frequency_hz: u32,
    /// Spreading factor (SF5..SF12).
    pub sf: SpreadingFactor,
    /// Signal bandwidth.
    pub bw: LoRaBandwidth,
    /// Coding rate (4/5..4/8).
    pub cr: CodingRate,
    /// TX power in dBm.
    pub power_dbm: i8,
    /// Optional LoRa sync word (network id). `None` leaves the chip default in place.
    pub sync_word: Option<u16>,
}

/// A received packet tagged with the modem it was captured under, plus the radio metadata
/// gathered atomically with it.
///
/// Produced by [`Sx126xDriver::process_irqs_with_mode`] so a caller holding the driver lock
/// gets a `(mode, payload, metadata)` tuple that cannot skew if a profile switch happens
/// immediately after the call returns — the driver stays the single source of truth for
/// which modem a packet arrived on.
#[derive(Debug, Clone)]
pub struct ModeTaggedPacket {
    /// The modem (GFSK/wM-Bus or LoRa) the packet was received under.
    pub mode: PacketType,
    /// Raw payload bytes (undecoded).
    pub payload: Vec<u8>,
    /// Instantaneous RSSI for this packet, in dBm.
    pub rssi_dbm: i16,
    /// LoRa demodulator metadata; `Some` only when `mode == PacketType::LoRa`.
    pub lora: Option<LoRaRxInfo>,
}

/// LoRa-only receive metadata accompanying a [`ModeTaggedPacket`].
#[derive(Debug, Clone, PartialEq)]
pub struct LoRaRxInfo {
    /// Signal-to-noise ratio for this packet, in dB (signed).
    pub snr_db: f32,
    /// Per-packet carrier frequency error in Hz, if the LoRa demodulator reported one.
    pub freq_error_hz: Option<i32>,
    /// Active spreading factor.
    pub sf: SpreadingFactor,
    /// Active signal bandwidth.
    pub bw: LoRaBandwidth,
}

/// Decoded SX126x GetPacketStatus (0x14) fields. RSSI values are in dBm; `snr_db` is a
/// signed LoRa SNR and is only meaningful for LoRa packets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PacketStatus {
    /// Per-packet RSSI in dBm (`RssiPkt`).
    pub rssi_pkt_dbm: i16,
    /// LoRa SNR in dB (`SnrPkt`, signed).
    pub snr_db: f32,
    /// Post-despread signal RSSI in dBm (`SignalRssiPkt`).
    pub signal_rssi_dbm: i16,
}

/// SX126x-specific dual-mode capability, layered on top of
/// [`RadioDriver`](crate::wmbus::radio::radio_driver::RadioDriver).
///
/// The common `RadioDriver` trait is chip-agnostic. Switching a single radio between a
/// wM-Bus (GFSK) and a LoRa [`RadioProfile`] is an SX126x capability, so it lives in this
/// extension trait rather than widening `RadioDriver` (which the RFM69, for instance,
/// cannot satisfy for LoRa). A generic dual-mode consumer — a scheduler or dual-mode
/// handle — bounds on `D: RadioDriver + Sx126xExt` to require it.
#[async_trait::async_trait]
pub trait Sx126xExt {
    /// Atomically switch the radio to a complete [`RadioProfile`]. Leaves the radio in
    /// standby (see [`Sx126xDriver::switch_profile`]); the caller then starts RX or transmits.
    async fn switch_profile(
        &mut self,
        profile: &RadioProfile,
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError>;

    /// The packet (modem) type currently configured, or `None` if no profile has been
    /// applied yet.
    fn active_packet_type(&self) -> Option<PacketType>;
}

#[async_trait::async_trait]
impl<H: Hal + Send + Sync> Sx126xExt for Sx126xDriver<H> {
    async fn switch_profile(
        &mut self,
        profile: &RadioProfile,
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        // Delegate to the inherent (sync) method. The fully-qualified inherent path cannot
        // resolve to this trait method, so there is no accidental recursion.
        Sx126xDriver::switch_profile(self, profile).map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "profile switch failed: {e:?}"
            ))
        })
    }

    fn active_packet_type(&self) -> Option<PacketType> {
        self.current_packet_type
    }
}

impl<H: Hal> Sx126xDriver<H> {
    /// Create a new SX126x driver instance
    ///
    /// # Arguments
    ///
    /// * `hal` - Hardware abstraction layer implementation
    /// * `xtal_freq` - Crystal oscillator frequency in Hz (typically 32_000_000)
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use mbus_rs::wmbus::radio::driver::Sx126xDriver;
    /// // Substitute your own `Hal` implementation for `MockHal` on real hardware.
    /// use mbus_rs::wmbus::radio::hal::MockHal;
    ///
    /// let hal = MockHal::new();
    /// let driver = Sx126xDriver::new(hal, 32_000_000);
    /// ```
    pub fn new(hal: H, xtal_freq: u32) -> Self {
        Self {
            hal,
            xtal_freq,
            current_mod_params: None,
            current_packet_params: None,
            current_freq: None,
            tx_base_addr: 0,
            rx_base_addr: 0,
            current_state: RadioState::Sleep, // Start in sleep state
            current_packet_type: None,
            last_state_change: None,
        }
    }

    /// Set the RF carrier frequency
    ///
    /// Configures the radio's carrier frequency for transmission and reception.
    /// The frequency is calculated using the crystal frequency and converted to
    /// the SX126x internal format.
    ///
    /// # Arguments
    ///
    /// * `frequency_hz` - Target frequency in Hz (e.g., 868_950_000 for 868.95 MHz)
    ///
    /// # Returns
    ///
    /// * `Ok(())` on success
    /// * `Err(DriverError::Hal)` if SPI communication fails
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use mbus_rs::wmbus::radio::driver::{Sx126xDriver, DriverError};
    /// # use mbus_rs::wmbus::radio::hal::MockHal;
    /// # fn main() -> Result<(), DriverError> {
    /// # let mut driver = Sx126xDriver::new(MockHal::new(), 32_000_000);
    /// // Set frequency to 868.95 MHz (EU wM-Bus S-mode)
    /// driver.set_rf_frequency(868_950_000)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_rf_frequency(&mut self, frequency_hz: u32) -> Result<(), DriverError> {
        // Calculate frequency step based on crystal frequency
        // Frequency resolution = Xtal_freq / 2^25
        let rf_freq = (frequency_hz as u64 * (1u64 << 25) / self.xtal_freq as u64) as u32;

        let mut buf = [0u8; 4];
        buf[0] = (rf_freq >> 24) as u8;
        buf[1] = (rf_freq >> 16) as u8;
        buf[2] = (rf_freq >> 8) as u8;
        buf[3] = rf_freq as u8;

        self.hal.write_command(0x86, &buf)?; // SetRfFrequency command
        self.current_freq = Some(rf_freq);
        Ok(())
    }

    /// Set packet type (GFSK or LoRa)
    ///
    /// Switches the radio modem type. Must be called before setting modulation or packet params.
    ///
    /// # Arguments
    ///
    /// * `packet_type` - The packet type to configure
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Packet type set successfully
    /// * `Err(DriverError)` - Command failed
    pub fn set_packet_type(&mut self, packet_type: PacketType) -> Result<(), DriverError> {
        let param = match packet_type {
            PacketType::Gfsk => 0x00,
            PacketType::LoRa => 0x01,
        };
        self.hal.write_command(0x8A, &[param])?;
        // Track the active modem type so mode-aware reads (e.g. the LoRa
        // frequency-error register) know which demodulator is live. Without this,
        // `current_packet_type` stayed `None` and `get_lora_frequency_error` always
        // short-circuited to `Ok(None)`.
        self.current_packet_type = Some(packet_type);
        Ok(())
    }

    /// Configure packet parameters
    ///
    /// Sets up packet structure including preamble length, header type, payload size,
    /// CRC configuration, and sync word settings. Supports both GFSK and LoRa.
    ///
    /// # Arguments
    ///
    /// * `packet_params` - Packet configuration parameters
    ///
    /// # Returns
    ///
    /// * `Ok(())` on success
    /// * `Err(DriverError::Hal)` if SPI communication fails
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mbus_rs::wmbus::radio::modulation::*;
    /// # use mbus_rs::wmbus::radio::driver::{Sx126xDriver, DriverError};
    /// # use mbus_rs::wmbus::radio::hal::MockHal;
    ///
    /// # fn main() -> Result<(), DriverError> {
    /// # let mut driver = Sx126xDriver::new(MockHal::new(), 32_000_000);
    /// // GFSK for wM-Bus
    /// let gfsk_packet = PacketParams::Gfsk {
    ///     preamble_len: 48,                    // 48-bit preamble
    ///     header_type: HeaderType::Variable,   // Variable length packets
    ///     payload_len: 255,                    // Max payload size
    ///     crc_on: true,                        // Enable CRC
    ///     crc_type: CrcType::Byte2,           // 2-byte CRC
    ///     sync_word_len: 4,                    // 4-byte sync word
    /// };
    /// driver.set_packet_params(gfsk_packet)?;
    ///
    /// // LoRa example (explicit header, CRC on)
    /// let lora_packet = PacketParams::LoRa {
    ///     params: LoRaPacketParams {
    ///         preamble_len: 8,                 // 8 symbols preamble
    ///         implicit_header: false,          // Explicit header
    ///         payload_len: 255,                // Max payload size
    ///         crc_on: true,                    // Enable CRC
    ///         iq_inverted: false,
    ///     }
    /// };
    /// driver.set_packet_params(lora_packet)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_packet_params(&mut self, packet_params: PacketParams) -> Result<(), DriverError> {
        match packet_params {
            PacketParams::Gfsk {
                preamble_len,
                header_type,
                payload_len,
                crc_on,
                crc_type,
                sync_word_len,
                preamble_detect_bits,
            } => {
                // Datasheet order for SetPacketParams in GFSK. There is no packet-type
                // byte here — the type is selected separately by SetPacketType (0x8A) —
                // and every field is positional, so an extra leading byte shifts the
                // whole frame and silently programs a receiver that detects nothing.
                let buf = [
                    (preamble_len >> 8) as u8,
                    preamble_len as u8,
                    match preamble_detect_bits {
                        0 => 0x00,
                        1..=8 => 0x04,
                        9..=16 => 0x05,
                        17..=24 => 0x06,
                        _ => 0x07,
                    },
                    // Sync word length is expressed in BITS, while callers describe it
                    // in bytes.
                    sync_word_len.saturating_mul(8),
                    0x00, // address filtering off
                    match header_type {
                        HeaderType::Variable => 0x01,
                        HeaderType::Fixed => 0x00,
                    },
                    payload_len,
                    // Note the encoding is not a flag: 0x01 disables the CRC while
                    // 0x00 selects a one-byte CRC, so the intuitive "0 means off" is
                    // precisely backwards and silently makes the radio drop every frame.
                    match (crc_on, crc_type) {
                        (false, _) => 0x01,
                        (true, CrcType::Byte1) => 0x00,
                        (true, CrcType::Byte2) => 0x02,
                    },
                    0x00, // whitening off
                ];

                self.hal.write_command(0x8C, &buf)?; // SetPacketParams command
            }
            PacketParams::LoRa { params } => {
                let mut buf = [0u8; 6];

                // Preamble length in symbols (16-bit value)
                buf[0] = (params.preamble_len >> 8) as u8; // MSB
                buf[1] = params.preamble_len as u8; // LSB

                // Header type (Implicit=0x01, Explicit=0x00)
                buf[2] = if params.implicit_header { 0x01 } else { 0x00 };

                // Maximum payload length
                buf[3] = params.payload_len;

                // CRC enable/disable
                buf[4] = if params.crc_on { 0x01 } else { 0x00 };

                // IQ inverted
                buf[5] = if params.iq_inverted { 0x01 } else { 0x00 };

                self.hal.write_command(0x8C, &buf)?; // SetPacketParams command for LoRa
            }
        }
        self.current_packet_params = Some(packet_params);
        Ok(())
    }

    /// Set modulation parameters for the radio
    ///
    /// Configures the modulation scheme (GFSK or LoRa) with specific parameters.
    ///
    /// # Arguments
    ///
    /// * `mod_params` - Modulation configuration parameters
    ///
    /// # Returns
    ///
    /// * `Ok(())` on success
    /// * `Err(DriverError::Hal)` if SPI communication fails
    pub fn set_modulation_params(
        &mut self,
        mod_params: ModulationParams,
    ) -> Result<(), DriverError> {
        match mod_params {
            ModulationParams::Gfsk { params } => {
                let mut buf = [0u8; 8];

                // Calculate bitrate register value
                let bitrate_reg = (32_000_000u32 * 32) / params.bitrate;
                buf[0] = ((bitrate_reg >> 16) & 0xFF) as u8;
                buf[1] = ((bitrate_reg >> 8) & 0xFF) as u8;
                buf[2] = (bitrate_reg & 0xFF) as u8;

                // Modulation shaping
                buf[3] = params.modulation_shaping;

                // RX bandwidth. The chip takes an index into a fixed set of filter
                // settings, not a frequency, so a bandwidth in kHz written straight to
                // this byte is simply an invalid setting.
                buf[4] = Self::gfsk_rx_bandwidth_reg(params.bandwidth);

                // Frequency deviation
                let fdev_reg = (params.fdev as u64 * (1 << 25)) / self.xtal_freq as u64;
                buf[5] = ((fdev_reg >> 16) & 0xFF) as u8;
                buf[6] = ((fdev_reg >> 8) & 0xFF) as u8;
                buf[7] = (fdev_reg & 0xFF) as u8;

                self.hal.write_command(0x8B, &buf)?; // SetModulationParams command
            }
            ModulationParams::LoRa { params } => {
                let mut buf = [0u8; 4];

                // Spreading Factor
                buf[0] = params.sf as u8;

                // Bandwidth
                buf[1] = params.bw as u8;

                // Coding Rate
                buf[2] = params.cr as u8;

                // Low Data Rate Optimize
                buf[3] = if params.low_data_rate_optimize {
                    0x01
                } else {
                    0x00
                };

                self.hal.write_command(0x8B, &buf)?; // SetModulationParams command for LoRa
            }
        }
        self.current_mod_params = Some(mod_params);
        Ok(())
    }

    /// Map a double-sideband bandwidth in kHz to its SX126x filter setting,
    /// choosing the narrowest filter that is still at least as wide as requested.
    fn gfsk_rx_bandwidth_reg(khz: u16) -> u8 {
        const TABLE: &[(u16, u8)] = &[
            (5, 0x1F),
            (6, 0x17),
            (7, 0x0F),
            (10, 0x1E),
            (12, 0x16),
            (15, 0x0E),
            (20, 0x1D),
            (23, 0x15),
            (29, 0x0D),
            (39, 0x1C),
            (47, 0x14),
            (59, 0x0C),
            (78, 0x1B),
            (94, 0x13),
            (117, 0x0B),
            (156, 0x1A),
            (187, 0x12),
            (234, 0x0A),
            (312, 0x19),
            (374, 0x11),
            (467, 0x09),
        ];
        TABLE
            .iter()
            .find(|(w, _)| *w >= khz)
            .map(|(_, reg)| *reg)
            .unwrap_or(0x09)
    }

    pub fn set_sync_word(&mut self, sync_word: [u8; 8]) -> Result<(), DriverError> {
        // Write to registers 0x06C0 - 0x06C7
        self.hal.write_register(0x06C0, &sync_word)?;
        Ok(())
    }

    pub fn configure_crc(&mut self, polynomial: u16) -> Result<(), DriverError> {
        let msb = (polynomial >> 8) as u8;
        let lsb = polynomial as u8;
        self.hal.write_register(0x06BE, &[msb])?;
        self.hal.write_register(0x06BF, &[lsb])?;
        Ok(())
    }

    pub fn disable_whitening(&mut self) -> Result<(), DriverError> {
        // Set whitening initial value to disable (specific config needed)
        self.hal.write_register(0x06B8, &[0x00])?; // MSB to 0
        self.hal.write_register(0x06B9, &[0x00])?; // LSB to 0
        Ok(())
    }

    pub fn set_buffer_base_addresses(
        &mut self,
        tx_base: u8,
        rx_base: u8,
    ) -> Result<(), DriverError> {
        let buf = [tx_base, rx_base];
        self.hal.write_command(0x8F, &buf)?; // SetBufferBaseAddress
        self.tx_base_addr = tx_base;
        self.rx_base_addr = rx_base;
        Ok(())
    }

    pub fn write_buffer(&mut self, offset: u8, data: &[u8]) -> Result<(), DriverError> {
        let mut buf = vec![0x0E]; // WriteBuffer opcode
        buf.extend_from_slice(&[offset]);
        buf.extend_from_slice(data);
        self.hal.write_command(0x0E, &buf[1..])?; // Payload after offset
        Ok(())
    }

    pub fn read_buffer(&mut self, offset: u8, len: u8, buf: &mut [u8]) -> Result<(), DriverError> {
        let cmd_buf = [0x1E, offset, 0x00, len]; // ReadBuffer: offset, offset2 (0), len
        self.hal.write_command(0x1E, &cmd_buf[1..])?;
        self.hal.read_command(0x1E, buf)?;
        Ok(())
    }

    pub fn set_tx(&mut self, timeout: u32) -> Result<(), DriverError> {
        let mut buf = [0u8; 4];
        let tout = timeout & 0x00FFFFFF; // 24-bit timeout
        buf[0] = (tout >> 16) as u8;
        buf[1] = (tout >> 8) as u8;
        buf[2] = tout as u8;
        buf[3] = 0x00; // Freq hop off
        self.hal.write_command(0x83, &buf)?; // SetTx
        Ok(())
    }

    pub fn set_rx(&mut self, timeout: u32) -> Result<(), DriverError> {
        let mut buf = [0u8; 5];
        let tout = timeout & 0x00FFFFFF; // 24-bit timeout
        buf[0] = (tout >> 16) as u8;
        buf[1] = (tout >> 8) as u8;
        buf[2] = tout as u8;
        buf[3] = 0x00; // Continuous mode
        buf[4] = 0x00; // Freq hop off
        self.hal.write_command(0x82, &buf)?; // SetRx
        Ok(())
    }

    /// Direct access to the HAL, for diagnostics that need the chip's raw status,
    /// IRQ and buffer registers rather than the driver's interpretation of them.
    pub fn hal_mut(&mut self) -> &mut H {
        &mut self.hal
    }

    pub fn set_rx_continuous(&mut self) -> Result<(), DriverError> {
        self.set_rx(0xFFFFFF)?; // Infinite timeout
        Ok(())
    }

    pub fn get_rx_buffer_status(&mut self, buf: &mut [u8; 3]) -> Result<(), DriverError> {
        self.hal.read_command(0x13, buf)?; // GetRxBufferStatus: [size, start_addr, rx_current_addr]
        Ok(())
    }

    pub fn set_pa_config(
        &mut self,
        pa_duty_cycle: u8,
        hp_max: u8,
        device_sel: u8,
    ) -> Result<(), DriverError> {
        let buf = [device_sel, hp_max, pa_duty_cycle];
        self.hal.write_command(0x95, &buf)?; // SetPaConfig
        Ok(())
    }

    pub fn set_tx_params(&mut self, power: i8, ramp_time: u8) -> Result<(), DriverError> {
        let mut buf = [0u8; 2];
        buf[0] = power as u8; // -17 to +15 dBm
        buf[1] = ramp_time;
        self.hal.write_command(0x8E, &buf)?; // SetTxParams
        Ok(())
    }

    pub fn get_irq_status(&mut self) -> Result<IrqStatus, DriverError> {
        let mut buf = [0u8; 2];
        self.hal.read_command(0x12, &mut buf)?; // GetIrqStatus
        Ok(IrqStatus::from(((buf[0] as u16) << 8) | (buf[1] as u16)))
    }

    pub fn clear_irq_status(&mut self, irq: u16) -> Result<(), DriverError> {
        let buf = [(irq >> 8) as u8, irq as u8];
        self.hal.write_command(0x02, &buf)?; // ClearIrqStatus
        Ok(())
    }

    pub fn set_dio_irq_params(
        &mut self,
        irq_mask: u16,
        dio1_mask: u16,
        dio2_mask: u16,
        dio3_mask: u16,
    ) -> Result<(), DriverError> {
        let mut buf = [0u8; 8];
        buf[0] = (irq_mask >> 8) as u8;
        buf[1] = irq_mask as u8;
        buf[2] = (dio1_mask >> 8) as u8;
        buf[3] = dio1_mask as u8;
        buf[4] = (dio2_mask >> 8) as u8;
        buf[5] = dio2_mask as u8;
        buf[6] = (dio3_mask >> 8) as u8;
        buf[7] = dio3_mask as u8;
        self.hal.write_command(0x08, &buf)?; // SetDioIrqParams
        Ok(())
    }

    /// Process pending interrupts and handle radio events
    ///
    /// This method should be called regularly (typically in a polling loop or from an
    /// interrupt handler) to process radio events. It checks the interrupt status,
    /// handles received data, and logs relevant events. Supports both GFSK and LoRa.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(payload))` - Received data payload if RX completed successfully
    /// * `Ok(None)` - No received data (TX done, error, or no interrupts pending)
    /// * `Err(DriverError)` - Hardware communication error
    ///
    /// # Interrupt Handling
    ///
    /// The method handles these interrupt conditions:
    /// - **RX Done**: Retrieves received payload from radio buffer
    /// - **TX Done**: Logs transmission completion
    /// - **CRC Error**: Logs CRC validation failure
    /// - **Timeout**: Logs operation timeout
    /// - **Header Valid**: For LoRa, confirms valid header before RxDone
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use mbus_rs::wmbus::radio::driver::{Sx126xDriver, DriverError};
    /// # use mbus_rs::wmbus::radio::hal::MockHal;
    /// # fn main() -> Result<(), DriverError> {
    /// # let mut driver = Sx126xDriver::new(MockHal::new(), 32_000_000);
    /// // Polling loop for received data (GFSK or LoRa)
    /// loop {
    ///     if let Some(payload) = driver.process_irqs()? {
    ///         println!("Received {} bytes: {:?}", payload.len(), payload);
    ///         // Process the received wM-Bus or LoRa frame
    ///     }
    ///     std::thread::sleep(std::time::Duration::from_millis(10));
    /// }
    /// # }
    /// ```
    pub fn process_irqs(&mut self) -> Result<Option<Vec<u8>>, DriverError> {
        // Read current interrupt status
        let irq_status = self.get_irq_status()?;

        // Clear all pending interrupts
        self.clear_irq_status(0xFFFF)?;

        // Handle RX completion
        if irq_status.rx_done() {
            let mut status = [0u8; 3];
            self.get_rx_buffer_status(&mut status)?;

            // Extract received packet length (status[0] contains length)
            let rx_len = status[0] as usize;

            if rx_len > 0 {
                // Read received payload from radio buffer
                let mut payload = vec![0u8; rx_len];
                self.read_buffer(self.rx_base_addr, rx_len as u8, &mut payload)?;
                log::info!("RX done, received {rx_len} bytes");
                return Ok(Some(payload));
            }
        }

        // Handle TX completion
        if irq_status.tx_done() {
            log::info!("TX done - transmission completed successfully");
        }

        // Handle CRC errors
        if irq_status.crc_err() {
            log::warn!("CRC error - received packet failed CRC validation");
        }

        // Handle timeouts
        if irq_status.timeout() {
            log::warn!("Timeout - operation did not complete within expected time");
        }

        // Handle LoRa-specific: Header Valid (for variable-length LoRa packets)
        if irq_status.header_valid() {
            log::debug!("LoRa header valid - packet reception started");
        }

        Ok(None)
    }

    /// Atomically poll for a received packet **and** tag it with the active modem plus its
    /// receive metadata.
    ///
    /// This is the single-source-of-truth reception op for a dual-mode radio: in one
    /// `&mut self` call it snapshots the current packet type, drains any received payload,
    /// reads the RSSI, and — in LoRa mode — the frequency error and modulation params. A
    /// caller holding the driver lock therefore gets a consistent `(mode, payload, metadata)`
    /// tuple that cannot skew even if a scheduler switches profiles right afterwards (which
    /// a later `active_packet_type()` query could otherwise miss).
    ///
    /// Returns `Ok(None)` when no packet is pending, or when no profile has been applied yet.
    pub fn process_irqs_with_mode(&mut self) -> Result<Option<ModeTaggedPacket>, DriverError> {
        // Snapshot the active modem *before* draining the packet.
        let Some(mode) = self.current_packet_type else {
            return Ok(None);
        };
        let Some(payload) = self.process_irqs()? else {
            return Ok(None);
        };
        let rssi_dbm = self.get_rssi_instant()?;
        let lora = match mode {
            PacketType::LoRa => {
                let freq_error_hz = self.get_lora_frequency_error()?;
                let snr_db = self.get_packet_status()?.snr_db;
                match self.current_mod_params {
                    Some(ModulationParams::LoRa { params }) => Some(LoRaRxInfo {
                        snr_db,
                        freq_error_hz,
                        sf: params.sf,
                        bw: params.bw,
                    }),
                    // LoRa modem selected but no LoRa modulation params recorded: an
                    // inconsistent radio state. Fail rather than emit a LoRa packet without
                    // the metadata `ReceivedItem::Lora` requires.
                    _ => return Err(DriverError::InvalidParams),
                }
            }
            PacketType::Gfsk => None,
        };
        Ok(Some(ModeTaggedPacket {
            mode,
            payload,
            rssi_dbm,
            lora,
        }))
    }

    /// Configure radio for wireless M-Bus operation
    ///
    /// This is a convenience method that configures all radio parameters for optimal
    /// wM-Bus operation. It sets up GFSK modulation, appropriate packet parameters,
    /// CRC configuration, sync word, and power amplifier settings.
    ///
    /// # Arguments
    ///
    /// * `frequency_hz` - Operating frequency in Hz (e.g., 868_950_000 for EU S-mode)
    /// * `bitrate` - Data rate in bits per second (typically 100_000 for wM-Bus)
    ///
    /// # Returns
    ///
    /// * `Ok(())` on successful configuration
    /// * `Err(DriverError)` if any configuration step fails
    ///
    /// # wM-Bus Configuration Details
    ///
    /// This method configures:
    /// - 2-FSK with no shaping, as mode C is plain NRZ
    /// - 234 kHz double-sideband receiver bandwidth
    /// - Frequency deviation = bitrate / 2
    /// - Mode-C sync word `54 3D 54`, leaving the frame marker as the first payload byte
    /// - Fixed-length reception: the byte after the sync word is that marker, not a length
    /// - Hardware CRC off, because wM-Bus carries its own per-block CRC-16/EN-13757
    /// - Whitening disabled (wM-Bus requirement)
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use mbus_rs::wmbus::radio::driver::{Sx126xDriver, DriverError};
    /// # use mbus_rs::wmbus::radio::hal::MockHal;
    /// # fn main() -> Result<(), DriverError> {
    /// # let mut driver = Sx126xDriver::new(MockHal::new(), 32_000_000);
    /// // Configure for EU wM-Bus S-mode
    /// driver.configure_for_wmbus(868_950_000, 100_000)?;
    ///
    /// // Start receiving
    /// driver.set_rx_continuous()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn configure_for_wmbus(
        &mut self,
        frequency_hz: u32,
        bitrate: u32,
    ) -> Result<(), DriverError> {
        // Select the GFSK modem *before* applying GFSK modulation/packet params.
        // (Previously omitted here — switching back from a LoRa profile left the modem
        // in LoRa, so GFSK reception silently failed.)
        self.set_packet_type(PacketType::Gfsk)?;
        self.apply_wmbus_profile(&WmbusProfile::mode_c(frequency_hz, bitrate))
    }

    /// Apply the parameter set for a wM-Bus (GFSK) profile: modulation, packet framing,
    /// CRC, whitening, sync word, PA/TX, buffers and IRQ routing.
    ///
    /// Assumes the GFSK packet type is already selected (see [`Sx126xDriver::set_packet_type`])
    /// and the radio is in standby. Both [`Sx126xDriver::configure_for_wmbus`] and
    /// [`Sx126xDriver::switch_profile`] funnel through here, so there is a single
    /// GFSK-apply path.
    pub fn apply_wmbus_profile(&mut self, profile: &WmbusProfile) -> Result<(), DriverError> {
        // Set RF frequency, then calibrate the image for that band — skipping this
        // leaves the chip calibrated for its default band and quietly costs
        // sensitivity, which presents as a receiver that hears only strong signals.
        self.set_rf_frequency(profile.frequency_hz)?;
        self.calibrate_image(profile.frequency_hz)?;

        // Configure GFSK modulation parameters
        let mod_params = ModulationParams::Gfsk {
            params: GfskModParams {
                bitrate: profile.bitrate,
                // No shaping on receive: mode C is plain NRZ 2-FSK with no encoding,
                // and this field takes a datasheet code (0x00 none, 0x08-0x0B Gaussian)
                // rather than a free-form number.
                modulation_shaping: 0x00,
                // Double-sideband, so ~2x the deviation plus the bitrate.
                bandwidth: 234,
                fdev: profile.bitrate / 2, // Frequency deviation = bitrate/2 (typical FSK)
            },
        };
        self.set_modulation_params(mod_params)?;

        // Packet handling mirrors the RFM69 configuration that receives this fleet
        // today. Two settings are load-bearing and were previously wrong here:
        //
        // * `crc_on: false` — wM-Bus carries its own per-block CRC-16/EN-13757 *inside*
        //   the payload, which this crate verifies in software. Letting the radio also
        //   apply a CRC makes it discard every frame as corrupt.
        // * fixed-length reception — the byte after the sync word is the mode-C frame
        //   marker (0x3D type B / 0xCD type A), not a length field, so the variable-
        //   length engine would mis-size every packet. The link decoder derives the
        //   true length from the L-field.
        let packet_params = PacketParams::Gfsk {
            preamble_len: 32, // 4 bytes, as the working RFM69 config uses
            header_type: HeaderType::Fixed,
            payload_len: 255, // read the whole buffer; the L-field bounds the frame
            crc_on: false,
            crc_type: CrcType::Byte2,
            // 54 3D 54. The mode-C air format is preamble 55 55 55 55, then 54 3D,
            // then the signalling byte 54, then the frame marker (0xCD type A / 0x3D
            // type B). Matching all three leaves that marker as the first payload byte,
            // which is what the link decoder expects.
            sync_word_len: profile.sync_word_len,
            preamble_detect_bits: profile.preamble_detect_bits,
        };
        self.set_packet_params(packet_params)?;

        // No whitening on wM-Bus.
        self.disable_whitening()?;

        // Mode-C sync word: 54 3D 54. This is what triggers reception — the previous
        // value here was the S-mode pattern (B4 B6 5A 5A), which never matches mode-C
        // traffic and is why the label said mode_c while the radio listened for S.
        self.set_sync_word(profile.sync_word)?;

        // Configure power amplifier for +14 dBm output
        self.set_pa_config(0x04, 0x00, 0x00)?;
        self.set_tx_params(14, 0x07)?; // +14 dBm with ramp time

        // Set buffer base addresses
        self.set_buffer_base_addresses(0, 0)?;

        // Interrupt routing.
        //
        // The general mask decides which events may set a bit in GetIrqStatus at all,
        // so PreambleDetected and SyncwordValid must be enabled even though nothing
        // waits on them: without that a diagnostic reading GetIrqStatus can never
        // observe them, and "no preamble detected" becomes indistinguishable from
        // "preamble detection not reported".
        //
        // DIO2 is deliberately given no IRQ. On boards that use SetDIO2AsRfSwitchCtrl
        // the pin drives the antenna switch and cannot also be an interrupt output —
        // assigning one here silently disables the RF switch.
        self.set_dio_irq_params(
            IrqMaskBit::RxDone as u16
                | IrqMaskBit::TxDone as u16
                | IrqMaskBit::PreambleDetected as u16
                | IrqMaskBit::SyncwordValid as u16
                | IrqMaskBit::CrcErr as u16
                | IrqMaskBit::Timeout as u16,
            IrqMaskBit::RxDone as u16, // DIO1: the only line we wait on
            0,                         // DIO2: reserved for the RF switch
            0,                         // DIO3: unused
        )?;

        Ok(())
    }

    // ========================== STATE MANAGEMENT METHODS ==========================

    /// Get the current radio state from the device
    ///
    /// Reads the actual state from the SX126x status register and updates
    /// internal state tracking.
    ///
    /// # Returns
    ///
    /// * `Ok(RadioState)` - Current radio state
    /// * `Err(DriverError::Hal)` - SPI communication error
    pub fn get_state(&mut self) -> Result<RadioState, DriverError> {
        let mut status = [0u8; 1];
        self.hal.read_command(0xC0, &mut status)?; // GetStatus command

        // Extract chip mode from bits [6:4]
        let chip_mode = (status[0] >> 4) & 0x07;
        let state = match chip_mode {
            0x0 => RadioState::Sleep,
            0x2 => RadioState::StandbyRc,
            0x3 => RadioState::StandbyXosc,
            0x4 => RadioState::FreqSynth,
            0x5 => RadioState::Rx,
            0x6 => RadioState::Tx,
            _ => {
                log::warn!("Unknown chip mode: 0x{chip_mode:02X}");
                self.current_state // Return last known state
            }
        };

        // Update internal state tracking
        if state != self.current_state {
            log::debug!("State changed: {:?} -> {:?}", self.current_state, state);
            self.current_state = state;
            self.last_state_change = Some(Instant::now());
        }

        Ok(state)
    }

    /// Wait for the radio to reach a specific state with timeout
    ///
    /// Polls the radio state until it matches the target state or timeout occurs.
    /// This is essential for operations that require state transitions to complete.
    ///
    /// # Arguments
    ///
    /// * `target_state` - The state to wait for
    /// * `timeout_ms` - Maximum time to wait in milliseconds
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Target state reached
    /// * `Err(DriverError::Timeout)` - Timeout occurred
    /// * `Err(DriverError::Hal)` - Communication error
    pub fn wait_for_state(
        &mut self,
        target_state: RadioState,
        timeout_ms: u32,
    ) -> Result<(), DriverError> {
        let start = Instant::now();
        let timeout = Duration::from_millis(timeout_ms as u64);

        while start.elapsed() < timeout {
            let current = self.get_state()?;
            if current == target_state {
                return Ok(());
            }
            // Small delay to avoid excessive SPI traffic
            std::thread::sleep(Duration::from_millis(1));
        }

        Err(DriverError::Timeout)
    }

    /// Check if a state transition is valid
    ///
    /// Validates state transitions according to SX126x state machine rules.
    ///
    /// # Arguments
    ///
    /// * `from` - Current state
    /// * `to` - Desired state
    ///
    /// # Returns
    ///
    /// * `true` if transition is valid
    /// * `false` if transition is invalid
    fn is_valid_transition(&self, from: RadioState, to: RadioState) -> bool {
        use RadioState::*;
        match (from, to) {
            // From Sleep
            (Sleep, StandbyRc) | (Sleep, StandbyXosc) => true,
            // From Standby modes
            (StandbyRc, StandbyXosc) | (StandbyXosc, StandbyRc) => true,
            (StandbyRc, FreqSynth) | (StandbyXosc, FreqSynth) => true,
            (StandbyRc, Sleep) | (StandbyXosc, Sleep) => true,
            // From FreqSynth
            (FreqSynth, Tx) | (FreqSynth, Rx) => true,
            (FreqSynth, StandbyRc) | (FreqSynth, StandbyXosc) => true,
            // From Tx/Rx
            (Tx, StandbyRc) | (Tx, StandbyXosc) => true,
            (Rx, StandbyRc) | (Rx, StandbyXosc) => true,
            (Tx, Rx) | (Rx, Tx) => true,
            // Same state (no-op)
            (s1, s2) if s1 == s2 => true,
            // All other transitions are invalid
            _ => false,
        }
    }

    // ========================== POWER MANAGEMENT METHODS ==========================

    /// Set the radio into standby mode
    ///
    /// Standby mode allows configuration while consuming low power.
    /// Choose between RC oscillator (faster wake-up) or crystal oscillator (lower power).
    ///
    /// # Arguments
    ///
    /// * `mode` - Standby mode (RC or XOSC)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Successfully entered standby mode
    /// * `Err(DriverError)` - Operation failed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use mbus_rs::wmbus::radio::driver::StandbyMode;
    /// # use mbus_rs::wmbus::radio::driver::{Sx126xDriver, DriverError};
    /// # use mbus_rs::wmbus::radio::hal::MockHal;
    /// # fn main() -> Result<(), DriverError> {
    /// # let mut driver = Sx126xDriver::new(MockHal::new(), 32_000_000);
    /// // Use RC oscillator for faster wake-up
    /// driver.set_standby(StandbyMode::RC)?;
    ///
    /// // Use crystal oscillator for lower power
    /// driver.set_standby(StandbyMode::XOSC)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_standby(&mut self, mode: StandbyMode) -> Result<(), DriverError> {
        let target_state = match mode {
            StandbyMode::RC => RadioState::StandbyRc,
            StandbyMode::XOSC => RadioState::StandbyXosc,
        };

        // Validate transition
        if !self.is_valid_transition(self.current_state, target_state) {
            return Err(DriverError::InvalidStateTransition {
                from: self.current_state,
                to: target_state,
            });
        }

        // Send SetStandby command
        let cmd = [mode as u8];
        self.hal.write_command(0x80, &cmd)?;

        // Wait for transition to complete
        self.wait_for_state(target_state, 500)?; // 500ms timeout

        log::info!("Radio entered standby mode: {mode:?}");
        Ok(())
    }

    /// Set the radio into sleep mode for ultra-low power consumption
    ///
    /// Sleep mode provides the lowest power consumption (~160nA cold start, ~600nA warm start).
    /// Configuration can be retained (warm start) or cleared (cold start).
    ///
    /// # Arguments
    ///
    /// * `config` - Sleep configuration options
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Successfully entered sleep mode
    /// * `Err(DriverError)` - Operation failed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use mbus_rs::wmbus::radio::driver::SleepConfig;
    /// # use mbus_rs::wmbus::radio::driver::{Sx126xDriver, DriverError};
    /// # use mbus_rs::wmbus::radio::hal::MockHal;
    /// # fn main() -> Result<(), DriverError> {
    /// # let mut driver = Sx126xDriver::new(MockHal::new(), 32_000_000);
    /// // Warm start (faster wake-up, slightly higher power)
    /// driver.set_sleep(SleepConfig::default())?;
    ///
    /// // Cold start (ultra-low power, slower wake-up)
    /// driver.set_sleep(SleepConfig { warm_start: false, rtc_wake: false })?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_sleep(&mut self, config: SleepConfig) -> Result<(), DriverError> {
        // Validate transition - can only sleep from standby modes
        if !matches!(
            self.current_state,
            RadioState::StandbyRc | RadioState::StandbyXosc
        ) {
            return Err(DriverError::InvalidStateTransition {
                from: self.current_state,
                to: RadioState::Sleep,
            });
        }

        // Build sleep configuration byte
        let mut sleep_config = 0u8;
        if config.warm_start {
            sleep_config |= 0x04; // Retain configuration in warm start
        }
        if config.rtc_wake {
            sleep_config |= 0x01; // Enable RTC wake-up
        }

        // Send SetSleep command
        let cmd = [sleep_config];
        self.hal.write_command(0x84, &cmd)?;

        // Update state immediately (cannot read status when in sleep)
        self.current_state = RadioState::Sleep;
        self.last_state_change = Some(Instant::now());

        log::info!(
            "Radio entered sleep mode: warm_start={}, rtc_wake={}",
            config.warm_start,
            config.rtc_wake
        );
        Ok(())
    }

    // ========================== DIAGNOSTIC METHODS ==========================

    /// Get instantaneous RSSI measurement
    ///
    /// Measures the current received signal strength. The radio must be in RX mode
    /// for accurate measurements. Used for LBT (Listen Before Talk) operations.
    ///
    /// # Returns
    ///
    /// * `Ok(i16)` - RSSI in dBm (negative values)
    /// * `Err(DriverError)` - Measurement failed
    ///
    /// # Note
    ///
    /// The radio should be in RX mode for at least a few hundred microseconds
    /// before taking RSSI measurements to allow the measurement to settle.
    pub fn get_rssi_instant(&mut self) -> Result<i16, DriverError> {
        let mut rssi_raw = [0u8; 1];
        self.hal.read_command(0x15, &mut rssi_raw)?; // GetRssiInst command

        // Convert to dBm: Signal power = -RssiInst / 2
        let rssi_dbm = -(rssi_raw[0] as i16) / 2;

        Ok(rssi_dbm)
    }

    /// Get packet status information from last received packet
    ///
    /// Returns RSSI and other statistics from the most recently received packet.
    /// Useful for link quality assessment.
    ///
    /// # Returns
    ///
    /// * `Ok((rssi_pkt, rssi_sync))` - Packet RSSI values in dBm
    /// * `Err(DriverError)` - Read failed
    ///
    /// Note: `GetPacketStatus` does NOT report frequency error — its three bytes are
    /// RSSI/SNR fields (LoRa: RssiPkt, SnrPkt, SignalRssiPkt; GFSK: RxStatus, RssiSync,
    /// RssiAvg). A previous version mislabeled the third byte as "FreqError"; the real
    /// per-packet frequency error comes from [`Self::get_lora_frequency_error`].
    pub fn get_packet_status(&mut self) -> Result<PacketStatus, DriverError> {
        let mut status = [0u8; 3];
        self.hal.read_command(0x14, &mut status)?; // GetPacketStatus command

        // LoRa: status[0] = RssiPkt, status[1] = SnrPkt (signed, quarter-dB),
        //       status[2] = SignalRssiPkt.
        Ok(PacketStatus {
            rssi_pkt_dbm: -(status[0] as i16) / 2,
            snr_db: lora_snr_db(status[1]),
            signal_rssi_dbm: -(status[2] as i16) / 2,
        })
    }

    /// Read the LoRa modem's per-packet frequency-error estimate, in Hz.
    ///
    /// Returns `Ok(None)` when the radio is not in LoRa mode (or no LoRa modulation is
    /// configured): the SX126x exposes a per-packet frequency error only from its LoRa
    /// demodulator — in GFSK there is no equivalent register, so this is `None` there.
    ///
    /// In LoRa mode it reads the signed 20-bit value from registers 0x076B..0x076D and
    /// scales it by the configured bandwidth (see [`lora_freq_error_hz`]).
    pub fn get_lora_frequency_error(&mut self) -> Result<Option<i32>, DriverError> {
        match self.current_packet_type {
            Some(PacketType::LoRa) => {}
            _ => return Ok(None),
        }
        let bw_khz = match self.current_mod_params {
            Some(ModulationParams::LoRa { params }) => params.bw.bandwidth_khz(),
            _ => return Ok(None),
        };
        let mut regs = [0u8; 3];
        self.hal.read_register(0x076B, &mut regs)?; // SX126x LoRa FreqError[19:0]
        Ok(Some(lora_freq_error_hz(regs, bw_khz)))
    }

    /// Get device error status
    ///
    /// Reads the device error register to check for calibration failures,
    /// oscillator problems, and other hardware issues.
    ///
    /// # Returns
    ///
    /// * `Ok(DeviceErrors)` - Current error status
    /// * `Err(DriverError)` - Read failed
    pub fn get_device_errors(&mut self) -> Result<DeviceErrors, DriverError> {
        let mut errors = [0u8; 2];
        self.hal.read_command(0x17, &mut errors)?; // GetDeviceErrors command

        let error_word = ((errors[0] as u16) << 8) | (errors[1] as u16);
        let device_errors = DeviceErrors::from_raw(error_word);

        if device_errors.has_errors() {
            log::warn!("Device errors detected: {device_errors:?}");
        }

        Ok(device_errors)
    }

    /// Clear device error flags
    ///
    /// Clears all error flags in the device error register.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Errors cleared successfully
    /// * `Err(DriverError)` - Clear operation failed
    pub fn clear_device_errors(&mut self) -> Result<(), DriverError> {
        // ClearDeviceErrors command (opcode 0x07, no parameters)
        self.hal.write_command(0x07, &[])?;
        Ok(())
    }

    // ========================== TRANSMISSION METHODS ==========================

    /// Transmit data packet
    ///
    /// Loads the provided data into the radio buffer and initiates transmission.
    /// This is a complete transmission operation that handles buffer loading,
    /// mode switching, and completion detection. Performs a single LBT check before TX.
    ///
    /// # Arguments
    ///
    /// * `data` - Data to transmit (up to 255 bytes)
    /// * `lbt_config` - LBT configuration for channel check (threshold, duration)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Transmission completed successfully
    /// * `Err(DriverError)` - Transmission failed (e.g., channel busy, timeout)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use mbus_rs::wmbus::radio::driver::LbtConfig;
    /// # use mbus_rs::wmbus::radio::driver::{Sx126xDriver, DriverError};
    /// # use mbus_rs::wmbus::radio::hal::MockHal;
    /// # fn main() -> Result<(), DriverError> {
    /// # let mut driver = Sx126xDriver::new(MockHal::new(), 32_000_000);
    /// let lbt_config = LbtConfig::default(); // EU compliant settings
    /// let wmbus_frame = [0x44, 0x12, 0x34, 0x56, 0x78]; // Example frame
    /// driver.transmit(&wmbus_frame, &lbt_config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn transmit(&mut self, data: &[u8], lbt_config: &LbtConfig) -> Result<(), DriverError> {
        if data.len() > 255 {
            return Err(DriverError::InvalidParams);
        }

        // Ensure we're in a valid state for transmission (standby mode)
        let current_state = self.get_state()?;
        if !matches!(
            current_state,
            RadioState::StandbyRc | RadioState::StandbyXosc
        ) {
            return Err(DriverError::WrongState {
                expected: RadioState::StandbyRc,
                actual: current_state,
            });
        }

        // Perform Listen Before Talk (LBT) check for ETSI compliance
        // Default threshold is -85 dBm per ETSI EN 300 220-1

        // Switch to RX mode briefly to measure RSSI
        self.set_rx(0)?;
        std::thread::sleep(Duration::from_millis(1)); // Allow RX to stabilize

        if !self.check_channel_clear(lbt_config)? {
            let rssi = self.get_rssi_instant()?;
            log::warn!(
                "Channel busy: RSSI {} dBm exceeds threshold {} dBm",
                rssi,
                lbt_config.rssi_threshold_dbm
            );
            return Err(DriverError::ChannelBusy {
                rssi_dbm: rssi,
                threshold_dbm: lbt_config.rssi_threshold_dbm,
            });
        }

        // Return to standby mode after LBT check
        self.set_standby(StandbyMode::RC)?;

        // Load data into radio buffer
        self.write_buffer(self.tx_base_addr, data)?;

        // Start transmission with reasonable timeout (1 second)
        self.set_tx(1000)?;

        // Wait for transmission to complete
        log::info!("Transmitting {} bytes", data.len());

        // Poll for TX completion
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            let irq_status = self.get_irq_status()?;

            if irq_status.tx_done() {
                // Clear TX done interrupt
                self.clear_irq_status(IrqMaskBit::TxDone as u16)?;
                log::info!("Transmission completed successfully");
                return Ok(());
            }

            if irq_status.timeout() {
                // Clear timeout interrupt
                self.clear_irq_status(IrqMaskBit::Timeout as u16)?;
                log::error!("Transmission timeout");
                return Err(DriverError::Timeout);
            }

            std::thread::sleep(Duration::from_millis(1));
        }

        Err(DriverError::Timeout)
    }

    // ========================== LBT (LISTEN BEFORE TALK) METHODS ==========================

    /// Check if the channel is clear for transmission
    ///
    /// Performs Listen Before Talk (LBT) check by measuring RSSI and comparing
    /// against the configured threshold. Required for regulatory compliance in many regions.
    ///
    /// # Arguments
    ///
    /// * `config` - LBT configuration (RSSI threshold, listen duration)
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Channel is clear for transmission
    /// * `Ok(false)` - Channel is busy
    /// * `Err(DriverError)` - LBT check failed
    pub fn check_channel_clear(&mut self, config: &LbtConfig) -> Result<bool, DriverError> {
        // Enter RX mode for RSSI measurement
        self.set_rx_continuous()?;

        // Wait for RSSI to settle (typical settling time is a few hundred microseconds)
        std::thread::sleep(Duration::from_millis(config.listen_duration_ms as u64));

        // Measure instantaneous RSSI
        let rssi_dbm = self.get_rssi_instant()?;

        log::debug!(
            "LBT check: RSSI = {} dBm, threshold = {} dBm",
            rssi_dbm,
            config.rssi_threshold_dbm
        );

        // Channel is clear if RSSI is below threshold
        let channel_clear = rssi_dbm < config.rssi_threshold_dbm;

        if !channel_clear {
            log::debug!(
                "Channel busy: RSSI {} dBm above threshold {} dBm",
                rssi_dbm,
                config.rssi_threshold_dbm
            );
        }

        Ok(channel_clear)
    }

    /// Transmit with Listen Before Talk (LBT) compliance
    ///
    /// Performs LBT check before transmission to ensure regulatory compliance.
    /// Will retry transmission if channel is initially busy.
    ///
    /// # Arguments
    ///
    /// * `data` - Data to transmit
    /// * `lbt_config` - LBT configuration parameters
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Transmission completed successfully
    /// * `Err(DriverError::ChannelBusy)` - Channel remained busy after all retries
    /// * `Err(DriverError)` - Other transmission error
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use mbus_rs::wmbus::radio::driver::LbtConfig;
    /// # use mbus_rs::wmbus::radio::driver::{Sx126xDriver, DriverError};
    /// # use mbus_rs::wmbus::radio::hal::MockHal;
    /// # fn main() -> Result<(), DriverError> {
    /// # let mut driver = Sx126xDriver::new(MockHal::new(), 32_000_000);
    /// # let wmbus_frame = [0x44, 0x12, 0x34, 0x56, 0x78];
    /// let lbt_config = LbtConfig::default(); // EU compliant settings
    /// driver.lbt_transmit(&wmbus_frame, lbt_config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn lbt_transmit(&mut self, data: &[u8], lbt_config: LbtConfig) -> Result<(), DriverError> {
        for attempt in 0..=lbt_config.max_retries {
            // Check if channel is clear
            let channel_clear = self.check_channel_clear(&lbt_config)?;

            if channel_clear {
                // Channel is clear, proceed with transmission
                log::debug!("LBT: Channel clear, transmitting (attempt {})", attempt + 1);
                return self.transmit(data, &lbt_config);
            } else {
                // Channel is busy
                if attempt < lbt_config.max_retries {
                    // Exponential backoff before retry
                    let backoff_ms = 10 * (1 << attempt); // 10ms, 20ms, 40ms, etc.
                    log::debug!("LBT: Channel busy, backing off for {backoff_ms}ms");
                    std::thread::sleep(Duration::from_millis(backoff_ms));
                } else {
                    // All retries exhausted
                    let rssi_dbm = self.get_rssi_instant()?;
                    return Err(DriverError::ChannelBusy {
                        rssi_dbm,
                        threshold_dbm: lbt_config.rssi_threshold_dbm,
                    });
                }
            }
        }

        unreachable!("Should have returned from loop")
    }

    // ========================== STATISTICS METHODS ==========================

    /// Get radio statistics
    ///
    /// Retrieves packet and error statistics from the radio. Useful for monitoring
    /// link quality and debugging reception issues.
    ///
    /// # Returns
    ///
    /// * `Ok(RadioStats)` - Current radio statistics
    /// * `Err(DriverError)` - Read failed
    pub fn get_stats(&mut self) -> Result<RadioStats, DriverError> {
        let mut stats = [0u8; 6];
        self.hal.read_command(0x10, &mut stats)?; // GetStats command

        let radio_stats = RadioStats {
            packets_received: ((stats[0] as u16) << 8) | (stats[1] as u16),
            packets_crc_error: ((stats[2] as u16) << 8) | (stats[3] as u16),
            packets_length_error: ((stats[4] as u16) << 8) | (stats[5] as u16),
        };

        Ok(radio_stats)
    }

    /// Clear radio statistics counters
    ///
    /// Resets all packet counters to zero. Useful for starting fresh measurements
    /// during testing or after configuration changes.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Statistics cleared successfully
    /// * `Err(DriverError)` - Clear operation failed
    pub fn clear_stats(&mut self) -> Result<(), DriverError> {
        // ClearStats command (opcode 0x00, no parameters)
        self.hal.write_command(0x00, &[])?;
        Ok(())
    }

    /// Reset radio statistics to initial values
    ///
    /// This is an alias for `clear_stats()` for compatibility with some documentation
    /// that refers to this operation as "reset".
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Statistics reset successfully
    /// * `Err(DriverError)` - Reset operation failed
    pub fn reset_stats(&mut self) -> Result<(), DriverError> {
        self.clear_stats()
    }

    // ========================== UTILITY METHODS ==========================

    /// Configure radio for LoRa operation
    ///
    /// Sets up LoRa modulation and packet parameters for optimal LoRa reception/transmission.
    /// Includes common defaults for non-LoRaWAN use (e.g., explicit header, CRC on).
    ///
    /// # Arguments
    ///
    /// * `frequency_hz` - Operating frequency in Hz (e.g., 868_100_000 for EU LoRa)
    /// * `sf` - Spreading factor (SF7-SF12)
    /// * `bw` - Bandwidth (e.g., BW125 for long range)
    /// * `cr` - Coding rate (4/5 to 4/8)
    /// * `power_dbm` - TX power in dBm (e.g., 14)
    ///
    /// # Returns
    ///
    /// * `Ok(())` on successful configuration
    /// * `Err(DriverError)` if configuration fails
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mbus_rs::wmbus::radio::modulation::{SpreadingFactor, LoRaBandwidth, CodingRate};
    /// # use mbus_rs::wmbus::radio::driver::{Sx126xDriver, DriverError};
    /// # use mbus_rs::wmbus::radio::hal::MockHal;
    ///
    /// # fn main() -> Result<(), DriverError> {
    /// # let mut driver = Sx126xDriver::new(MockHal::new(), 32_000_000);
    /// // Long range LoRa (SF12, 125kHz BW, 4/5 CR)
    /// driver.configure_for_lora(
    ///     868_100_000,   // EU 868.1 MHz
    ///     SpreadingFactor::SF12,
    ///     LoRaBandwidth::BW125,
    ///     CodingRate::CR4_5,
    ///     14,            // 14 dBm TX power
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn configure_for_lora(
        &mut self,
        frequency_hz: u32,
        sf: SpreadingFactor,
        bw: LoRaBandwidth,
        cr: CodingRate,
        power_dbm: i8,
    ) -> Result<(), DriverError> {
        // Select the LoRa modem before applying LoRa modulation/packet params.
        self.set_packet_type(PacketType::LoRa)?;
        self.apply_lora_profile(&LoRaProfile {
            frequency_hz,
            sf,
            bw,
            cr,
            power_dbm,
            sync_word: None,
        })
    }

    /// Apply the parameter set for a LoRa profile: modulation (with automatic LDRO),
    /// packet params, frequency, optional sync word, PA/TX, buffers and IRQ routing.
    ///
    /// Assumes the LoRa packet type is already selected and the radio is in standby. Both
    /// [`Sx126xDriver::configure_for_lora`] and [`Sx126xDriver::switch_profile`] funnel
    /// through here, so there is a single LoRa-apply path.
    pub fn apply_lora_profile(&mut self, profile: &LoRaProfile) -> Result<(), DriverError> {
        let LoRaProfile {
            frequency_hz,
            sf,
            bw,
            cr,
            power_dbm,
            sync_word,
        } = *profile;

        // Set RF frequency
        self.set_rf_frequency(frequency_hz)?;

        // Configure LoRa modulation parameters
        // Per AN1200.22: Enable LDRO for SF11/SF12 when BW <= 125kHz
        let ldro_needed = matches!(sf, SpreadingFactor::SF11 | SpreadingFactor::SF12)
            && matches!(
                bw,
                LoRaBandwidth::BW7_8
                    | LoRaBandwidth::BW10_4
                    | LoRaBandwidth::BW15_6
                    | LoRaBandwidth::BW20_8
                    | LoRaBandwidth::BW31_2
                    | LoRaBandwidth::BW41_7
                    | LoRaBandwidth::BW62_5
                    | LoRaBandwidth::BW125
            );

        let mod_params = ModulationParams::LoRa {
            params: LoRaModParams {
                sf,
                bw,
                cr,
                low_data_rate_optimize: ldro_needed,
            },
        };
        self.set_modulation_params(mod_params)?;

        // Configure LoRa packet parameters (explicit header, CRC on, standard preamble)
        let packet_params = PacketParams::LoRa {
            params: LoRaPacketParams {
                preamble_len: 8,        // Standard 8-symbol preamble
                implicit_header: false, // Explicit header for non-WAN
                payload_len: 255,       // Max payload
                crc_on: true,           // Enable CRC
                iq_inverted: false,     // Standard IQ
            },
        };
        self.set_packet_params(packet_params)?;

        // Optional LoRa sync word (network id) for private-network filtering.
        if let Some(network_id) = sync_word {
            self.set_lora_sync_word(network_id)?;
        }

        // Configure power amplifier
        self.set_pa_config(0x04, 0x00, 0x00)?; // Standard PA config
        self.set_tx_params(power_dbm, 0x07)?; // Power and ramp time

        // Set buffer base addresses
        self.set_buffer_base_addresses(0, 0)?;

        // Configure interrupt routing for LoRa (RxDone, HeaderValid, CrcErr)
        self.set_dio_irq_params(
            // IRQ mask: RxDone, HeaderValid, CrcErr, Timeout
            IrqMaskBit::RxDone as u16
                | IrqMaskBit::HeaderValid as u16
                | IrqMaskBit::CrcErr as u16
                | IrqMaskBit::Timeout as u16,
            IrqMaskBit::RxDone as u16 | IrqMaskBit::HeaderValid as u16, // DIO1: Rx events
            0,                                                          // DIO2: unused for LoRa
            0,                                                          // DIO3: unused
        )?;

        log::info!("LoRa configured: SF{sf:?}, BW{bw:?}, CR{cr:?}, Power {power_dbm} dBm");
        Ok(())
    }

    /// Set LoRa sync word for network identification
    ///
    /// Configures the LoRa sync word (network ID) for filtering packets.
    /// Use 0x34 for public LoRaWAN, custom values for private networks.
    ///
    /// # Arguments
    ///
    /// * `network_id` - 16-bit network ID (sync word)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Sync word set successfully
    /// * `Err(DriverError)` - Register write failed
    pub fn set_lora_sync_word(&mut self, network_id: u16) -> Result<(), DriverError> {
        // Private LoRa sync word register (0x0741)
        let buf = [network_id as u8, (network_id >> 8) as u8];
        self.hal.write_register(0x0741, &buf)?;
        log::info!("LoRa sync word set to 0x{network_id:04X}");
        Ok(())
    }

    /// Switch radio to LoRa mode
    ///
    /// Convenience method to configure the radio for LoRa operation.
    /// Sets packet type to LoRa and updates internal state.
    pub fn switch_to_lora_mode(&mut self) -> Result<(), DriverError> {
        self.set_packet_type(PacketType::LoRa)?;
        log::debug!("Switched to LoRa mode");
        Ok(())
    }

    /// Switch radio to GFSK mode
    ///
    /// Convenience method that only issues `SetPacketType(GFSK)`. It does **not** reapply
    /// modulation/packet/frequency/sync state, so it cannot by itself restore a working
    /// wM-Bus configuration after LoRa. For a reliable mode change use
    /// [`Sx126xDriver::switch_profile`].
    pub fn switch_to_gfsk_mode(&mut self) -> Result<(), DriverError> {
        self.set_packet_type(PacketType::Gfsk)?;
        log::debug!("Switched to GFSK mode");
        Ok(())
    }

    /// Atomically switch the radio to a complete [`RadioProfile`].
    ///
    /// Runs the full ordered sequence needed to change modem/profile safely on a single
    /// SX126x:
    ///
    /// 1. force **standby** — `SetPacketType` and reconfiguration are only valid in standby;
    /// 2. select the packet (modem) type (also updates `current_packet_type`);
    /// 3. apply the entire profile — modulation, packet params, frequency, optional sync,
    ///    PA/TX, buffers and IRQ routing — via [`Sx126xDriver::apply_wmbus_profile`] /
    ///    [`Sx126xDriver::apply_lora_profile`].
    ///
    /// The radio is left in **standby**; the caller then issues
    /// [`start_receive`](crate::wmbus::radio::radio_driver::RadioDriver::start_receive) or a
    /// transmit. Unlike [`Sx126xDriver::switch_to_gfsk_mode`] /
    /// [`Sx126xDriver::switch_to_lora_mode`] (which only flip the packet type), this restores
    /// the complete radio state, so a GFSK→LoRa→GFSK round-trip is reliable.
    pub fn switch_profile(&mut self, profile: &RadioProfile) -> Result<(), DriverError> {
        // 1. Standby — required before changing packet type / reconfiguring.
        self.set_standby(StandbyMode::RC)?;
        // 2. Select the modem type (also records `current_packet_type`).
        self.set_packet_type(profile.packet_type())?;
        // 3. Apply the complete parameter set for the chosen profile.
        match profile {
            RadioProfile::Wmbus(p) => self.apply_wmbus_profile(p)?,
            RadioProfile::LoRa(p) => self.apply_lora_profile(p)?,
        }
        log::info!("Radio profile switched to {:?}", profile.packet_type());
        Ok(())
    }

    // ========================== PERFORMANCE ENHANCEMENT METHODS ==========================

    /// Sets receiver gain mode for improved sensitivity
    ///
    /// From SX126x Development Kit User Guide Fig. 12: RxBoost provides +6dB sensitivity
    /// improvement at the cost of +20mA current consumption (25mA vs 4.6mA).
    /// Recommended for noisy urban environments or when using SF >= 10.
    ///
    /// # Arguments
    ///
    /// * `enabled` - true for boosted gain (+6dB), false for normal gain
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Gain mode set successfully
    /// * `Err(DriverError)` - Configuration failed
    pub fn set_rx_boosted_gain(&mut self, enabled: bool) -> Result<(), DriverError> {
        self.set_standby(StandbyMode::RC)?;

        // RegRxGain (0x08AC): 0x96 for boost, 0x94 for normal
        let gain_value = if enabled { 0x96 } else { 0x94 };
        self.hal.write_register(0x08AC, &[gain_value])?;

        log::info!(
            "RX gain mode: {} ({}mA current)",
            if enabled { "boosted (+6dB)" } else { "normal" },
            if enabled { "25" } else { "4.6" }
        );
        Ok(())
    }

    /// Run the chip's self-calibration.
    ///
    /// `0x7F` calibrates every block (RC64k, RC13M, PLL, ADC bulk/pulse/tuning, image).
    /// The power-on calibration happens before the chip knows its final configuration,
    /// and an uncalibrated ADC or PLL leaves the receiver reporting a healthy RSSI while
    /// the demodulator never recovers a single bit.
    pub fn calibrate(&mut self, blocks: u8) -> Result<(), DriverError> {
        self.set_standby(StandbyMode::RC)?;
        self.hal
            .write_command(0x89, &[blocks])
            .map_err(DriverError::Hal)?;
        // Calibration drives BUSY high for a few ms. No sleep here: the HAL waits for
        // BUSY to fall before issuing the next command, which is the authoritative
        // signal and does not stall callers that are driving a mock.
        Ok(())
    }

    /// Calibrate the image rejection for the band containing `frequency_hz`.
    ///
    /// Required by the datasheet whenever the operating band changes: the
    /// calibration performed at reset is for the default band, and running outside
    /// it costs receive sensitivity. Band edges are the datasheet's (Table 9-3).
    pub fn calibrate_image(&mut self, frequency_hz: u32) -> Result<(), DriverError> {
        let mhz = frequency_hz / 1_000_000;
        let (f1, f2) = match mhz {
            430..=440 => (0x6B, 0x6F),
            470..=510 => (0x75, 0x81),
            779..=787 => (0xC1, 0xC5),
            863..=870 => (0xD7, 0xDB),
            902..=928 => (0xE1, 0xE9),
            _ => return Ok(()), // outside the documented bands: leave calibration alone
        };
        self.hal
            .write_command(0x98, &[f1, f2])
            .map_err(DriverError::Hal)
    }

    /// Let the chip drive DIO2 as the antenna-switch control.
    ///
    /// Boards with an external RF switch (e.g. a PE4259) route DIO2 to it, so without
    /// this the antenna is never connected to the receiver and the radio hears
    /// nothing while looking perfectly healthy.
    pub fn set_dio2_as_rf_switch(&mut self, enabled: bool) -> Result<(), DriverError> {
        self.hal
            .write_command(0x9D, &[u8::from(enabled)])
            .map_err(DriverError::Hal)
    }

    /// Sets regulator mode for optimal power efficiency
    ///
    /// From AN1200.37: Use DC-DC regulator for TX power >+15dBm or long packets
    /// to reduce heat generation and frequency drift by up to 50%.
    /// LDO mode provides lower noise but higher power consumption.
    ///
    /// # Arguments
    ///
    /// * `use_dcdc` - true for DC-DC mode (efficient), false for LDO mode (low noise)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Regulator mode set successfully
    /// * `Err(DriverError)` - Configuration failed
    pub fn set_regulator_mode(&mut self, use_dcdc: bool) -> Result<(), DriverError> {
        self.set_standby(StandbyMode::RC)?;

        // SetRegulatorMode command (0x96)
        let mode = if use_dcdc { 0x01 } else { 0x00 };
        self.hal.write_command(0x96, &[mode])?;

        log::info!(
            "Regulator mode: {}",
            if use_dcdc {
                "DC-DC (efficient, lower drift)"
            } else {
                "LDO (low noise)"
            }
        );
        Ok(())
    }

    /// Configures external Temperature Compensated Crystal Oscillator (TCXO)
    ///
    /// From AN1200.37: TCXO provides frequency stability across -40°C to +85°C
    /// with typical ±2ppm accuracy. Essential for outdoor/industrial deployments.
    /// Requires external TCXO hardware connected to DIO3.
    ///
    /// # Arguments
    ///
    /// * `voltage_mv` - TCXO supply voltage in millivolts (1600-3600mV)
    /// * `startup_time_us` - TCXO startup time in microseconds (typically 1000-5000µs)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - TCXO configured successfully
    /// * `Err(DriverError)` - Invalid parameters or configuration failed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use mbus_rs::wmbus::radio::driver::{Sx126xDriver, DriverError};
    /// # use mbus_rs::wmbus::radio::hal::MockHal;
    /// # fn main() -> Result<(), DriverError> {
    /// # let mut driver = Sx126xDriver::new(MockHal::new(), 32_000_000);
    /// // Configure for 3.3V TCXO with 1ms startup time
    /// driver.configure_tcxo(3300, 1000)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn configure_tcxo(
        &mut self,
        voltage_mv: u16,
        startup_time_us: u16,
    ) -> Result<(), DriverError> {
        if !(1600..=3600).contains(&voltage_mv) {
            return Err(DriverError::InvalidParams);
        }

        self.set_standby(StandbyMode::RC)?;

        // Calculate TCXO trim value (1.6V=0x00, 1.7V=0x01, ... 3.3V=0x07)
        let trim = ((voltage_mv.saturating_sub(1600)) / 200).min(7) as u8;

        // Convert startup time to 15.625µs units (24-bit value)
        let delay_units = ((startup_time_us as u32) * 64 / 1000) & 0xFFFFFF;

        let buf = [
            trim & 0x07,
            (delay_units >> 16) as u8,
            (delay_units >> 8) as u8,
            delay_units as u8,
        ];

        // SetDio3AsTcxoCtrl command (0x97)
        self.hal.write_command(0x97, &buf)?;

        log::info!("TCXO configured: {voltage_mv}mV supply, {startup_time_us}µs startup");
        Ok(())
    }

    /// Configure LoRa modulation with enhanced features
    ///
    /// Combines standard LoRa configuration with optional performance enhancements
    /// based on Semtech application notes. Automatically enables optimizations
    /// when `auto_optimize` is true.
    ///
    /// # Arguments
    ///
    /// * `freq_hz` - RF frequency in Hz
    /// * `sf` - Spreading Factor (SF5-SF12)
    /// * `bw` - Bandwidth
    /// * `cr` - Coding Rate
    /// * `tx_power_dbm` - Transmit power in dBm
    /// * `auto_optimize` - Enable automatic optimizations (RX boost for SF>=10, DC-DC for >15dBm)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Configuration successful
    /// * `Err(DriverError)` - Configuration failed
    pub fn configure_for_lora_enhanced(
        &mut self,
        freq_hz: u32,
        sf: crate::wmbus::radio::modulation::SpreadingFactor,
        bw: crate::wmbus::radio::modulation::LoRaBandwidth,
        cr: crate::wmbus::radio::modulation::CodingRate,
        tx_power_dbm: i8,
        auto_optimize: bool,
    ) -> Result<(), DriverError> {
        use crate::wmbus::radio::modulation::{
            LoRaModParams, LoRaPacketParams, ModulationParams, PacketParams,
        };

        // Set packet type to LoRa
        self.set_packet_type(PacketType::LoRa)?;

        // Configure modulation parameters
        // Check if LDRO is needed (SF11/SF12 with BW <= 125kHz)
        let ldro = matches!(
            sf,
            crate::wmbus::radio::modulation::SpreadingFactor::SF11
                | crate::wmbus::radio::modulation::SpreadingFactor::SF12
        ) && matches!(
            bw,
            crate::wmbus::radio::modulation::LoRaBandwidth::BW7_8
                | crate::wmbus::radio::modulation::LoRaBandwidth::BW10_4
                | crate::wmbus::radio::modulation::LoRaBandwidth::BW15_6
                | crate::wmbus::radio::modulation::LoRaBandwidth::BW20_8
                | crate::wmbus::radio::modulation::LoRaBandwidth::BW31_2
                | crate::wmbus::radio::modulation::LoRaBandwidth::BW41_7
                | crate::wmbus::radio::modulation::LoRaBandwidth::BW62_5
                | crate::wmbus::radio::modulation::LoRaBandwidth::BW125
        );

        let lora_mod_params = LoRaModParams {
            sf,
            bw,
            cr,
            low_data_rate_optimize: ldro,
        };
        let mod_params = ModulationParams::LoRa {
            params: lora_mod_params,
        };
        self.set_modulation_params(mod_params)?;

        // Configure packet parameters (standard defaults)
        let lora_packet_params = LoRaPacketParams {
            preamble_len: 8,
            implicit_header: false,
            payload_len: 255,
            crc_on: true,
            iq_inverted: false,
        };
        let packet_params = PacketParams::LoRa {
            params: lora_packet_params,
        };
        self.set_packet_params(packet_params)?;

        // Set frequency
        self.set_rf_frequency(freq_hz)?;

        // Set TX power
        self.set_tx_params(tx_power_dbm, 0x04)?; // 0x04 = 200µs ramp time

        if auto_optimize {
            // Auto-enable RX boost for long-range SF
            let rx_boost = sf as u8 >= 10;
            if rx_boost {
                self.set_rx_boosted_gain(true)?;
                log::debug!("Auto-enabled RX boost for SF{}", sf as u8);
            }

            // Auto-enable DC-DC for high power
            if tx_power_dbm > 15 {
                self.set_regulator_mode(true)?;
                log::debug!("Auto-enabled DC-DC regulator for {tx_power_dbm}dBm TX power");
            }
        }

        log::info!(
            "LoRa configured: {:.3}MHz, SF{}, BW{:?}, CR{:?}, {}dBm{}",
            freq_hz as f64 / 1_000_000.0,
            sf as u8,
            bw,
            cr,
            tx_power_dbm,
            if auto_optimize { " (optimized)" } else { "" }
        );

        Ok(())
    }

    // ========================== CAD (CHANNEL ACTIVITY DETECTION) METHODS ==========================

    /// Sets Channel Activity Detection parameters
    ///
    /// From AN1200.48: CAD provides 50-80% better detection accuracy than RSSI
    /// for LoRa signals with typical detection time of 1-2ms.
    ///
    /// # Arguments
    ///
    /// * `params` - CAD configuration parameters
    ///
    /// # Returns
    ///
    /// * `Ok(())` - CAD parameters set successfully
    /// * `Err(DriverError)` - Configuration failed
    pub fn set_cad_params(
        &mut self,
        params: &crate::wmbus::radio::lora::LoRaCadParams,
    ) -> Result<(), DriverError> {
        self.set_standby(StandbyMode::RC)?;

        // SetCadParams command (0x88)
        // Format: SymbolNum(1), DetPeak(1), DetMin(1), ExitMode(1), Timeout(3)
        let buf = [
            params.symbol_num,
            params.det_peak,
            params.det_min,
            params.exit_mode as u8,
            0x00,
            0x00,
            0x00, // 24-bit timeout (0 = indefinite)
        ];

        self.hal.write_command(0x88, &buf)?;

        log::debug!(
            "CAD configured: {} symbols, peak={}, min={}, mode={:?}",
            params.symbol_num,
            params.det_peak,
            params.det_min,
            params.exit_mode
        );
        Ok(())
    }

    /// Performs Channel Activity Detection
    ///
    /// Executes a single CAD operation to detect LoRa signals on the current
    /// frequency. Returns whether activity was detected.
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - LoRa activity detected
    /// * `Ok(false)` - No activity detected (channel clear)
    /// * `Err(DriverError)` - CAD operation failed
    pub fn perform_cad(&mut self) -> Result<bool, DriverError> {
        // Start CAD operation (0xC5)
        self.hal.write_command(0xC5, &[])?;

        // Wait for CAD completion (typical 1-10ms depending on symbols)
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(100);

        loop {
            let irq_status = self.get_irq_status()?;

            if irq_status.cad_done() {
                let detected = irq_status.cad_detected();
                self.clear_irq_status(0x0180)?; // Clear CAD bits (7 and 8)

                log::debug!(
                    "CAD complete: {} ({}ms)",
                    if detected {
                        "activity detected"
                    } else {
                        "channel clear"
                    },
                    start.elapsed().as_millis()
                );
                return Ok(detected);
            }

            if start.elapsed() > timeout {
                return Err(DriverError::Timeout);
            }

            std::thread::sleep(std::time::Duration::from_micros(100));
        }
    }

    /// Performs CAD-based Listen Before Talk
    ///
    /// Uses optimal CAD parameters for the given SF/BW combination to check
    /// for channel activity before transmission. Provides 50-80% better
    /// accuracy than RSSI-based LBT with faster detection time.
    ///
    /// # Arguments
    ///
    /// * `sf` - Spreading Factor
    /// * `bw` - Bandwidth
    /// * `retries` - Number of CAD attempts if activity detected
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Channel is clear for transmission
    /// * `Ok(false)` - Channel is busy
    /// * `Err(DriverError)` - LBT check failed
    pub fn cad_lbt(
        &mut self,
        sf: crate::wmbus::radio::modulation::SpreadingFactor,
        bw: crate::wmbus::radio::modulation::LoRaBandwidth,
        retries: u8,
    ) -> Result<bool, DriverError> {
        use crate::wmbus::radio::lora::LoRaCadParams;

        // Use optimal CAD parameters from AN1200.48
        let cad_params = LoRaCadParams::optimal(sf, bw);
        self.set_cad_params(&cad_params)?;

        // Perform CAD with retries
        for attempt in 0..=retries {
            if !self.perform_cad()? {
                // Channel clear
                return Ok(true);
            }

            if attempt < retries {
                log::debug!("CAD detected activity, retry {}/{}", attempt + 1, retries);
                // Wait before retry (backoff)
                std::thread::sleep(std::time::Duration::from_millis(10 * (attempt as u64 + 1)));
            }
        }

        log::debug!("Channel busy after {} CAD attempts", retries + 1);
        Ok(false)
    }
}

// Implementation of the RadioDriver trait for SX126x
#[async_trait::async_trait]
impl<H: Hal + Send + Sync> crate::wmbus::radio::radio_driver::RadioDriver for Sx126xDriver<H> {
    async fn initialize(
        &mut self,
        config: crate::wmbus::radio::radio_driver::WMBusConfig,
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        // Configure for wM-Bus using the existing method
        self.configure_for_wmbus(config.frequency_hz, config.bitrate)
            .map_err(|e| {
                crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                    "SX126x init failed: {e}"
                ))
            })
    }

    async fn start_receive(
        &mut self,
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        self.set_rx_continuous().map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to start RX: {e}"
            ))
        })
    }

    async fn stop_receive(
        &mut self,
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        self.set_standby(StandbyMode::RC).map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to stop RX: {e}"
            ))
        })
    }

    async fn transmit(
        &mut self,
        data: &[u8],
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        let lbt_config = LbtConfig::default();
        self.transmit(data, &lbt_config).map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Transmission failed: {e}"
            ))
        })
    }

    async fn get_received_packet(
        &mut self,
    ) -> Result<
        Option<crate::wmbus::radio::radio_driver::ReceivedPacket>,
        crate::wmbus::radio::radio_driver::RadioDriverError,
    > {
        match self.process_irqs().map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "IRQ processing failed: {e}"
            ))
        })? {
            Some(data) => {
                // RSSI from GetPacketStatus; per-packet frequency error from the LoRa modem
                // registers (None in GFSK — the SX126x has no per-packet FEI there).
                let rssi_pkt = self
                    .get_packet_status()
                    .map(|s| s.rssi_pkt_dbm)
                    .unwrap_or(-80);
                let freq_error_hz = self.get_lora_frequency_error().unwrap_or(None);

                let packet = crate::wmbus::radio::radio_driver::ReceivedPacket {
                    data,
                    rssi_dbm: rssi_pkt,
                    freq_error_hz,
                    lqi: None,       // SX126x doesn't provide LQI
                    crc_valid: true, // process_irqs only returns packets with valid CRC
                };
                Ok(Some(packet))
            }
            None => Ok(None),
        }
    }

    async fn get_stats(
        &mut self,
    ) -> Result<
        crate::wmbus::radio::radio_driver::RadioStats,
        crate::wmbus::radio::radio_driver::RadioDriverError,
    > {
        let stats = self.get_stats().map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to get stats: {e}"
            ))
        })?;

        let rssi = self.get_rssi_instant().unwrap_or(-80);

        Ok(crate::wmbus::radio::radio_driver::RadioStats {
            packets_received: stats.packets_received as u32,
            packets_crc_valid: stats.packets_received as u32 - stats.packets_crc_error as u32,
            packets_crc_error: stats.packets_crc_error as u32,
            packets_length_error: stats.packets_length_error as u32,
            last_rssi_dbm: rssi,
        })
    }

    async fn reset_stats(
        &mut self,
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        self.reset_stats().map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to reset stats: {e}"
            ))
        })
    }

    async fn get_mode(
        &mut self,
    ) -> Result<
        crate::wmbus::radio::radio_driver::RadioMode,
        crate::wmbus::radio::radio_driver::RadioDriverError,
    > {
        let state = self.get_state().map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to get state: {e}"
            ))
        })?;

        let mode = match state {
            RadioState::Sleep => crate::wmbus::radio::radio_driver::RadioMode::Sleep,
            RadioState::StandbyRc | RadioState::StandbyXosc | RadioState::FreqSynth => {
                crate::wmbus::radio::radio_driver::RadioMode::Standby
            }
            RadioState::Tx => crate::wmbus::radio::radio_driver::RadioMode::Transmit,
            RadioState::Rx => crate::wmbus::radio::radio_driver::RadioMode::Receive,
        };
        Ok(mode)
    }

    async fn sleep(&mut self) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        self.set_sleep(SleepConfig::default()).map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to sleep: {e}"
            ))
        })
    }

    async fn wake_up(&mut self) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        self.set_standby(StandbyMode::RC).map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to wake up: {e}"
            ))
        })
    }

    async fn get_rssi(
        &mut self,
    ) -> Result<i16, crate::wmbus::radio::radio_driver::RadioDriverError> {
        self.get_rssi_instant().map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to get RSSI: {e}"
            ))
        })
    }

    async fn is_channel_clear(
        &mut self,
        threshold_dbm: i16,
        listen_duration: std::time::Duration,
    ) -> Result<bool, crate::wmbus::radio::radio_driver::RadioDriverError> {
        let lbt_config = LbtConfig {
            rssi_threshold_dbm: threshold_dbm,
            listen_duration_ms: listen_duration.as_millis() as u32,
            max_retries: 0, // Just check once
        };

        self.check_channel_clear(&lbt_config).map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "LBT check failed: {e}"
            ))
        })
    }

    fn get_driver_info(&self) -> crate::wmbus::radio::radio_driver::DriverInfo {
        crate::wmbus::radio::radio_driver::DriverInfo {
            name: "SX126x".to_string(),
            version: "1.0.0".to_string(),
            frequency_bands: vec![
                (150_000_000, 960_000_000), // SX126x full range
            ],
            max_packet_size: 255,
            supported_bitrates: vec![100_000, 50_000, 32_768, 25_000, 10_000],
            power_range_dbm: (-17, 22), // SX126x power range
            features: vec![
                "GFSK".to_string(),
                "LoRa".to_string(),
                "wM-Bus".to_string(),
                "LBT".to_string(),
                "Variable_Length".to_string(),
                "CRC".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{lora_freq_error_hz, lora_snr_db};

    #[test]
    fn lora_freq_error_zero_is_zero() {
        assert_eq!(lora_freq_error_hz([0x00, 0x00, 0x00], 125.0), 0);
    }

    #[test]
    fn lora_freq_error_positive() {
        // raw = 0x001000 = 4096; 1.55 * 4096 / (1600/125) = 1.55 * 4096 / 12.8 = 496 Hz.
        assert_eq!(lora_freq_error_hz([0x00, 0x10, 0x00], 125.0), 496);
    }

    #[test]
    fn lora_freq_error_sign_extends_negative() {
        // Bit 19 set -> negative. Only the low nibble of byte 0 is significant, so the high
        // nibble (0xF0) is ignored: raw20 = 0x0F0000 -> sign-extended = -65536.
        // 1.55 * -65536 / 12.8 = -7936 Hz.
        assert_eq!(lora_freq_error_hz([0xFF, 0x00, 0x00], 125.0), -7936);
    }

    #[test]
    fn lora_freq_error_scales_with_bandwidth() {
        // Same raw value scales linearly with bandwidth: 500 kHz is 4x the 125 kHz result.
        let bw125 = lora_freq_error_hz([0x00, 0x10, 0x00], 125.0);
        let bw500 = lora_freq_error_hz([0x00, 0x10, 0x00], 500.0);
        assert_eq!(bw500, bw125 * 4);
    }

    #[test]
    fn lora_snr_positive_raw() {
        // SnrPkt is a signed byte in quarter-dB: 0x14 = 20 -> +5.0 dB.
        assert_eq!(lora_snr_db(0x14), 5.0);
        assert_eq!(lora_snr_db(0x00), 0.0);
    }

    #[test]
    fn lora_snr_negative_raw() {
        // Two's-complement negatives: 0xFC = -4 -> -1.0 dB; 0x80 = -128 -> -32.0 dB.
        assert_eq!(lora_snr_db(0xFC), -1.0);
        assert_eq!(lora_snr_db(0x80), -32.0);
    }

    // TODO: Implement mock HAL for comprehensive testing
    // The LBT functionality is integrated in the transmit() function
    // and will be tested during hardware integration tests

    #[test]
    #[ignore] // Requires mock HAL implementation
    fn test_transmit_lbt_channel_busy() {
        // TODO: Test implementation requires mock HAL
        /*
        // Create a mock HAL that reports high RSSI (channel busy)
        let mut mock_hal = MockHal::new();

        // Set up mock to return high RSSI when read_command is called with 0x15 (GetRssiInst)
        mock_hal.expect_read_command()
            .withf(|cmd, _| *cmd == 0x15)
            .returning(|_, buf| {
                buf[0] = 120; // High RSSI: -(120/2) = -60 dBm (above -85 dBm threshold)
                Ok(())
            });

        // Expect set_rx to be called for RSSI measurement
        mock_hal.expect_write_command()
            .withf(|cmd, _| *cmd == 0x82) // SetRx command
            .returning(|_, _| Ok(()));

        // Expect get_state to return standby
        mock_hal.expect_read_command()
            .withf(|cmd, _| *cmd == 0xC0) // GetStatus command
            .returning(|_, buf| {
                buf[0] = 0x20; // StandbyRc state
                Ok(())
            });

        let mut driver = Sx126xDriver::new(mock_hal);
        driver.tx_base_addr = 0;

        // Attempt to transmit should fail due to channel busy
        let test_data = vec![0x01, 0x02, 0x03];
        let result = driver.transmit(&test_data);

        // Verify that transmission was aborted due to channel busy
        assert!(result.is_err());
        if let Err(DriverError::ChannelBusy { rssi_dbm, threshold_dbm }) = result {
            assert_eq!(rssi_dbm, -60);
            assert_eq!(threshold_dbm, -85);
        } else {
            panic!("Expected ChannelBusy error, got: {:?}", result);
        }
        */
    }

    #[test]
    #[ignore] // Requires mock HAL implementation
    fn test_transmit_lbt_channel_clear() {
        // TODO: Test implementation requires mock HAL
        /*
        // Create a mock HAL that reports low RSSI (channel clear)
        let mut mock_hal = MockHal::new();

        // Set up mock to return low RSSI
        mock_hal.expect_read_command()
            .withf(|cmd, _| *cmd == 0x15)
            .returning(|_, buf| {
                buf[0] = 180; // Low RSSI: -(180/2) = -90 dBm (below -85 dBm threshold)
                Ok(())
            });

        // Expect various commands for successful transmission
        mock_hal.expect_write_command().returning(|_, _| Ok(()));
        mock_hal.expect_read_command().returning(|cmd, buf| {
            match cmd {
                0xC0 => buf[0] = 0x20, // GetStatus: StandbyRc
                0x1D => {              // GetIrqStatus
                    buf[0] = 0x00;
                    buf[1] = 0x01;     // TxDone bit set
                },
                _ => {}
            }
            Ok(())
        });

        let mut driver = Sx126xDriver::new(mock_hal);
        driver.tx_base_addr = 0;

        // Transmission should proceed since channel is clear
        let test_data = vec![0x01, 0x02, 0x03];
        let result = driver.transmit(&test_data);

        // For this test, we expect it to proceed past LBT check
        // (though it may fail later due to mock limitations)
        match result {
            Ok(_) => {}, // Success
            Err(DriverError::ChannelBusy { .. }) => {
                panic!("Should not get ChannelBusy with low RSSI");
            },
            Err(_) => {}, // Other errors are acceptable for this test
        }
        */
    }

    // ---- Dual-mode RadioProfile / switch_profile tests -------------------------------
    //
    // These exercise the single-SX126x profile switch on a recording HAL, with no radio.

    use super::{LoRaProfile, RadioProfile, Sx126xDriver, Sx126xExt, WmbusProfile};
    use crate::wmbus::radio::hal::RecordingHal;
    use crate::wmbus::radio::modulation::{CodingRate, LoRaBandwidth, PacketType, SpreadingFactor};

    fn lora_profile() -> LoRaProfile {
        LoRaProfile {
            frequency_hz: 868_100_000,
            sf: SpreadingFactor::SF7,
            bw: LoRaBandwidth::BW125,
            cr: CodingRate::CR4_5,
            power_dbm: 14,
            sync_word: None,
        }
    }

    #[test]
    fn switch_profile_forces_standby_before_packet_type() {
        let mut driver = Sx126xDriver::new(RecordingHal::default(), 32_000_000);
        driver
            .switch_profile(&RadioProfile::LoRa(lora_profile()))
            .unwrap();

        let standby = driver.hal.first_cmd(0x80).expect("SetStandby issued");
        let pkt_type = driver.hal.first_cmd(0x8A).expect("SetPacketType issued");
        assert!(
            standby < pkt_type,
            "SetStandby (0x80) must precede SetPacketType (0x8A)"
        );
        assert!(
            driver.hal.has_cmd(0x8A, &[0x01]),
            "LoRa packet type must be selected"
        );
    }

    #[test]
    fn switching_back_to_wmbus_selects_gfsk_packet_type() {
        // The original bug: configure_for_wmbus never issued SetPacketType(GFSK), so a
        // LoRa→wM-Bus switch left the modem in LoRa. Guard against its return.
        let mut driver = Sx126xDriver::new(RecordingHal::default(), 32_000_000);
        driver
            .switch_profile(&RadioProfile::LoRa(lora_profile()))
            .unwrap();
        driver
            .switch_profile(&RadioProfile::Wmbus(WmbusProfile::mode_c(
                868_950_000,
                100_000,
            )))
            .unwrap();
        assert!(
            driver.hal.has_cmd(0x8A, &[0x00]),
            "GFSK packet type must be selected on switch-back"
        );
    }

    #[test]
    fn configure_for_wmbus_selects_gfsk_packet_type() {
        // The init path must also select the GFSK modem explicitly.
        let mut driver = Sx126xDriver::new(RecordingHal::default(), 32_000_000);
        driver.configure_for_wmbus(868_950_000, 100_000).unwrap();
        assert!(
            driver.hal.has_cmd(0x8A, &[0x00]),
            "configure_for_wmbus must select the GFSK packet type"
        );
    }

    #[test]
    fn packet_type_tracking_gates_lora_frequency_error() {
        // Bug: current_packet_type was never written, so get_lora_frequency_error always
        // returned None. After a LoRa profile it must be Some; after wM-Bus, None.
        let mut driver = Sx126xDriver::new(RecordingHal::default(), 32_000_000);

        driver
            .switch_profile(&RadioProfile::LoRa(lora_profile()))
            .unwrap();
        assert!(
            driver.get_lora_frequency_error().unwrap().is_some(),
            "LoRa profile should expose a frequency-error reading"
        );

        driver
            .switch_profile(&RadioProfile::Wmbus(WmbusProfile::mode_c(
                868_950_000,
                100_000,
            )))
            .unwrap();
        assert!(
            driver.get_lora_frequency_error().unwrap().is_none(),
            "GFSK profile has no LoRa frequency-error register"
        );
    }

    #[tokio::test]
    async fn sx126x_ext_switches_profile_through_trait() {
        // Drive the switch through the `Sx126xExt` bound a generic scheduler/dual-mode
        // handle would use, and confirm `active_packet_type` reflects each switch.
        async fn switch<D: Sx126xExt>(d: &mut D, p: &RadioProfile) {
            d.switch_profile(p).await.unwrap();
        }

        let mut driver = Sx126xDriver::new(RecordingHal::default(), 32_000_000);
        assert_eq!(driver.active_packet_type(), None);

        switch(&mut driver, &RadioProfile::LoRa(lora_profile())).await;
        assert_eq!(driver.active_packet_type(), Some(PacketType::LoRa));

        switch(
            &mut driver,
            &RadioProfile::Wmbus(WmbusProfile::mode_c(868_950_000, 100_000)),
        )
        .await;
        assert_eq!(driver.active_packet_type(), Some(PacketType::Gfsk));
    }

    #[test]
    fn process_irqs_with_mode_none_before_any_profile() {
        // No profile applied → current_packet_type is None → nothing to tag.
        let mut driver = Sx126xDriver::new(RecordingHal::new(), 32_000_000);
        assert!(driver.process_irqs_with_mode().unwrap().is_none());
    }

    #[test]
    fn process_irqs_with_mode_none_when_no_packet_pending() {
        // With a profile applied but no RxDone pending, it returns None (not an error) in
        // both modes — the RecordingHal reports no IRQ.
        let mut gfsk = Sx126xDriver::new(RecordingHal::new(), 32_000_000);
        gfsk.configure_for_wmbus(868_950_000, 100_000).unwrap();
        assert!(gfsk.process_irqs_with_mode().unwrap().is_none());

        let mut lora = Sx126xDriver::new(RecordingHal::new(), 32_000_000);
        lora.configure_for_lora(
            868_100_000,
            SpreadingFactor::SF7,
            LoRaBandwidth::BW125,
            CodingRate::CR4_5,
            14,
        )
        .unwrap();
        assert!(lora.process_irqs_with_mode().unwrap().is_none());
    }

    #[test]
    fn process_irqs_with_mode_reports_lora_snr() {
        // Deliver a LoRa packet with a scripted GetPacketStatus SNR byte (0x14 = 20 -> +5 dB)
        // and confirm the signed SNR surfaces in the tagged packet's LoRa metadata.
        let hal = RecordingHal::new();
        let probe = hal.clone();
        let mut driver = Sx126xDriver::new(hal, 32_000_000);
        driver
            .configure_for_lora(
                868_100_000,
                SpreadingFactor::SF7,
                LoRaBandwidth::BW125,
                CodingRate::CR4_5,
                14,
            )
            .unwrap();
        probe.set_packet_status([0x00, 0x14, 0x00]); // SnrPkt = 20 -> +5.0 dB
        probe.queue_rx(vec![0xDE, 0xAD, 0xBE, 0xEF]);

        let packet = driver.process_irqs_with_mode().unwrap().expect("a packet");
        assert_eq!(packet.mode, PacketType::LoRa);
        let lora = packet.lora.expect("LoRa metadata present");
        assert_eq!(lora.snr_db, 5.0);
    }
}
