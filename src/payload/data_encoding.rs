//! # M-Bus Data Encoding and Decoding
//!
//! This module provides functions for encoding and decoding various data types
//! used in the M-Bus protocol, such as BCD, integer, float, and time data.

use crate::error::MBusError;
use nom::{
    bytes::complete::take,
    combinator::map,
    number::complete::{be_u16, be_u32, be_u64, be_u8},
    IResult,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Decodes a string from the input data.
pub fn mbus_data_str_decode(dst: &mut String, src: &[u8], len: usize) {
    dst.clear();
    for item in src.iter().take(len).rev() {
        dst.push(*item as char);
    }
}

/// Decodes M-Bus time data from the input, handling the different time formats and validating the input.
#[derive(Debug, thiserror::Error)]
pub enum MBusTimeDecodeError {
    #[error("Invalid time data length: expected 2, 4, or 6 bytes, got {0}")]
    InvalidTimeDataLength(usize),
    #[error("Time data is not valid")]
    InvalidTimeData,
}

/// Decodes M-Bus time data from the input byte slice and returns a SystemTime, handling the different time data types and validating the input.
pub fn decode_mbus_time(input: &[u8]) -> Result<SystemTime, MBusTimeDecodeError> {
    let mut time = UNIX_EPOCH;

    match input.len() {
        2 => {
            // Type G: Compound CP16 (Date)
            let year = u64::from(100 + (((input[0] & 0xE0) >> 5) | ((input[1] & 0xF0) >> 1)));
            let month = u64::from((input[1] & 0x0F) - 1);
            let day = u64::from(input[0] & 0x1F);
            time += Duration::from_secs(year * 31_536_000 + month * 2_592_000 + day * 86_400);
        }
        4 => {
            // Type F: Compound CP32 (Date and Time)
            if (input[0] & 0x80) != 0 {
                return Err(MBusTimeDecodeError::InvalidTimeData);
            }
            let minute = u64::from(input[0] & 0x3F);
            let hour = u64::from(input[1] & 0x1F);
            let day = u64::from(input[2] & 0x1F);
            let month = u64::from((input[3] & 0x0F) - 1);
            let year = u64::from(100 + (((input[2] & 0xE0) >> 5) | ((input[3] & 0xF0) >> 1)));
            time += Duration::from_secs(
                year * 31_536_000 + month * 2_592_000 + day * 86_400 + hour * 3_600 + minute * 60,
            );
        }
        6 => {
            // Type I: Compound CP48 (Date and Time)
            if (input[0] & 0x40) != 0 {
                return Err(MBusTimeDecodeError::InvalidTimeData);
            }
            let second = u64::from(input[0] & 0x3F);
            let minute = u64::from(input[1] & 0x3F);
            let hour = u64::from(input[2] & 0x1F);
            let day = u64::from(input[3] & 0x1F);
            let month = u64::from((input[4] & 0x0F) - 1);
            let year = u64::from(100 + (((input[3] & 0xE0) >> 5) | ((input[4] & 0xF0) >> 1)));
            time += Duration::from_secs(
                year * 31_536_000
                    + month * 2_592_000
                    + day * 86_400
                    + hour * 3_600
                    + minute * 60
                    + second,
            );
        }
        _ => return Err(MBusTimeDecodeError::InvalidTimeDataLength(input.len())),
    }

    Ok(time)
}

/// Decodes binary data from the input.
pub fn mbus_data_bin_decode(dst: &mut String, src: &[u8], len: usize, max_len: usize) {
    dst.clear();
    let mut pos = 0;
    for item in src.iter().take(len) {
        let hex = format!("{:02X} ", *item);
        if pos + hex.len() > max_len {
            break;
        }
        dst.push_str(&hex);
        pos += hex.len();
    }
    if dst.ends_with(' ') {
        dst.pop(); // remove last space
    }
}

/// Decodes a binary-coded decimal (BCD) value to a 32-bit unsigned integer.
pub fn decode_bcd(input: &[u8]) -> IResult<&[u8], u32> {
    let (input, bytes) = take(4usize)(input)?;

    for byte in bytes {
        if (byte & 0xF) > 9 || ((byte >> 4) & 0xF) > 9 {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Verify,
            )));
        }
    }

    let mut value = 0u32;
    let mut multiplier = 1u32;
    // Process bytes in forward order (big-endian) to match encode_bcd
    for &byte in bytes.iter().rev() {
        // Low nibble is ones digit, high nibble is tens digit in BCD
        value += (byte as u32 & 0xF) * multiplier;
        multiplier *= 10;
        value += ((byte >> 4) as u32 & 0xF) * multiplier;
        multiplier *= 10;
    }

    Ok((input, value))
}

/// Encodes a 32-bit unsigned integer to a binary-coded decimal (BCD) representation.
/// Returns bytes compatible with decode_bcd's little-endian processing.
pub fn encode_bcd(mut input: u32) -> Vec<u8> {
    let mut result = vec![0u8; 4];

    // Extract each pair of decimal digits and store in BCD format
    for idx in (0..4).rev() {
        if input > 0 {
            let ones = (input % 10) as u8;
            input /= 10;
            let tens = (input % 10) as u8;
            input /= 10;

            result[idx] = (tens << 4) | ones;
        }
    }

    result
}

/// Decodes an integer value from the input data.
pub fn decode_int(input: &[u8], size: usize) -> IResult<&[u8], i32> {
    match size {
        1 => map(be_u8, |v| v as i32)(input),
        2 => map(be_u16, |v| v as i32)(input),
        4 => map(be_u32, |v| v as i32)(input),
        8 => map(be_u64, |v| v as i32)(input),
        _ => Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        ))),
    }
}

/// Decodes a big-endian signed integer of 6 or 8 bytes into i64.
pub fn decode_long_long(input: &[u8], size: usize) -> IResult<&[u8], i64> {
    match size {
        6 => map(take(6usize), |bytes: &[u8]| {
            let mut v: u64 = 0;
            for b in bytes {
                v = (v << 8) | (*b as u64);
            }
            v as i64
        })(input),
        8 => map(be_u64, |v| v as i64)(input),
        _ => Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        ))),
    }
}
pub fn decode_bcd_hex(input: &[u8]) -> IResult<&[u8], u32> {
    map(take(4usize), |bytes: &[u8]| {
        // Interpret as little-endian hex value
        let mut value = 0u32;
        for (i, &byte) in bytes.iter().enumerate() {
            value |= (byte as u32) << (i * 8);
        }
        value
    })(input)
}

/// Decodes an integer value from the input data.
#[derive(Debug, thiserror::Error)]
pub enum MBusIntDecodeError {
    #[error("Invalid integer size: expected 1, 2, 4, or 8 bytes, got {0}")]
    InvalidIntegerSize(usize),
    #[error("Integer value out of range for the requested type")]
    IntegerOutOfRange,
}

/// Simple integer encoder for common widths.
pub fn encode_int_u64(value: u64, output: &mut [u8]) -> Result<(), MBusIntEncodeError> {
    match output.len() {
        1 => {
            output[0] = value as u8;
            Ok(())
        }
        2 => {
            output.copy_from_slice(&(value as u16).to_be_bytes());
            Ok(())
        }
        4 => {
            output.copy_from_slice(&(value as u32).to_be_bytes());
            Ok(())
        }
        8 => {
            output.copy_from_slice(&value.to_be_bytes());
            Ok(())
        }
        _ => Err(MBusIntEncodeError::InvalidIntegerSize),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MBusIntEncodeError {
    #[error("Insufficient output buffer size")]
    InsufficientOutputBuffer,
    #[error("Integer value out of range for the requested type")]
    IntegerOutOfRange,
    #[error("Invalid integer size: expected 1, 2, 4, or 8 bytes")]
    InvalidIntegerSize,
}

/// Decodes a float value from the input data.
pub fn decode_float(input: &[u8]) -> IResult<&[u8], f32> {
    map(take(4usize), |bytes: &[u8]| {
        let mut value = 0u32;
        value |= (bytes[0] as u32) << 24;
        value |= (bytes[1] as u32) << 16;
        value |= (bytes[2] as u32) << 8;
        value |= bytes[3] as u32;
        f32::from_bits(value)
    })(input)
}

/// Decodes a time value from the input data.
pub fn decode_time(input: &[u8], size: usize) -> IResult<&[u8], SystemTime> {
    map(take(size), |bytes: &[u8]| {
        let mut time = UNIX_EPOCH;

        match size {
            2 => {
                // Type G: Compound CP16: Date
                let year = u64::from(100 + (((bytes[0] & 0xE0) >> 5) | ((bytes[1] & 0xF0) >> 1)));
                let month = u64::from((bytes[1] & 0x0F) - 1);
                let day = u64::from(bytes[0] & 0x1F);
                time += Duration::from_secs(year * 31_536_000 + month * 2_592_000 + day * 86_400);
            }
            4 => {
                // Type F = Compound CP32: Date and Time
                let minute = u64::from(bytes[0] & 0x3F);
                let hour = u64::from(bytes[1] & 0x1F);
                let day = u64::from(bytes[2] & 0x1F);
                let month = u64::from((bytes[3] & 0x0F) - 1);
                let year = u64::from(100 + (((bytes[2] & 0xE0) >> 5) | ((bytes[3] & 0xF0) >> 1)));
                time += Duration::from_secs(
                    year * 31_536_000
                        + month * 2_592_000
                        + day * 86_400
                        + hour * 3_600
                        + minute * 60,
                );
            }
            6 => {
                // Type I = Compound CP48: Date and Time
                let second = u64::from(bytes[0] & 0x3F);
                let minute = u64::from(bytes[1] & 0x3F);
                let hour = u64::from(bytes[2] & 0x1F);
                let day = u64::from(bytes[3] & 0x1F);
                let month = u64::from((bytes[4] & 0x0F) - 1);
                let year = u64::from(100 + (((bytes[3] & 0xE0) >> 5) | ((bytes[4] & 0xF0) >> 1)));
                time += Duration::from_secs(
                    year * 31_536_000
                        + month * 2_592_000
                        + day * 86_400
                        + hour * 3_600
                        + minute * 60
                        + second,
                );
            }
            _ => {}
        }

        time
    })(input)
}

/// Encodes the manufacturer ID according to the manufacturer's 3-byte ASCII code.
pub fn mbus_data_manufacturer_encode(manufacturer: &str) -> Result<[u8; 2], MBusError> {
    if manufacturer.len() != 3 || !manufacturer.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(MBusError::InvalidManufacturer);
    }

    let id = (((manufacturer.chars().next().unwrap() as u32 - 64) & 0x1F) * 32 * 32)
        + (((manufacturer.chars().nth(1).unwrap() as u32 - 64) & 0x1F) * 32)
        + ((manufacturer.chars().nth(2).unwrap() as u32 - 64) & 0x1F);

    if !(0x0421..=0x6B5A).contains(&id) {
        return Err(MBusError::InvalidManufacturerId);
    }

    Ok([(id >> 8) as u8, (id & 0xFF) as u8])
}

/// Decodes the manufacturer ID from the 2-byte encoded data.
pub fn mbus_decode_manufacturer(byte1: u8, byte2: u8) -> String {
    let mut id = ((byte1 as u32) << 8) + (byte2 as u32);
    let mut manufacturer = String::with_capacity(3);

    manufacturer.push(char::from_u32((id / (32 * 32)) + 64).unwrap_or('?'));
    id %= 32 * 32;
    manufacturer.push(char::from_u32((id / 32) + 64).unwrap_or('?'));
    id %= 32;
    manufacturer.push(char::from_u32(id + 64).unwrap_or('?'));

    manufacturer
}
