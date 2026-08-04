//! Extended Link Layer (ELL) — EN 13757-4 §12.
//!
//! The ELL sits between the data link layer and the transport layer, and carries its
//! own encryption for the application payload. Kamstrup Multical meters (and many
//! other OMS devices) use it in preference to the TPL-level security profiles, so a
//! wM-Bus receiver that cannot process ELL cannot read them at all.
//!
//! Four variants exist, distinguished by the CI byte that opens the payload:
//!
//! | CI     | Variant | Fields after CI                     |
//! |--------|---------|-------------------------------------|
//! | `0x8C` | ELL-I   | CC, ACC                             |
//! | `0x8D` | ELL-II  | CC, ACC, SN, payload field (2 bytes)|
//! | `0x8E` | ELL-III | CC, ACC, M2, A2                     |
//! | `0x8F` | ELL-IV  | CC, ACC, M2, A2, SN, payload field  |
//!
//! Only the variants carrying a session number (`0x8D`, `0x8F`) can be encrypted: the
//! SN field's top three bits select the security profile, and its remaining bits feed
//! the AES-CTR initialisation vector.
//!
//! ## Encryption
//!
//! With `ENC == 1` the payload is AES-128 in counter mode. The initial counter block
//! is built entirely from cleartext link-layer material:
//!
//! ```text
//! IV = M(2) ‖ A(6) ‖ CC(1) ‖ SN(4) ‖ 0x00 0x00 0x00
//!      └─ link header, wire order ─┘   └ frame no. ┘ block counter
//! ```
//!
//! Because the session number changes per transmission, the keystream does too — which
//! is what makes CTR safe to reuse here across a meter's lifetime.
//!
//! ## Verifying a decrypt
//!
//! ELL-II/IV place a two-byte field at the start of the *encrypted* region. It is
//! widely described as a payload CRC, but captured Kamstrup traffic refutes that
//! reading: two frames from the same meter carry byte-identical values while their
//! payloads differ (volume `25.539` vs. differing info codes), and no CRC-16 variant
//! over any prefix or suffix of the plaintext reproduces it. It is therefore exposed
//! verbatim as [`DecryptedEll::leading_field`] and **not** used to authenticate.
//!
//! Instead, a decrypt is accepted when the byte following that field is a plausible
//! transport-layer CI ([`is_plausible_tpl_ci`]). A wrong key yields uniformly random
//! bytes, so this rejects the overwhelming majority of bad keys — but it is a
//! heuristic, not a MAC, and callers that need authenticity must validate the parsed
//! records as well.

use crate::wmbus::crypto::AesKey;
use crate::wmbus::mode_c::WMBusLinkFrame;
use thiserror::Error;

/// ELL CI bytes.
pub const CI_ELL_I: u8 = 0x8C;
pub const CI_ELL_II: u8 = 0x8D;
pub const CI_ELL_III: u8 = 0x8E;
pub const CI_ELL_IV: u8 = 0x8F;

/// Failures specific to the extended link layer.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum EllError {
    #[error("CI 0x{0:02X} is not an extended link layer header")]
    NotEll(u8),
    #[error("ELL header truncated: need {needed} bytes, have {actual}")]
    Truncated { needed: usize, actual: usize },
    #[error("ELL variant CI 0x{0:02X} carries no session number, so it cannot be encrypted")]
    NotEncryptable(u8),
    #[error(
        "unsupported ELL security profile {0} (only 0 = none and 1 = AES-128-CTR are defined)"
    )]
    UnsupportedProfile(u8),
    #[error("payload is not encrypted; read it directly")]
    NotEncrypted,
    #[error("decrypt rejected: plaintext does not begin with a plausible TPL CI (byte 0x{0:02X}) — wrong key")]
    ImplausiblePlaintext(u8),
}

/// Security profile selected by the top three bits of the session number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EllSecurity {
    /// `ENC == 0`: payload is cleartext.
    None,
    /// `ENC == 1`: payload is AES-128-CTR with the IV described in the module docs.
    Aes128Ctr,
    /// Any other value — reserved by the standard; treated as an error when decrypting.
    Reserved(u8),
}

impl EllSecurity {
    fn from_sn(sn: u32) -> Self {
        match (sn >> 29) & 0x07 {
            0 => Self::None,
            1 => Self::Aes128Ctr,
            other => Self::Reserved(other as u8),
        }
    }
}

/// A parsed extended link layer header.
#[derive(Debug, Clone, PartialEq)]
pub struct EllHeader {
    /// The CI byte that introduced this header.
    pub ci: u8,
    /// Communication control field.
    pub cc: u8,
    /// Access number.
    pub acc: u8,
    /// Session number, present only for ELL-II and ELL-IV.
    pub session_number: Option<u32>,
    /// Security profile derived from the session number.
    pub security: EllSecurity,
    /// Bytes consumed by this header, i.e. the offset of the payload within the
    /// frame payload buffer.
    pub header_len: usize,
}

impl EllHeader {
    /// Whether the payload following this header is encrypted.
    pub fn is_encrypted(&self) -> bool {
        matches!(self.security, EllSecurity::Aes128Ctr)
    }
}

/// The result of decrypting an ELL payload.
#[derive(Debug, Clone, PartialEq)]
pub struct DecryptedEll {
    /// The two bytes opening the encrypted region, verbatim. Commonly documented as a
    /// payload CRC, but see the module docs — captured traffic contradicts that, so
    /// this crate does not interpret it.
    pub leading_field: [u8; 2],
    /// Decrypted transport-layer payload, beginning at its CI byte.
    pub payload: Vec<u8>,
}

/// Whether `ci` is a transport-layer CI a decrypted ELL payload could plausibly start
/// with. Used as the decrypt acceptance heuristic — see the module docs.
pub fn is_plausible_tpl_ci(ci: u8) -> bool {
    // Only *response* CIs can open a payload a meter transmits; command CIs (0x50,
    // 0x51, …) travel in the other direction and would be nonsense here. Keeping the
    // set this tight matters: every extra accepted value raises the odds that a wrong
    // key's random first byte is mistaken for a successful decrypt.
    matches!(
        ci,
        0x6E        // response, no header, from a repeater
            | 0x6F
            | 0x72  // response, long TPL header
            | 0x78  // response, no TPL header (full frame)
            | 0x79  // response, compact frame
            | 0x7A  // response, short TPL header
            | 0x7B
    )
}

/// Parse an extended link layer header from a frame payload (which begins at the CI
/// byte, as [`WMBusLinkFrame::payload`] does).
pub fn parse_ell(payload: &[u8]) -> Result<EllHeader, EllError> {
    let ci = *payload.first().ok_or(EllError::Truncated {
        needed: 1,
        actual: 0,
    })?;
    // Field layout per variant: (bytes after CI before the payload, has session number)
    let (len_after_ci, has_sn) = match ci {
        CI_ELL_I => (2, false),    // CC, ACC
        CI_ELL_II => (6, true),    // CC, ACC, SN
        CI_ELL_III => (10, false), // CC, ACC, M2, A2
        CI_ELL_IV => (14, true),   // CC, ACC, M2, A2, SN
        other => return Err(EllError::NotEll(other)),
    };
    let needed = 1 + len_after_ci;
    if payload.len() < needed {
        return Err(EllError::Truncated {
            needed,
            actual: payload.len(),
        });
    }

    let cc = payload[1];
    let acc = payload[2];
    // The session number always occupies the last four bytes of the header.
    let session_number = has_sn.then(|| {
        let sn = &payload[needed - 4..needed];
        u32::from_le_bytes([sn[0], sn[1], sn[2], sn[3]])
    });
    let security = session_number.map_or(EllSecurity::None, EllSecurity::from_sn);

    Ok(EllHeader {
        ci,
        cc,
        acc,
        session_number,
        security,
        header_len: needed,
    })
}

/// Build the AES-CTR initial counter block for an ELL payload.
///
/// `link_header` is M(2) ‖ A(6) exactly as received — see
/// [`WMBusLinkFrame::link_header`].
pub fn ell_ctr_iv(link_header: &[u8; 8], cc: u8, session_number: u32) -> [u8; 16] {
    let mut iv = [0u8; 16];
    iv[..8].copy_from_slice(link_header);
    iv[8] = cc;
    iv[9..13].copy_from_slice(&session_number.to_le_bytes());
    // iv[13..16] = frame number (2) ‖ block counter (1), both zero for a single-part
    // telegram; the CTR mode itself increments from here.
    iv
}

/// Decrypt an ELL-encrypted payload.
///
/// `payload` is the frame payload beginning at the ELL CI byte, `link_header` the
/// wire-order M and A fields. Returns the decrypted transport payload, or an error if
/// the header says the payload is not encrypted, the profile is unsupported, or the
/// plaintext fails the plausibility check described in the module docs.
pub fn decrypt_ell_payload(
    payload: &[u8],
    link_header: &[u8; 8],
    key: &AesKey,
) -> Result<DecryptedEll, EllError> {
    use aes::cipher::{KeyIvInit, StreamCipher};

    let header = parse_ell(payload)?;
    let sn = header
        .session_number
        .ok_or(EllError::NotEncryptable(header.ci))?;
    match header.security {
        EllSecurity::Aes128Ctr => {}
        EllSecurity::None => return Err(EllError::NotEncrypted),
        EllSecurity::Reserved(p) => return Err(EllError::UnsupportedProfile(p)),
    }

    let ciphertext = &payload[header.header_len..];
    if ciphertext.len() < 3 {
        return Err(EllError::Truncated {
            needed: header.header_len + 3,
            actual: payload.len(),
        });
    }

    let iv = ell_ctr_iv(link_header, header.cc, sn);
    let mut buf = ciphertext.to_vec();
    ctr::Ctr128BE::<aes::Aes128>::new(key.as_bytes().into(), &iv.into()).apply_keystream(&mut buf);

    let tpl_ci = buf[2];
    if !is_plausible_tpl_ci(tpl_ci) {
        return Err(EllError::ImplausiblePlaintext(tpl_ci));
    }
    Ok(DecryptedEll {
        leading_field: [buf[0], buf[1]],
        payload: buf[2..].to_vec(),
    })
}

/// Convenience wrapper: decrypt the ELL payload of a decoded mode-C frame.
pub fn decrypt_frame(frame: &WMBusLinkFrame, key: &AesKey) -> Result<DecryptedEll, EllError> {
    decrypt_ell_payload(&frame.payload, &frame.link_header, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic ELL-II frame built with an independent AES implementation
    /// (PyCryptodome) from a published test key — never a real meter key. See
    /// `tests/wmbus_frames/README.md`.
    const SYNTHETIC_FRAME: &str = "3d1b442d2c785634121b168d207e4523012038e54482ea11982e333309";
    const TEST_KEY: &str = "000102030405060708090a0b0c0d0e0f";
    const EXPECTED_PLAINTEXT: &str = "0000780413e8030000";

    fn frame() -> WMBusLinkFrame {
        crate::wmbus::mode_c::decode_mode_c(&hex::decode(SYNTHETIC_FRAME).unwrap()).unwrap()
    }

    #[test]
    fn parses_ell_ii_header() {
        let f = frame();
        let h = parse_ell(&f.payload).unwrap();
        assert_eq!(h.ci, CI_ELL_II);
        assert_eq!(h.cc, 0x20);
        assert_eq!(h.acc, 0x7E);
        assert_eq!(h.security, EllSecurity::Aes128Ctr);
        assert!(h.is_encrypted());
        assert_eq!(h.header_len, 7); // CI + CC + ACC + SN
    }

    #[test]
    fn rejects_non_ell_ci() {
        assert_eq!(parse_ell(&[0x72, 0x00]), Err(EllError::NotEll(0x72)));
        assert_eq!(
            parse_ell(&[CI_ELL_II, 0x20]),
            Err(EllError::Truncated {
                needed: 7,
                actual: 2
            })
        );
    }

    #[test]
    fn security_profile_comes_from_session_number_high_bits() {
        assert_eq!(EllSecurity::from_sn(0), EllSecurity::None);
        assert_eq!(EllSecurity::from_sn(1 << 29), EllSecurity::Aes128Ctr);
        assert_eq!(EllSecurity::from_sn(5 << 29), EllSecurity::Reserved(5));
    }

    /// Known-answer test: the ciphertext and expected plaintext were produced by
    /// PyCryptodome, so this asserts against an independent implementation rather
    /// than round-tripping our own.
    #[test]
    fn decrypts_synthetic_ell_frame_to_known_plaintext() {
        let f = frame();
        let key = AesKey::from_hex(TEST_KEY).unwrap();
        let out = decrypt_frame(&f, &key).unwrap();
        let expected = hex::decode(EXPECTED_PLAINTEXT).unwrap();
        assert_eq!(out.leading_field, [expected[0], expected[1]]);
        assert_eq!(out.payload, &expected[2..]);
        assert_eq!(out.payload[0], 0x78, "TPL CI: full frame, no header");
    }

    #[test]
    fn ctr_iv_is_link_header_cc_and_session_number() {
        let f = frame();
        let h = parse_ell(&f.payload).unwrap();
        let iv = ell_ctr_iv(&f.link_header, h.cc, h.session_number.unwrap());
        assert_eq!(&iv[..8], &f.link_header, "M ‖ A occupy the first 8 bytes");
        assert_eq!(iv[8], h.cc);
        assert_eq!(
            &iv[13..],
            &[0, 0, 0],
            "frame number and block counter start at 0"
        );
        assert_eq!(hex::encode(iv), "2d2c785634121b162045230120000000");
    }

    /// The plausibility check is a heuristic, not a MAC, so this measures it rather
    /// than pretending it is exact: 7 of 256 leading-byte values are accepted, so a
    /// wrong key should be rejected ~97% of the time. Asserting the *rate* keeps the
    /// test honest about the residual false-accept probability documented above.
    #[test]
    fn wrong_keys_are_overwhelmingly_rejected() {
        let f = frame();
        let (mut rejected, total) = (0u32, 256u32);
        for i in 0..total {
            let mut kb = [0u8; 16];
            // Vary every byte so the trial keys are not a low-entropy family.
            for (j, b) in kb.iter_mut().enumerate() {
                *b = (i as u8).wrapping_mul(31).wrapping_add(j as u8 * 7 + 1);
            }
            let key = AesKey::from_bytes(&kb).unwrap();
            if matches!(
                decrypt_ell_payload(&f.payload, &f.link_header, &key),
                Err(EllError::ImplausiblePlaintext(_))
            ) {
                rejected += 1;
            }
        }
        let rate = f64::from(rejected) / f64::from(total);
        assert!(
            rate > 0.90,
            "wrong-key rejection rate {rate:.3} — the CI plausibility set has grown too permissive"
        );
    }

    #[test]
    fn unencrypted_ell_payload_is_not_decrypted() {
        // Same header shape with ENC = 0 in the session number.
        let mut payload = vec![CI_ELL_II, 0x20, 0x7E];
        payload.extend_from_slice(&0x0012_3450u32.to_le_bytes());
        payload.extend_from_slice(&[0x78, 0x04, 0x13]);
        let key = AesKey::from_hex(TEST_KEY).unwrap();
        assert_eq!(
            decrypt_ell_payload(&payload, &[0u8; 8], &key),
            Err(EllError::NotEncrypted)
        );
    }
}
