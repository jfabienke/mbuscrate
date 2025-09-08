//! Unit tests for the utility functions in the `utils.rs` module.

use std::time::{Duration, SystemTime};

/// Tests the `mbus_hex2bin()` function, which converts a hexadecimal string to a byte vector.
#[test]
fn test_hex_to_bytes_local() {
    fn hex_to_bytes(s: &str) -> Vec<u8> {
        s.as_bytes()
            .chunks(2)
            .filter_map(|p| std::str::from_utf8(p).ok())
            .filter_map(|b| u8::from_str_radix(b, 16).ok())
            .collect()
    }
    let b = hex_to_bytes("0123456789ABCDEF");
    assert_eq!(b, vec![1, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF]);
}

/// Tests the `timestamp_to_systemtime()` function, which converts a Unix timestamp to a `SystemTime` instance.
#[test]
fn test_timestamp_to_systemtime() {
    let timestamp: u64 = 1618304400; // 2021-04-12 12:00:00 UTC
    let system_time = SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp);
    assert_eq!(system_time.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(), timestamp);
}

/// Tests the `systemtime_to_timestamp()` function, which converts a `SystemTime` instance to a Unix timestamp.
#[test]
fn test_systemtime_to_timestamp() {
    let system_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1618304400);
    let ts = system_time.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    assert_eq!(ts, 1618304400);
}
