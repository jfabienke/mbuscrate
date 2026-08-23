//! Compact frame expansion (OMS / EN 13757-3 §5.3).
//!
//! A meter that transmits frequently sends most of its telegrams as *compact frames*
//! (transport CI `0x79`): the record headers are omitted and only the values are sent.
//! The receiver is expected to have seen a *full frame* (CI `0x78`) from the same meter
//! and to have remembered its record layout — the sequence of DIF/VIF headers — keyed
//! by a two-byte **format signature**.
//!
//! Without this, a Kamstrup-style meter yields a reading only when its periodic full
//! frame arrives (tens of minutes apart) while every compact frame in between decodes
//! to an opaque blob.
//!
//! ## Format signature
//!
//! The signature is CRC-16/EN-13757 (poly `0x3D65`, init `0x0000`, xorout `0xFFFF` —
//! the same CRC the link layer uses) computed over the concatenated DIF/VIF header
//! bytes of the full frame, data bytes excluded. This was confirmed against captured
//! traffic: a Kamstrup Multical 21 full frame with headers
//! `02 FF 20 | 04 13 | 44 13 | 61 5B | 61 67` produces `0xA8ED`, exactly the signature
//! its compact frames carry.
//!
//! ## Expansion
//!
//! Expansion re-interleaves the remembered headers with the compact frame's values,
//! producing a byte stream identical in shape to a full frame's record area, which the
//! ordinary record parser then decodes. The number of value bytes each record consumes
//! comes from its DIF, so the walk stays aligned.

use crate::payload::record::parse_variable_record_consumed;
use crate::wmbus::crc::calculate_wmbus_crc;
use std::collections::HashMap;
use thiserror::Error;

/// Failures specific to compact-frame handling.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum CompactError {
    #[error("compact frame truncated: need at least {needed} bytes, have {actual}")]
    Truncated { needed: usize, actual: usize },
    #[error("no cached record layout for format signature 0x{0:04X} — waiting for a full frame")]
    UnknownSignature(u16),
    #[error("cached layout needs {needed} value bytes but the frame supplies {actual}")]
    ValueLengthMismatch { needed: usize, actual: usize },
    #[error("could not parse the full frame's record layout: {0}")]
    LayoutParse(String),
}

/// One record's header (DIF, any DIFEs, VIF, any VIFEs) and the number of data bytes
/// that follow it.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutEntry {
    /// Header bytes exactly as they appeared in the full frame.
    pub header: Vec<u8>,
    /// Data bytes this record carries.
    pub data_len: usize,
}

/// The record layout of a full frame: what a compact frame's bare values mean.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecordLayout {
    pub entries: Vec<LayoutEntry>,
}

impl RecordLayout {
    /// Total value bytes a compact frame using this layout must supply.
    pub fn value_len(&self) -> usize {
        self.entries.iter().map(|e| e.data_len).sum()
    }

    /// The format signature identifying this layout.
    pub fn signature(&self) -> u16 {
        let headers: Vec<u8> = self.entries.iter().flat_map(|e| e.header.clone()).collect();
        calculate_wmbus_crc(&headers)
    }
}

/// Extract the record layout from a full frame's record area (the bytes after the
/// transport CI `0x78`).
pub fn extract_layout(records: &[u8]) -> Result<RecordLayout, CompactError> {
    let mut entries = Vec::new();
    let mut offset = 0usize;
    while offset < records.len() {
        if records[offset] == 0x2F {
            offset += 1; // idle filler
            continue;
        }
        let (rec, consumed) = parse_variable_record_consumed(&records[offset..])
            .map_err(|e| CompactError::LayoutParse(e.to_string()))?;
        if consumed == 0 {
            break;
        }
        // Defensive: `consumed` counts the header plus the data, so it should always be
        // at least `data_len`. This was a bare subtraction — a debug-mode panic and a
        // release-mode wraparound feeding the slice below — on a value that comes from a
        // parser reading untrusted bytes. Cheap to check, and the alternative failure is
        // a wildly out-of-range slice index.
        let Some(header_len) = consumed.checked_sub(rec.data.len()) else {
            break;
        };
        entries.push(LayoutEntry {
            header: records[offset..offset + header_len].to_vec(),
            data_len: rec.data.len(),
        });
        offset += consumed;
    }
    Ok(RecordLayout { entries })
}

/// Re-interleave a cached layout with a compact frame's values, yielding a record
/// stream shaped like a full frame's.
pub fn expand(layout: &RecordLayout, values: &[u8]) -> Result<Vec<u8>, CompactError> {
    let needed = layout.value_len();
    if values.len() < needed {
        return Err(CompactError::ValueLengthMismatch {
            needed,
            actual: values.len(),
        });
    }
    let mut out = Vec::with_capacity(needed + layout.entries.len() * 2);
    let mut at = 0usize;
    for entry in &layout.entries {
        out.extend_from_slice(&entry.header);
        out.extend_from_slice(&values[at..at + entry.data_len]);
        at += entry.data_len;
    }
    Ok(out)
}

/// The header a compact frame carries ahead of its values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactHeader {
    /// Identifies the record layout to apply.
    pub format_signature: u16,
    /// CRC of the full frame this compact frame stands in for. Retained for callers
    /// that want to detect a layout change; expansion does not depend on it.
    pub full_frame_crc: u16,
}

/// Split a compact frame's record area (the bytes after transport CI `0x79`) into its
/// header and its value bytes.
pub fn parse_compact(body: &[u8]) -> Result<(CompactHeader, &[u8]), CompactError> {
    if body.len() < 4 {
        return Err(CompactError::Truncated {
            needed: 4,
            actual: body.len(),
        });
    }
    Ok((
        CompactHeader {
            format_signature: u16::from_le_bytes([body[0], body[1]]),
            full_frame_crc: u16::from_le_bytes([body[2], body[3]]),
        },
        &body[4..],
    ))
}

/// Remembers record layouts learned from full frames, so compact frames from the same
/// meter can be expanded.
///
/// Keyed by **(meter address, format signature)**, not by signature alone. A signature
/// is only 16 bits, and a gateway hears many meters: keying globally lets one device's
/// layout be applied to another's values on a collision, which produces confidently
/// wrong readings rather than an error. Scoping to the meter makes a hit mean "this
/// meter's own full frame".
///
/// Callers must only [`learn`](Self::learn) from frames whose CRC validated — a layout
/// taken from a corrupted frame poisons every compact frame that matches it.
#[derive(Debug, Default)]
pub struct CompactLayoutCache {
    layouts: HashMap<(u32, u16), RecordLayout>,
}

impl CompactLayoutCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Learn a layout from a CRC-valid full frame's record area. Returns its signature.
    pub fn learn(&mut self, meter: u32, full_frame_records: &[u8]) -> Result<u16, CompactError> {
        let layout = extract_layout(full_frame_records)?;
        let sig = layout.signature();
        self.layouts.insert((meter, sig), layout);
        Ok(sig)
    }

    /// Expand a compact frame's record area into a full-frame-shaped record stream,
    /// using a layout previously learned from the *same* meter.
    pub fn expand_compact(&self, meter: u32, body: &[u8]) -> Result<Vec<u8>, CompactError> {
        let (header, values) = parse_compact(body)?;
        let layout = self
            .layouts
            .get(&(meter, header.format_signature))
            .ok_or(CompactError::UnknownSignature(header.format_signature))?;
        expand(layout, values)
    }

    pub fn len(&self) -> usize {
        self.layouts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layouts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Record layout of a Kamstrup Multical 21 full frame: manufacturer-specific info
    /// codes, current volume, due-date volume, minimum flow and ambient temperature.
    /// The headers are a property of the meter model; the values here are synthetic.
    const FULL_RECORDS: &[u8] = &[
        0x02, 0xFF, 0x20, 0x00, 0x00, // info codes
        0x04, 0x13, 0xD3, 0x63, 0x00, 0x00, // volume, 25555 l
        0x44, 0x13, 0x1F, 0x63, 0x00, 0x00, // due-date volume, 25375 l
        0x61, 0x5B, 0x12, // flow temperature, 18 C
        0x61, 0x67, 0x18, // ambient temperature, 24 C
    ];

    #[test]
    fn extracts_layout_and_signature_from_a_full_frame() {
        let layout = extract_layout(FULL_RECORDS).unwrap();
        assert_eq!(layout.entries.len(), 5);
        assert_eq!(layout.entries[0].header, vec![0x02, 0xFF, 0x20]);
        assert_eq!(layout.entries[0].data_len, 2);
        assert_eq!(layout.entries[1].header, vec![0x04, 0x13]);
        assert_eq!(layout.entries[1].data_len, 4);
        assert_eq!(layout.entries[3].header, vec![0x61, 0x5B]);
        assert_eq!(layout.entries[3].data_len, 1);
        assert_eq!(layout.value_len(), 2 + 4 + 4 + 1 + 1);
    }

    /// The signature a real Multical 21 puts in its compact frames, recomputed from the
    /// layout of its full frame. This pins the algorithm (CRC-16/EN-13757 over the
    /// header bytes) to observed traffic rather than to an assumption.
    #[test]
    fn signature_matches_observed_kamstrup_value() {
        assert_eq!(extract_layout(FULL_RECORDS).unwrap().signature(), 0xA8ED);
    }

    #[test]
    fn expands_a_compact_frame_back_to_full_frame_shape() {
        let mut cache = CompactLayoutCache::new();
        let sig = cache.learn(74644444, FULL_RECORDS).unwrap();
        assert_eq!(sig, 0xA8ED);

        // Compact body: signature (LE), full-frame CRC, then bare values.
        let mut body = vec![0xED, 0xA8, 0x00, 0xC3];
        body.extend_from_slice(&[0x08, 0x00]); // info codes
        body.extend_from_slice(&[0xC3, 0x63, 0x00, 0x00]); // volume, 25539 l
        body.extend_from_slice(&[0x1F, 0x63, 0x00, 0x00]); // due-date volume
        body.push(0x12); // flow temp
        body.push(0x18); // ambient temp

        let expanded = cache.expand_compact(74644444, &body).unwrap();
        // Same shape as a full frame, carrying the compact frame's values.
        assert_eq!(&expanded[..3], &[0x02, 0xFF, 0x20]);
        assert_eq!(&expanded[5..11], &[0x04, 0x13, 0xC3, 0x63, 0x00, 0x00]);

        // And it parses as ordinary records, with the volume correctly scaled.
        let (rec, _) = parse_variable_record_consumed(&expanded[5..]).unwrap();
        // Integer coding -> exact Scaled; as_f64 gives the scaled reading.
        assert!(
            (rec.value.as_f64() - 25.539).abs() < 1e-9,
            "got {:?}",
            rec.value
        );
    }

    /// A layout learned from one meter must never be applied to another's values: a
    /// 16-bit signature collides readily across a neighbourhood of meters, and a wrong
    /// layout yields confident nonsense (a water meter reporting watts).
    #[test]
    fn layouts_do_not_leak_between_meters() {
        let mut cache = CompactLayoutCache::new();
        cache.learn(74644444, FULL_RECORDS).unwrap();
        let mut body = vec![0xED, 0xA8, 0x00, 0xC3];
        body.extend_from_slice(&[0u8; 12]);
        assert!(cache.expand_compact(74644444, &body).is_ok());
        assert_eq!(
            cache.expand_compact(63398862, &body),
            Err(CompactError::UnknownSignature(0xA8ED)),
            "another meter's layout must not be reused"
        );
    }

    #[test]
    fn compact_frame_without_a_learned_layout_is_reported_not_guessed() {
        let cache = CompactLayoutCache::new();
        let body = [0xED, 0xA8, 0x00, 0xC3, 0x01, 0x02];
        assert_eq!(
            cache.expand_compact(74644444, &body),
            Err(CompactError::UnknownSignature(0xA8ED))
        );
    }

    #[test]
    fn truncated_values_are_rejected_rather_than_padded() {
        let mut cache = CompactLayoutCache::new();
        cache.learn(74644444, FULL_RECORDS).unwrap();
        let body = [0xED, 0xA8, 0x00, 0xC3, 0x08, 0x00]; // only 2 of 12 value bytes
        assert_eq!(
            cache.expand_compact(74644444, &body),
            Err(CompactError::ValueLengthMismatch {
                needed: 12,
                actual: 2
            })
        );
    }
}
