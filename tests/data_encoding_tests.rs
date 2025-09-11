use mbus_rs::payload::data_encoding::*;
use std::time::UNIX_EPOCH;

#[test]
fn test_decode_bcd_valid() {
    // decode_bcd always expects 4 bytes
    // Implementation processes bytes in REVERSE (little-endian)
    // For BCD 0x42 in last position: high=4 (tens), low=2 (ones) = 42
    let input = &[0x00, 0x00, 0x00, 0x42];
    let (remaining, value) = decode_bcd(input).unwrap();
    assert_eq!(value, 42); // Fixed: nibbles now processed correctly
    assert!(remaining.is_empty());

    // For 0x78,0x56,0x34,0x12 processed in reverse: 0x12,0x34,0x56,0x78
    // 0x12: 2*1 + 1*10 = 12
    // 0x34: 4*100 + 3*1000 = 3400
    // 0x56: 6*10000 + 5*100000 = 560000
    // 0x78: 8*1000000 + 7*10000000 = 78000000
    // Total: 78563412
    let input = &[0x78, 0x56, 0x34, 0x12];
    let (remaining, value) = decode_bcd(input).unwrap();
    assert_eq!(value, 78563412); // Correct value with fixed nibble order
    assert!(remaining.is_empty());
}

#[test]
fn test_decode_bcd_with_remainder() {
    // Little-endian + nibble swap: 0x99 at start
    // Processes as: 00, 00, 00, 99
    // 0x99: 9*1000000 + 9*10000000 = 99000000
    let input = &[0x99, 0x00, 0x00, 0x00, 0xFF];
    let (remaining, value) = decode_bcd(input).unwrap();
    assert_eq!(value, 99000000); // Due to little-endian processing
    assert_eq!(remaining, &[0xFF]);
}

#[test]
fn test_decode_bcd_invalid() {
    // Test invalid BCD (digit > 9)
    let input = &[0xAB, 0x00, 0x00, 0x00]; // Invalid BCD
    let result = decode_bcd(input);
    assert!(result.is_err());
}

#[test]
fn test_encode_bcd() {
    // encode_bcd returns bytes in format compatible with decode_bcd
    // The format places least significant BCD digits at the end
    assert_eq!(encode_bcd(0), vec![0x00, 0x00, 0x00, 0x00]);
    assert_eq!(encode_bcd(9), vec![0x00, 0x00, 0x00, 0x09]);

    // Test two digits
    assert_eq!(encode_bcd(42), vec![0x00, 0x00, 0x00, 0x42]);
    assert_eq!(encode_bcd(99), vec![0x00, 0x00, 0x00, 0x99]);

    // Test larger numbers - most significant at start
    assert_eq!(encode_bcd(1234), vec![0x00, 0x00, 0x12, 0x34]);
    assert_eq!(encode_bcd(12345678), vec![0x12, 0x34, 0x56, 0x78]);
}

#[test]
fn test_bcd_round_trip() {
    // Test round trip with various values
    // Both encode_bcd and decode_bcd now use little-endian
    let test_values = vec![0, 1, 42, 99, 1234, 999999, 12345678];

    for value in test_values {
        let encoded = encode_bcd(value);
        let (_, decoded) = decode_bcd(&encoded).unwrap();
        assert_eq!(decoded, value, "Round trip failed for {}", value);
    }
}

#[test]
fn test_decode_int() {
    // Test 1 byte
    let input = &[0x42];
    let (_, value) = decode_int(input, 1).unwrap();
    assert_eq!(value, 0x42);

    // Test 2 bytes (big endian)
    let input = &[0x12, 0x34];
    let (_, value) = decode_int(input, 2).unwrap();
    assert_eq!(value, 0x1234);

    // Test 4 bytes (big endian)
    let input = &[0x12, 0x34, 0x56, 0x78];
    let (_, value) = decode_int(input, 4).unwrap();
    assert_eq!(value, 0x12345678);

    // Test negative values (sign extension)
    let input = &[0xFF]; // 255 as unsigned, -1 if treated as signed
    let (_, value) = decode_int(input, 1).unwrap();
    assert_eq!(value, 255); // It's cast from u8, so unsigned
}

#[test]
fn test_encode_int_u64() {
    let mut output = [0u8; 8];

    // Test 1 byte
    encode_int_u64(0x42, &mut output[..1]).unwrap();
    assert_eq!(output[0], 0x42);

    // Test 2 bytes (big endian)
    encode_int_u64(0x1234, &mut output[..2]).unwrap();
    assert_eq!(&output[..2], &[0x12, 0x34]);

    // Test 4 bytes (big endian)
    encode_int_u64(0x12345678, &mut output[..4]).unwrap();
    assert_eq!(&output[..4], &[0x12, 0x34, 0x56, 0x78]);

    // Test 8 bytes (big endian)
    encode_int_u64(0x123456789ABCDEF0, &mut output).unwrap();
    assert_eq!(output, [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0]);
}

#[test]
fn test_encode_int_u64_invalid_size() {
    let mut output = [0u8; 3];

    // Invalid size (not 1, 2, 4, or 8)
    let result = encode_int_u64(123, &mut output);
    assert!(result.is_err());
}

#[test]
fn test_decode_float() {
    // IEEE 754 float: 42.0 = 0x42280000 (big endian)
    let input = &[0x42, 0x28, 0x00, 0x00];
    let (remaining, value) = decode_float(input).unwrap();
    assert_eq!(value, 42.0);
    assert!(remaining.is_empty());

    // Test negative float: -1.0 = 0xBF800000 (big endian)
    let input = &[0xBF, 0x80, 0x00, 0x00];
    let (_, value) = decode_float(input).unwrap();
    assert_eq!(value, -1.0);
}

#[test]
fn test_mbus_decode_manufacturer() {
    // Test known manufacturer codes
    // ABC: A=1, B=2, C=3 in 5-bit encoding = 0x0443
    let result = mbus_decode_manufacturer(0x04, 0x43);
    assert_eq!(result, "ABC");

    // ZZZ: Z=26 in 5-bit encoding = 0x6B5A
    let result = mbus_decode_manufacturer(0x6B, 0x5A);
    assert_eq!(result, "ZZZ");
}

#[test]
fn test_mbus_data_manufacturer_encode() {
    // Test encoding manufacturer codes
    let result = mbus_data_manufacturer_encode("ABC").unwrap();
    assert_eq!(result, [0x04, 0x43]); // 0x0443 in big-endian

    let result = mbus_data_manufacturer_encode("ZZZ").unwrap();
    assert_eq!(result, [0x6B, 0x5A]); // 0x6B5A in big-endian

    // Test invalid input (not 3 chars)
    let result = mbus_data_manufacturer_encode("AB");
    assert!(result.is_err());

    let result = mbus_data_manufacturer_encode("ABCD");
    assert!(result.is_err());
}

#[test]
fn test_manufacturer_round_trip() {
    let manufacturers = vec!["ABC", "XYZ", "ZZZ", "AAA"];

    for manufacturer in manufacturers {
        let encoded = mbus_data_manufacturer_encode(manufacturer).unwrap();
        let decoded = mbus_decode_manufacturer(encoded[0], encoded[1]);
        assert_eq!(
            decoded, manufacturer,
            "Round trip failed for {}",
            manufacturer
        );
    }
}

#[test]
fn test_mbus_data_str_decode() {
    let mut output = String::new();

    // Test ASCII string - mbus_data_str_decode reverses the string
    let input = b"Hello";
    mbus_data_str_decode(&mut output, input, 5);
    assert_eq!(output, "olleH"); // Reversed for M-Bus protocol

    // Test with null terminator
    output.clear();
    let input = b"Test\0";
    mbus_data_str_decode(&mut output, input, 5);
    assert_eq!(output, "\0tseT"); // Reversed including null

    // Test partial read
    output.clear();
    let input = b"LongString";
    mbus_data_str_decode(&mut output, input, 4);
    assert_eq!(output, "gnoL"); // Reversed: "Long" -> "gnoL"
}

#[test]
fn test_mbus_data_bin_decode() {
    let mut output = String::new();

    // Test binary to hex conversion - includes spaces between bytes
    let input = &[0x12, 0x34, 0xAB, 0xCD];
    mbus_data_bin_decode(&mut output, input, 4, 100);
    assert_eq!(output, "12 34 AB CD"); // Spaces between bytes

    // Test with length limit
    output.clear();
    mbus_data_bin_decode(&mut output, input, 2, 100);
    assert_eq!(output, "12 34"); // Two bytes with space

    // Test max_len truncation - stops when adding next item would exceed max_len
    output.clear();
    mbus_data_bin_decode(&mut output, input, 4, 5);
    assert_eq!(output, "12"); // Only first byte fits in 5 chars (including space)
}

#[test]
fn test_decode_bcd_hex() {
    // Test valid BCD hex (each nibble 0-F)
    let input = &[0x12, 0xAB, 0xCD, 0xEF];
    let (remaining, value) = decode_bcd_hex(input).unwrap();
    assert_eq!(value, 0xEFCDAB12); // Little endian
    assert!(remaining.is_empty());

    // Test with padding (decode_bcd_hex expects 4 bytes)
    let input = &[0xFF, 0x00, 0x00, 0x00];
    let (_, value) = decode_bcd_hex(input).unwrap();
    assert_eq!(value, 0xFF); // Little-endian: 0x000000FF
}

#[test]
fn test_decode_long_long() {
    // Test 8-byte integer (big endian)
    let input = &[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
    let (remaining, value) = decode_long_long(input, 8).unwrap();
    assert_eq!(value, 0x123456789ABCDEF0i64);
    assert!(remaining.is_empty());

    // Test 6-byte integer (special case)
    let input = &[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];
    let (_, value) = decode_long_long(input, 6).unwrap();
    // Should be interpreted as 0x0000123456789ABC
    assert_eq!(value, 0x123456789ABCi64);
}

#[test]
fn test_decode_time_basic() {
    // Create a known time: 2024-01-15 12:30:45
    // M-Bus time format varies by size

    // Test Type F (2 bytes) - Date only
    let input = &[0x0F, 0x01]; // Day 15, Month 1
    let (_, time) = decode_time(input, 2).unwrap();
    // Verify it's a valid SystemTime (exact value depends on implementation)
    assert!(time > UNIX_EPOCH);

    // Test Type G (4 bytes) - Date and time
    let input = &[0x2D, 0x1E, 0x0F, 0x01]; // Min 45, Hour 12, Day 15, Month 1
    let (_, time) = decode_time(input, 4).unwrap();
    assert!(time > UNIX_EPOCH);
}

#[test]
fn test_decode_mbus_time() {
    // Test various time formats
    // Format: Minutes since epoch or specific date/time encoding

    // Test with valid 2-byte date
    let input = &[0x1F, 0x0C]; // Day 31, Month 12
    let result = decode_mbus_time(input);
    assert!(result.is_ok());

    // Test with 4-byte date/time
    let input = &[0x3B, 0x17, 0x01, 0x01]; // Sec 59, Min 23, Hour 1, Day 1
    let result = decode_mbus_time(input);
    assert!(result.is_ok());
}
