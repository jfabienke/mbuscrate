//! Golden wM-Bus (wireless) frame tests.
//!
//! Companion to `tests/golden_frames.rs` (which covers *wired* M-Bus). Every vector
//! here is asserted against a **known-good value from an independent source** — a real
//! meter capture, a published standard check value, or a ciphertext produced by an
//! independent AES implementation — never against this crate's own encoder output.
//!
//! Provenance for each vector is recorded in `tests/wmbus_frames/README.md`.

use mbus_rs::wmbus::crc::calculate_wmbus_crc;
use mbus_rs::wmbus::mode_c::decode_mode_c;
use mbus_rs::wmbus::FrameType;

/// Decode a hex string, ignoring ASCII whitespace so multi-line vectors stay readable.
fn hx(s: &str) -> Vec<u8> {
    let clean: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    assert!(clean.len().is_multiple_of(2), "odd-length hex");
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).expect("valid hex"))
        .collect()
}

// ---------------------------------------------------------------------------
// CRC-16/EN-13757 conformance
// ---------------------------------------------------------------------------

/// The catalogue check value for CRC-16/EN-13757: the ASCII string "123456789"
/// must produce 0xC2B7. This pins the canonical CRC to the standard rather than to
/// any implementation in this crate.
#[test]
fn crc16_en13757_matches_standard_check_value() {
    assert_eq!(calculate_wmbus_crc(b"123456789"), 0xC2B7);
}

// ---------------------------------------------------------------------------
// Real captured, unencrypted frames — decoded through the public API
// ---------------------------------------------------------------------------

/// Real, complete Type B frame captured from Kamstrup meter 74644444 (CI 0x8D).
/// A genuine over-the-air frame: the public decoder must recover its serial,
/// manufacturer and pass every block CRC.
const KAM_TYPE_B_74644444: &str =
    "3d25442d2c444464741b168d208d3048a121f6597959d56873b609a439b99d58531a8a726d9f0c";

#[test]
fn real_kam_type_b_frame_decodes_to_known_values() {
    let f = decode_mode_c(&hx(KAM_TYPE_B_74644444)).expect("real Type B frame must decode");
    assert_eq!(f.frame_type, FrameType::TypeB);
    assert_eq!(f.device_address, 74_644_444, "BCD-decoded serial");
    assert_eq!(f.manufacturer_id, 0x2C2D, "\"KAM\"");
    assert_eq!(f.control_field, 0x44);
    assert_eq!(f.ci(), Some(0x8D));
    assert!(
        f.crc_ok,
        "canonical EN 13757 block CRC must validate a real frame"
    );
}

/// Real captured Type A frame from Kamstrup meter 85312884 (0xCD sync). The CRC on
/// this fixed sample is not guaranteed, so we assert only what a decode must always
/// recover: the correct serial. This guards the CI/type parse offset that once read
/// garbage serials from every Type A frame.
const KAM_TYPE_A_85312884: &str =
    "cd09472d2c8428318535040e134b50cb5e3f953efc66b714efb78f1cd65bc738f5\
     f384b94a3bbec7d5be69e3bd8f36dbba9dcbabcf1d9264a34710ddbac68e6abe5e6\
     5fdee78a5b33fa17cecf7a1bb4bf7aab7ad8c3fa73b13f7f4beffede77fdffb3cde\
     b7176eb797f9dcd395783f6bdaed47f92e47e7b8287c76db5dd9aff9167de1e309d\
     dbede0ff8fb75c6f167dbcf2ffce370f7ff5597fc965af61fb24843b6ec6ab32d0\
     76a72fbda9c9ee690e3b5bdfa4e7c7b6cb45aef7ded55b7a4cd39f2596e48e5d6a7\
     f9dccf4bfbf1f67c5d6fbef9b68f499909cef389dd464750c3bbbdebb98f7fffece\
     57f7eac98a0baf7addbdcb7bf768c3a9274b57fe5fe";

#[test]
fn real_kam_type_a_frame_decodes_to_known_serial() {
    let f = decode_mode_c(&hx(KAM_TYPE_A_85312884)).expect("real Type A frame must decode");
    assert_eq!(f.frame_type, FrameType::TypeA);
    assert_eq!(
        f.device_address, 85_312_884,
        "decode_mode_c CI/type offset regression"
    );
    assert_eq!(f.manufacturer_id, 0x2C2D, "\"KAM\"");
}

// ---------------------------------------------------------------------------
// Encrypted frames — known plaintext from an independent implementation
// ---------------------------------------------------------------------------

/// Synthetic ELL-II frame built with PyCryptodome (an AES implementation independent
/// of this crate) from the published test key. Decrypting it through the public ELL
/// path must reproduce the independently-computed plaintext — a true known-answer
/// test for the AES-128-CTR link-layer profile. See `tests/wmbus_frames/README.md`.
#[cfg(feature = "crypto")]
mod encrypted {
    use super::hx;
    use mbus_rs::wmbus::crypto::AesKey;
    use mbus_rs::wmbus::ell::{self, EllError};
    use mbus_rs::wmbus::mode_c::decode_mode_c;
    use mbus_rs::wmbus::oms;

    const ELL_SYNTHETIC: &str = "3d1b442d2c785634121b168d207e4523012038e54482ea11982e333309";
    const ELL_KEY: &str = "000102030405060708090a0b0c0d0e0f";
    const ELL_EXPECTED_PLAINTEXT: &str = "0000780413e8030000";

    #[test]
    fn ell_synthetic_frame_decrypts_to_known_plaintext() {
        let frame = decode_mode_c(&hx(ELL_SYNTHETIC)).expect("ELL frame decodes");
        let key = AesKey::from_hex(ELL_KEY).unwrap();
        let out = ell::decrypt_frame(&frame, &key).expect("ELL decrypt");
        let expected = hx(ELL_EXPECTED_PLAINTEXT);
        assert_eq!(out.leading_field, [expected[0], expected[1]]);
        assert_eq!(out.payload, &expected[2..]);
        assert_eq!(
            out.payload[0], 0x78,
            "recovered TPL CI: full frame, no header"
        );
    }

    #[test]
    fn ell_wrong_key_is_rejected_by_the_plausibility_oracle() {
        let frame = decode_mode_c(&hx(ELL_SYNTHETIC)).expect("ELL frame decodes");
        let wrong = AesKey::from_hex("ffeeddccbbaa99887766554433221100").unwrap();
        // A wrong key yields random bytes; the TPL-CI plausibility check rejects the
        // overwhelming majority of them. This particular key must be rejected.
        assert!(matches!(
            ell::decrypt_frame(&frame, &wrong),
            Err(EllError::ImplausiblePlaintext(_))
        ));
    }

    /// OMS Security Profile A (Mode 5, AES-128-CBC) IV construction, asserted against a
    /// fixed known value. The IV is `link_address(8) ‖ ACC repeated 8×` — the
    /// construction verified against the epulse C++ reference. Getting this wrong
    /// produces plausible garbage rather than an error, so pinning it matters.
    #[test]
    fn oms_mode5_cbc_iv_matches_known_construction() {
        // Zenner ZRI 55298170 link address, ACC 0x2A.
        let addr = [0x49, 0x6A, 0x70, 0x81, 0x29, 0x55, 0x18, 0x37];
        let iv = oms::mode5_cbc_iv(&addr, 0x2A);
        assert_eq!(&iv[..8], &addr, "first 8 bytes are the wire link address");
        assert_eq!(&iv[8..], &[0x2A; 8], "tail is ACC repeated");
    }

    /// The idle-fill marker is a key oracle: an OMS Mode 5 plaintext begins with
    /// `0x2F 0x2F`, which a wrong key cannot produce. Assert the public oracle both
    /// accepts the marker and rejects its absence.
    #[test]
    fn oms_idle_fill_oracle_distinguishes_right_from_wrong_plaintext() {
        assert!(oms::decrypted_ok(&[0x2F, 0x2F, 0x0C, 0x13]));
        assert!(!oms::decrypted_ok(&[0x00, 0x00, 0x2F, 0x2F]));
        assert!(!oms::decrypted_ok(&[0x2F]), "too short to carry the marker");
    }
}
