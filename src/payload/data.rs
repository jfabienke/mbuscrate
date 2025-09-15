use crate::constants::*;
use crate::error::MBusError;
use crate::payload::data_encoding::mbus_data_str_decode;
use crate::payload::vif::{normalize_vib, parse_vib, VifInfo};
use nom::{
    bytes::complete::take,
    combinator::map,
    number::complete::{be_u32, be_u8},
    IResult,
};
use std::time::SystemTime;

/// Type alias for complex nom parser result in special VIF chain parsing
type SpecialVifResult<'a> = Result<(String, f64, String), nom::Err<nom::error::Error<&'a [u8]>>>;

/// Enhanced data record for standards-compliant variable block parsing
#[derive(Debug, Clone)]
pub struct EnhancedDataRecord {
    pub dif_chain: Vec<u8>,     // DIF + up to 10 DIFEs
    pub vif_chain: Vec<u8>,     // VIF + VIFEs
    pub value: MBusRecordValue, // Parsed value
    pub tariff: u8,             // From DIFE bits [5:4]
    pub storage_number: u16,    // Accumulated from DIF/DIFE chain
    pub unit: String,           // Resolved from VIF chain
    pub quantity: String,       // Physical quantity
    pub timestamp: SystemTime,
    pub is_numeric: bool,
}

#[derive(Debug)]
pub struct MBusDataRecord {
    pub timestamp: SystemTime,
    pub storage_number: u32,
    pub tariff: i32,
    pub device: i32,
    pub is_numeric: bool,
    pub value: MBusRecordValue,
    pub unit: String,
    pub function_medium: String,
    pub quantity: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MBusRecordValue {
    Numeric(f64),
    String(String),
}

pub fn mbus_data_record_decode(input: &[u8]) -> IResult<&[u8], MBusDataRecord> {
    let (input, timestamp) = map(be_u32, |t| {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(t.into())
    })(input)?;
    let (input, dif) = be_u8(input)?;
    let (input, vib) = parse_vib(input)?;

    let (unit, value, quantity) =
        normalize_vib(&vib).unwrap_or((String::new(), 0.0, String::new()));

    let (input, data) = take(mbus_dif_datalength_lookup(dif))(input)?;

    let mut record = MBusDataRecord {
        timestamp,
        storage_number: mbus_data_record_storage_number(&vib),
        tariff: mbus_data_record_tariff(&vib),
        device: mbus_data_record_device(&vib),
        is_numeric: true,
        value: MBusRecordValue::Numeric(value),
        unit,
        function_medium: mbus_data_record_function(dif).to_string(),
        quantity,
    };

    if (dif & MBUS_DATA_RECORD_DIF_MASK_DATA) == 0x0D {
        // Variable-length data
        record.is_numeric = false;
        let mut s = String::new();
        mbus_data_str_decode(&mut s, data, data.len());
        record.value = MBusRecordValue::String(s);
    } else {
        let value_result = mbus_data_record_value_decode(dif, data);
        if let Ok(decoded_value) = value_result {
            record.value = MBusRecordValue::Numeric(decoded_value);
        } else {
            crate::logging::log_error(&format!(
                "Error decoding data record value: {value_result:?}"
            ));
            record.value = MBusRecordValue::Numeric(0.0);
        }
    }

    Ok((input, record))
}

/// Enhanced variable data block parser that handles DIF/VIFE chains according to EN 13757-3
pub fn parse_enhanced_variable_data_record(input: &[u8]) -> IResult<&[u8], EnhancedDataRecord> {
    let (remaining, dif_chain) = parse_dif_chain(input)?;
    let (remaining, vif_chain) = parse_vif_chain(remaining)?;

    // Extract standards-compliant tariff and storage from DIFE chain
    let tariff = extract_tariff_from_dife_chain(&dif_chain);
    let storage_number = extract_storage_number_from_dife_chain(&dif_chain);

    // Determine data length from primary DIF
    let data_length = mbus_dif_datalength_lookup(dif_chain[0]);
    let (remaining, data) = if data_length > 0 {
        take(data_length)(remaining)?
    } else {
        // Variable length data (DIF = 0x0D)
        if (dif_chain[0] & MBUS_DATA_RECORD_DIF_MASK_DATA) == 0x0D {
            // LVAR: first byte is length
            let (remaining, len) = be_u8(remaining)?;
            take(len as usize)(remaining)?
        } else {
            (remaining, &[] as &[u8])
        }
    };

    // Parse VIF chain to get unit and quantity information
    let (unit, exponent, quantity) = parse_special_vif_chain(&vif_chain, remaining)?;

    // Decode the data value
    let (value, is_numeric) = if (dif_chain[0] & MBUS_DATA_RECORD_DIF_MASK_DATA) == 0x0D {
        // Variable-length string data
        let mut decoded_string = String::new();
        mbus_data_str_decode(&mut decoded_string, data, data.len());
        (MBusRecordValue::String(decoded_string), false)
    } else {
        // Numeric data
        match mbus_data_record_value_decode(dif_chain[0], data) {
            Ok(numeric_value) => {
                let scaled_value = numeric_value * exponent;
                (MBusRecordValue::Numeric(scaled_value), true)
            }
            Err(_) => (MBusRecordValue::Numeric(0.0), true),
        }
    };

    let record = EnhancedDataRecord {
        dif_chain,
        vif_chain,
        value,
        tariff,
        storage_number,
        unit,
        quantity,
        timestamp: SystemTime::now(),
        is_numeric,
    };

    Ok((remaining, record))
}

/// Parse DIF + DIFE chain with up to 10 extensions per EN 13757-3
/// Parse special VIF chain including 0x7C, 0x7E, 0x7F codes
fn parse_special_vif_chain<'a>(
    vif_chain: &[u8],
    remaining: &'a [u8],
) -> SpecialVifResult<'a> {
    if vif_chain.is_empty() {
        return Ok(("".to_string(), 1.0, "".to_string()));
    }

    let primary_vif = vif_chain[0];

    // Handle special VIF codes
    match primary_vif {
        // Plain-text ASCII VIF (0x7C/0xFC)
        0x7C | 0xFC => {
            if vif_chain.len() < 2 {
                return Ok(("Unknown".to_string(), 1.0, "Special".to_string()));
            }
            let length = vif_chain[1] as usize;
            if vif_chain.len() < 2 + length {
                return Ok(("Unknown".to_string(), 1.0, "Special".to_string()));
            }
            // Extract ASCII text
            let ascii_bytes = &vif_chain[2..2 + length];
            let unit = String::from_utf8_lossy(ascii_bytes).to_string();
            Ok((unit, 1.0, "Plain-text".to_string()))
        }
        // Wildcard VIF (0x7E/0xFE) - "any VIF"
        0x7E | 0xFE => Ok(("Any".to_string(), 1.0, "Wildcard".to_string())),
        // Manufacturer-specific VIF (0x7F/0xFF)
        0x7F | 0xFF => {
            // Extract manufacturer-specific data
            let mfg_data: Vec<String> = vif_chain[1..]
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect();
            let unit = format!("MFG[{}]", mfg_data.join(" "));
            Ok((unit, 1.0, "Manufacturer".to_string()))
        }
        // Standard VIF codes
        _ => {
            let vib_infos: Result<Vec<_>, _> = vif_chain
                .iter()
                .enumerate()
                .map(|(i, &vif_byte)| {
                    if i == 0 {
                        // Primary VIF
                        crate::payload::vif_maps::lookup_primary_vif(vif_byte)
                            .ok_or(MBusError::UnknownVif(vif_byte))
                    } else {
                        // VIFE
                        crate::payload::vif_maps::lookup_vife_fd(vif_byte)
                            .or_else(|| crate::payload::vif_maps::lookup_vife_fb(vif_byte))
                            .ok_or(MBusError::UnknownVife(vif_byte))
                    }
                })
                .collect();

            let vib_infos = vib_infos.map_err(|_e| {
                nom::Err::Error(nom::error::Error::new(
                    remaining,
                    nom::error::ErrorKind::Tag,
                ))
            })?;

            if !vib_infos.is_empty() {
                let combined_exponent = vib_infos.iter().fold(1.0, |acc, info| acc * info.exponent);
                Ok((
                    vib_infos[0].unit.to_string(),
                    combined_exponent,
                    vib_infos[0].quantity.to_string(),
                ))
            } else {
                Ok(("".to_string(), 1.0, "".to_string()))
            }
        }
    }
}

/// Parse DIF + DIFE chain with up to 10 extensions per EN 13757-3
fn parse_dif_chain(input: &[u8]) -> IResult<&[u8], Vec<u8>> {
    let (mut remaining, dif) = be_u8(input)?;
    let mut chain = vec![dif];

    // Parse DIFE extensions while extension bit (0x80) is set
    let mut current_byte = dif;
    while (current_byte & MBUS_DIB_DIF_EXTENSION_BIT) != 0 {
        if remaining.is_empty() {
            return Err(nom::Err::Error(nom::error::Error::new(
                remaining,
                nom::error::ErrorKind::Eof,
            )));
        }

        if chain.len() > 10 {
            return Err(nom::Err::Error(nom::error::Error::new(
                remaining,
                nom::error::ErrorKind::TooLarge,
            )));
        }

        let (new_remaining, dife) = be_u8(remaining)?;
        chain.push(dife);
        remaining = new_remaining;
        current_byte = dife;
    }

    Ok((remaining, chain))
}

/// Parse VIF + VIFE chain including 0xFD/0xFB extensions and special VIFs
fn parse_vif_chain(input: &[u8]) -> IResult<&[u8], Vec<u8>> {
    let (mut remaining, vif) = be_u8(input)?;
    let mut chain = vec![vif];

    // Handle special VIF codes according to EN 13757-3
    match vif {
        // Extended VIF (0xFD/0xFB means next byte is the real VIF code)
        0xFD | 0xFB => {
            if remaining.is_empty() {
                return Err(nom::Err::Error(nom::error::Error::new(
                    remaining,
                    nom::error::ErrorKind::Eof,
                )));
            }
            let (new_remaining, extended_vif) = be_u8(remaining)?;
            chain.push(extended_vif);
            remaining = new_remaining;
        }
        // Plain-text ASCII VIF (0x7C/0xFC)
        0x7C | 0xFC => {
            // Next n bytes are ASCII text describing the unit
            // First byte is length
            if remaining.is_empty() {
                return Err(nom::Err::Error(nom::error::Error::new(
                    remaining,
                    nom::error::ErrorKind::Eof,
                )));
            }
            let (new_remaining, length) = be_u8(remaining)?;
            let (new_remaining, ascii_bytes) = take(length as usize)(new_remaining)?;
            chain.push(length);
            chain.extend_from_slice(ascii_bytes);
            remaining = new_remaining;
        }
        // Wildcard VIF (0x7E/0xFE) - "any VIF"
        0x7E | 0xFE => {
            // No additional data needed for wildcard
            // Indicates any VIF is acceptable
        }
        // Manufacturer-specific VIF (0x7F/0xFF)
        0x7F | 0xFF => {
            // Following bytes are manufacturer-specific
            // Usually 2-4 bytes of manufacturer data
            // We'll read until no extension bit or max 10 bytes
            let mut mfg_bytes = 0;
            while !remaining.is_empty() && mfg_bytes < 10 {
                let (new_remaining, mfg_byte) = be_u8(remaining)?;
                chain.push(mfg_byte);
                remaining = new_remaining;
                mfg_bytes += 1;
                // Check if manufacturer byte has extension bit
                if (mfg_byte & 0x80) == 0 {
                    break;
                }
            }
        }
        _ => {}
    }

    // Now handle VIFE chains (extension bit 0x80 in VIF/VIFE)
    // Skip special VIFs that don't support extensions
    if !matches!(vif, 0x7C | 0xFC | 0x7E | 0xFE | 0x7F | 0xFF) {
        while let Some(&last_byte) = chain.last() {
            if (last_byte & MBUS_DIB_VIF_EXTENSION_BIT) == 0 {
                break;
            }

            if remaining.is_empty() {
                return Err(nom::Err::Error(nom::error::Error::new(
                    remaining,
                    nom::error::ErrorKind::Eof,
                )));
            }

            // Enforce 10 VIFE limit per EN 13757-3
            if chain.len() >= 11 {
                // 1 VIF + 10 VIFEs max
                break;
            }

            let (new_remaining, vife) = be_u8(remaining)?;
            chain.push(vife);
            remaining = new_remaining;
        }
    }

    Ok((remaining, chain))
}

fn mbus_dif_datalength_lookup(dif: u8) -> usize {
    match dif & MBUS_DATA_RECORD_DIF_MASK_DATA {
        0x00 => 0,
        0x01 => 1,
        0x02 => 2,
        0x03 => 3,
        0x04 => 4,
        0x05 => 4,
        0x06 => 6,
        0x07 => 8,
        0x0D => 0, // Variable-length data, length stored in data field
        _ => 0,
    }
}

fn mbus_data_record_value_decode(dif: u8, data: &[u8]) -> Result<f64, MBusError> {
    // M-Bus uses little-endian encoding (LSB first)
    let le_to_u64 = |bytes: &[u8]| -> u64 {
        bytes
            .iter()
            .enumerate()
            .fold(0u64, |acc, (i, b)| acc | ((*b as u64) << (8 * i)))
    };

    match dif & MBUS_DATA_RECORD_DIF_MASK_DATA {
        0x01 => Ok(data.first().copied().unwrap_or(0) as f64),
        0x02 => Ok(le_to_u64(&data[..data.len().min(2)]) as f64),
        0x03 => Ok(le_to_u64(&data[..data.len().min(3)]) as f64),
        0x04 => Ok(le_to_u64(&data[..data.len().min(4)]) as f64),
        0x05 => {
            // 32-bit IEEE 754 float in little-endian
            if data.len() >= 4 {
                let bits = (data[0] as u32)
                    | ((data[1] as u32) << 8)
                    | ((data[2] as u32) << 16)
                    | ((data[3] as u32) << 24);
                Ok(f32::from_bits(bits) as f64)
            } else {
                Ok(0.0)
            }
        }
        0x06 => Ok(le_to_u64(&data[..data.len().min(6)]) as f64),
        0x07 => Ok(le_to_u64(&data[..data.len().min(8)]) as f64),
        0x09 | 0x0A | 0x0B | 0x0C | 0x0E => {
            let s: String = data.iter().map(|b| format!("{b:02X}")).collect();
            Ok(u64::from_str_radix(&s, 16).unwrap_or(0) as f64)
        }
        _ => Err(MBusError::UnknownDif(dif)),
    }
}

/// Extract storage number from DIF/DIFE chain according to EN 13757-3
/// Storage number is built across the DIFE chain (bits [3:0] in each DIFE)
fn extract_storage_number_from_dife_chain(dif_chain: &[u8]) -> u16 {
    let mut storage_number = 0u16;

    for (i, &dife) in dif_chain.iter().enumerate().skip(1) {
        // Skip DIF itself
        if i > 10 {
            break;
        } // Max 10 DIFE extensions per EN 13757-3

        // DIFE bits [3:0] contribute to storage number
        let nibble = (dife & MBUS_DATA_RECORD_DIFE_MASK_STORAGE_NO) as u16;
        // Prevent overflow: storage number is 16-bit, so max 4 nibbles (16 bits / 4 bits per nibble)
        if (i - 1) < 4 {
            storage_number |= nibble << (4 * (i - 1)); // Each DIFE contributes 4 bits
        }
    }

    storage_number
}

fn mbus_data_record_storage_number(vib: &[VifInfo]) -> u32 {
    let mut storage_number = 0;
    for info in vib {
        if ((info.vif as u8) & MBUS_DATA_RECORD_DIF_MASK_STORAGE_NO) >> 6 != 0 {
            storage_number |=
                (((info.vif as u8) & MBUS_DATA_RECORD_DIF_MASK_STORAGE_NO) >> 6) as u32;
        }
        if ((info.vif as u8) & MBUS_DATA_RECORD_DIFE_MASK_STORAGE_NO) != 0 {
            storage_number |=
                (((info.vif as u8) & MBUS_DATA_RECORD_DIFE_MASK_STORAGE_NO) as u32) << 4;
        }
    }
    storage_number
}

/// Extract tariff from first DIFE with tariff information according to EN 13757-3
/// Tariff is encoded in DIFE bits [5:4]
fn extract_tariff_from_dife_chain(dif_chain: &[u8]) -> u8 {
    for &dife in dif_chain.iter().skip(1) {
        // Skip DIF itself
        let tariff_bits = (dife & MBUS_DATA_RECORD_DIFE_MASK_TARIFF) >> 4;
        if tariff_bits != 0 {
            return tariff_bits;
        }
    }
    0 // Default tariff if no DIFE specifies tariff
}

fn mbus_data_record_tariff(vib: &[VifInfo]) -> i32 {
    let mut tariff = 0;
    for info in vib {
        if (((info.vif as u8) & MBUS_DATA_RECORD_DIFE_MASK_TARIFF) >> 4) != 0 {
            tariff |= (((info.vif as u8) & MBUS_DATA_RECORD_DIFE_MASK_TARIFF) >> 4) as i32;
        }
    }
    tariff
}

fn mbus_data_record_device(vib: &[VifInfo]) -> i32 {
    let mut device = 0;
    for info in vib {
        if (((info.vif as u8) & MBUS_DATA_RECORD_DIFE_MASK_DEVICE) >> 6) != 0 {
            device |= (((info.vif as u8) & MBUS_DATA_RECORD_DIFE_MASK_DEVICE) >> 6) as i32;
        }
    }
    device
}

fn mbus_data_record_function(dif: u8) -> &'static str {
    match dif & MBUS_DATA_RECORD_DIF_MASK_FUNCTION {
        0x00 => "Instantaneous value",
        0x10 => "Maximum value",
        0x20 => "Minimum value",
        0x30 => "Value during error state",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enhanced_variable_data_parser_basic() {
        // Basic test: DIF=0x04 (32-bit integer), VIF=0x20 (seconds, no scaling)
        // Value: 0x12345678 as little-endian bytes: 0x78, 0x56, 0x34, 0x12
        let data = vec![
            0x04, // DIF: 32-bit integer
            0x20, // VIF: seconds (exponent = 1.0, no scaling)
            0x78, 0x56, 0x34, 0x12, // 32-bit LE value = 305419896
        ];

        let (remaining, record) = parse_enhanced_variable_data_record(&data).unwrap();

        assert!(remaining.is_empty());
        assert_eq!(record.dif_chain, vec![0x04]);
        assert_eq!(record.vif_chain, vec![0x20]);
        assert_eq!(record.tariff, 0);
        assert_eq!(record.storage_number, 0);
        assert!(record.is_numeric);

        if let MBusRecordValue::Numeric(value) = record.value {
            println!("Actual value: {value}, Expected: 305419896");
            assert!(
                (value - 305419896.0).abs() < 1e-6,
                "Expected 305419896, got {value}"
            );
        } else {
            panic!("Expected numeric value");
        }
    }

    #[test]
    fn test_multi_tariff_dife_chain() {
        // Multi-tariff example: DIF with extension + DIFE with tariff 1
        let data = vec![
            0x84, // DIF: 32-bit integer + extension bit
            0x10, // DIFE: tariff=1 (bits 5:4), storage=0 (bits 3:0)
            0x20, // VIF: seconds (no scaling)
            0x34, 0x12, 0x00, 0x00, // 32-bit LE value = 4660
        ];

        let (remaining, record) = parse_enhanced_variable_data_record(&data).unwrap();

        assert!(remaining.is_empty());
        assert_eq!(record.dif_chain, vec![0x84, 0x10]);
        assert_eq!(record.vif_chain, vec![0x20]);
        assert_eq!(record.tariff, 1); // From DIFE bits [5:4] = 0x10 >> 4 = 1
        assert_eq!(record.storage_number, 0); // From DIFE bits [3:0] = 0

        if let MBusRecordValue::Numeric(value) = record.value {
            println!("Multi-tariff actual value: {value}, Expected: 4660");
            assert!(
                (value - 4660.0).abs() < 1e-6,
                "Expected 4660, got {value}"
            );
        } else {
            panic!("Expected numeric value");
        }
    }

    #[test]
    fn test_little_endian_value_parsing() {
        // Test various data sizes with little-endian encoding

        // 8-bit value
        let data = vec![0x01, 0x20, 0x42]; // DIF=0x01, VIF=0x20, value=66
        let (_, record) = parse_enhanced_variable_data_record(&data).unwrap();
        if let MBusRecordValue::Numeric(value) = record.value {
            println!("8-bit actual value: {value}, Expected: 66");
            assert_eq!(value, 66.0, "Expected 66, got {value}");
        }

        // 16-bit value: 0x1234 as LE bytes: 0x34, 0x12
        let data = vec![0x02, 0x20, 0x34, 0x12]; // DIF=0x02, VIF=0x20, value=4660
        let (_, record) = parse_enhanced_variable_data_record(&data).unwrap();
        if let MBusRecordValue::Numeric(value) = record.value {
            assert_eq!(value, 4660.0);
        }
    }

    #[test]
    fn test_special_vif_plain_text() {
        // Test VIF=0x7C (plain-text ASCII)
        // Format: DIF, VIF=0x7C, length, ASCII bytes, data
        let data = vec![
            0x04, // DIF: 32-bit integer
            0x7C, // VIF: Plain-text ASCII follows
            0x03, // Length: 3 bytes
            0x6B, 0x57, 0x68, // ASCII: "kWh"
            0x10, 0x00, 0x00, 0x00, // 32-bit LE value = 16
        ];

        let (remaining, record) = parse_enhanced_variable_data_record(&data).unwrap();

        assert!(remaining.is_empty());
        assert_eq!(record.vif_chain, vec![0x7C, 0x03, 0x6B, 0x57, 0x68]);
        assert_eq!(record.unit, "kWh");
        assert_eq!(record.quantity, "Plain-text");

        if let MBusRecordValue::Numeric(value) = record.value {
            assert_eq!(value, 16.0);
        } else {
            panic!("Expected numeric value");
        }
    }

    #[test]
    fn test_special_vif_wildcard() {
        // Test VIF=0x7E (wildcard "any VIF")
        let data = vec![
            0x02, // DIF: 16-bit integer
            0x7E, // VIF: Wildcard (any VIF)
            0x34, 0x12, // 16-bit LE value = 4660
        ];

        let (remaining, record) = parse_enhanced_variable_data_record(&data).unwrap();

        assert!(remaining.is_empty());
        assert_eq!(record.vif_chain, vec![0x7E]);
        assert_eq!(record.unit, "Any");
        assert_eq!(record.quantity, "Wildcard");

        if let MBusRecordValue::Numeric(value) = record.value {
            assert_eq!(value, 4660.0);
        }
    }

    #[test]
    fn test_special_vif_manufacturer() {
        // Test VIF=0x7F (manufacturer-specific)
        // Manufacturer bytes should not have extension bit set for this test
        let data = vec![
            0x01, // DIF: 8-bit integer
            0x7F, // VIF: Manufacturer-specific
            0x2B, // Manufacturer byte 1 (no extension bit)
            0x42, // 8-bit value = 66
        ];

        let (remaining, record) = parse_enhanced_variable_data_record(&data).unwrap();

        assert!(remaining.is_empty());
        assert_eq!(record.vif_chain, vec![0x7F, 0x2B]);
        assert_eq!(record.unit, "MFG[2B]");
        assert_eq!(record.quantity, "Manufacturer");

        if let MBusRecordValue::Numeric(value) = record.value {
            assert_eq!(value, 66.0);
        }
    }

    #[test]
    fn test_vife_chain_limit() {
        // Test that parse_vif_chain enforces max 10 VIFE extensions per EN 13757-3
        // We test the chain parsing directly to avoid VIF lookup issues

        // Test 1: Exactly 10 VIFEs with proper termination
        let mut input = vec![
            0x13, // VIF: Volume (valid primary VIF without extension)
        ];

        let result = parse_vif_chain(&input);
        assert!(result.is_ok());
        if let Ok((_, chain)) = result {
            assert_eq!(chain.len(), 1); // Just the VIF
        }

        // Test 2: VIF with 10 extensions - should parse all
        input = vec![
            0x93, // VIF with extension bit
        ];
        for i in 0..9 {
            input.push(0x80 | i); // 9 VIFEs with extension bits
        }
        input.push(0x09); // 10th VIFE without extension bit

        let result = parse_vif_chain(&input);
        assert!(result.is_ok());
        if let Ok((remaining, chain)) = result {
            assert_eq!(chain.len(), 11); // 1 VIF + 10 VIFEs
            assert!(remaining.is_empty()); // All consumed
        }

        // Test 3: VIF with 11+ extensions - should stop at 10
        input = vec![
            0x93, // VIF with extension bit
        ];
        for i in 0..12 {
            input.push(0x80 | i); // 12 VIFEs all with extension bits
        }

        let result = parse_vif_chain(&input);
        assert!(result.is_ok());
        if let Ok((remaining, chain)) = result {
            assert_eq!(chain.len(), 11); // Limited to 1 VIF + 10 VIFEs
            assert_eq!(remaining.len(), 2); // Last 2 bytes not consumed
        }
    }
}
