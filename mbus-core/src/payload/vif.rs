use crate::error::ProtocolError;
use nom::{number::complete::be_u8, IResult};

/// A VIB chain: one VIF plus up to ten VIFEs.
///
/// Ten is the extension-bit limit the record parsers already enforce, and matches the
/// `[u8; 10]` VIFE array in `MBusRecord`'s header.
pub type Vib = heapless::Vec<VifInfo, 11>;

/// `10^exp` for the decimal exponents M-Bus VIFs use, without touching `f64::powi`.
///
/// `powi` lives in `std`; a core build has no floating-point maths library. These
/// exponents are a small fixed set — EN 13757-3 VIF codes span 10^-12 (current, nnnn=0)
/// to 10^7 — so a const table is both portable and exact, where repeated multiplication
/// would accumulate rounding error.
///
/// Out-of-range exponents return `1.0`: a VIF code cannot produce one, since the nibble
/// that drives it is four bits wide.
pub const fn pow10(exp: i32) -> f64 {
    const POW10: [f64; 20] = [
        1e-12, 1e-11, 1e-10, 1e-9, 1e-8, 1e-7, 1e-6, 1e-5, 1e-4, 1e-3, 1e-2, 1e-1, 1e0, 1e1, 1e2,
        1e3, 1e4, 1e5, 1e6, 1e7,
    ];
    let idx = exp + 12;
    if idx < 0 || idx >= POW10.len() as i32 {
        1.0
    } else {
        POW10[idx as usize]
    }
}

#[derive(Debug)]
pub struct VifInfo {
    pub vif: u16,
    pub unit: &'static str,
    pub exponent: f64,
    pub quantity: &'static str,
}

pub fn parse_vif(input: &[u8]) -> IResult<&[u8], VifInfo> {
    let (remaining, vif) = be_u8(input)?;

    let vif_info = crate::payload::vif_maps::lookup_primary_vif(vif).ok_or(nom::Err::Error(
        nom::error::Error::new(remaining, nom::error::ErrorKind::Tag),
    ))?;

    Ok((remaining, vif_info))
}

pub fn parse_vife(input: &[u8]) -> IResult<&[u8], VifInfo> {
    let (remaining, first) = be_u8(input)?;

    if first == 0xFD {
        let (remaining, code) = be_u8(remaining)?;
        let vif_info = crate::payload::vif_maps::lookup_vife_fd(code).ok_or(nom::Err::Error(
            nom::error::Error::new(remaining, nom::error::ErrorKind::Tag),
        ))?;
        Ok((remaining, vif_info))
    } else if first == 0xFB {
        let (remaining, code) = be_u8(remaining)?;
        let vif_info = crate::payload::vif_maps::lookup_vife_fb(code).ok_or(nom::Err::Error(
            nom::error::Error::new(remaining, nom::error::ErrorKind::Tag),
        ))?;
        Ok((remaining, vif_info))
    } else {
        let vif_info = crate::payload::vif_maps::lookup_vife_fd(first)
            .or_else(|| crate::payload::vif_maps::lookup_vife_fb(first))
            .ok_or(nom::Err::Error(nom::error::Error::new(
                remaining,
                nom::error::ErrorKind::Tag,
            )))?;
        Ok((remaining, vif_info))
    }
}

pub fn parse_vib(input: &[u8]) -> IResult<&[u8], Vib> {
    let (mut remaining, vif) = parse_vif(input)?;
    // The chain is bounded at 11 (1 VIF + 10 VIFEs); a longer run of extension bytes
    // stops the walk rather than growing without limit.
    let mut vifes = Vib::new();

    // Parse VIFEs if present (check for FD or FB extension codes)
    while !remaining.is_empty() {
        if remaining[0] == 0xFD || remaining[0] == 0xFB {
            match parse_vife(remaining) {
                Ok((new_remaining, vife)) => {
                    if vifes.push(vife).is_err() {
                        break; // chain longer than the standard permits
                    }
                    remaining = new_remaining;
                }
                Err(_) => break, // Stop on parse error
            }
        } else {
            break; // No more extensions
        }
    }

    // 1 VIF plus at most 10 VIFEs — the extension-bit chain the parsers already cap.
    let mut out = Vib::new();
    let _ = out.push(vif);
    for v in vifes {
        let _ = out.push(v);
    }
    Ok((remaining, out))
}

/// Collapse a VIB chain to its effective unit, scale and quantity.
///
/// Returns `&'static str`, not `String`. Every unit and quantity in the crate originates
/// in a `const` table and `VifInfo` already holds them as `&'static str` — this function
/// used to `.to_string()` them on the way out, which is where `MBusRecord`'s owned
/// `unit`/`quantity` strings came from and, with them, two heap allocations per record.
/// Callers that need owned strings convert at their own boundary.
pub fn normalize_vib(vib: &[VifInfo]) -> Result<(&'static str, f64, &'static str), ProtocolError> {
    let Some(last) = vib.last() else {
        return Err(ProtocolError::InvalidField("empty VIB"));
    };
    // The scale is the product of the whole chain; unit and quantity are the last entry's,
    // which is what the previous loop computed by overwriting on each iteration.
    let value = vib.iter().map(|i| i.exponent).product();
    Ok((last.unit, value, last.quantity))
}

#[cfg(test)]
mod tests {
    use super::{normalize_vib, parse_vib, parse_vif, parse_vife, pow10, Vib, VifInfo};
    use crate::payload::vif_maps::{
        lookup_primary_vif, lookup_vife_fb, lookup_vife_fd, VIFE_FD_CODES, VIF_CODES,
    };
    use proptest::prelude::*;
    use proptest::proptest;

    /// EN 13757-3 Table 10 defines most primary VIFs algorithmically: a base unit and
    /// a decimal exponent taken from the low bits of the code. Pinning the table to
    /// those formulas is what keeps a reading correct — a wrong exponent here silently
    /// scales every meter value by a power of ten, which no frame-level test catches.
    #[test]
    fn primary_vif_exponents_follow_en13757_3() {
        // (first code, last code, base unit, exponent offset, uses low two bits)
        const RANGES: &[(u8, u8, &str, i32, bool)] = &[
            (0x00, 0x07, "Wh", 3, false),
            (0x08, 0x0F, "J", 0, false),
            (0x10, 0x17, "m^3", 6, false),
            (0x18, 0x1F, "kg", 3, false),
            (0x28, 0x2F, "W", 3, false),
            (0x30, 0x37, "J/h", 0, false),
            (0x38, 0x3F, "m^3/h", 6, false),
            (0x40, 0x47, "m^3/min", 7, false),
            (0x48, 0x4F, "m^3/s", 9, false),
            (0x50, 0x57, "kg/h", 3, false),
            (0x58, 0x5B, "C", 3, true),
            (0x5C, 0x5F, "C", 3, true),
            (0x60, 0x63, "K", 3, true),
            (0x64, 0x67, "C", 3, true),
            (0x68, 0x6B, "bar", 3, true),
        ];
        for &(lo, hi, unit, off, low2) in RANGES {
            for code in lo..=hi {
                let info = lookup_primary_vif(code).expect("primary VIF present");
                let n = i32::from(if low2 { code - lo } else { code & 0x07 });
                let want = pow10(n - off);
                assert_eq!(info.unit, unit, "VIF {code:#04x} base unit");
                assert!(
                    (info.exponent - want).abs() <= want * 1e-9,
                    "VIF {code:#04x}: table {} but EN 13757-3 says 10^{} {unit}",
                    info.exponent,
                    n - off
                );
            }
        }
    }

    #[test]
    fn test_lookup_primary_vif_all_cases() {
        for (code, unit, exponent, quantity) in VIF_CODES.iter() {
            let info = lookup_primary_vif(*code).unwrap();
            assert_eq!(info.vif as u8, *code);
            assert_eq!(info.unit, *unit);
            assert_eq!(info.exponent, *exponent);
            assert_eq!(info.quantity, *quantity);
        }
        assert!(lookup_primary_vif(0xFF).is_some());
    }

    #[test]
    fn test_lookup_vife_fd_all_cases() {
        for (code, unit, exponent, quantity) in VIFE_FD_CODES.iter() {
            let info = lookup_vife_fd(*code).unwrap();
            assert_eq!(info.vif, 0x100 + *code as u16);
            assert_eq!(info.unit, *unit);
            assert_eq!(info.exponent, *exponent);
            assert_eq!(info.quantity, *quantity);
        }
        assert!(lookup_vife_fd(0xFF).is_none());
    }

    #[test]
    fn test_lookup_vife_fb_all_cases() {
        assert!(lookup_vife_fb(0x40).is_none());
        assert!(lookup_vife_fb(0xFF).is_none());
    }

    #[test]
    fn test_parse_vif_success() {
        let input = [0x00];
        let (_, info) = parse_vif(&input).unwrap();
        assert_eq!(info.vif, 0x00);
        assert_eq!(info.quantity, "Energy");
    }

    #[test]
    fn test_parse_vif_invalid() {
        let input = [0x90];
        let result = parse_vif(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_vife_success() {
        let input = [0x00];
        let (_, info) = parse_vife(&input).unwrap();
        assert_eq!(info.vif, 0x100);
        assert_eq!(info.quantity, "Credit");
    }

    #[test]
    fn test_parse_vife_invalid() {
        let input = [0xFF];
        let result = parse_vife(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_vib_single() {
        let input = [0x00];
        let (_, vib) = parse_vib(&input).unwrap();
        assert_eq!(vib.len(), 1);
        assert_eq!(vib[0].vif, 0x00);
    }

    #[test]
    fn test_parse_vib_with_extensions() {
        let input = [0x00, 0xFD, 0x00, 0xFD, 0x08];
        let (_, vib) = parse_vib(&input).unwrap();
        assert_eq!(vib.len(), 3);
    }

    #[test]
    fn test_normalize_vib_single() {
        let vib = vec![VifInfo {
            vif: 0x00,
            unit: "Wh",
            exponent: 1e-3,
            quantity: "Energy",
        }];
        let result = normalize_vib(&vib).unwrap();
        assert_eq!(result.0, "Wh");
        assert_eq!(result.1, 1e-3);
        assert_eq!(result.2, "Energy");
    }

    #[test]
    fn test_normalize_vib_multiple() {
        let vib = vec![
            VifInfo {
                vif: 0x00,
                unit: "Wh",
                exponent: 1e-3,
                quantity: "Energy",
            },
            VifInfo {
                vif: 0x10,
                unit: "m^3",
                exponent: 1e-6,
                quantity: "Volume",
            },
        ];
        let result = normalize_vib(&vib).unwrap();
        assert_eq!(result.0, "m^3");
        assert_eq!(result.1, 1e-9);
        assert_eq!(result.2, "Volume");
    }

    #[test]
    fn test_normalize_vib_empty() {
        let vib = Vib::new();
        let result = normalize_vib(&vib);
        assert!(result.is_err());
    }

    proptest! {
        #[test]
        fn prop_parse_vif_valid_codes(vif_code in 0u8..=0xFFu8) {
            let input = [vif_code];
            if let Ok((_, info)) = parse_vif(&input) {
                prop_assert!(info.vif as u8 == vif_code);
            }
        }
    }
}
