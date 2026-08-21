//! Canonical wireless M-Bus CRC (CRC-16/EN-13757).
//!
//! This is the single source of truth for wM-Bus CRC in the crate. Every other
//! CRC path (`frame`, `frame_decode`, `block`) routes here.
//!
//! # Parameters (CRC-16/EN-13757)
//!
//! | field   | value    |
//! |---------|----------|
//! | width   | 16       |
//! | poly    | `0x3D65` |
//! | init    | `0x0000` |
//! | refin   | false    |
//! | refout  | false    |
//! | xorout  | `0xFFFF` |
//! | check   | `0xC2B7` | (CRC of the ASCII string `"123456789"`)
//!
//! This matches the deployed epulse gateway's `wmbus_crc` (`~crc16(data, 0x3D65)`
//! with a zero init) byte-for-byte — the authoritative reference for what the
//! meters on this network actually transmit.
//!
//! # On-wire byte order
//!
//! wM-Bus transmits each block's CRC **most-significant byte first** (big-endian).
//! Use [`read_crc_be`] to extract a stored CRC. (Note: the legacy single-CRC helpers
//! in [`super::frame`] historically used little-endian; that is a self-consistent
//! synthetic model and is being reconciled against real captures — see `frame.rs`.)

/// wM-Bus CRC polynomial (non-reflected, MSB-first).
pub const WMBUS_CRC_POLY: u16 = 0x3D65;

/// Compile-time CRC lookup table for the 0x3D65 polynomial (MSB-first).
const CRC_TABLE: [u16; 256] = build_table();

const fn build_table() -> [u16; 256] {
    let mut table = [0u16; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut crc = (i as u16) << 8;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ WMBUS_CRC_POLY
            } else {
                crc << 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

/// Calculate the wM-Bus CRC-16/EN-13757 over `data`.
///
/// Returns the final, complemented value (xorout `0xFFFF`) — i.e. the value that
/// appears on the wire, ready to compare against a received CRC.
///
/// # Examples
/// ```
/// use mbus_core::wmbus::crc::calculate_wmbus_crc;
/// assert_eq!(calculate_wmbus_crc(b"123456789"), 0xC2B7); // EN-13757 check value
/// ```
#[inline]
pub fn calculate_wmbus_crc(data: &[u8]) -> u16 {
    let mut crc: u16 = 0x0000;
    for &b in data {
        let idx = (((crc >> 8) ^ b as u16) & 0xFF) as usize;
        crc = (crc << 8) ^ CRC_TABLE[idx];
    }
    !crc // xorout 0xFFFF
}

/// Read a stored wM-Bus CRC (big-endian / MSB-first, as transmitted).
#[inline]
pub fn read_crc_be(bytes: &[u8]) -> u16 {
    debug_assert!(bytes.len() >= 2);
    ((bytes[0] as u16) << 8) | (bytes[1] as u16)
}

/// Verify `data` followed immediately by its 2-byte big-endian CRC.
#[inline]
pub fn verify_crc_be(data_with_crc: &[u8]) -> bool {
    if data_with_crc.len() < 2 {
        return false;
    }
    let split = data_with_crc.len() - 2;
    calculate_wmbus_crc(&data_with_crc[..split]) == read_crc_be(&data_with_crc[split..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn en13757_check_value() {
        // The defining check value for CRC-16/EN-13757.
        assert_eq!(calculate_wmbus_crc(b"123456789"), 0xC2B7);
    }

    #[test]
    fn empty_input_is_xorout() {
        // init 0x0000, no data, xorout 0xFFFF.
        assert_eq!(calculate_wmbus_crc(b""), 0xFFFF);
    }

    #[test]
    fn matches_epulse_reference_algorithm() {
        // Bit-serial reference (epulse `~crc16(data, 0x3D65)` with zero init).
        fn reference(data: &[u8]) -> u16 {
            let mut rem: u16 = 0;
            for &b in data {
                rem ^= (b as u16) << 8;
                for _ in 0..8 {
                    rem = if rem & 0x8000 != 0 {
                        (rem << 1) ^ 0x3D65
                    } else {
                        rem << 1
                    };
                }
            }
            !rem
        }
        for sample in [
            &b"123456789"[..],
            &[0x42u8; 14],
            &[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07],
            &[],
        ] {
            assert_eq!(
                calculate_wmbus_crc(sample),
                reference(sample),
                "mismatch on {sample:02X?}"
            );
        }
    }

    #[test]
    fn verify_be_round_trip() {
        let data = [0x0Au8, 0x44, 0x93, 0x15, 0x68, 0x61, 0x05, 0x28];
        let crc = calculate_wmbus_crc(&data);
        // A fixed array rather than `to_vec()`: this crate has no allocator, so even
        // test code must stay heap-free or it cannot build for the bare-metal target.
        let mut framed = [0u8; 10];
        framed[..8].copy_from_slice(&data);
        framed[8] = (crc >> 8) as u8; // big-endian, as transmitted
        framed[9] = (crc & 0xFF) as u8;
        assert!(verify_crc_be(&framed));
        framed[2] ^= 0x01; // corrupt
        assert!(!verify_crc_be(&framed));
    }
}
