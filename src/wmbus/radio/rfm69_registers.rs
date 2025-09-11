//! # RFM69 Register Definitions and Constants
//!
//! This module contains all the register addresses, operating modes, and bit field definitions
//! for the HopeRF RFM69HCW transceiver. These definitions are based on the RFM69 datasheet
//! and industry best practices.
//!
//! ## Register Map
//!
//! The RFM69 has a comprehensive register set for configuration:
//! - 0x00-0x0F: Basic configuration (FIFO, operation mode, data modulation)
//! - 0x10-0x2F: RF settings (frequency, power, bandwidth, RSSI)  
//! - 0x30-0x3F: Packet configuration and addressing
//! - 0x40-0x4F: AES encryption and temperature sensor
//!
//! ## Operating Modes
//!
//! The RFM69 supports multiple operating modes for power management:
//! - Sleep: Ultra-low power mode
//! - Standby: Ready for fast transition to TX/RX
//! - Frequency Synthesis: PLL locked, ready for immediate TX/RX
//! - TX: Transmitting data
//! - RX: Receiving data

// =============================================================================
// RFM69 Register Addresses
// =============================================================================

/// FIFO read/write access register
pub const REG_FIFO: u8 = 0x00;

/// Operating mode and frequency band selection  
pub const REG_OPMODE: u8 = 0x01;

/// Data processing mode and modulation scheme
pub const REG_DATAMODUL: u8 = 0x02;

/// Bit rate setting (MSB)
pub const REG_BITRATEMSB: u8 = 0x03;

/// Bit rate setting (LSB)  
pub const REG_BITRATELSB: u8 = 0x04;

/// Frequency deviation setting (MSB)
pub const REG_FDEVMSB: u8 = 0x05;

/// Frequency deviation setting (LSB)
pub const REG_FDEVLSB: u8 = 0x06;

/// RF carrier frequency setting (MSB)
pub const REG_FRFMSB: u8 = 0x07;

/// RF carrier frequency setting (MID)
pub const REG_FRFMID: u8 = 0x08;

/// RF carrier frequency setting (LSB)
pub const REG_FRFLSB: u8 = 0x09;

/// RC oscillator settings
pub const REG_OSC1: u8 = 0x0A;

/// AFC control in low modulation index situations
pub const REG_AFCCTRL: u8 = 0x0B;

/// Low battery indicator settings
pub const REG_LOWBAT: u8 = 0x0C;

/// Listen mode settings 1
pub const REG_LISTEN1: u8 = 0x0D;

/// Listen mode settings 2  
pub const REG_LISTEN2: u8 = 0x0E;

/// Listen mode settings 3
pub const REG_LISTEN3: u8 = 0x0F;

/// Chip version (read-only)
pub const REG_VERSION: u8 = 0x10;

/// PA selection and output power control
pub const REG_PALEVEL: u8 = 0x11;

/// Control of PA ramp time in FSK mode
pub const REG_PARAMP: u8 = 0x12;

/// Over current protection control
pub const REG_OCP: u8 = 0x13;

/// AGC reference level
pub const REG_AGCREF: u8 = 0x14;

/// AGC threshold 1
pub const REG_AGCTHRESH1: u8 = 0x15;

/// AGC threshold 2
pub const REG_AGCTHRESH2: u8 = 0x16;

/// AGC threshold 3  
pub const REG_AGCTHRESH3: u8 = 0x17;

/// LNA settings
pub const REG_LNA: u8 = 0x18;

/// Channel filter bandwidth control
pub const REG_RXBW: u8 = 0x19;

/// AFC bandwidth control
pub const REG_AFCBW: u8 = 0x1A;

/// OOK demodulator selection and control
pub const REG_OOKPEAK: u8 = 0x1B;

/// Average threshold control of OOK demodulator
pub const REG_OOKAVG: u8 = 0x1C;

/// Fixed threshold control of OOK demodulator
pub const REG_OOKFIX: u8 = 0x1D;

/// AFC and FEI control and status
pub const REG_AFCFEI: u8 = 0x1E;

/// MSB of AFC correction in Hz
pub const REG_AFCMSB: u8 = 0x1F;

/// LSB of AFC correction in Hz
pub const REG_AFCLSB: u8 = 0x20;

/// MSB of FEI value in Hz
pub const REG_FEIMSB: u8 = 0x21;

/// LSB of FEI value in Hz
pub const REG_FEILSB: u8 = 0x22;

/// RSSI-related settings
pub const REG_RSSICONFIG: u8 = 0x23;

/// RSSI value in dBm
pub const REG_RSSIVALUE: u8 = 0x24;

/// Mapping of pins DIO0 to DIO3
pub const REG_DIOMAPPING1: u8 = 0x25;

/// Mapping of pins DIO4 and DIO5, ClkOut frequency
pub const REG_DIOMAPPING2: u8 = 0x26;

/// Status register: PLL lock state, timeout, RSSI
pub const REG_IRQFLAGS1: u8 = 0x27;

/// Status register: FIFO handling flags
pub const REG_IRQFLAGS2: u8 = 0x28;

/// RSSI trigger level for RSSI interrupt
pub const REG_RSSITHRESH: u8 = 0x29;

/// Timeout duration between RX request and PayloadReady IRQ
pub const REG_RXTIMEOUT1: u8 = 0x2A;

/// Timeout duration between RSSI IRQ and PayloadReady IRQ
pub const REG_RXTIMEOUT2: u8 = 0x2B;

/// Preamble length (MSB)
pub const REG_PREAMBLEMSB: u8 = 0x2C;

/// Preamble length (LSB)
pub const REG_PREAMBLELSB: u8 = 0x2D;

/// Sync word recognition control
pub const REG_SYNCCONFIG: u8 = 0x2E;

/// Sync word byte 1
pub const REG_SYNCVALUE1: u8 = 0x2F;

/// Sync word byte 2
pub const REG_SYNCVALUE2: u8 = 0x30;

/// Sync word byte 3
pub const REG_SYNCVALUE3: u8 = 0x31;

/// Sync word byte 4
pub const REG_SYNCVALUE4: u8 = 0x32;

/// Sync word byte 5
pub const REG_SYNCVALUE5: u8 = 0x33;

/// Sync word byte 6
pub const REG_SYNCVALUE6: u8 = 0x34;

/// Sync word byte 7
pub const REG_SYNCVALUE7: u8 = 0x35;

/// Sync word byte 8
pub const REG_SYNCVALUE8: u8 = 0x36;

/// Packet mode settings
pub const REG_PACKETCONFIG1: u8 = 0x37;

/// Packet mode settings
pub const REG_PAYLOADLENGTH: u8 = 0x38;

/// Node address
pub const REG_NODEADRS: u8 = 0x39;

/// Broadcast address
pub const REG_BROADCASTADRS: u8 = 0x3A;

/// Auto modes settings
pub const REG_AUTOMODES: u8 = 0x3B;

/// FIFO threshold, TX start condition
pub const REG_FIFOTHRESH: u8 = 0x3C;

/// Packet mode settings
pub const REG_PACKETCONFIG2: u8 = 0x3D;

/// AES encryption key bytes 1-16
pub const REG_AESKEY1: u8 = 0x3E;
pub const REG_AESKEY2: u8 = 0x3F;
pub const REG_AESKEY3: u8 = 0x40;
pub const REG_AESKEY4: u8 = 0x41;
pub const REG_AESKEY5: u8 = 0x42;
pub const REG_AESKEY6: u8 = 0x43;
pub const REG_AESKEY7: u8 = 0x44;
pub const REG_AESKEY8: u8 = 0x45;
pub const REG_AESKEY9: u8 = 0x46;
pub const REG_AESKEY10: u8 = 0x47;
pub const REG_AESKEY11: u8 = 0x48;
pub const REG_AESKEY12: u8 = 0x49;
pub const REG_AESKEY13: u8 = 0x4A;
pub const REG_AESKEY14: u8 = 0x4B;
pub const REG_AESKEY15: u8 = 0x4C;
pub const REG_AESKEY16: u8 = 0x4D;

/// Temperature sensor control
pub const REG_TEMP1: u8 = 0x4E;

/// Temperature sensor control 
pub const REG_TEMP2: u8 = 0x4F;

/// Test register for AGC
pub const REG_TESTDAGC: u8 = 0x6F;

// =============================================================================
// Operating Mode Constants
// =============================================================================

/// Operating modes for RFM69 (used with REG_OPMODE)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatingMode {
    /// Sleep mode - lowest power consumption
    Sleep = 0,
    /// Standby mode - crystal oscillator running
    Standby = 1,
    /// Frequency synthesis mode - PLL running
    FrequencySynthesis = 2,
    /// Transmit mode
    Transmit = 3,
    /// Receive mode  
    Receive = 4,
}

/// Operating mode bit patterns for REG_OPMODE
pub const RF_OPMODE_SLEEP: u8 = 0x00;
pub const RF_OPMODE_STANDBY: u8 = 0x04;
pub const RF_OPMODE_SYNTHESIZER: u8 = 0x08;
pub const RF_OPMODE_TRANSMITTER: u8 = 0x0C;
pub const RF_OPMODE_RECEIVER: u8 = 0x10;

// =============================================================================
// IRQ Flag Definitions
// =============================================================================

/// IRQ flags in REG_IRQFLAGS1
pub const RF_IRQFLAGS1_SYNCADDRESSMATCH: u8 = 0x01;
pub const RF_IRQFLAGS1_AUTOMODE: u8 = 0x02;
pub const RF_IRQFLAGS1_TIMEOUT: u8 = 0x04;
pub const RF_IRQFLAGS1_RSSI: u8 = 0x08;
pub const RF_IRQFLAGS1_PLLLOCK: u8 = 0x10;
pub const RF_IRQFLAGS1_TXREADY: u8 = 0x20;
pub const RF_IRQFLAGS1_RXREADY: u8 = 0x40;
pub const RF_IRQFLAGS1_MODEREADY: u8 = 0x80;

/// IRQ flags in REG_IRQFLAGS2
pub const RF_IRQFLAGS2_LOWBAT: u8 = 0x01;
pub const RF_IRQFLAGS2_CRCOK: u8 = 0x02;
pub const RF_IRQFLAGS2_PAYLOADREADY: u8 = 0x04;
pub const RF_IRQFLAGS2_PACKETSENT: u8 = 0x08;
pub const RF_IRQFLAGS2_FIFOOVERRUN: u8 = 0x10;
pub const RF_IRQFLAGS2_FIFOLEVEL: u8 = 0x20;
pub const RF_IRQFLAGS2_FIFONOTEMPTY: u8 = 0x40;
pub const RF_IRQFLAGS2_FIFOFULL: u8 = 0x80;

// =============================================================================
// Configuration Constants  
// =============================================================================

/// RC oscillator calibration flags
pub const RF_OSC1_RCCAL_START: u8 = 0x80;
pub const RF_OSC1_RCCAL_DONE: u8 = 0x40;

/// Packet configuration flags
pub const RF_PACKETCONFIG_CRC: u8 = 0x10;
pub const RF_PACKETCONFIG_CRCACO: u8 = 0x08;
pub const RF_PACKETCONFIG_VARLEN: u8 = 0x80;
pub const RF_PACKETCONFIG_MANCHESTER: u8 = 0x20;
pub const RF_PACKETCONFIG_WHITENING: u8 = 0x40;

/// Packet configuration 2 flags
pub const RF_PACKET2_EAS_ON: u8 = 0x01;
pub const RF_PACKET2_RXRESTART: u8 = 0x04;

/// DIO mapping flags
pub const RF_DIOMAPPING1_DIO0_01: u8 = 0x40;

// =============================================================================
// wM-Bus Specific Constants per EN 13757-4
// =============================================================================

/// Bitrate configuration for 100 kbps
pub const RF_BITRATEMSB_100KBPS: u8 = 0x01;
pub const RF_BITRATELSB_100KBPS: u8 = 0x40;

/// Frequency deviation for 50 kHz  
pub const RF_FDEVMSB_50000: u8 = 0x03;
pub const RF_FDEVLSB_50000: u8 = 0x33;

/// Default SPI communication speed
pub const SPI_SPEED: u32 = 1_000_000; // 1 MHz

/// RF frequency step for calculation (61.03515625 Hz per step)
pub const FSTEP: f64 = 61.03515625;

/// Default frequency for wM-Bus operation (868.95 MHz)
pub const WMBUS_FREQUENCY: f64 = 868.95e6;

/// Default GPIO pins (can be overridden in configuration)
pub const DEFAULT_RESET_PIN: u8 = 5;
pub const DEFAULT_INTERRUPT_PIN: u8 = 23;

/// FIFO size in bytes
pub const FIFO_SIZE: usize = 66;

/// Maximum packet size for wM-Bus
pub const MAX_PACKET_SIZE: usize = 255;