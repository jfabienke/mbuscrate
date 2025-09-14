//! LoRa payload decoder framework for metering devices
//!
//! This module provides a flexible decoder system for various LoRa metering payload formats.
//! The backend device manager can specify which decoder to use for each device.

use crate::error::MBusError;
use crate::payload::record::MBusRecordValue;
use std::collections::HashMap;
use std::time::SystemTime;
use thiserror::Error;

/// Errors that can occur during LoRa payload decoding
#[derive(Error, Debug)]
pub enum LoRaDecodeError {
    #[error("Unknown decoder type: {0}")]
    UnknownDecoder(String),
    #[error("Invalid payload length: expected {expected}, got {actual}")]
    InvalidLength { expected: usize, actual: usize },
    #[error("Invalid data at offset {offset}: {reason}")]
    InvalidData { offset: usize, reason: String },
    #[error("Unsupported format version: {0}")]
    UnsupportedVersion(u8),
    #[error("CRC check failed")]
    CrcError,
    #[error("No decoder configured for device")]
    NoDecoder,
    #[error("M-Bus error: {0}")]
    MBusError(#[from] MBusError),
}

/// Decoded metering data from a LoRa payload
#[derive(Debug, Clone)]
pub struct MeteringData {
    /// Timestamp of the reading (device time if available, otherwise reception time)
    pub timestamp: SystemTime,
    /// List of meter readings
    pub readings: Vec<Reading>,
    /// Battery voltage/percentage if available
    pub battery: Option<BatteryStatus>,
    /// Device status flags
    pub status: DeviceStatus,
    /// Raw payload for logging/debugging
    pub raw_payload: Vec<u8>,
    /// Decoder used
    pub decoder_type: String,
}

/// A single meter reading
#[derive(Debug, Clone)]
pub struct Reading {
    /// The measured value
    pub value: MBusRecordValue,
    /// Unit of measurement (e.g., "m³", "kWh", "°C")
    pub unit: String,
    /// Physical quantity (e.g., "Volume", "Energy", "Temperature")
    pub quantity: String,
    /// Tariff number if applicable
    pub tariff: Option<u8>,
    /// Storage number for historical values
    pub storage_number: Option<u32>,
    /// Description or label for this reading
    pub description: Option<String>,
}

/// Battery status information
#[derive(Debug, Clone)]
pub struct BatteryStatus {
    /// Voltage in volts
    pub voltage: Option<f32>,
    /// Percentage (0-100)
    pub percentage: Option<u8>,
    /// Low battery warning
    pub low_battery: bool,
}

/// Device status flags
#[derive(Debug, Clone, Default)]
pub struct DeviceStatus {
    /// Device has an active alarm
    pub alarm: bool,
    /// Tamper detection triggered
    pub tamper: bool,
    /// Leak detected (for water meters)
    pub leak: bool,
    /// Reverse flow detected
    pub reverse_flow: bool,
    /// Device error code if any
    pub error_code: Option<u16>,
    /// Additional status flags (manufacturer specific)
    pub flags: u32,
}

/// Trait for implementing LoRa payload decoders
pub trait LoRaPayloadDecoder: Send + Sync + std::fmt::Debug {
    /// Decode a LoRa payload into metering data
    fn decode(&self, payload: &[u8], f_port: u8) -> Result<MeteringData, LoRaDecodeError>;

    /// Get the name/type of this decoder
    fn decoder_type(&self) -> &str;

    /// Check if this decoder can handle the given payload
    fn can_decode(&self, payload: &[u8], f_port: u8) -> bool {
        // Default: try to decode and see if it works
        self.decode(payload, f_port).is_ok()
    }

    /// Clone the decoder into a boxed trait object
    fn clone_box(&self) -> Box<dyn LoRaPayloadDecoder>;
}

/// Available decoder types
#[derive(Debug, Clone)]
pub enum DecoderType {
    /// EN 13757-3 Compact Frame (Type F)
    En13757Compact,
    /// Generic counter/pulse format
    GenericCounter(GenericCounterConfig),
    /// Decentlab sensor format
    Decentlab(DecentlabConfig),
    /// Dragino sensor format
    Dragino(DraginoModel),
    /// Elvaco meter format
    Elvaco(ElvacoModel),
    /// Sensative Strips format
    Sensative,
    /// Raw binary passthrough (no decoding)
    RawBinary,
}

/// Configuration for generic counter decoder
#[derive(Debug, Clone)]
pub struct GenericCounterConfig {
    /// Byte order (true = big endian, false = little endian)
    pub big_endian: bool,
    /// Size of counter value in bytes
    pub counter_size: usize,
    /// Unit of measurement
    pub unit: String,
    /// Scaling factor to apply to raw value
    pub scale_factor: f64,
    /// Include timestamp in payload
    pub has_timestamp: bool,
    /// Include battery status
    pub has_battery: bool,
}

impl Default for GenericCounterConfig {
    fn default() -> Self {
        Self {
            big_endian: false,
            counter_size: 4,
            unit: "pulses".to_string(),
            scale_factor: 1.0,
            has_timestamp: false,
            has_battery: true,
        }
    }
}

/// Decentlab device configuration
#[derive(Debug, Clone)]
pub struct DecentlabConfig {
    /// Device protocol version
    pub protocol_version: u8,
    /// Sensor channel definitions
    pub channels: Vec<DecentlabChannel>,
}

#[derive(Debug, Clone)]
pub struct DecentlabChannel {
    pub name: String,
    pub unit: String,
    pub scale_factor: f64,
    pub offset: f64,
}

/// Dragino device models
#[derive(Debug, Clone)]
pub enum DraginoModel {
    /// SW3L Water Flow Sensor
    SW3L,
    /// LWL03A Water Leak Sensor
    LWL03A,
    /// Custom model with format specification
    Custom(DraginoFormat),
}

#[derive(Debug, Clone)]
pub struct DraginoFormat {
    pub name: String,
    pub fields: Vec<DraginoField>,
}

#[derive(Debug, Clone)]
pub struct DraginoField {
    pub name: String,
    pub offset: usize,
    pub size: usize,
    pub unit: Option<String>,
    pub scale: f64,
}

/// Elvaco device models
#[derive(Debug, Clone)]
pub enum ElvacoModel {
    /// CMi4110 Water/Heat meter
    CMi4110,
    /// CMe3100 Electricity meter
    CMe3100,
    /// Generic Elvaco format
    Generic,
}

/// Decode using a specific decoder type
pub fn decode_with_type(
    decoder_type: &DecoderType,
    payload: &[u8],
    f_port: u8,
) -> Result<MeteringData, LoRaDecodeError> {
    match decoder_type {
        DecoderType::RawBinary => {
            let decoder = RawBinaryDecoder;
            decoder.decode(payload, f_port)
        }
        _ => {
            // For now, other decoder types are not implemented
            // Return raw binary as fallback
            let decoder = RawBinaryDecoder;
            decoder.decode(payload, f_port)
        }
    }
}

/// Device manager for handling multiple devices with different decoders
pub struct LoRaDeviceManager {
    /// Map of device addresses to their configured decoders
    pub decoders: HashMap<String, DecoderType>,
    /// Default decoder for unknown devices
    pub default_decoder: Option<DecoderType>,
    /// Enable automatic format detection
    auto_detect: bool,
    /// Minimum confidence level for auto-detection
    min_confidence: u8,
}

impl Default for LoRaDeviceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LoRaDeviceManager {
    /// Create a new device manager
    pub fn new() -> Self {
        Self {
            decoders: HashMap::new(),
            default_decoder: Some(DecoderType::RawBinary),
            auto_detect: true,
            min_confidence: 60, // Default to Medium confidence
        }
    }

    /// Enable or disable automatic format detection
    pub fn set_auto_detect(&mut self, enabled: bool) {
        self.auto_detect = enabled;
    }

    /// Set minimum confidence level for auto-detection (0-100)
    pub fn set_min_confidence(&mut self, confidence: u8) {
        self.min_confidence = confidence.min(100);
    }

    /// Register a decoder for a specific device
    pub fn register_device(&mut self, device_addr: String, decoder_type: DecoderType) {
        self.decoders.insert(device_addr, decoder_type);
    }

    /// Set the default decoder for unknown devices
    pub fn set_default_decoder(&mut self, decoder_type: DecoderType) {
        self.default_decoder = Some(decoder_type);
    }

    /// Decode a payload from a specific device
    pub fn decode_payload(
        &self,
        device_addr: &str,
        payload: &[u8],
        f_port: u8,
    ) -> Result<MeteringData, LoRaDecodeError> {
        // Try device-specific decoder first
        if let Some(decoder_type) = self.decoders.get(device_addr) {
            return decode_with_type(decoder_type, payload, f_port);
        }

        // Fall back to default decoder
        if let Some(default) = &self.default_decoder {
            return decode_with_type(default, payload, f_port);
        }

        Err(LoRaDecodeError::NoDecoder)
    }

    /// Auto-detect decoder based on payload characteristics
    pub fn auto_detect_decoder(&self, _payload: &[u8], _f_port: u8) -> Option<&str> {
        // TODO: Implement auto-detection logic
        None
    }
}

/// Raw binary decoder (passthrough)
#[derive(Debug, Clone)]
pub struct RawBinaryDecoder;

impl LoRaPayloadDecoder for RawBinaryDecoder {
    fn decode(&self, payload: &[u8], _f_port: u8) -> Result<MeteringData, LoRaDecodeError> {
        Ok(MeteringData {
            timestamp: SystemTime::now(),
            readings: vec![Reading {
                value: MBusRecordValue::String(hex::encode(payload)),
                unit: "hex".to_string(),
                quantity: "Raw Data".to_string(),
                tariff: None,
                storage_number: None,
                description: Some("Undecoded binary payload".to_string()),
            }],
            battery: None,
            status: DeviceStatus::default(),
            raw_payload: payload.to_vec(),
            decoder_type: "RawBinary".to_string(),
        })
    }

    fn decoder_type(&self) -> &str {
        "RawBinary"
    }

    fn clone_box(&self) -> Box<dyn LoRaPayloadDecoder> {
        Box::new(self.clone())
    }
}

/// Helper functions for common decoding operations
pub mod helpers {
    use super::*;

    /// Extract a big-endian integer from bytes
    pub fn read_be_uint(data: &[u8], offset: usize, size: usize) -> Result<u64, LoRaDecodeError> {
        if offset + size > data.len() {
            return Err(LoRaDecodeError::InvalidLength {
                expected: offset + size,
                actual: data.len(),
            });
        }

        let mut value = 0u64;
        for i in 0..size {
            value = (value << 8) | data[offset + i] as u64;
        }
        Ok(value)
    }

    /// Extract a little-endian integer from bytes
    pub fn read_le_uint(data: &[u8], offset: usize, size: usize) -> Result<u64, LoRaDecodeError> {
        if offset + size > data.len() {
            return Err(LoRaDecodeError::InvalidLength {
                expected: offset + size,
                actual: data.len(),
            });
        }

        let mut value = 0u64;
        for i in 0..size {
            value |= (data[offset + i] as u64) << (i * 8);
        }
        Ok(value)
    }

    /// Convert battery ADC reading to voltage
    pub fn adc_to_voltage(adc_value: u16, reference_voltage: f32, max_adc: u16) -> f32 {
        (adc_value as f32 / max_adc as f32) * reference_voltage
    }

    /// Calculate battery percentage from voltage
    pub fn voltage_to_percentage(voltage: f32, min_voltage: f32, max_voltage: f32) -> u8 {
        let percentage = ((voltage - min_voltage) / (max_voltage - min_voltage) * 100.0)
            .clamp(0.0, 100.0);
        percentage as u8
    }
}
