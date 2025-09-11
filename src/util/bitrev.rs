//! # Bit Reversal Utilities
//!
//! This module provides bit manipulation functions, particularly the critical
//! `rev8` function needed for wM-Bus communication where data transmission
//! order differs between the protocol specification and radio hardware.
//!
//! ## Background
//!
//! wM-Bus transmits data MSB-first on the wire, but many radio modules like
//! the RFM69 expect data LSB-first. This requires bit reversal to maintain
//! proper frame synchronization and data integrity.
//!
//! ## Usage
//!
//! ```rust
//! use mbus_rs::util::bitrev::rev8;
//!
//! // Convert wM-Bus sync bytes
//! assert_eq!(rev8(0xB3), 0xCD); // Type A sync
//! assert_eq!(rev8(0xBC), 0x3D); // Type B sync
//! ```

/// Reverse bits in a byte (MSB-first to LSB-first conversion)
///
/// This addresses the common bit ordering challenge between wM-Bus protocol 
/// specification and radio hardware implementation.
///
/// wM-Bus transmits data MSB-first on the wire, but RFM69 expects LSB-first.
/// This function performs the necessary bit reversal using an efficient
/// bit manipulation algorithm.
///
/// # Examples
///
/// ```rust
/// use mbus_rs::util::bitrev::rev8;
///
/// // Basic bit reversal
/// assert_eq!(rev8(0b10000000), 0b00000001);
/// assert_eq!(rev8(0b01000000), 0b00000010);
/// assert_eq!(rev8(0b10101010), 0b01010101);
///
/// // wM-Bus sync byte conversion
/// assert_eq!(rev8(0xB3), 0xCD); // Type A sync: raw → normalized
/// assert_eq!(rev8(0xBC), 0x3D); // Type B sync: raw → normalized
/// ```
#[inline]
pub fn rev8(mut byte: u8) -> u8 {
    // Efficient bit reversal using bit manipulation
    // This is faster than table lookups for single bytes
    
    // Swap nibbles (4-bit groups)
    byte = (byte & 0xF0) >> 4 | (byte & 0x0F) << 4;
    
    // Swap pairs within nibbles (2-bit groups)
    byte = (byte & 0xCC) >> 2 | (byte & 0x33) << 2;
    
    // Swap individual bits within pairs (1-bit groups)
    byte = (byte & 0xAA) >> 1 | (byte & 0x55) << 1;
    
    byte
}

/// Reverse bits in a 16-bit value
///
/// Extends the 8-bit reversal to 16-bit values, useful for
/// CRC calculations and multi-byte field processing.
#[inline]
pub fn rev16(value: u16) -> u16 {
    let low = rev8((value & 0xFF) as u8) as u16;
    let high = rev8((value >> 8) as u8) as u16;
    (low << 8) | high
}

/// Reverse bits in a 32-bit value
///
/// For completeness, though rarely needed in M-Bus protocols.
#[inline]
pub fn rev32(value: u32) -> u32 {
    let b0 = rev8((value & 0xFF) as u8) as u32;
    let b1 = rev8(((value >> 8) & 0xFF) as u8) as u32;
    let b2 = rev8(((value >> 16) & 0xFF) as u8) as u32;
    let b3 = rev8(((value >> 24) & 0xFF) as u8) as u32;
    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
}

/// Reverse a slice of bytes in-place
///
/// Applies bit reversal to each byte in the slice, modifying
/// the original data. Useful for bulk processing of wM-Bus frames.
pub fn rev8_slice(data: &mut [u8]) {
    for byte in data.iter_mut() {
        *byte = rev8(*byte);
    }
}

/// Reverse a slice of bytes and return a new vector
///
/// Non-destructive version that returns a new vector with
/// bit-reversed bytes.
pub fn rev8_vec(data: &[u8]) -> Vec<u8> {
    data.iter().map(|&byte| rev8(byte)).collect()
}

/// Check if a byte needs bit reversal for wM-Bus normalization
///
/// Some bytes in wM-Bus frames may already be in the correct
/// bit order depending on the processing pipeline stage.
pub fn needs_reversal(byte: u8, context: BitContext) -> bool {
    match context {
        BitContext::WMBusSync => matches!(byte, 0xB3 | 0xBC), // Raw sync bytes
        BitContext::Always => true,
        BitContext::Never => false,
    }
}

/// Context for determining when bit reversal is needed
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BitContext {
    /// Always apply bit reversal
    Always,
    /// Never apply bit reversal
    Never,
    /// Apply only for wM-Bus sync bytes that need normalization
    WMBusSync,
}

/// Utilities for working with bit patterns in wM-Bus frames
pub mod patterns {
    use super::*;
    
    /// wM-Bus Type A sync byte (before bit reversal)
    pub const WMBUS_SYNC_A_RAW: u8 = 0xB3;
    /// wM-Bus Type B sync byte (before bit reversal)
    pub const WMBUS_SYNC_B_RAW: u8 = 0xBC;
    
    /// wM-Bus Type A sync byte (after bit reversal)
    pub const WMBUS_SYNC_A_NORM: u8 = 0xCD;
    /// wM-Bus Type B sync byte (after bit reversal)
    pub const WMBUS_SYNC_B_NORM: u8 = 0x3D;
    
    /// Check if a byte is a raw wM-Bus sync pattern
    pub fn is_raw_sync(byte: u8) -> bool {
        matches!(byte, WMBUS_SYNC_A_RAW | WMBUS_SYNC_B_RAW)
    }
    
    /// Check if a byte is a normalized wM-Bus sync pattern
    pub fn is_normalized_sync(byte: u8) -> bool {
        matches!(byte, WMBUS_SYNC_A_NORM | WMBUS_SYNC_B_NORM)
    }
    
    /// Normalize a sync byte (apply bit reversal if needed)
    pub fn normalize_sync(byte: u8) -> u8 {
        match byte {
            WMBUS_SYNC_A_RAW => WMBUS_SYNC_A_NORM,
            WMBUS_SYNC_B_RAW => WMBUS_SYNC_B_NORM,
            _ => byte, // Already normalized or not a sync byte
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::patterns::*;

    #[test]
    fn test_rev8_basic() {
        // Test with all zeros and all ones
        assert_eq!(rev8(0b00000000), 0b00000000);
        assert_eq!(rev8(0b11111111), 0b11111111);
        
        // Test single bit positions
        assert_eq!(rev8(0b10000000), 0b00000001);
        assert_eq!(rev8(0b01000000), 0b00000010);
        assert_eq!(rev8(0b00100000), 0b00000100);
        assert_eq!(rev8(0b00010000), 0b00001000);
        assert_eq!(rev8(0b00001000), 0b00010000);
        assert_eq!(rev8(0b00000100), 0b00100000);
        assert_eq!(rev8(0b00000010), 0b01000000);
        assert_eq!(rev8(0b00000001), 0b10000000);
    }

    #[test]
    fn test_rev8_patterns() {
        // Test alternating patterns
        assert_eq!(rev8(0b10101010), 0b01010101);
        assert_eq!(rev8(0b01010101), 0b10101010);
        
        // Test other common patterns
        assert_eq!(rev8(0b11110000), 0b00001111);
        assert_eq!(rev8(0b00001111), 0b11110000);
    }

    #[test]
    fn test_wmbus_sync_reversal() {
        // Test wM-Bus sync byte conversion (critical for Fix #1)
        assert_eq!(rev8(WMBUS_SYNC_A_RAW), WMBUS_SYNC_A_NORM); // 0xB3 → 0xCD
        assert_eq!(rev8(WMBUS_SYNC_B_RAW), WMBUS_SYNC_B_NORM); // 0xBC → 0x3D
        
        // Test reverse direction
        assert_eq!(rev8(WMBUS_SYNC_A_NORM), WMBUS_SYNC_A_RAW);
        assert_eq!(rev8(WMBUS_SYNC_B_NORM), WMBUS_SYNC_B_RAW);
    }

    #[test]
    fn test_rev8_invertible() {
        // Test that rev8 is its own inverse
        for i in 0..=255u8 {
            assert_eq!(rev8(rev8(i)), i);
        }
    }

    #[test]
    fn test_rev16() {
        assert_eq!(rev16(0x0000), 0x0000);
        assert_eq!(rev16(0xFFFF), 0xFFFF);
        assert_eq!(rev16(0x8000), 0x0001);
        assert_eq!(rev16(0x0080), 0x0100);
        
        // Test with wM-Bus sync pattern
        let sync_16 = (WMBUS_SYNC_A_RAW as u16) << 8 | (WMBUS_SYNC_B_RAW as u16);
        let expected = (WMBUS_SYNC_B_NORM as u16) << 8 | (WMBUS_SYNC_A_NORM as u16);
        assert_eq!(rev16(sync_16), expected);
    }

    #[test]
    fn test_rev32() {
        assert_eq!(rev32(0x00000000), 0x00000000);
        assert_eq!(rev32(0xFFFFFFFF), 0xFFFFFFFF);
        assert_eq!(rev32(0x80000000), 0x00000001);
        assert_eq!(rev32(0x00008000), 0x00010000);
    }

    #[test]
    fn test_rev8_slice() {
        let mut data = vec![WMBUS_SYNC_A_RAW, WMBUS_SYNC_B_RAW, 0x12, 0x34];
        let expected = vec![WMBUS_SYNC_A_NORM, WMBUS_SYNC_B_NORM, rev8(0x12), rev8(0x34)];
        
        rev8_slice(&mut data);
        assert_eq!(data, expected);
    }

    #[test]
    fn test_rev8_vec() {
        let data = vec![WMBUS_SYNC_A_RAW, WMBUS_SYNC_B_RAW, 0x12, 0x34];
        let expected = vec![WMBUS_SYNC_A_NORM, WMBUS_SYNC_B_NORM, rev8(0x12), rev8(0x34)];
        
        let result = rev8_vec(&data);
        assert_eq!(result, expected);
        assert_eq!(data, vec![WMBUS_SYNC_A_RAW, WMBUS_SYNC_B_RAW, 0x12, 0x34]); // Original unchanged
    }

    #[test]
    fn test_needs_reversal() {
        assert!(needs_reversal(WMBUS_SYNC_A_RAW, BitContext::WMBusSync));
        assert!(needs_reversal(WMBUS_SYNC_B_RAW, BitContext::WMBusSync));
        assert!(!needs_reversal(WMBUS_SYNC_A_NORM, BitContext::WMBusSync));
        assert!(!needs_reversal(0x12, BitContext::WMBusSync));
        
        assert!(needs_reversal(0x12, BitContext::Always));
        assert!(!needs_reversal(0x12, BitContext::Never));
    }

    #[test]
    fn test_sync_patterns() {
        assert!(is_raw_sync(WMBUS_SYNC_A_RAW));
        assert!(is_raw_sync(WMBUS_SYNC_B_RAW));
        assert!(!is_raw_sync(WMBUS_SYNC_A_NORM));
        assert!(!is_raw_sync(0x12));
        
        assert!(is_normalized_sync(WMBUS_SYNC_A_NORM));
        assert!(is_normalized_sync(WMBUS_SYNC_B_NORM));
        assert!(!is_normalized_sync(WMBUS_SYNC_A_RAW));
        assert!(!is_normalized_sync(0x12));
    }

    #[test]
    fn test_normalize_sync() {
        assert_eq!(normalize_sync(WMBUS_SYNC_A_RAW), WMBUS_SYNC_A_NORM);
        assert_eq!(normalize_sync(WMBUS_SYNC_B_RAW), WMBUS_SYNC_B_NORM);
        assert_eq!(normalize_sync(WMBUS_SYNC_A_NORM), WMBUS_SYNC_A_NORM); // Already normalized
        assert_eq!(normalize_sync(0x12), 0x12); // Not a sync byte
    }
}