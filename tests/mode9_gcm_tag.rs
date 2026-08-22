//! OMS Mode 9 (AES-GCM) with the spec's 12-byte truncated tag.
//!
//! OMS 7.3.6 truncates the GCM tag to 12 bytes on air. The decrypt path used to
//! zero-pad that back to 16 and hand it to `aes-gcm` for verification — but a genuine
//! 16-byte tag's last four bytes are not zeros, so verification failed with probability
//! 1 − 2⁻³². Only the non-standard `set_tag_mode(true)` "compatibility" path, which keeps
//! the full 16-byte tag, ever worked.

use mbus_rs::wmbus::crypto::{AesKey, DeviceInfo, WMBusCrypto};

fn device() -> DeviceInfo {
    DeviceInfo {
        device_id: 0x1234_5678,
        manufacturer: 0xABCD,
        version: 0x01,
        device_type: 0x02,
        access_number: None,
    }
}

/// Exactly the frame layout the crate's own `test_mode9_gcm_round_trip` uses, so the only
/// variable between these tests and that one is the tag length.
const PAYLOAD: [u8; 8] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];

fn plaintext_frame() -> Vec<u8> {
    let mut f = vec![
        0x44, 0x10, 0xCD, 0xAB, 0x78, 0x56, 0x34, 0x12, 0x01, 0x02, 0x89, // CI=0x89
    ];
    f.extend_from_slice(&PAYLOAD);
    f
}

#[test]
fn mode9_round_trips_with_the_oms_12_byte_tag() {
    let key = AesKey::from_hex("0123456789ABCDEF0123456789ABCDEF").unwrap();
    let mut c = WMBusCrypto::new(key);
    c.set_tag_mode(false); // OMS standard: 12-byte truncated tag

    let plain = plaintext_frame();
    let encrypted = c.encrypt_mode9_gcm(&plain, &device()).expect("encrypt");
    let decrypted = c
        .decrypt_mode9_gcm(&encrypted, &device())
        .expect("a frame this crate encrypted must authenticate and decrypt");

    assert_eq!(
        &decrypted[11..],
        &PAYLOAD,
        "Mode 9 must recover the payload"
    );
}

#[test]
fn mode9_still_round_trips_in_full_tag_compatibility_mode() {
    let key = AesKey::from_hex("0123456789ABCDEF0123456789ABCDEF").unwrap();
    let mut c = WMBusCrypto::new(key);
    c.set_tag_mode(true); // non-standard 16-byte tag

    let plain = plaintext_frame();
    let encrypted = c.encrypt_mode9_gcm(&plain, &device()).expect("encrypt");
    let decrypted = c.decrypt_mode9_gcm(&encrypted, &device()).expect("decrypt");
    assert_eq!(&decrypted[11..], &PAYLOAD);
}

#[test]
fn a_tampered_12_byte_tag_is_still_rejected() {
    let key = AesKey::from_hex("0123456789ABCDEF0123456789ABCDEF").unwrap();
    let mut c = WMBusCrypto::new(key);
    c.set_tag_mode(false);

    let plain = plaintext_frame();
    let mut encrypted = c.encrypt_mode9_gcm(&plain, &device()).expect("encrypt");
    let last = encrypted.len() - 1;
    encrypted[last] ^= 0x01; // flip a bit in the tag

    assert!(
        c.decrypt_mode9_gcm(&encrypted, &device()).is_err(),
        "truncating the tag must not weaken authentication to the point of accepting forgeries"
    );
}
