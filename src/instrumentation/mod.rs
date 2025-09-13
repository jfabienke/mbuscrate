//! Unified Instrumentation Model for All Device Types
//!
//! This module provides a unified instrumentation model that combines
//! metrics from M-Bus, wM-Bus, and LoRa devices into a single format
//! suitable for external device management systems.

pub mod converters;
pub mod stats;

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::time::SystemTime;

/// Unified instrumentation data combining all device types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedInstrumentation {
    // Device identification
    pub device_id: String,
    pub manufacturer: String,
    pub device_type: DeviceType,
    pub version: Option<String>,
    pub model: Option<String>,

    // Radio metrics (for wireless devices)
    pub radio_metrics: Option<RadioMetrics>,

    // Device health
    pub battery_status: Option<BatteryStatus>,
    pub device_status: DeviceStatus,

    // Protocol metrics
    pub protocol: ProtocolType,
    pub frame_statistics: FrameStatistics,

    // Meter readings
    pub readings: Vec<Reading>,

    // Vendor-specific data
    pub vendor_metrics: HashMap<String, f64>,
    pub vendor_data: Option<serde_json::Value>,

    // Metadata
    pub timestamp: SystemTime,
    pub source_address: Option<String>,
    pub raw_payload: Option<Vec<u8>>,
}

/// Device type classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceType {
    WaterMeter,
    ElectricityMeter,
    GasMeter,
    HeatMeter,
    CoolingMeter,
    HotWaterMeter,
    PressureSensor,
    TemperatureSensor,
    FlowSensor,
    Other(String),
}

/// Protocol type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtocolType {
    MBusWired,
    WMBusMode(String), // S, T, C, etc.
    LoRa,
    LoRaWAN,
    Other(String),
}

/// Radio-level metrics for wireless protocols
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioMetrics {
    pub rssi_dbm: Option<i16>,
    pub snr_db: Option<f32>,
    pub frequency_hz: Option<u32>,
    pub spreading_factor: Option<u8>,
    pub bandwidth_khz: Option<u32>,
    pub packet_counter: Option<u32>,
}

/// Battery status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryStatus {
    pub voltage: Option<f32>,
    pub percentage: Option<u8>,
    pub low_battery: bool,
    pub estimated_days_remaining: Option<u32>,
}

/// Device status flags
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub alarm: bool,
    pub tamper: bool,
    pub leak_detected: bool,
    pub reverse_flow: bool,
    pub burst_detected: bool,
    pub dry_running: bool,
    pub error_code: Option<u16>,
    pub error_description: Option<String>,
    pub additional_flags: HashMap<String, bool>,
}

/// Frame/packet statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FrameStatistics {
    pub frames_received: u64,
    pub frames_valid: u64,
    pub crc_errors: u64,
    pub decryption_errors: u64,
    pub parsing_errors: u64,
    pub last_frame_time: Option<SystemTime>,
}

/// Meter reading with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reading {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub timestamp: SystemTime,
    pub tariff: Option<u32>,
    pub storage_number: Option<u32>,
    pub quality: ReadingQuality,
}

/// Reading quality indicator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReadingQuality {
    Good,
    Estimated,
    Substitute,
    Manual,
    Invalid,
}

/// Source of instrumentation data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InstrumentationSource {
    MBusFrame,
    WMBusFrame,
    LoRaPayload,
    VendorExtension(String),
}

// Conversion implementations from specific types

impl UnifiedInstrumentation {
    /// Create new instrumentation with basic fields
    pub fn new(device_id: String, manufacturer: String, protocol: ProtocolType) -> Self {
        Self {
            device_id,
            manufacturer,
            device_type: DeviceType::Other("Unknown".to_string()),
            version: None,
            model: None,
            radio_metrics: None,
            battery_status: None,
            device_status: DeviceStatus::default(),
            protocol,
            frame_statistics: FrameStatistics::default(),
            readings: Vec::new(),
            vendor_metrics: HashMap::new(),
            vendor_data: None,
            timestamp: SystemTime::now(),
            source_address: None,
            raw_payload: None,
        }
    }

    /// Add a meter reading
    pub fn add_reading(&mut self, name: String, value: f64, unit: String) {
        self.readings.push(Reading {
            name,
            value,
            unit,
            timestamp: SystemTime::now(),
            tariff: None,
            storage_number: None,
            quality: ReadingQuality::Good,
        });
    }

    /// Set radio metrics for wireless devices
    pub fn set_radio_metrics(&mut self, rssi: i16, snr: Option<f32>) {
        self.radio_metrics = Some(RadioMetrics {
            rssi_dbm: Some(rssi),
            snr_db: snr,
            frequency_hz: None,
            spreading_factor: None,
            bandwidth_khz: None,
            packet_counter: None,
        });
    }

    /// Set battery status
    pub fn set_battery(&mut self, voltage: Option<f32>, percentage: Option<u8>) {
        self.battery_status = Some(BatteryStatus {
            voltage,
            percentage,
            low_battery: percentage.map(|p| p < 20).unwrap_or(false)
                || voltage.map(|v| v < 2.5).unwrap_or(false),
            estimated_days_remaining: None,
        });
    }

    /// Determine device type from medium code
    pub fn set_device_type_from_medium(&mut self, medium_code: u8) {
        self.device_type = match medium_code {
            0x00 => DeviceType::Other("Other".to_string()),
            0x01 => DeviceType::Other("Oil".to_string()),
            0x02 => DeviceType::ElectricityMeter,
            0x03 => DeviceType::GasMeter,
            0x04 => DeviceType::HeatMeter,
            0x05 => DeviceType::Other("Steam".to_string()),
            0x06 => DeviceType::HotWaterMeter,
            0x07 => DeviceType::WaterMeter,
            0x08 => DeviceType::Other("Heat Cost Allocator".to_string()),
            0x09 => DeviceType::Other("Compressed Air".to_string()),
            0x0A => DeviceType::CoolingMeter,
            0x0B => DeviceType::CoolingMeter,
            0x0C => DeviceType::HeatMeter,
            0x0D => DeviceType::CoolingMeter,
            0x0E => DeviceType::Other("Heat/Cooling".to_string()),
            0x0F => DeviceType::Other("Bus/System".to_string()),
            0x15 => DeviceType::WaterMeter, // Cold water
            0x16 => DeviceType::HotWaterMeter, // Hot water
            0x17 => DeviceType::Other("Dual Water".to_string()),
            0x18 => DeviceType::PressureSensor,
            0x19 => DeviceType::Other("A/D Converter".to_string()),
            0x1A => DeviceType::Other("Smoke Detector".to_string()),
            0x1B => DeviceType::Other("Room Sensor".to_string()),
            0x1C => DeviceType::GasMeter, // Gas detector
            0x20 => DeviceType::Other("Breaker".to_string()),
            0x21 => DeviceType::Other("Valve".to_string()),
            0x25 => DeviceType::Other("Customer Unit".to_string()),
            0x28 => DeviceType::Other("Waste Water".to_string()),
            0x29 => DeviceType::Other("Garbage".to_string()),
            0x2A => DeviceType::Other("Carbon Dioxide".to_string()),
            0x2B => DeviceType::Other("Environmental".to_string()),
            _ => DeviceType::Other(format!("Unknown (0x{medium_code:02X})")),
        };
    }

    /// Export as JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Export as pretty JSON
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Export as CBOR
    #[cfg(feature = "cbor")]
    pub fn to_cbor(&self) -> Result<Vec<u8>, ciborium::ser::Error> {
        let mut buffer = Vec::new();
        ciborium::ser::into_writer(self, &mut buffer)?;
        Ok(buffer)
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_instrumentation_creation() {
        let mut inst = UnifiedInstrumentation::new(
            "12345678".to_string(),
            "KAM".to_string(),
            ProtocolType::MBusWired,
        );

        inst.add_reading("Volume".to_string(), 123.45, "m³".to_string());
        inst.set_battery(Some(3.0), Some(75));
        inst.set_device_type_from_medium(0x07);

        assert_eq!(inst.device_id, "12345678");
        assert_eq!(inst.readings.len(), 1);
        assert!(inst.battery_status.is_some());
        assert!(!inst.battery_status.as_ref().unwrap().low_battery);
        assert!(matches!(inst.device_type, DeviceType::WaterMeter));
    }

    #[test]
    fn test_json_serialization() {
        let inst = UnifiedInstrumentation::new(
            "test".to_string(),
            "TST".to_string(),
            ProtocolType::WMBusMode("T".to_string()),
        );

        let json = inst.to_json().unwrap();
        assert!(json.contains("\"device_id\":\"test\""));

        // Test round-trip
        let inst2: UnifiedInstrumentation = serde_json::from_str(&json).unwrap();
        assert_eq!(inst2.device_id, inst.device_id);
    }

    #[test]
    fn test_battery_low_detection() {
        let mut inst = UnifiedInstrumentation::new(
            "test".to_string(),
            "TST".to_string(),
            ProtocolType::LoRa,
        );

        // Test low percentage
        inst.set_battery(Some(3.0), Some(15));
        assert!(inst.battery_status.as_ref().unwrap().low_battery);

        // Test low voltage
        inst.set_battery(Some(2.0), Some(50));
        assert!(inst.battery_status.as_ref().unwrap().low_battery);

        // Test good battery
        inst.set_battery(Some(3.6), Some(85));
        assert!(!inst.battery_status.as_ref().unwrap().low_battery);
    }
}