//! Secondary Addressing Implementation for M-Bus (EN 13757-2 Section 5.3)
//!
//! This module implements secondary addressing for M-Bus devices, allowing
//! communication with devices using their 8-byte unique identifiers instead
//! of primary addresses (1-250).

use crate::error::MBusError;
use crate::vendors;
use nom::{bytes::complete::take, IResult};
use std::fmt;

/// 8-byte secondary address as defined in EN 13757-2
/// Contains device identification, manufacturer, version, and device type
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecondaryAddress {
    /// Device identification number (4 bytes, little-endian)
    pub device_id: u32,
    /// Manufacturer code (2 bytes, little-endian)
    pub manufacturer: u16,
    /// Version number (1 byte)
    pub version: u8,
    /// Device type code (1 byte)
    pub device_type: u8,
}

impl SecondaryAddress {
    /// Create a new secondary address
    pub fn new(device_id: u32, manufacturer: u16, version: u8, device_type: u8) -> Self {
        Self {
            device_id,
            manufacturer,
            version,
            device_type,
        }
    }

    /// Convert secondary address to 8-byte array (little-endian format)
    pub fn to_bytes(&self) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        bytes[0..4].copy_from_slice(&self.device_id.to_le_bytes());
        bytes[4..6].copy_from_slice(&self.manufacturer.to_le_bytes());
        bytes[6] = self.version;
        bytes[7] = self.device_type;
        bytes
    }

    /// Create secondary address from 8-byte array (little-endian format)
    pub fn from_bytes(data: &[u8]) -> Result<Self, MBusError> {
        if data.len() < 8 {
            return Err(MBusError::FrameParseError(
                "Secondary address requires 8 bytes".to_string(),
            ));
        }

        Ok(SecondaryAddress {
            device_id: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            manufacturer: u16::from_le_bytes([data[4], data[5]]),
            version: data[6],
            device_type: data[7],
        })
    }

    /// Create secondary address with vendor enrichment
    ///
    /// This function allows vendor extensions to enrich or validate the device
    /// information extracted from M-Bus frames.
    pub fn from_bytes_with_vendor(
        data: &[u8],
        registry: Option<&vendors::VendorRegistry>,
    ) -> Result<Self, MBusError> {
        let basic_addr = Self::from_bytes(data)?;

        if let Some(reg) = registry {
            // Convert manufacturer ID to string
            let mfr_code = vendors::manufacturer_id_to_string(basic_addr.manufacturer);

            // Convert to VendorDeviceInfo for hook
            let basic_info = vendors::VendorDeviceInfo::from(basic_addr.clone());

            // Try vendor enrichment
            if let Some(enriched) = vendors::dispatch_header_hook(reg, &mfr_code, basic_info)? {
                // Convert back to SecondaryAddress
                return Ok(SecondaryAddress {
                    device_id: enriched.device_id,
                    manufacturer: enriched.manufacturer_id,
                    version: enriched.version,
                    device_type: enriched.device_type,
                });
            }
        }

        Ok(basic_addr)
    }

    /// Check if this secondary address matches a wildcard pattern
    /// Wildcard bytes are represented as 0xF
    pub fn matches_wildcard(&self, pattern: &[u8; 8]) -> bool {
        let self_bytes = self.to_bytes();
        for (&pattern_byte, &self_byte) in pattern.iter().zip(self_bytes.iter()) {
            if pattern_byte != 0xF && pattern_byte != self_byte {
                return false;
            }
        }
        true
    }

    /// Create a wildcard pattern from this address with wildcards at specified positions
    pub fn to_wildcard_pattern(&self, wildcard_positions: &[usize]) -> [u8; 8] {
        let mut pattern = self.to_bytes();
        for &pos in wildcard_positions {
            if pos < 8 {
                pattern[pos] = 0xF;
            }
        }
        pattern
    }
}

impl fmt::Display for SecondaryAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ID:{:08X} MFG:{:04X} VER:{:02X} TYPE:{:02X}",
            self.device_id, self.manufacturer, self.version, self.device_type
        )
    }
}

/// Result of wildcard search collision detection
#[derive(Debug, Clone, PartialEq)]
pub enum WildcardResult {
    /// No devices responded to the wildcard pattern
    None,
    /// Exactly one device responded (no collision)
    Single,
    /// Multiple devices responded (collision detected)
    Multiple,
}

/// Wildcard search manager for device discovery
#[derive(Debug)]
pub struct WildcardSearchManager {
    /// Maximum recursion depth for wildcard narrowing
    max_depth: usize,
    /// Discovered secondary addresses
    discovered: Vec<SecondaryAddress>,
}

impl WildcardSearchManager {
    /// Create a new wildcard search manager
    pub fn new() -> Self {
        Self {
            max_depth: 8, // Maximum 8 bytes to narrow down
            discovered: Vec::new(),
        }
    }

    /// Set maximum search depth (number of bytes to narrow down)
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth.min(8);
        self
    }

    /// Get all discovered secondary addresses
    pub fn discovered_addresses(&self) -> &[SecondaryAddress] {
        &self.discovered
    }

    /// Clear discovered addresses
    pub fn clear(&mut self) {
        self.discovered.clear();
    }

    /// Generate wildcard search sequence for systematic device discovery
    /// This implements the collision resolution algorithm from EN 13757-2
    pub fn generate_search_patterns(&self) -> Vec<[u8; 8]> {
        let mut patterns = Vec::new();

        // Start with all wildcards
        let mut base_pattern = [0xF; 8];
        self.generate_patterns_recursive(&mut base_pattern, 0, &mut patterns);

        patterns
    }

    /// Recursive function to generate wildcard patterns with collision resolution
    /// Implements EN 13757-2 binary search tree algorithm
    fn generate_patterns_recursive(
        &self,
        pattern: &mut [u8; 8],
        position: usize,
        patterns: &mut Vec<[u8; 8]>,
    ) {
        if position >= self.max_depth {
            patterns.push(*pattern);
            return;
        }

        // For full tree search: narrow down by nibbles (4-bit values)
        // This follows the EN 13757-2 collision resolution strategy

        // First, try with wildcard at current position to detect collisions
        patterns.push(*pattern);

        // Then narrow down by trying each nibble value
        let byte_pos = position / 2; // Which byte we're narrowing
        let is_high_nibble = position % 2 == 0; // High or low nibble

        if byte_pos < 8 {
            // Save original value
            let original = pattern[byte_pos];

            // Try each nibble value (0x0 to 0xF)
            for nibble in 0x0..=0xF {
                if is_high_nibble {
                    // Modify high nibble, keep low nibble
                    pattern[byte_pos] = (nibble << 4) | (original & 0x0F);
                } else {
                    // Modify low nibble, keep high nibble
                    pattern[byte_pos] = (original & 0xF0) | nibble;
                }

                // Recursively narrow down further positions
                if position + 1 < self.max_depth {
                    self.generate_patterns_recursive(pattern, position + 1, patterns);
                } else {
                    patterns.push(*pattern);
                }
            }

            // Restore original wildcard
            pattern[byte_pos] = original;
        }
    }

    /// Perform wildcard tree search with collision resolution
    /// Returns narrowed patterns for collision resolution
    pub fn narrow_wildcard_collision(
        &self,
        base_pattern: &[u8; 8],
        collision_byte: usize,
    ) -> Vec<[u8; 8]> {
        let mut patterns = Vec::new();
        let mut pattern = *base_pattern;

        // For the collision byte, try both nibbles separately
        if collision_byte < 8 && pattern[collision_byte] == 0xFF {
            // Try high nibble values (0x0F to 0xFF step 0x10)
            for high_nibble in 0x0..=0xF {
                pattern[collision_byte] = (high_nibble << 4) | 0x0F;
                patterns.push(pattern);
            }

            // Try low nibble values (0xF0 to 0xFF step 0x01)
            for low_nibble in 0x0..=0xF {
                pattern[collision_byte] = 0xF0 | low_nibble;
                patterns.push(pattern);
            }
        }

        patterns
    }

    /// Add a discovered secondary address
    pub fn add_discovered(&mut self, address: SecondaryAddress) {
        if !self.discovered.contains(&address) {
            self.discovered.push(address);
        }
    }
}

impl Default for WildcardSearchManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse secondary address from M-Bus frame user data
/// Expected format: 8-byte secondary address at start of user data
pub fn parse_secondary_from_frame_data(data: &[u8]) -> IResult<&[u8], SecondaryAddress> {
    let (remaining, secondary_bytes) = take(8usize)(data)?;

    let secondary_address = SecondaryAddress::from_bytes(secondary_bytes)
        .map_err(|_| nom::Err::Error(nom::error::Error::new(data, nom::error::ErrorKind::Tag)))?;

    Ok((remaining, secondary_address))
}

/// Build M-Bus frame for secondary address selection
/// Uses primary address 0xFD and CI field 0x52 for SND_UD command
pub fn build_secondary_selection_frame(secondary_pattern: &[u8; 8]) -> Vec<u8> {
    let mut frame = Vec::new();

    // M-Bus Long Frame format with secondary addressing
    frame.push(0x68); // Start byte 1
    frame.push(0x0B); // L field (11 bytes: C + A + CI + 8 bytes secondary)
    frame.push(0x0B); // L field (repeated)
    frame.push(0x68); // Start byte 2
    frame.push(0x53); // C field: SND_UD (Send User Data)
    frame.push(0xFD); // A field: 253 (secondary addressing indicator)
    frame.push(0x52); // CI field: Secondary addressing selection

    // 8-byte secondary address pattern (with wildcards)
    frame.extend_from_slice(secondary_pattern);

    // Calculate checksum (C + A + CI + data bytes)
    let mut checksum = 0u8;
    for &byte in &frame[4..] {
        checksum = checksum.wrapping_add(byte);
    }
    frame.push(checksum);

    frame.push(0x16); // Stop byte

    frame
}

/// Build advanced secondary search frame with VIF-based search
/// Supports VIF=0x78 (search by fabrication number), 0x79 (by medium), 0x7A (by ID)
pub fn build_vif_search_frame(search_type: VifSearchType, pattern: &[u8]) -> Vec<u8> {
    let mut frame = Vec::new();

    // Determine CI and data based on search type
    let (ci, data_len) = match search_type {
        VifSearchType::FabricationNumber => (0x78, 4), // 4-byte fabrication number
        VifSearchType::Medium => (0x79, 1),            // 1-byte medium
        VifSearchType::Identification => (0x7A, 4),    // 4-byte ID
    };

    let l_field = 3 + data_len; // C + A + CI + data

    // Build frame
    frame.push(0x68); // Start byte 1
    frame.push(l_field);
    frame.push(l_field); // L field repeated
    frame.push(0x68); // Start byte 2
    frame.push(0x53); // C field: SND_UD
    frame.push(0xFD); // A field: Secondary addressing
    frame.push(ci); // CI field: VIF-based search type

    // Add search pattern (padded or truncated to expected length)
    let pattern_len = data_len as usize;
    if pattern.len() >= pattern_len {
        frame.extend_from_slice(&pattern[..pattern_len]);
    } else {
        frame.extend_from_slice(pattern);
        // Pad with wildcards (0xFF)
        for _ in pattern.len()..pattern_len {
            frame.push(0xFF);
        }
    }

    // Calculate checksum
    let mut checksum = 0u8;
    for &byte in &frame[4..] {
        checksum = checksum.wrapping_add(byte);
    }
    frame.push(checksum);
    frame.push(0x16); // Stop byte

    frame
}

/// VIF-based secondary search types according to EN 13757-3
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VifSearchType {
    /// Search by fabrication number (VIF=0x78)
    FabricationNumber,
    /// Search by medium/device type (VIF=0x79)
    Medium,
    /// Search by identification number (VIF=0x7A)
    Identification,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secondary_address_creation() {
        let addr = SecondaryAddress::new(0x12345678, 0xABCD, 0x01, 0x02);

        assert_eq!(addr.device_id, 0x12345678);
        assert_eq!(addr.manufacturer, 0xABCD);
        assert_eq!(addr.version, 0x01);
        assert_eq!(addr.device_type, 0x02);
    }

    #[test]
    fn test_secondary_address_bytes_conversion() {
        let addr = SecondaryAddress::new(0x12345678, 0xABCD, 0x01, 0x02);
        let bytes = addr.to_bytes();

        // Should be in little-endian format
        assert_eq!(bytes, [0x78, 0x56, 0x34, 0x12, 0xCD, 0xAB, 0x01, 0x02]);

        // Round-trip conversion
        let addr2 = SecondaryAddress::from_bytes(&bytes).unwrap();
        assert_eq!(addr, addr2);
    }

    #[test]
    fn test_wildcard_matching() {
        let addr = SecondaryAddress::new(0x12345678, 0xABCD, 0x01, 0x02);

        // Exact match
        let exact_pattern = [0x78, 0x56, 0x34, 0x12, 0xCD, 0xAB, 0x01, 0x02];
        assert!(addr.matches_wildcard(&exact_pattern));

        // Wildcard match (first byte wildcard)
        let wildcard_pattern = [0xF, 0x56, 0x34, 0x12, 0xCD, 0xAB, 0x01, 0x02];
        assert!(addr.matches_wildcard(&wildcard_pattern));

        // All wildcards
        let all_wildcards = [0xF; 8];
        assert!(addr.matches_wildcard(&all_wildcards));

        // No match
        let no_match = [0x77, 0x56, 0x34, 0x12, 0xCD, 0xAB, 0x01, 0x02];
        assert!(!addr.matches_wildcard(&no_match));
    }

    #[test]
    fn test_wildcard_search_manager() {
        let mut manager = WildcardSearchManager::new();

        let addr1 = SecondaryAddress::new(0x11111111, 0x1111, 0x01, 0x01);
        let addr2 = SecondaryAddress::new(0x22222222, 0x2222, 0x02, 0x02);

        manager.add_discovered(addr1.clone());
        manager.add_discovered(addr2.clone());

        assert_eq!(manager.discovered_addresses().len(), 2);
        assert!(manager.discovered_addresses().contains(&addr1));
        assert!(manager.discovered_addresses().contains(&addr2));
    }

    #[test]
    fn test_secondary_selection_frame_building() {
        let pattern = [0x12, 0x34, 0x56, 0x78, 0xAB, 0xCD, 0xEF, 0x01];
        let frame = build_secondary_selection_frame(&pattern);

        // Check frame structure
        assert_eq!(frame[0], 0x68); // Start 1
        assert_eq!(frame[1], 0x0B); // L field
        assert_eq!(frame[2], 0x0B); // L field (repeated)
        assert_eq!(frame[3], 0x68); // Start 2
        assert_eq!(frame[4], 0x53); // C field: SND_UD
        assert_eq!(frame[5], 0xFD); // A field: 253 (secondary addressing)
        assert_eq!(frame[6], 0x52); // CI field: Secondary selection

        // Check secondary address pattern
        assert_eq!(&frame[7..15], &pattern);

        // Check frame termination
        assert_eq!(frame[frame.len() - 1], 0x16); // Stop byte
    }

    #[test]
    fn test_parse_secondary_from_frame_data() {
        let data = [0x78, 0x56, 0x34, 0x12, 0xCD, 0xAB, 0x01, 0x02, 0xFF, 0xFF];
        let (remaining, addr) = parse_secondary_from_frame_data(&data).unwrap();

        assert_eq!(addr.device_id, 0x12345678);
        assert_eq!(addr.manufacturer, 0xABCD);
        assert_eq!(addr.version, 0x01);
        assert_eq!(addr.device_type, 0x02);
        assert_eq!(remaining, &[0xFF, 0xFF]);
    }

    #[test]
    fn test_vif_search_frame_fabrication() {
        // Test VIF=0x78 search by fabrication number
        let pattern = [0x12, 0x34, 0x56, 0x78];
        let frame = build_vif_search_frame(VifSearchType::FabricationNumber, &pattern);

        // Verify frame structure
        assert_eq!(frame[0], 0x68); // Start 1
        assert_eq!(frame[1], 0x07); // L field (3 + 4 bytes data)
        assert_eq!(frame[2], 0x07); // L field repeated
        assert_eq!(frame[3], 0x68); // Start 2
        assert_eq!(frame[4], 0x53); // C field: SND_UD
        assert_eq!(frame[5], 0xFD); // A field: Secondary addressing
        assert_eq!(frame[6], 0x78); // CI field: Search by fabrication number

        // Verify search pattern
        assert_eq!(&frame[7..11], &pattern);

        // Verify frame termination
        assert_eq!(frame[frame.len() - 1], 0x16); // Stop byte

        // Verify checksum
        let mut expected_checksum = 0u8;
        for &byte in &frame[4..frame.len() - 2] {
            expected_checksum = expected_checksum.wrapping_add(byte);
        }
        assert_eq!(frame[frame.len() - 2], expected_checksum);
    }

    #[test]
    fn test_vif_search_frame_medium() {
        // Test VIF=0x79 search by medium
        let pattern = [0x04]; // Water medium
        let frame = build_vif_search_frame(VifSearchType::Medium, &pattern);

        assert_eq!(frame[0], 0x68); // Start 1
        assert_eq!(frame[1], 0x04); // L field (3 + 1 byte data)
        assert_eq!(frame[2], 0x04); // L field repeated
        assert_eq!(frame[3], 0x68); // Start 2
        assert_eq!(frame[4], 0x53); // C field: SND_UD
        assert_eq!(frame[5], 0xFD); // A field: Secondary addressing
        assert_eq!(frame[6], 0x79); // CI field: Search by medium
        assert_eq!(frame[7], 0x04); // Medium: Water

        assert_eq!(frame[frame.len() - 1], 0x16); // Stop byte
    }

    #[test]
    fn test_vif_search_frame_identification() {
        // Test VIF=0x7A search by identification
        let pattern = [0xAB, 0xCD, 0xEF]; // Partial ID (will be padded)
        let frame = build_vif_search_frame(VifSearchType::Identification, &pattern);

        assert_eq!(frame[6], 0x7A); // CI field: Search by ID

        // Verify padding with wildcards
        assert_eq!(&frame[7..10], &pattern);
        assert_eq!(frame[10], 0xFF); // Padded with wildcard
    }

    #[test]
    fn test_vif_search_pattern_truncation() {
        // Test that long patterns are truncated
        let pattern = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]; // 6 bytes
        let frame = build_vif_search_frame(VifSearchType::FabricationNumber, &pattern);

        // Should only use first 4 bytes for fabrication number
        assert_eq!(&frame[7..11], &[0x11, 0x22, 0x33, 0x44]);
    }
}
