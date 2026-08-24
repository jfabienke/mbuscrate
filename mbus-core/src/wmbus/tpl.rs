//! Transport-layer (TPL) header location.
//!
//! `mbus-core` owns the crypto ([`oms::decrypt_mode5_cbc`](crate::wmbus::oms)) but until
//! now not the *location* of the ciphertext or the cleartext records: the
//! CI → header-length → payload-offset walk lived scattered in consumers, each
//! re-deriving it. That is a silent-failure seam — a consumer that starts the record
//! parser at "everything after the CI byte" is correct for a headerless CI `0x78` but
//! four bytes early for a short header `0x7A`, and a record walk from four bytes early
//! decodes into *plausible* records rather than an error. (That exact bug fabricated
//! Energy/Power readings in a downstream reader before it was found.)
//!
//! [`parse_tpl_header`] returns the header layout once, with the records offset and the
//! ciphertext offset as distinct outputs, and models `0x78` as its own no-header case
//! rather than a zero-length header — there is no access/status/config to read there at
//! all, so the distinction is load-bearing, not pedantic.
//!
//! This module is pure framing (no AES), so it needs no `crypto` feature.

/// Where the payload after a TPL header begins, and what kind of payload it is.
///
/// The input to [`parse_tpl_header`] is the frame payload beginning at the CI byte
/// (the same convention as [`parse_ell`](crate::wmbus::ell::parse_ell)); every offset
/// here is relative to that slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TplHeader {
    /// The CI byte that introduced this header.
    pub ci: u8,
    /// Access number from the header, or `None` for a headerless CI (`0x78`).
    pub access_no: Option<u8>,
    /// Status byte, or `None` for a headerless CI.
    pub status: Option<u8>,
    /// Raw 2-byte Configuration Field (host order; the wire is little-endian), or `None`
    /// for a headerless CI. Kept raw so a caller that reads its bits differently to
    /// [`security_mode`](Self::security_mode) is not stuck with our interpretation.
    pub config_field: Option<u16>,
    /// OMS security mode from the Configuration Field: `0` = cleartext, `5` = AES-128-CBC
    /// (Profile A), `7`, `13`, … A headerless CI is always cleartext, so `0`.
    pub security_mode: u8,
    /// Number of 16-byte encrypted blocks the Configuration Field declares (`0` when
    /// cleartext). The ciphertext is `encrypted_blocks * 16` bytes; any bytes after it
    /// are trailing cleartext records (partial encryption). Callers should clamp this
    /// against the bytes actually available.
    pub encrypted_blocks: u8,
    /// Bytes consumed by this header, i.e. the offset — from the CI byte — at which the
    /// post-header payload begins. `1` for `0x78` (just past the CI), `5` for a short
    /// header, `13` for a long header.
    pub header_len: usize,
}

impl TplHeader {
    /// Whether the post-header payload is ciphertext.
    pub fn is_encrypted(&self) -> bool {
        self.security_mode != 0
    }

    /// Offset at which cleartext **data records** begin, or `None` when the frame is
    /// encrypted (the records then live in the decrypted plaintext, not in the frame).
    /// This is the offset consumers most often get wrong; return it explicitly.
    pub fn records_offset(&self) -> Option<usize> {
        (!self.is_encrypted()).then_some(self.header_len)
    }

    /// Offset at which **ciphertext** begins, or `None` when the frame is cleartext.
    /// The ciphertext runs for [`encrypted_blocks`](Self::encrypted_blocks) × 16 bytes.
    pub fn ciphertext_offset(&self) -> Option<usize> {
        self.is_encrypted().then_some(self.header_len)
    }
}

/// Why a TPL header could not be located.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TplError {
    /// The payload was empty — there is not even a CI byte.
    Empty,
    /// The CI byte does not introduce a TPL header this module recognises.
    NotTpl(u8),
    /// The payload is shorter than the header the CI declares.
    Truncated { ci: u8, need: usize, have: usize },
}

impl core::fmt::Display for TplError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty payload: no CI byte"),
            Self::NotTpl(ci) => write!(f, "CI 0x{ci:02X} does not introduce a TPL header"),
            Self::Truncated { ci, need, have } => write!(
                f,
                "TPL header for CI 0x{ci:02X} truncated: need {need} bytes, have {have}"
            ),
        }
    }
}

impl core::error::Error for TplError {}

/// Extract the OMS security mode (bits 8..12) and encrypted-block count (bits 4..7) from
/// a Configuration Field. These bit positions are the ones a live decoder validated
/// against real mode-5 traffic; the raw field is kept on [`TplHeader`] for a caller who
/// needs to read it differently.
fn decode_config_field(cf: u16) -> (u8, u8) {
    let mode = ((cf >> 8) & 0x0F) as u8;
    let blocks = ((cf >> 4) & 0x0F) as u8;
    (mode, blocks)
}

/// Parse the TPL header from a frame payload that begins at the CI byte.
///
/// Recognised CIs: `0x78` (no TPL header — records follow the CI directly), `0x7A`/`0x7B`
/// (short header: ACC, STATUS, CF), `0x72`/`0x73` (long header: 8-byte TPL address, then
/// ACC, STATUS, CF). Compact-frame (`0x79`/`0x69`) and other CIs return
/// [`TplError::NotTpl`] — they are a different shape, not a TPL header.
pub fn parse_tpl_header(payload: &[u8]) -> Result<TplHeader, TplError> {
    let &ci = payload.first().ok_or(TplError::Empty)?;

    // `addr_prefix` = TPL-address bytes between the CI and the ACC (8 for a long header,
    // 0 for a short one). A headerless CI has no fields at all.
    let addr_prefix = match ci {
        0x78 => {
            return Ok(TplHeader {
                ci,
                access_no: None,
                status: None,
                config_field: None,
                security_mode: 0,
                encrypted_blocks: 0,
                header_len: 1, // just the CI byte; records begin immediately after
            });
        }
        0x7A | 0x7B => 0,
        0x72 | 0x73 => 8,
        other => return Err(TplError::NotTpl(other)),
    };

    // Layout after the CI: [addr_prefix][ACC(1)][STATUS(1)][CF(2)]. Read via `get` so the
    // extraction is provably panic-free (this crate is under the panic ratchet), which
    // also handles truncation without a separate length check.
    let header_len = 1 + addr_prefix + 4;
    let base = 1 + addr_prefix;
    let (Some(&acc), Some(&status), Some(&cf0), Some(&cf1)) = (
        payload.get(base),
        payload.get(base + 1),
        payload.get(base + 2),
        payload.get(base + 3),
    ) else {
        return Err(TplError::Truncated {
            ci,
            need: header_len,
            have: payload.len(),
        });
    };
    let cf = u16::from_le_bytes([cf0, cf1]);
    let (security_mode, encrypted_blocks) = decode_config_field(cf);

    Ok(TplHeader {
        ci,
        access_no: Some(acc),
        status: Some(status),
        config_field: Some(cf),
        security_mode,
        encrypted_blocks,
        header_len,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_header_ci_0x78_records_follow_directly() {
        // Records begin at offset 1 (just past the CI) — NOT a zero-length header with
        // acc/status/config to skip. This distinction is the whole point.
        let h = parse_tpl_header(&[0x78, 0x0C, 0x13, 0x11, 0x22, 0x33, 0x44]).unwrap();
        assert_eq!(h.header_len, 1);
        assert_eq!(h.records_offset(), Some(1));
        assert_eq!(h.ciphertext_offset(), None);
        assert!(!h.is_encrypted());
        assert_eq!(h.access_no, None);
    }

    #[test]
    fn short_header_0x7a_skips_four_bytes() {
        // CI, ACC=0x2A, STATUS=0x00, CF=0x0000 (cleartext) → records at offset 5.
        // A consumer starting at offset 1 would read ACC/STATUS/CF as a record.
        let h = parse_tpl_header(&[0x7A, 0x2A, 0x00, 0x00, 0x00, 0x0C, 0x13]).unwrap();
        assert_eq!(h.header_len, 5);
        assert_eq!(h.access_no, Some(0x2A));
        assert_eq!(h.status, Some(0x00));
        assert_eq!(h.security_mode, 0);
        assert_eq!(h.records_offset(), Some(5));
        assert_eq!(h.ciphertext_offset(), None);
    }

    #[test]
    fn short_header_0x7a_mode5_five_blocks() {
        // CF low byte 0x50 = blocks 5 (bits 4..7); high byte 0x05 = mode 5 (bits 8..12).
        // Wire CF is little-endian: bytes 0x50, 0x05 → u16 0x0550.
        let h = parse_tpl_header(&[0x7A, 0x2A, 0x00, 0x50, 0x05, 0xAA, 0xBB]).unwrap();
        assert_eq!(h.config_field, Some(0x0550));
        assert_eq!(h.security_mode, 5);
        assert_eq!(h.encrypted_blocks, 5);
        assert!(h.is_encrypted());
        assert_eq!(h.ciphertext_offset(), Some(5));
        assert_eq!(h.records_offset(), None); // records live in the plaintext
    }

    #[test]
    fn long_header_0x72_skips_twelve_bytes() {
        // CI + 8-byte TPL address + ACC + STATUS + CF(2) = 13.
        let mut frame = vec![0x72];
        frame.extend_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x2D, 0x2C, 0x18, 0x37]); // TPL addr
        frame.extend_from_slice(&[0x2A, 0x00, 0x00, 0x00]); // ACC STATUS CF(cleartext)
        frame.extend_from_slice(&[0x0C, 0x13]); // first record
        let h = parse_tpl_header(&frame).unwrap();
        assert_eq!(h.header_len, 13);
        assert_eq!(h.access_no, Some(0x2A));
        assert_eq!(h.records_offset(), Some(13));
    }

    #[test]
    fn truncated_and_unknown_cis_are_errors() {
        assert_eq!(parse_tpl_header(&[]), Err(TplError::Empty));
        assert_eq!(parse_tpl_header(&[0x79]), Err(TplError::NotTpl(0x79))); // compact, not TPL
        assert_eq!(
            parse_tpl_header(&[0x7A, 0x2A, 0x00]), // short header needs 5 bytes
            Err(TplError::Truncated {
                ci: 0x7A,
                need: 5,
                have: 3
            })
        );
    }
}
