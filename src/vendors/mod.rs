//! Vendor Extension System for Manufacturer-Specific M-Bus/wM-Bus Handling
//!
//! This module provides a pluggable system for manufacturer-specific extensions
//! to the M-Bus protocol, allowing external crates to override standard behavior
//! at specific extension points defined in EN 13757.

pub mod qundis_hca;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::error::MBusError;
use crate::mbus::secondary_addressing::SecondaryAddress;
use serde::{Serialize, Deserialize};
use serde_json::Value;

/// Type of CRC error for vendor tolerance decisions
#[derive(Debug, Clone, PartialEq)]
pub enum CrcErrorType {
    /// Frame-level CRC error
    Frame,
    /// Block-level CRC error in multi-block frame
    Block,
    /// Type A specific CRC error
    TypeA,
    /// Type B specific CRC error
    TypeB,
    /// Other CRC error
    Other(String),
}

/// Context information for CRC error tolerance decisions
#[derive(Debug, Clone)]
pub struct CrcErrorContext {
    /// Block index for block-level errors (0-based)
    pub block_index: Option<usize>,
    /// Total number of blocks if known
    pub total_blocks: Option<usize>,
    /// Expected CRC value
    pub crc_expected: u16,
    /// Received CRC value
    pub crc_received: u16,
    /// Frame type information
    pub frame_type: Option<String>,
    /// Additional vendor-specific context
    pub vendor_context: HashMap<String, String>,
}

/// Data record representation for vendor extensions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorDataRecord {
    pub dif: u8,
    pub vif: u8,
    pub unit: String,
    pub value: VendorVariable,
    pub quantity: String,
}

/// Variable types that vendor extensions can return
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VendorVariable {
    Numeric(f64),
    String(String),
    Binary(Vec<u8>),
    Boolean(bool),
    Custom { name: String, value: Value },
    ErrorFlags { flags: u32 },
}

/// Enhanced device information from vendor extensions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorDeviceInfo {
    pub manufacturer_id: u16,
    pub device_id: u32,
    pub version: u8,
    pub device_type: u8,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub firmware_version: Option<String>,
    pub additional_info: HashMap<String, Value>,
}

impl From<SecondaryAddress> for VendorDeviceInfo {
    fn from(addr: SecondaryAddress) -> Self {
        Self {
            manufacturer_id: addr.manufacturer,
            device_id: addr.device_id,
            version: addr.version,
            device_type: addr.device_type,
            model: None,
            serial_number: None,
            firmware_version: None,
            additional_info: HashMap::new(),
        }
    }
}

/// Trait for vendor-specific extensions
///
/// Implement this trait in external crates to provide manufacturer-specific
/// handling for M-Bus/wM-Bus devices. Each method returns Option<T> where:
/// - Some(value) = Use custom handling (bypass standard)
/// - None = Fall back to standard protocol handling
pub trait VendorExtension: Send + Sync {
    /// Hook 1: Handle DIF 0x0F/0x1F manufacturer data blocks
    ///
    /// Called when DIF indicates manufacturer-specific data.
    /// Return custom records or None for standard opaque handling.
    fn handle_dif_manufacturer_block(
        &self,
        _manufacturer_id: &str,
        _dif: u8,
        _remaining_payload: &[u8],
    ) -> Result<Option<Vec<VendorDataRecord>>, MBusError> {
        Ok(None)
    }

    /// Hook 2: Parse VIF 0x7F/0xFF manufacturer-specific format
    ///
    /// Called for manufacturer-specific VIF codes.
    /// Return (unit, exponent, quantity, value) or None for standard.
    fn parse_vif_manufacturer_specific(
        &self,
        _manufacturer_id: &str,
        _vif: u8,
        _data: &[u8],
    ) -> Result<Option<(String, i8, String, VendorVariable)>, MBusError> {
        Ok(None)
    }

    /// Hook 3: Handle CI 0xA0-0xB7 manufacturer commands (wM-Bus)
    ///
    /// Called for manufacturer-specific control information.
    /// Return custom record or None for standard unknown CI handling.
    fn handle_ci_manufacturer_range(
        &self,
        _manufacturer_id: &str,
        _ci: u8,
        _payload: &[u8],
    ) -> Result<Option<VendorDataRecord>, MBusError> {
        Ok(None)
    }

    /// Hook 4: Decode manufacturer status bits [7:5]
    ///
    /// Called to interpret vendor-defined status flags.
    /// Return custom variables or None for standard status.
    fn decode_status_bits(
        &self,
        _manufacturer_id: &str,
        _status_byte: u8,
    ) -> Result<Option<Vec<VendorVariable>>, MBusError> {
        Ok(None)
    }

    /// Hook 5: Enrich/validate device header fields
    ///
    /// Called after parsing M/A/V/T fields.
    /// Return enhanced info or None for standard.
    fn enrich_device_header(
        &self,
        _manufacturer_id: &str,
        _basic_info: VendorDeviceInfo,
    ) -> Result<Option<VendorDeviceInfo>, MBusError> {
        Ok(None)
    }

    /// Hook 6: Provision encryption key
    ///
    /// Called before decryption operations.
    /// Return AES-128 key or None for standard derivation.
    fn provision_key(
        &self,
        _manufacturer_id: &str,
        _device_info: &VendorDeviceInfo,
        _frame_data: &[u8],
    ) -> Result<Option<[u8; 16]>, MBusError> {
        Ok(None)
    }

    /// Tolerate CRC failures for known vendor issues (Hook 7)
    ///
    /// Some vendors have known CRC calculation bugs in specific blocks or frames.
    /// This hook allows vendor extensions to tolerate these known issues.
    ///
    /// # Returns
    ///
    /// * `Some(true)` - Tolerate this CRC error
    /// * `Some(false)` - Reject this CRC error
    /// * `None` - Use default CRC validation
    fn tolerate_crc_failure(
        &self,
        _manufacturer_id: &str,
        _device_info: Option<&VendorDeviceInfo>,
        _error_type: &CrcErrorType,
        _error_context: &CrcErrorContext,
    ) -> Result<Option<bool>, MBusError> {
        // Default implementation: no tolerance
        Ok(None)
    }

    /// Tolerate block-specific CRC failures (Hook 7b - Enterprise)
    ///
    /// Enhanced CRC tolerance for specific blocks within multi-block frames.
    /// Critical for vendors like QDS that have known block 3 CRC issues.
    ///
    /// # Arguments
    ///
    /// * `block_index` - 0-based index of the block (e.g., 2 for block 3)
    /// * `block_data` - Raw block data including CRC bytes
    /// * `calculated_crc` - What the CRC should be
    /// * `received_crc` - What the CRC actually is
    ///
    /// # Returns
    ///
    /// * `true` - Ignore CRC mismatch for this block
    /// * `false` - Enforce CRC validation
    fn tolerate_block_crc(
        &self,
        manufacturer_id: &str,
        _device_info: Option<&VendorDeviceInfo>,
        block_index: usize,
        _block_data: &[u8],
        calculated_crc: u16,
        received_crc: u16,
    ) -> bool {
        // Example: QDS devices ignore block 3 CRC
        if manufacturer_id == "QDS" && block_index == 2 {
            log::warn!(
                "Tolerating known CRC issue in QDS block 3 (calc: {calculated_crc:#04x}, recv: {received_crc:#04x})"
            );
            return true;
        }

        // Default: enforce CRC
        false
    }

    /// Extract metrics for instrumentation
    fn extract_metrics(
        &self,
        _manufacturer_id: &str,
        _data: &[u8],
    ) -> Result<HashMap<String, f64>, MBusError> {
        Ok(HashMap::new())
    }

    /// Serialize traces for debugging
    fn serialize_traces(
        &self,
        _data: &[u8],
    ) -> Result<Value, MBusError> {
        Ok(Value::Null)
    }
}

/// Registry for vendor extensions
#[derive(Default, Clone)]
pub struct VendorRegistry {
    inner: Arc<Mutex<HashMap<String, Arc<dyn VendorExtension>>>>,
}

impl VendorRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a vendor extension
    pub fn register(
        &self,
        manufacturer_id: &str,
        extension: Arc<dyn VendorExtension>,
    ) -> Result<(), MBusError> {
        let mut inner = self.inner.lock().unwrap();
        let key = manufacturer_id.to_uppercase();

        if inner.contains_key(&key) {
            return Err(MBusError::Other(format!(
                "Vendor extension already registered for manufacturer: {manufacturer_id}"
            )));
        }

        inner.insert(key, extension);
        Ok(())
    }

    /// Unregister a vendor extension
    pub fn unregister(&self, manufacturer_id: &str) -> Result<(), MBusError> {
        let mut inner = self.inner.lock().unwrap();
        let key = manufacturer_id.to_uppercase();

        if inner.remove(&key).is_none() {
            return Err(MBusError::Other(format!(
                "No vendor extension registered for manufacturer: {manufacturer_id}"
            )));
        }

        Ok(())
    }

    /// Get a vendor extension
    pub fn get(&self, manufacturer_id: &str) -> Option<Arc<dyn VendorExtension>> {
        let inner = self.inner.lock().unwrap();
        let key = manufacturer_id.to_uppercase();
        inner.get(&key).cloned()
    }

    /// Check if a manufacturer has a registered extension
    pub fn has_extension(&self, manufacturer_id: &str) -> bool {
        let inner = self.inner.lock().unwrap();
        let key = manufacturer_id.to_uppercase();
        inner.contains_key(&key)
    }

    /// Get list of registered manufacturers
    pub fn registered_manufacturers(&self) -> Vec<String> {
        let inner = self.inner.lock().unwrap();
        inner.keys().cloned().collect()
    }

    /// Create a new registry with default vendor extensions registered
    pub fn with_defaults() -> Result<Self, MBusError> {
        let registry = Self::new();

        // Register QUNDIS HCA extension
        let qundis_extension = Arc::new(crate::vendors::qundis_hca::QundisHcaExtension::new());
        registry.register("QDS", qundis_extension)?;

        // Future vendor extensions can be added here
        // e.g., registry.register("KAM", kamstrup_extension)?;

        Ok(registry)
    }
}

/// Helper function to convert manufacturer ID to string
pub fn manufacturer_id_to_string(id: u16) -> String {
    // Convert 3-letter manufacturer code from u16
    let c1 = ((id >> 10) & 0x1F) as u8 + b'A' - 1;
    let c2 = ((id >> 5) & 0x1F) as u8 + b'A' - 1;
    let c3 = (id & 0x1F) as u8 + b'A' - 1;

    String::from_utf8(vec![c1, c2, c3]).unwrap_or_else(|_| format!("{id:04X}"))
}

/// Helper function to parse manufacturer string to ID
pub fn parse_manufacturer_id(manufacturer: &str) -> u16 {
    if manufacturer.len() != 3 {
        return 0;
    }

    let bytes = manufacturer.as_bytes();
    let c1 = (bytes[0].saturating_sub(b'A').saturating_add(1) & 0x1F) as u16;
    let c2 = (bytes[1].saturating_sub(b'A').saturating_add(1) & 0x1F) as u16;
    let c3 = (bytes[2].saturating_sub(b'A').saturating_add(1) & 0x1F) as u16;

    (c1 << 10) | (c2 << 5) | c3
}

/// Dispatch helper for DIF manufacturer block hook
pub fn dispatch_dif_hook(
    registry: &VendorRegistry,
    manufacturer_id: &str,
    dif: u8,
    payload: &[u8],
) -> Result<Option<Vec<VendorDataRecord>>, MBusError> {
    if let Some(extension) = registry.get(manufacturer_id) {
        extension.handle_dif_manufacturer_block(manufacturer_id, dif, payload)
    } else {
        Ok(None)
    }
}

/// Dispatch helper for VIF manufacturer-specific hook
pub fn dispatch_vif_hook(
    registry: &VendorRegistry,
    manufacturer_id: &str,
    vif: u8,
    data: &[u8],
) -> Result<Option<(String, i8, String, VendorVariable)>, MBusError> {
    if let Some(extension) = registry.get(manufacturer_id) {
        extension.parse_vif_manufacturer_specific(manufacturer_id, vif, data)
    } else {
        Ok(None)
    }
}

/// Dispatch helper for CI manufacturer range hook
pub fn dispatch_ci_hook(
    registry: &VendorRegistry,
    manufacturer_id: &str,
    ci: u8,
    payload: &[u8],
) -> Result<Option<VendorDataRecord>, MBusError> {
    if let Some(extension) = registry.get(manufacturer_id) {
        extension.handle_ci_manufacturer_range(manufacturer_id, ci, payload)
    } else {
        Ok(None)
    }
}

/// Dispatch helper for status bits hook
pub fn dispatch_status_hook(
    registry: &VendorRegistry,
    manufacturer_id: &str,
    status_byte: u8,
) -> Result<Option<Vec<VendorVariable>>, MBusError> {
    if let Some(extension) = registry.get(manufacturer_id) {
        extension.decode_status_bits(manufacturer_id, status_byte)
    } else {
        Ok(None)
    }
}

/// Dispatch helper for device header hook
pub fn dispatch_header_hook(
    registry: &VendorRegistry,
    manufacturer_id: &str,
    basic_info: VendorDeviceInfo,
) -> Result<Option<VendorDeviceInfo>, MBusError> {
    if let Some(extension) = registry.get(manufacturer_id) {
        extension.enrich_device_header(manufacturer_id, basic_info)
    } else {
        Ok(None)
    }
}

/// Dispatch helper for key provisioning hook
pub fn dispatch_key_hook(
    registry: &VendorRegistry,
    manufacturer_id: &str,
    device_info: &VendorDeviceInfo,
    frame_data: &[u8],
) -> Result<Option<[u8; 16]>, MBusError> {
    if let Some(extension) = registry.get(manufacturer_id) {
        extension.provision_key(manufacturer_id, device_info, frame_data)
    } else {
        Ok(None)
    }
}

/// Dispatch helper for CRC tolerance hook
pub fn dispatch_crc_tolerance(
    registry: &VendorRegistry,
    manufacturer_id: &str,
    device_info: Option<&VendorDeviceInfo>,
    error_type: &CrcErrorType,
    error_context: &CrcErrorContext,
) -> Result<Option<bool>, MBusError> {
    if let Some(extension) = registry.get(manufacturer_id) {
        extension.tolerate_crc_failure(manufacturer_id, device_info, error_type, error_context)
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockVendorExtension {
        #[allow(dead_code)]
        manufacturer: String,
    }

    impl VendorExtension for MockVendorExtension {
        fn handle_dif_manufacturer_block(
            &self,
            _manufacturer_id: &str,
            dif: u8,
            payload: &[u8],
        ) -> Result<Option<Vec<VendorDataRecord>>, MBusError> {
            if dif == 0x0F && !payload.is_empty() {
                let record = VendorDataRecord {
                    dif,
                    vif: 0xFF,
                    unit: "Custom".to_string(),
                    value: VendorVariable::Binary(payload.to_vec()),
                    quantity: "Manufacturer Data".to_string(),
                };
                Ok(Some(vec![record]))
            } else {
                Ok(None)
            }
        }
    }

    #[test]
    fn test_vendor_registry() {
        let registry = VendorRegistry::new();
        let extension = Arc::new(MockVendorExtension {
            manufacturer: "TST".to_string(),
        });

        // Register
        assert!(registry.register("TST", extension.clone()).is_ok());
        assert!(registry.has_extension("TST"));
        assert!(registry.has_extension("tst")); // Case insensitive

        // Duplicate registration should fail
        assert!(registry.register("TST", extension).is_err());

        // Get
        assert!(registry.get("TST").is_some());

        // Unregister
        assert!(registry.unregister("TST").is_ok());
        assert!(!registry.has_extension("TST"));
    }

    #[test]
    fn test_manufacturer_id_conversion() {
        // Test known manufacturer codes
        assert_eq!(manufacturer_id_to_string(0x2C2D), "KAM"); // Kamstrup

        // Test edge cases
        let id = ((26 << 10) | (26 << 5) | 26) as u16; // ZZZ
        assert_eq!(manufacturer_id_to_string(id), "ZZZ");

        // Test round-trip conversion
        assert_eq!(parse_manufacturer_id("KAM"), 0x2C2D);
        assert_eq!(manufacturer_id_to_string(parse_manufacturer_id("ABC")), "ABC");
    }

    #[test]
    fn test_dispatch_hooks() {
        let registry = VendorRegistry::new();
        let extension = Arc::new(MockVendorExtension {
            manufacturer: "TST".to_string(),
        });
        registry.register("TST", extension).unwrap();

        // Test DIF hook dispatch
        let payload = vec![0x01, 0x02, 0x03];
        let result = dispatch_dif_hook(&registry, "TST", 0x0F, &payload).unwrap();
        assert!(result.is_some());
        let records = result.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].dif, 0x0F);

        // Test fallback for unregistered manufacturer
        let result = dispatch_dif_hook(&registry, "UNK", 0x0F, &payload).unwrap();
        assert!(result.is_none());
    }
}