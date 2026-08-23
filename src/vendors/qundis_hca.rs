//! QUNDIS HCA (Heat Cost Allocator) Vendor Extension
//!
//! This module provides vendor-specific handling for QUNDIS HCA devices,
//! particularly addressing the MbusValueDateG date encoding discrepancy
//! that causes 10-year offsets in date parsing.
//!
//! ## QUNDIS Date Encoding Issue
//!
//! QUNDIS uses a non-standard bit packing for VIF 0x04 date fields:
//! - Standard M-Bus: Contiguous BCD year encoding
//! - QUNDIS: Split year across non-contiguous bit fields
//!   - High nibble: bits 12-15 (shifted right 9)
//!   - Low nibble: bits 5-7 (shifted right 5)
//!   - Combined with OR and base year 2000
//!
//! This fixes the common "10-year offset" bug in HCA due dates.
//!
//! ## Usage Example
//!
//! ```rust
//! use mbus_rs::VendorRegistry;
//! use mbus_rs::payload::record::parse_variable_record_with_vendor;
//!
//! // Create registry with QUNDIS extension
//! let registry = VendorRegistry::with_defaults().unwrap();
//!
//! // Parse M-Bus record with QUNDIS support
//! let manufacturer_id = "QDS";
//! let mbus_data = [0x04u8, 0x6D, 0x17, 0x2E, 0xCC, 0x13];
//! let record = parse_variable_record_with_vendor(
//!     &mbus_data,
//!     Some(manufacturer_id),
//!     Some(&registry),
//! );
//!
//! // QUNDIS VIF 0x04 dates will now be decoded correctly
//! // instead of having 10-year offsets
//! assert!(record.is_ok());
//! ```

use crate::error::MBusError;
use crate::payload::record::MBusRecord;
use crate::vendors::quirks::{
    Evidence, QuirkApplied, QuirkManifest, QuirkStatus, VendorQuirks, VendorScope,
};
use crate::vendors::{VendorDeviceInfo, VendorExtension};
use chrono::{DateTime, NaiveDate, Utc};
use log::debug;
use std::collections::HashMap;

/// QUNDIS manufacturer code
pub const QUNDIS_MANUFACTURER_ID: &str = "QDS";

/// VIF codes that QUNDIS handles with special encoding
pub const QUNDIS_VIF_DATE: u8 = 0x04; // Date field with custom bit packing

/// QUNDIS HCA vendor extension implementation
pub struct QundisHcaExtension;

impl QundisHcaExtension {
    pub fn new() -> Self {
        Self
    }

    /// Decode QUNDIS MbusValueDateG format
    ///
    /// Handles the proprietary QUNDIS date packing for VIF 0x04 that causes
    /// the 10-year discrepancy in standard M-Bus decoders.
    ///
    /// # Bit Layout (from decompiled QUNDIS MbusValueDateG)
    ///
    /// ```text
    /// Bits 15-12: Year tens digit (high nibble)
    /// Bits 11-8:  Reserved/other data
    /// Bits 7-5:   Year units digit (low nibble)
    /// Bits 4-1:   Month (1-12)
    /// Bit 0:      Reserved
    /// ```
    ///
    /// # Formula
    ///
    /// ```text
    /// year = ((data & 0xF000) >> 9) | ((data & 0x00E0) >> 5) + 2000
    /// month = (data & 0x001E) >> 1
    /// ```
    ///
    /// # Arguments
    ///
    /// * `raw_data` - Raw 16-bit or 32-bit value from M-Bus payload
    ///
    /// # Returns
    ///
    /// * `Ok(DateTime<Utc>)` - Parsed date/time
    /// * `Err(MBusError)` - Invalid date data
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mbus_rs::QundisHcaExtension;
    ///
    /// use chrono::Datelike;
    ///
    /// // Sample data that would decode incorrectly with standard M-Bus.
    /// // Year bits: (0x1000 >> 9) | (0x00E0 >> 5) = 8 | 7 = 15 => 2015. Month bits => 6.
    /// let raw = 0x10ECu32; // Example QUNDIS date encoding
    /// let date = QundisHcaExtension::decode_mbus_value_date_g(raw).unwrap();
    /// // Produces the correct year, not one offset by 10 years
    /// assert_eq!(date.year(), 2015);
    /// assert_eq!(date.month(), 6);
    /// ```
    pub fn decode_mbus_value_date_g(raw_data: u32) -> Result<DateTime<Utc>, MBusError> {
        debug!("Decoding QUNDIS MbusValueDateG: 0x{:08X}", raw_data);

        // Extract year using QUNDIS bit packing
        // Formula from user: ((dataValue & 0xF000) >> 9) | ((dataValue & 0x00E0) >> 5) + 2000
        let year_parts = ((raw_data & 0xF000) >> 9) | ((raw_data & 0x00E0) >> 5);
        let year = year_parts + 2000;

        // Extract month from bits 1-4
        let month = (raw_data & 0x001E) >> 1;

        // Extract day if present (some QUNDIS variants include day)
        let day = if raw_data & 0x0001 != 0 {
            // Day might be encoded in upper bits for extended format
            ((raw_data & 0x0F800000) >> 23).max(1)
        } else {
            // Default to first day of month for date-only records
            1
        };

        debug!(
            "QUNDIS date components: year={}, month={}, day={}",
            year, month, day
        );

        // Validate components
        if !(2000..=2099).contains(&year) {
            return Err(MBusError::Other(format!(
                "Invalid QUNDIS year: {} (expected 2000-2099)",
                year
            )));
        }

        if !(1..=12).contains(&month) {
            return Err(MBusError::Other(format!(
                "Invalid QUNDIS month: {} (expected 1-12)",
                month
            )));
        }

        if !(1..=31).contains(&day) {
            return Err(MBusError::Other(format!(
                "Invalid QUNDIS day: {} (expected 1-31)",
                day
            )));
        }

        // Create NaiveDate and convert to DateTime<Utc>
        let naive_date = NaiveDate::from_ymd_opt(year as i32, month, day).ok_or_else(|| {
            MBusError::Other(format!(
                "Invalid QUNDIS date: {}-{:02}-{:02}",
                year, month, day
            ))
        })?;

        let naive_datetime = naive_date.and_hms_opt(0, 0, 0).ok_or_else(|| {
            MBusError::Other("Failed to create datetime from QUNDIS date".to_string())
        })?;

        Ok(DateTime::from_naive_utc_and_offset(naive_datetime, Utc))
    }

    /// Decode QUNDIS date/time with extended format
    ///
    /// For 32-bit QUNDIS date/time values that include hour/minute information.
    pub fn decode_mbus_value_datetime_g(raw_data: u32) -> Result<DateTime<Utc>, MBusError> {
        debug!("Decoding QUNDIS MbusValueDateTimeG: 0x{:08X}", raw_data);

        // Extract date components (same as date-only format)
        let year_parts = ((raw_data & 0xF000) >> 9) | ((raw_data & 0x00E0) >> 5);
        let year = year_parts + 2000;
        let month = (raw_data & 0x001E) >> 1;

        // Extract time components from upper bits
        let hour = (raw_data & 0x1F000000) >> 24; // Bits 24-28
        let minute = (raw_data & 0x00FC0000) >> 18; // Bits 18-23
        let day = ((raw_data & 0x003E0000) >> 17).max(1); // Bits 17-21

        debug!(
            "QUNDIS datetime components: year={}, month={}, day={}, hour={}, minute={}",
            year, month, day, hour, minute
        );

        // Validate all components
        if !(2000..=2099).contains(&year) {
            return Err(MBusError::Other(format!("Invalid year: {}", year)));
        }
        if !(1..=12).contains(&month) {
            return Err(MBusError::Other(format!("Invalid month: {}", month)));
        }
        if !(1..=31).contains(&day) {
            return Err(MBusError::Other(format!("Invalid day: {}", day)));
        }
        if hour > 23 {
            return Err(MBusError::Other(format!("Invalid hour: {}", hour)));
        }
        if minute > 59 {
            return Err(MBusError::Other(format!("Invalid minute: {}", minute)));
        }

        // Create full datetime
        let naive_date = NaiveDate::from_ymd_opt(year as i32, month, day)
            .ok_or_else(|| MBusError::Other(format!("Invalid date: {}-{}-{}", year, month, day)))?;

        let naive_datetime = naive_date
            .and_hms_opt(hour, minute, 0)
            .ok_or_else(|| MBusError::Other("Failed to create datetime".to_string()))?;

        Ok(DateTime::from_naive_utc_and_offset(naive_datetime, Utc))
    }
}

impl Default for QundisHcaExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl VendorExtension for QundisHcaExtension {
    /// Enrich QUNDIS device information
    fn enrich_device_header(
        &self,
        manufacturer_id: &str,
        mut basic_info: VendorDeviceInfo,
    ) -> Result<Option<VendorDeviceInfo>, MBusError> {
        if manufacturer_id != QUNDIS_MANUFACTURER_ID {
            return Ok(None);
        }

        // Add QUNDIS-specific device information
        basic_info.model = Some("HCA Heat Cost Allocator".to_string());
        basic_info.additional_info.insert(
            "vendor_quirks".to_string(),
            serde_json::Value::String("MbusValueDateG_fix_applied".to_string()),
        );
        basic_info.additional_info.insert(
            "date_encoding".to_string(),
            serde_json::Value::String("QUNDIS_proprietary".to_string()),
        );

        debug!(
            "Enriched QUNDIS device info for ID: 0x{:08X}",
            basic_info.device_id
        );

        Ok(Some(basic_info))
    }

    /// QUNDIS-specific metrics extraction
    fn extract_metrics(
        &self,
        manufacturer_id: &str,
        data: &[u8],
    ) -> Result<HashMap<String, f64>, MBusError> {
        if manufacturer_id != QUNDIS_MANUFACTURER_ID {
            return Ok(HashMap::new());
        }

        let mut metrics = HashMap::new();

        // Track QUNDIS-specific metrics
        metrics.insert("qundis_date_fields_processed".to_string(), 1.0);
        metrics.insert("payload_size_bytes".to_string(), data.len() as f64);

        // Check for potential date fields in payload
        let mut date_field_count = 0.0;
        for chunk in data.chunks(4) {
            if chunk.len() >= 2 {
                // Look for patterns that might be QUNDIS date fields
                let value = u16::from_le_bytes([chunk[0], chunk[1]]);

                // Heuristic: check if high nibble suggests year 2000-2099
                let year_candidate = ((value & 0xF000) >> 9) | (((value & 0x00E0) >> 5) + 2000);
                if (2000..=2099).contains(&year_candidate) {
                    date_field_count += 1.0;
                }
            }
        }

        if date_field_count > 0.0 {
            metrics.insert("potential_qundis_date_fields".to_string(), date_field_count);
        }

        Ok(metrics)
    }
}

/// The QUNDIS date quirk (Layer 2).
///
/// QUNDIS repurposes **VIF `0x04`** — Energy, 10¹ Wh in EN 13757-3 — as a date field
/// with the non-contiguous year packing decoded by
/// [`QundisHcaExtension::decode_mbus_value_date_g`]. Reading such a record per the
/// standard yields a confidently wrong energy value (the historical "10-year offset"
/// HCA bug), which is precisely the definition of a quirk: the standard interpretation
/// exists and is wrong for this device.
pub struct QundisDateQuirk {
    manifest: QuirkManifest,
}

impl Default for QundisDateQuirk {
    fn default() -> Self {
        Self::new()
    }
}

impl QundisDateQuirk {
    pub fn new() -> Self {
        Self {
            manifest: QuirkManifest {
                id: "qds-vif04-date",
                scope: VendorScope {
                    manufacturer: QUNDIS_MANUFACTURER_ID,
                    versions: None,
                    device_types: None,
                },
                deviation: "VIF 0x04 (Energy, 10^1 Wh per EN 13757-3) carries a date \
                            in QUNDIS MbusValueDateG bit packing",
                evidence: Evidence::Documented {
                    source: "QUNDIS MbusValueDateG encoding; pinned by the Dec-2015 \
                             (0x10F8) test vectors",
                },
                status: QuirkStatus::Verified,
            },
        }
    }
}

impl VendorQuirks for QundisDateQuirk {
    fn manifest(&self) -> &QuirkManifest {
        &self.manifest
    }

    fn reinterpret_record(&self, record: &mut MBusRecord) -> Option<QuirkApplied> {
        if record.drh.vib.vif != QUNDIS_VIF_DATE || record.data_len < 2 {
            return None;
        }
        let data = &record.data[..record.data_len];
        let raw_value = match data.len() {
            2 => u16::from_le_bytes([data[0], data[1]]) as u32,
            3 => u32::from_le_bytes([data[0], data[1], data[2], 0]),
            _ => u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        };
        let datetime = if data.len() >= 4 {
            QundisHcaExtension::decode_mbus_value_datetime_g(raw_value)
        } else {
            QundisHcaExtension::decode_mbus_value_date_g(raw_value)
        }
        .ok()?; // Undecodable bits: leave the standard reading rather than guess (P3).

        // Both are compile-time constants, so they cost nothing to store.
        record.unit = mbus_core::payload::text::UnitText::Static("Date");
        record.quantity = mbus_core::payload::text::QuantityText::Static("QUNDIS Date");
        record.is_numeric = false;
        record.value = crate::payload::record::MBusRecordValue::String(
            datetime.format("%Y-%m-%d %H:%M:%S").to_string(),
        );
        Some(QuirkApplied {
            quirk_id: self.manifest.id,
            provisional: self.manifest.status == QuirkStatus::Provisional,
        })
    }

    fn evidence_record(&self) -> &'static [u8] {
        // DIF 0x02, VIF 0x04, data 0xF8 0x10: December 2015 in QUNDIS encoding —
        // read per the standard this is 434.0 (10^1 Wh) energy; the deviation is
        // exactly that it is not.
        &[0x02, 0x04, 0xF8, 0x10]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_qundis_date_g_decoding() {
        // Test December 2015 (year=15, month=12)
        // For year 15 using QUNDIS formula: ((data & 0xF000) >> 9) | ((data & 0x00E0) >> 5) = 15
        // Year 15 = high nibble 1, low 3 bits 7 -> 0x1000 | 0x00E0
        // Month 12: bits 1-4 -> 12 << 1 = 0x18
        // Expected: 0x1000 | 0x00E0 | 0x18 = 0x10F8
        let raw_data = 0x10F8;

        println!("Testing raw_data: 0x{:04X}", raw_data);

        // Debug the bit extraction
        let year_parts = ((raw_data & 0xF000) >> 9) | ((raw_data & 0x00E0) >> 5);
        let year = year_parts + 2000;
        let month = (raw_data & 0x001E) >> 1;

        println!(
            "Extracted: year_parts=0x{:X}, year={}, month={}",
            year_parts, year, month
        );

        let result = QundisHcaExtension::decode_mbus_value_date_g(raw_data);
        if let Err(e) = &result {
            println!("Error: {}", e);
        }
        assert!(result.is_ok());

        let datetime = result.unwrap();
        assert_eq!(datetime.year(), 2015);
        assert_eq!(datetime.month(), 12);

        println!("Decoded QUNDIS date: {}", datetime.format("%Y-%m-%d"));
    }

    #[test]
    fn test_qundis_date_edge_cases() {
        // Test year 2000, month 1 (minimum year)
        // For year 0 using QUNDIS formula: result should be 0
        // Month 1: bits 1-4 -> 1 << 1 = 0x02
        let raw_2000 = 0x0002;
        let result = QundisHcaExtension::decode_mbus_value_date_g(raw_2000);
        assert!(result.is_ok());
        let datetime = result.unwrap();
        assert_eq!(datetime.year(), 2000);
        assert_eq!(datetime.month(), 1);

        // Test year 2099, month 12 (maximum year)
        // For year 99 = 0x63 using QUNDIS formula: need high bits 0xC000 and low bits 0x60
        // Month 12: bits 1-4 -> 12 << 1 = 0x18
        // Expected: 0xC000 | 0x60 | 0x18 = 0xC078
        let raw_2099 = 0xC078;
        let result = QundisHcaExtension::decode_mbus_value_date_g(raw_2099);
        assert!(result.is_ok());
        let datetime = result.unwrap();
        assert_eq!(datetime.year(), 2099);
        assert_eq!(datetime.month(), 12);
    }

    #[test]
    fn test_qundis_date_quirk_reinterprets_vif04() {
        use crate::payload::record::parse_variable_record;
        let quirk = QundisDateQuirk::new();

        // Record: DIF 0x02 (16-bit), VIF 0x04, data 0xF8 0x10 (Dec 2015 in QUNDIS
        // encoding) — the same vector the old extension-hook test used.
        let mut record = parse_variable_record(&[0x02, 0x04, 0xF8, 0x10]).unwrap();
        let applied = quirk.reinterpret_record(&mut record).expect("quirk fires");
        assert_eq!(applied.quirk_id, "qds-vif04-date");
        assert!(!applied.provisional);
        assert_eq!(record.unit, "Date");
        assert_eq!(record.quantity, "QUNDIS Date");
        match &record.value {
            crate::payload::record::MBusRecordValue::String(date_str) => {
                assert!(date_str.contains("2015"), "got {date_str}");
                assert!(date_str.contains("12"), "got {date_str}");
            }
            other => panic!("expected date string, got {other:?}"),
        }
    }

    #[test]
    fn test_quirk_scope_excludes_other_manufacturers() {
        use crate::vendors::context::DeviceIdentity;
        let quirk = QundisDateQuirk::new();
        // Scope matching happens OUTSIDE the quirk (registry/context), keyed by its
        // manifest — a KAM device never has this quirk in its context.
        let kam = DeviceIdentity {
            manufacturer: "KAM".into(),
            ..DeviceIdentity::default()
        };
        assert!(!quirk.manifest().scope.matches(&kam));
        let qds = DeviceIdentity {
            manufacturer: "QDS".into(),
            ..DeviceIdentity::default()
        };
        assert!(quirk.manifest().scope.matches(&qds));
    }

    #[test]
    fn test_device_enrichment() {
        let extension = QundisHcaExtension::new();
        let basic_info = VendorDeviceInfo {
            manufacturer_id: 0x1234,
            device_id: 0x39186386, // From user's sample
            version: 54,           // FW54
            device_type: 0x08,     // HCA type
            model: None,
            serial_number: None,
            firmware_version: None,
            additional_info: HashMap::new(),
        };

        let result = extension.enrich_device_header("QDS", basic_info).unwrap();

        assert!(result.is_some());
        let enriched = result.unwrap();
        assert_eq!(enriched.model, Some("HCA Heat Cost Allocator".to_string()));
        assert!(enriched.additional_info.contains_key("vendor_quirks"));
    }

    #[test]
    fn test_vendor_registry_integration() {
        // Test the registry with_defaults functionality
        let registry = crate::vendors::VendorRegistry::with_defaults().unwrap();

        // Verify QUNDIS is registered
        assert!(registry.has_extension("QDS"));
        assert!(registry.has_extension("qds")); // Should be case-insensitive

        // Test manufacturer listing
        let manufacturers = registry.registered_manufacturers();
        assert!(manufacturers.contains(&"QDS".to_string()));
    }

    #[test]
    fn test_qundis_metrics_extraction() {
        let extension = QundisHcaExtension::new();

        // Test with payload containing potential date fields
        let payload = vec![0xF8, 0x10, 0x02, 0x00]; // Contains our test date pattern
        let metrics = extension.extract_metrics("QDS", &payload).unwrap();

        assert_eq!(metrics.get("qundis_date_fields_processed"), Some(&1.0));
        assert_eq!(metrics.get("payload_size_bytes"), Some(&4.0));
        assert!(metrics.contains_key("potential_qundis_date_fields"));
    }

    #[test]
    fn test_invalid_date_handling() {
        // Test various invalid date scenarios

        // Invalid month (0)
        let raw_invalid_month = 0x1000; // Year bits set, month = 0
        let result = QundisHcaExtension::decode_mbus_value_date_g(raw_invalid_month);
        assert!(result.is_err());

        // Invalid month (13)
        let raw_invalid_month_high = 0x101A; // Month = 13
        let result = QundisHcaExtension::decode_mbus_value_date_g(raw_invalid_month_high);
        assert!(result.is_err());

        // Invalid year (too high)
        let raw_invalid_year = 0xF3E0 | 0x18; // Year parts = 0xFF = 255, month = 12
        let result = QundisHcaExtension::decode_mbus_value_date_g(raw_invalid_year);
        assert!(result.is_err());
    }

    #[test]
    fn test_datetime_decoding() {
        // Test the extended datetime format
        let raw_datetime = 0x10F8 | (15 << 24) | (30 << 18) | (15 << 17); // 15:30, day 15
        let result = QundisHcaExtension::decode_mbus_value_datetime_g(raw_datetime);

        assert!(result.is_ok());
        let datetime = result.unwrap();
        assert_eq!(datetime.year(), 2015);
        assert_eq!(datetime.month(), 12);
        // Note: day extraction might need adjustment based on actual QUNDIS format
    }

    #[test]
    fn test_real_world_dates() {
        // Test dates that would commonly appear in HCA devices

        // January 2020 (COVID year) - need year 20 = 0x14
        // To get 20: high_part (16) | low_part (4) = 20
        // High = 16 needs 0x2000, Low = 4 needs 0x0080
        let jan_2020 = 0x2000 | 0x0080 | 0x02; // Year parts for 20, month 1
        let result = QundisHcaExtension::decode_mbus_value_date_g(jan_2020);
        assert!(result.is_ok());
        let dt = result.unwrap();
        println!("Jan 2020 test: year={}, month={}", dt.year(), dt.month());
        assert_eq!(dt.year(), 2020);
        assert_eq!(dt.month(), 1);

        // December 2025 (future date) - need year 25 = 0x19
        // To get 25: high_part (24) | low_part (1) = 25
        // High = 24 needs 0x3000, Low = 1 needs 0x0020
        let dec_2025 = 0x3000 | 0x0020 | 0x18; // Year parts for 25, month 12
        let result = QundisHcaExtension::decode_mbus_value_date_g(dec_2025);
        assert!(result.is_ok());
        let dt = result.unwrap();
        println!("Dec 2025 test: year={}, month={}", dt.year(), dt.month());
        assert_eq!(dt.year(), 2025);
        assert_eq!(dt.month(), 12);
    }

    #[test]
    fn test_end_to_end_hca_parsing() {
        // End-to-end through THE decode path: a resolved context binds QUNDIS and the
        // record parser dispatches through it (vendor-layers P7/P9).
        use crate::vendors::{DecodeContext, DeviceIdentity, Integrity};
        let registry = crate::vendors::VendorRegistry::with_defaults().unwrap();
        let ctx = DecodeContext::resolve(
            Some(&registry),
            DeviceIdentity {
                manufacturer: "QDS".into(),
                ..DeviceIdentity::default()
            },
            Integrity::valid(),
        );

        // A variable M-Bus record with QUNDIS VIF 0x04 date, as a real HCA sends it.
        let mock_record_data = vec![
            // DIF: 2 bytes of data
            0x02, // VIF: 0x04 (QUNDIS date field)
            0x04, // Data: December 2015 in QUNDIS encoding (0x10F8)
            0xF8, 0x10,
        ];

        let result =
            crate::payload::record::parse_variable_record_in_context(&mock_record_data, &ctx);
        assert!(result.is_ok());
        let (record, consumed) = result.unwrap();
        assert_eq!(consumed, mock_record_data.len());

        // Verify the date was parsed correctly using QUNDIS algorithm
        if let crate::payload::record::MBusRecordValue::String(date_str) = &record.value {
            assert!(date_str.contains("2015"));
            assert!(date_str.contains("12"));
            println!("End-to-end test: decoded date = {}", date_str);
        } else {
            panic!("Expected string value for QUNDIS date");
        }

        // Verify vendor-specific metadata
        assert_eq!(record.unit, "Date");
        assert_eq!(record.quantity, "QUNDIS Date");
    }
}
