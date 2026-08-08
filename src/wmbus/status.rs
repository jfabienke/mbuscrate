//! Transport-layer status byte (the TPL "STS" field), decoded per EN 13757-3 §7.5.
//!
//! This byte travels in cleartext even in encrypted frames, so tamper, low-battery
//! and error signals are readable without the AES key — which makes decoding it
//! both operationally useful and worth doing precisely.
//!
//! Bits split into two halves with very different epistemic status:
//!
//! * bits 4:0 are **standard** — the meanings below are the specification's, the
//!   same for every manufacturer, and safe to name.
//! * bits 7:5 are **manufacturer-specific** — their meaning is defined per vendor,
//!   so this module reports them as raw bits and leaves naming to a
//!   [`VendorExtension`](crate::vendors::VendorExtension), rather than guessing.

/// A decoded TPL status byte: the raw value, the standard flags that are set, and
/// the manufacturer-specific bits kept as a raw 3-bit value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusByte {
    pub raw: u8,
    /// Standard flags set in bits 4:0, in spec order.
    pub flags: Vec<&'static str>,
    /// Bits 7:5, unshifted meaning removed — value 0..=7. Vendor-defined.
    pub manufacturer_bits: u8,
}

impl StatusByte {
    /// True when no standard error or busy flag is set (manufacturer bits ignored).
    pub fn is_nominal(&self) -> bool {
        self.flags.is_empty()
    }
}

/// Decode the standard portion of a TPL status byte.
///
/// Bits 1:0 encode a single application state (they are a 2-bit field, not two
/// independent flags); bits 2, 3, 4 are independent error flags.
pub fn decode_status_byte(sts: u8) -> StatusByte {
    let mut flags = Vec::new();

    // bits 1:0 — application state (mutually exclusive values, not OR-able flags).
    match sts & 0x03 {
        0b00 => {}
        0b01 => flags.push("application busy"),
        0b10 => flags.push("application error"),
        0b11 => flags.push("abnormal condition / alarm"),
        _ => unreachable!(),
    }
    if sts & 0x04 != 0 {
        flags.push("power low"); // low battery — the field crews care about most
    }
    if sts & 0x08 != 0 {
        flags.push("permanent error");
    }
    if sts & 0x10 != 0 {
        flags.push("temporary error");
    }

    StatusByte {
        raw: sts,
        flags,
        manufacturer_bits: (sts >> 5) & 0x07,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nominal_byte_has_no_flags() {
        let s = decode_status_byte(0x00);
        assert!(s.is_nominal());
        assert_eq!(s.manufacturer_bits, 0);
    }

    #[test]
    fn zenner_bench_unit_reports_temporary_error() {
        // 0x10 is exactly what 55298170 sends — bit 4, a standard temporary error,
        // and no manufacturer bits. Decoding it wrongly as a manufacturer flag would
        // have been the easy mistake.
        let s = decode_status_byte(0x10);
        assert_eq!(s.flags, vec!["temporary error"]);
        assert_eq!(s.manufacturer_bits, 0);
    }

    #[test]
    fn power_low_is_readable_without_the_key() {
        let s = decode_status_byte(0x04);
        assert_eq!(s.flags, vec!["power low"]);
    }

    #[test]
    fn application_state_is_one_value_not_two_flags() {
        // 0b11 is a single "abnormal condition", never "busy" + "error".
        let s = decode_status_byte(0x03);
        assert_eq!(s.flags, vec!["abnormal condition / alarm"]);
    }

    #[test]
    fn standard_and_manufacturer_bits_separate_cleanly() {
        // 0xB4 = 1011_0100: mfr bits 101, temporary error (bit4), power low (bit2).
        let s = decode_status_byte(0xB4);
        assert_eq!(s.manufacturer_bits, 0b101);
        assert!(s.flags.contains(&"power low"));
        assert!(s.flags.contains(&"temporary error"));
        assert!(!s.flags.contains(&"permanent error"));
    }
}
