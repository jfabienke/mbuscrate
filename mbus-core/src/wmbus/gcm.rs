//! OMS security mode 9 (AES-GCM) header construction.
//!
//! The additional authenticated data and the initialisation vector, both of which are
//! fixed-size functions of the frame header and device identity. These moved because they
//! are pure and already allocation-free; the cipher operations themselves stay in
//! `mbus-rs` for now, because `aes-gcm`'s allocating `Aead` API would have to be rewritten
//! onto `AeadInPlace` first.
//!
//! Getting either of these wrong produces a frame that authenticates against nothing, and
//! the failure is indistinguishable from a wrong key — so the layouts are spelled out.

use crate::wmbus::crypto::{CryptoError, DeviceInfo};

/// Bytes of additional authenticated data, per OMS 7.3.6.2.
pub const GCM_AAD_LEN: usize = 11;
/// Bytes of initialisation vector, per OMS 7.3.6.3. GCM's native nonce size.
pub const GCM_IV_LEN: usize = 12;

/// Build the 11-byte AAD from a frame's link header (OMS 7.3.6.2).
///
/// `AAD = L(1) ‖ C(1) ‖ M(2) ‖ A(4) ‖ V(1) ‖ T(1) ‖ Access(1)` — the header fields that
/// are transmitted in the clear and must therefore be authenticated rather than encrypted.
///
/// # The last byte is not what it claims to be
///
/// Bytes 0..10 are the header verbatim. Byte 10 is **not**: it is `frame[10] & 0x0F`,
/// and at offset 10 a mode-9 frame carries the *CI byte*, not an access number. So the
/// AAD's "Access" field is currently the low nibble of the CI.
///
/// That is preserved here byte for byte, because changing what goes into an AAD changes
/// which frames authenticate, and doing that silently during a refactor would be far
/// worse than the discrepancy itself. The original carried the admission in a comment:
/// *"For now, use the CI field position value or a default"*. It should be checked
/// against OMS 7.3.6.2 and, if wrong, fixed deliberately with test vectors — not fixed
/// by accident while moving the file.
pub fn build_gcm_aad(frame: &[u8]) -> Result<[u8; GCM_AAD_LEN], CryptoError> {
    if frame.len() < GCM_AAD_LEN {
        return Err(CryptoError::InvalidFrame("frame too short for GCM AAD"));
    }
    let mut aad = [0u8; GCM_AAD_LEN];
    aad[..10].copy_from_slice(&frame[..10]);
    aad[10] = frame[10] & 0x0F;
    Ok(aad)
}

/// Build the 12-byte IV from device identity (OMS 7.3.6.3).
///
/// `IV = M(2 LE) ‖ A(4 LE) ‖ Access(6 LE)`. Note this is 12 bytes, unlike modes 5 and 7
/// which use a 16-byte IV — GCM takes a 96-bit nonce.
///
/// When the frame carried no access number, one is synthesised from version and device
/// type. That is a compatibility fallback, not a spec behaviour: an IV must never repeat
/// under one key, and a synthesised one is constant per device, so it repeats on every
/// frame. Supply the frame's own access number wherever it is available.
pub fn build_gcm_iv(device_info: &DeviceInfo) -> [u8; GCM_IV_LEN] {
    let mut iv = [0u8; GCM_IV_LEN];
    iv[0..2].copy_from_slice(&device_info.manufacturer.to_le_bytes());
    iv[2..6].copy_from_slice(&device_info.device_id.to_le_bytes());

    let access_number = device_info
        .access_number
        .unwrap_or(((device_info.version as u64) << 8) | (device_info.device_type as u64));
    iv[6..12].copy_from_slice(&access_number.to_le_bytes()[0..6]);
    iv
}

/// The access number byte a wM-Bus frame carries at offset 10, if it is long enough.
pub fn extract_access_number(frame: &[u8]) -> Option<u64> {
    frame.get(10).map(|&b| b as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev() -> DeviceInfo {
        DeviceInfo {
            device_id: 0x1234_5678,
            manufacturer: 0xABCD,
            version: 0x01,
            device_type: 0x02,
            access_number: Some(0x2A),
        }
    }

    #[test]
    fn aad_is_the_header_except_for_its_last_byte() {
        let frame = [
            0x44, 0x10, 0xCD, 0xAB, 0x78, 0x56, 0x34, 0x12, 0x01, 0x02, 0x89, 0xFF,
        ];
        let aad = build_gcm_aad(&frame).unwrap();
        assert_eq!(&aad[..10], &frame[..10], "L C M A V T copied verbatim");
        // Byte 10 is masked, not copied. At this offset a mode-9 frame carries the CI
        // byte, so the AAD's nominal "access number" field is really `CI & 0x0F` — see
        // the note on `build_gcm_aad`. Asserted so the quirk cannot be lost silently.
        assert_eq!(aad[10], 0x09, "0x89 & 0x0F, not the raw 0x89");
        assert_eq!(aad.len(), GCM_AAD_LEN);
    }

    #[test]
    fn a_short_frame_cannot_produce_an_aad() {
        assert_eq!(
            build_gcm_aad(&[0u8; 10]),
            Err(CryptoError::InvalidFrame("frame too short for GCM AAD"))
        );
    }

    #[test]
    fn iv_is_manufacturer_address_and_access_all_little_endian() {
        let iv = build_gcm_iv(&dev());
        assert_eq!(&iv[0..2], &[0xCD, 0xAB], "manufacturer LE");
        assert_eq!(&iv[2..6], &[0x78, 0x56, 0x34, 0x12], "device id LE");
        assert_eq!(&iv[6..12], &[0x2A, 0, 0, 0, 0, 0], "access number LE");
        assert_eq!(iv.len(), 12, "GCM takes a 96-bit nonce, not 128");
    }

    #[test]
    fn a_missing_access_number_gives_a_constant_iv_which_is_the_hazard() {
        // Documents the fallback rather than endorsing it: with no access number the IV
        // depends only on fixed device fields, so it is identical for every frame from
        // that device. Reusing a GCM nonce under one key is catastrophic, so this exists
        // for compatibility and callers should supply the frame's access number.
        let mut a = dev();
        a.access_number = None;
        let b = a;
        assert_eq!(build_gcm_iv(&a), build_gcm_iv(&b));
        assert_ne!(
            build_gcm_iv(&a),
            build_gcm_iv(&dev()),
            "the fallback must at least differ from a real access number"
        );
    }

    #[test]
    fn access_number_comes_from_offset_ten_when_present() {
        let mut frame = [0u8; 11];
        frame[10] = 0x7F;
        assert_eq!(extract_access_number(&frame), Some(0x7F));
        assert_eq!(extract_access_number(&[0u8; 10]), None, "too short");
    }
}
