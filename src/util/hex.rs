//! # Hex Encoding/Decoding Utilities
//!
//! This module provides enhanced hex encoding and decoding functions that are
//! commonly used throughout the M-Bus and wM-Bus implementations for data
//! visualization, debugging, and test frame parsing.
//!
//! ## Features
//!
//! - Efficient hex encoding/decoding using the `hex` crate
//! - Pretty-printing with spacing and line breaks
//! - Error handling for invalid hex strings
//! - Support for both upper and lowercase output
//!
//! ## Usage
//!
//! ```rust
//! use mbus_rs::util::hex::{encode_hex, decode_hex, pretty_hex};
//!
//! let data = [0x68, 0x31, 0x31, 0x68];
//! let hex_str = encode_hex(&data);
//! assert_eq!(hex_str, "68313168");
//!
//! let decoded = decode_hex(&hex_str).unwrap();
//! assert_eq!(decoded, data);
//!
//! // Pretty printing for debugging
//! let pretty = pretty_hex(&data, 16);
//! println!("{}", pretty);
//! ```

use thiserror::Error;

/// Errors that can occur during hex operations
#[derive(Error, Debug, Clone, PartialEq)]
pub enum HexError {
    #[error("Invalid hex character: {0}")]
    InvalidCharacter(char),
    
    #[error("Odd number of hex characters: {0}")]
    OddLength(usize),
    
    #[error("Empty hex string")]
    EmptyString,
    
    #[error("Hex decoding error: {0}")]
    DecodeError(String),
}

/// Encode bytes to lowercase hex string
///
/// This is the primary encoding function used throughout the codebase
/// for consistent hex representation.
pub fn encode_hex(data: &[u8]) -> String {
    hex::encode(data)
}

/// Encode bytes to uppercase hex string
pub fn encode_hex_upper(data: &[u8]) -> String {
    hex::encode_upper(data)
}

/// Decode hex string to bytes
///
/// Accepts both uppercase and lowercase hex characters.
/// Whitespace is automatically stripped.
pub fn decode_hex(hex_str: &str) -> Result<Vec<u8>, HexError> {
    if hex_str.is_empty() {
        return Err(HexError::EmptyString);
    }
    
    // Remove whitespace and normalize
    let cleaned: String = hex_str.chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    
    if cleaned.len() % 2 != 0 {
        return Err(HexError::OddLength(cleaned.len()));
    }
    
    hex::decode(&cleaned).map_err(|e| HexError::DecodeError(e.to_string()))
}

/// Pretty-print hex data with spacing and line breaks
///
/// Creates a formatted hex dump similar to hexdump -C but with
/// better formatting for M-Bus frame analysis.
pub fn pretty_hex(data: &[u8], bytes_per_line: usize) -> String {
    if data.is_empty() {
        return String::new();
    }
    
    let mut result = String::new();
    
    for (i, chunk) in data.chunks(bytes_per_line).enumerate() {
        // Add offset
        result.push_str(&format!("{:04x}: ", i * bytes_per_line));
        
        // Add hex bytes with spacing
        for (j, byte) in chunk.iter().enumerate() {
            result.push_str(&format!("{:02x}", byte));
            if j % 2 == 1 {
                result.push(' '); // Space every 2 bytes
            }
        }
        
        // Pad if incomplete line
        if chunk.len() < bytes_per_line {
            let missing = bytes_per_line - chunk.len();
            for _ in 0..missing {
                result.push_str("  ");
                if (chunk.len() + 1) % 2 == 0 {
                    result.push(' ');
                }
            }
        }
        
        // Add ASCII representation
        result.push_str(" |");
        for &byte in chunk {
            if byte.is_ascii_graphic() || byte == b' ' {
                result.push(byte as char);
            } else {
                result.push('.');
            }
        }
        result.push('|');
        
        if i < (data.len() + bytes_per_line - 1) / bytes_per_line - 1 {
            result.push('\n');
        }
    }
    
    result
}

/// Format hex data for compact display (useful for logs)
///
/// Formats data as "68 31 31 68" with spaces between bytes.
pub fn format_hex_compact(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse hex string that may contain spaces or other separators
///
/// More lenient than decode_hex, strips all non-hex characters.
pub fn parse_hex_lenient(input: &str) -> Result<Vec<u8>, HexError> {
    let hex_chars: String = input.chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();
    
    if hex_chars.is_empty() {
        return Err(HexError::EmptyString);
    }
    
    if hex_chars.len() % 2 != 0 {
        return Err(HexError::OddLength(hex_chars.len()));
    }
    
    hex::decode(&hex_chars).map_err(|e| HexError::DecodeError(e.to_string()))
}

/// Convert a single hex byte string to u8
pub fn hex_byte(hex: &str) -> Result<u8, HexError> {
    if hex.len() != 2 {
        return Err(HexError::OddLength(hex.len()));
    }
    
    u8::from_str_radix(hex, 16)
        .map_err(|_| HexError::InvalidCharacter(hex.chars().find(|c| !c.is_ascii_hexdigit()).unwrap_or('?')))
}

/// Helper for creating test data from hex strings
///
/// This is commonly used in tests throughout the codebase.
/// Panics on invalid hex (intended for test code only).
pub fn hex_to_bytes(hex: &str) -> Vec<u8> {
    decode_hex(hex).expect("Invalid hex in test data")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let data = vec![0x68, 0x31, 0x31, 0x68, 0x08, 0x00, 0x72, 0x45];
        let encoded = encode_hex(&data);
        let decoded = decode_hex(&encoded).unwrap();
        assert_eq!(data, decoded);
    }

    #[test]
    fn test_encode_case() {
        let data = vec![0xAB, 0xCD, 0xEF];
        assert_eq!(encode_hex(&data), "abcdef");
        assert_eq!(encode_hex_upper(&data), "ABCDEF");
    }

    #[test]
    fn test_decode_with_whitespace() {
        let hex = "68 31 31 68";
        let expected = vec![0x68, 0x31, 0x31, 0x68];
        assert_eq!(decode_hex(hex).unwrap(), expected);
    }

    #[test]
    fn test_pretty_hex() {
        let data = vec![0x68, 0x31, 0x31, 0x68, 0x08, 0x00, 0x72, 0x45];
        let pretty = pretty_hex(&data, 8);
        assert!(pretty.contains("6831"));
        assert!(pretty.contains("|"));
    }

    #[test]
    fn test_format_compact() {
        let data = vec![0x68, 0x31, 0x31, 0x68];
        assert_eq!(format_hex_compact(&data), "68 31 31 68");
    }

    #[test]
    fn test_parse_lenient() {
        let input = "68-31:31 68";
        let expected = vec![0x68, 0x31, 0x31, 0x68];
        assert_eq!(parse_hex_lenient(input).unwrap(), expected);
    }

    #[test]
    fn test_hex_byte() {
        assert_eq!(hex_byte("68").unwrap(), 0x68);
        assert_eq!(hex_byte("FF").unwrap(), 0xFF);
        assert_eq!(hex_byte("ab").unwrap(), 0xAB);
    }

    #[test]
    fn test_hex_to_bytes() {
        let data = hex_to_bytes("68313168");
        assert_eq!(data, vec![0x68, 0x31, 0x31, 0x68]);
    }

    #[test]
    fn test_errors() {
        assert!(decode_hex("").is_err());
        assert!(decode_hex("1").is_err()); // Odd length
        assert!(decode_hex("GG").is_err()); // Invalid character
    }
}