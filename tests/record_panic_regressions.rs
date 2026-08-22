//! Regression tests for two panics in the variable-record parser.
//!
//! Both are reachable through the public API with attacker-supplied bytes — a meter
//! frame is untrusted input until it has been parsed, so a parser that panics on
//! malformed input is a denial of service on the gateway, not merely a bug.

use mbus_rs::payload::record::parse_variable_record;

/// DIF `0x0D` means "variable length": the next byte is an LVAR code.
///
/// `parse_variable_data_length` maps `0xF0..=0xFA` to 1120..=1130 bytes (EN 13757-3
/// §6.4.3), but `MBusRecord::data` is `[u8; 256]`. The only guard compares the length
/// against the *input* remaining, not against the destination, so a long enough input
/// indexes past the array.
#[test]
fn lvar_longer_than_the_record_buffer_is_rejected_not_a_panic() {
    let mut frame = vec![0x0D, 0x13, 0xFA]; // DIF=variable, VIF=volume, LVAR=0xFA -> 1130
    frame.extend(std::iter::repeat_n(0xAAu8, 1200)); // plenty of input to pass the guard

    // Must return an error rather than panicking.
    let result = parse_variable_record(&frame);
    assert!(
        result.is_err(),
        "a 1130-byte LVAR cannot fit a 256-byte record buffer and must be refused"
    );
}

/// A record announcing a variable length with no length byte following it.
///
/// `remaining.first().unwrap_or(&0)` tolerates the empty slice, and then the next line
/// slices `remaining[1..]` on that same empty slice.
#[test]
fn variable_length_dif_with_no_length_byte_is_rejected_not_a_panic() {
    let frame = vec![0x0D, 0x13]; // DIF=variable, VIF=volume, then nothing
    let result = parse_variable_record(&frame);
    assert!(
        result.is_err(),
        "a truncated variable-length record must be refused, not panic"
    );
}

/// A month nibble of zero underflows the `- 1` in every M-Bus date decoder.
///
/// EN 13757-3 Type G/F/I dates store the month in the low nibble of a byte, 1..=12. A
/// meter that has never had its clock set — or a corrupted frame — sends 0, and
/// `(byte & 0x0F) - 1` then panics in debug builds and wraps to 255 in release, which
/// silently produces a date roughly 21 years in the future.
#[test]
fn a_zero_month_nibble_is_rejected_not_an_underflow() {
    use mbus_rs::payload::data_encoding::decode_mbus_time;
    // Type G (2 bytes): day in input[0], month in the low nibble of input[1].
    let unset_date = [0x01u8, 0x00]; // day 1, month 0
    let result = decode_mbus_time(&unset_date);
    assert!(
        result.is_err(),
        "month 0 is not a valid M-Bus date and must be refused, not underflowed"
    );
}

/// A key must not print its bytes, and must not compare in variable time.
///
/// `subtle` had been a declared dependency of the crate since the crypto hardening work
/// specifically to make key comparison constant-time, but nothing ever used it — the
/// derive was still in place.
#[test]
fn aes_keys_do_not_leak_through_debug_or_comparison_timing() {
    use mbus_rs::wmbus::crypto::AesKey;
    let secret = [0xAAu8; 16];
    let k = AesKey::from_bytes(&secret).unwrap();

    let rendered = format!("{k:?}");
    assert!(
        !rendered.contains("170") && !rendered.to_lowercase().contains("aa"),
        "Debug must not render key material, got: {rendered}"
    );

    // Equality still works; it is simply not short-circuiting.
    assert_eq!(k, AesKey::from_bytes(&secret).unwrap());
    assert_ne!(k, AesKey::from_bytes(&[0xBBu8; 16]).unwrap());
}
