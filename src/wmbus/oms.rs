//! OMS Security Profile A (mode 5) transport-layer decryption.
//!
//! The ELL path in [`crate::wmbus::ell`] handles the CTR-based link-layer profile;
//! this handles the CBC-based transport profile carried under a short (CI `0x7A`) or
//! long (CI `0x72`) TPL header, which is what Kamstrup and Zenner OMS meters use.
//!
//! The IV construction is the one part worth stating precisely, because getting it
//! wrong produces plausible-looking garbage rather than an error — and the crate's
//! previous `build_cbc_iv` did exactly that, filling the tail with a fabricated
//! device-id pattern. The correct construction, verified against the epulse C++
//! reference (`wmbus-device.cc:179`):
//!
//! ```text
//! IV = link_address(8) ‖ ACC(1) repeated 8×
//! ```
//!
//! where `link_address` is the wire-order M(2) ‖ ID(4) ‖ version(1) ‖ type(1) — the
//! same eight bytes the link layer already carries — and ACC is the access counter
//! from the TPL header.

#![cfg(feature = "crypto")]

use crate::wmbus::crypto::AesKey;
use aes::cipher::{BlockDecryptMut, KeyIvInit};

/// OMS mode-5 plaintext begins with idle-fill bytes `0x2F 0x2F` (EN 13757-3 §6.4.4:
/// `0x2F` is the DIF/VIF idle filler). Two of them at the front is the standard
/// oracle for "this decrypt used the right key" — a wrong key cannot produce it.
pub const OMS_IDLE_FILL: u8 = 0x2F;

/// Build the OMS Security Profile A (mode 5) CBC IV.
///
/// `link_address` is the wire-order 8-byte M/ID/version/type field; `acc` the TPL
/// access counter. See the module docs for why the tail is ACC repeated.
pub fn mode5_cbc_iv(link_address: &[u8; 8], acc: u8) -> [u8; 16] {
    let mut iv = [0u8; 16];
    iv[..8].copy_from_slice(link_address);
    for b in iv[8..].iter_mut() {
        *b = acc;
    }
    iv
}

/// Decrypt an OMS mode-5 (AES-128-CBC) ciphertext.
///
/// Returns the plaintext without removing the leading idle-fill; callers inspect the
/// first bytes both to strip padding and to confirm the key (see
/// [`decrypted_ok`]). The ciphertext length must be a multiple of the 16-byte block
/// size, which the TPL configuration word guarantees for a well-formed frame.
pub fn decrypt_mode5_cbc(
    ciphertext: &[u8],
    link_address: &[u8; 8],
    acc: u8,
    key: &AesKey,
) -> Result<Vec<u8>, Mode5Error> {
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(16) {
        return Err(Mode5Error::BadLength(ciphertext.len()));
    }
    let iv = mode5_cbc_iv(link_address, acc);
    let mut buf = ciphertext.to_vec();
    // CBC with no padding: OMS pads with idle-fill inside the plaintext, not PKCS#7,
    // so the cipher must not try to interpret or strip a padding byte.
    cbc::Decryptor::<aes::Aes128>::new(key.as_bytes().into(), &iv.into())
        .decrypt_padded_mut::<cbc::cipher::block_padding::NoPadding>(&mut buf)
        .map_err(|_| Mode5Error::Cipher)?;
    Ok(buf)
}

/// Whether a decrypted mode-5 plaintext carries the OMS idle-fill marker, i.e. the
/// key was correct. This is the whole basis of the factory-default audit: a frame
/// that decrypts to `2F 2F …` under a given key was genuinely encrypted with it.
pub fn decrypted_ok(plaintext: &[u8]) -> bool {
    plaintext.len() >= 2 && plaintext[0] == OMS_IDLE_FILL && plaintext[1] == OMS_IDLE_FILL
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Mode5Error {
    #[error("ciphertext length {0} is not a non-zero multiple of the 16-byte block size")]
    BadLength(usize),
    #[error("CBC decryption failed")]
    Cipher,
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::{BlockEncryptMut, KeyIvInit};

    /// The Zenner factory default key — a published constant in Zenner's shipped
    /// software (`ZR_ClassLibrary/AES.cs`), not a secret, and therefore usable as a
    /// fixed golden-test value.
    const ZDK: &str = "5A8470C4806F4A87CEF4D5F2D985AB18";

    fn encrypt_mode5(plaintext: &[u8], link_address: &[u8; 8], acc: u8, key: &AesKey) -> Vec<u8> {
        let iv = mode5_cbc_iv(link_address, acc);
        let mut buf = plaintext.to_vec();
        cbc::Encryptor::<aes::Aes128>::new(key.as_bytes().into(), &iv.into())
            .encrypt_padded_mut::<cbc::cipher::block_padding::NoPadding>(&mut buf, plaintext.len())
            .unwrap();
        buf
    }

    #[test]
    fn round_trips_under_the_factory_default_and_marks_success() {
        let key = AesKey::from_hex(ZDK).unwrap();
        let addr = [0x49, 0x6A, 0x70, 0x81, 0x29, 0x55, 0x18, 0x37]; // ZRI 55298170
        let acc = 0x2A;
        // A block of OMS idle-fill followed by a volume record.
        let plain = [
            0x2F, 0x2F, 0x0C, 0x13, 0x70, 0x45, 0x23, 0x01, 0x2F, 0x2F, 0x2F, 0x2F, 0x2F, 0x2F,
            0x2F, 0x2F,
        ];
        let ct = encrypt_mode5(&plain, &addr, acc, &key);
        let pt = decrypt_mode5_cbc(&ct, &addr, acc, &key).unwrap();
        assert_eq!(pt, plain);
        assert!(decrypted_ok(&pt), "idle-fill marker must confirm the key");
    }

    #[test]
    fn a_wrong_key_fails_the_marker() {
        let key = AesKey::from_hex(ZDK).unwrap();
        let addr = [0x49, 0x6A, 0x70, 0x81, 0x29, 0x55, 0x18, 0x37];
        let plain = [0x2Fu8; 16];
        let ct = encrypt_mode5(&plain, &addr, 0x01, &key);

        let wrong = AesKey::from_hex("00112233445566778899AABBCCDDEEFF").unwrap();
        let pt = decrypt_mode5_cbc(&ct, &addr, 0x01, &wrong).unwrap();
        assert!(
            !decrypted_ok(&pt),
            "a wrong key must not produce the idle-fill marker"
        );
    }

    #[test]
    fn the_marker_is_a_key_oracle_not_an_acc_oracle() {
        // A CBC first block is P0 = Decrypt(C0) XOR IV. The IV's first 8 bytes are the
        // link address, so a wrong ACC (IV bytes 8..16) corrupts only the tail of
        // block 0, not the 0x2F 0x2F marker at its start. That is the right property
        // for the factory-default audit: the marker confirms the KEY, and the ACC
        // always comes from the frame itself, so it is never in question.
        let key = AesKey::from_hex(ZDK).unwrap();
        let addr = [0x49, 0x6A, 0x70, 0x81, 0x29, 0x55, 0x18, 0x37];
        let plain = [0x2Fu8; 16];
        let ct = encrypt_mode5(&plain, &addr, 0x01, &key);

        let right = decrypt_mode5_cbc(&ct, &addr, 0x01, &key).unwrap();
        let wrong_acc = decrypt_mode5_cbc(&ct, &addr, 0x99, &key).unwrap();
        assert!(decrypted_ok(&right));
        // Marker survives a wrong ACC (bytes 0..1), but the IV-affected tail does not.
        assert!(decrypted_ok(&wrong_acc));
        assert_ne!(
            right[8], wrong_acc[8],
            "the ACC region of block 0 must differ"
        );
    }

    #[test]
    fn rejects_non_block_lengths() {
        let key = AesKey::from_hex(ZDK).unwrap();
        let addr = [0u8; 8];
        assert!(matches!(
            decrypt_mode5_cbc(&[0u8; 15], &addr, 0, &key),
            Err(Mode5Error::BadLength(15))
        ));
    }
}
