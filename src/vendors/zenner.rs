//! Zenner (ZRI) error-flag interpretation.
//!
//! The device error field travels as a standard `VIF 0xFD 0x17` ("error flags")
//! data record — the crate already decodes its raw 16-bit value. The *bit meanings*
//! are Zenner-specific **and device-class-specific**: the water (EDCL) and heat-cost-
//! allocator (M8) enums assign the same bit positions entirely different meanings,
//! so a decoder keyed only on manufacturer would read a water meter's flags off the
//! HCA map and be confidently wrong (bit 4 "HCA" vs undefined; bit 10 "BLOCK" vs
//! unnamed). Selection is therefore by class, and an unclassified device gets no
//! interpretation rather than a guessed one.
//!
//! There is a **second fork within water/EDCL**, on the firmware `type_code`
//! (project_type): the generic path (`EDCL_AllMeters`, our "Map A") and a
//! LoRa-only `ShowDeviceStatus` path ("Map B", type_code 25/29) disagree
//! irreconcilably on the *lower* 8 bits — e.g. bit 0 is REMOVAL under A but
//! WARNING_APP_BUSY under B, and bit 4 is undefined under A but UNKNOWN_ERROR under
//! B. The *upper* bits (8..15) agree between A and B. This matters less than it
//! sounds for us, because Map B is proven LoRa-only: `ShowDeviceStatus` rejects our
//! type 44 outright, and a LoRa-firmware device never emits these OMS records over
//! wM-Bus — so **every frame this decode path sees is Map A**. The `type_code`
//! guard below refuses rather than mis-decodes if that invariant is ever violated.
//!
//! Evidence: the maps are transcribed from Zenner's own EDCL/M8 enums
//! (`Evidence::Documented`). BLOCK (bit 10) is stronger than a name lookup: Zenner's
//! six detection features (OverrideID 280..285) map 1:1 onto bits 13/11/15/10/9/8,
//! so BLOCK = standstill = no flow — which is exactly right for the live unit at
//! 0.000 m³, an independent semantic cross-check, and it sits in the upper bits
//! where A and B agree. Individual lower bits reach `Evidence::Captured` only when
//! observed set on a real device; and the promotion test should use a bit both maps
//! agree on (8..11, 13, 15), because a lower-bit fault could not distinguish "our
//! map is wrong" from "the wrong map applies". The cleanest is watching BLOCK clear
//! on flow.

/// Zenner device class, as far as it bears on error-flag interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZennerClass {
    Water,
    HeatCostAllocator,
}

/// Classify a device from its wM-Bus device-type byte, or `None` when we cannot say.
///
/// 0x37 is the type our live water unit (55298170) actually sends; 0x07 is the
/// standard water type. HCA classification is deliberately absent until a real
/// Zenner HCA is observed — asserting its device-type byte without one would be a
/// guess, and mis-classifying is exactly the failure this module exists to prevent.
pub fn classify(device_type: u8) -> Option<ZennerClass> {
    match device_type {
        0x07 | 0x37 => Some(ZennerClass::Water),
        _ => None,
    }
}

/// EDCL (water) error-flag bit map. Bit 4, 12, 14 are absent from Zenner's own enum
/// — genuinely undefined at the source, not merely unknown to us.
const EDCL_WATER: &[(u16, &str)] = &[
    (0x0001, "REMOVAL"),
    (0x0002, "BATTERY"),
    (0x0004, "BATTERY_END_LIFE"),
    (0x0008, "HARDWARE"),
    (0x0020, "TAMPER"),
    (0x0040, "RADIO_ERROR"),
    (0x0080, "TRANSCEIVER"),
    (0x0100, "OVERSIZE"),
    (0x0200, "UNDERSIZE"),
    (0x0400, "BLOCK"),
    (0x0800, "BACKFLOW"),
    (0x2000, "LEAK"),
    (0x8000, "BURST"),
];

/// M8 (heat cost allocator) error-flag bit map. Kept for when a Zenner HCA is seen;
/// not yet wired, because we cannot confirm the device-type that selects it.
#[allow(dead_code)]
const M8_HCA: &[(u16, &str)] = &[
    (0x0001, "TAMPER"),
    (0x0002, "BATTERY"),
    (0x0004, "BATTERY_END_LIFE"),
    (0x0008, "HARDWARE"),
    (0x0010, "HCA"),
    (0x0020, "VERSION_LUT"),
];

/// Named flags set in `raw`, plus any set bits the class map does not define.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorFlags {
    pub flags: Vec<&'static str>,
    /// Bit positions (0..15) that are set but have no definition in the vendor enum.
    pub undefined_bits: Vec<u8>,
}

impl ErrorFlags {
    pub fn is_clear(&self) -> bool {
        self.flags.is_empty() && self.undefined_bits.is_empty()
    }
}

/// Firmware type codes whose water error-flags follow the LoRa `ShowDeviceStatus`
/// map (Map B), not the generic EDCL map (Map A). We implement Map A only, so a
/// device of one of these types must not be decoded here — it would read the lower
/// 8 bits off the wrong map.
const LORA_SHOW_STATUS_TYPES: &[u8] = &[25, 29];

/// Decode a Zenner error-flags value for a device class and firmware type.
///
/// `type_code` is the firmware project_type when known (from the device profile),
/// or `None` — which is the case over wM-Bus, where it is unavailable from the frame
/// but also unnecessary, since Map B is LoRa-only. If a `type_code` is supplied that
/// selects Map B, this returns `None` rather than decode a water record off Map A.
pub fn decode_error_flags(
    class: ZennerClass,
    type_code: Option<u8>,
    raw: u16,
) -> Option<ErrorFlags> {
    if class == ZennerClass::Water && type_code.is_some_and(|t| LORA_SHOW_STATUS_TYPES.contains(&t))
    {
        return None; // Map B territory; we implement Map A only.
    }
    let map = match class {
        ZennerClass::Water => EDCL_WATER,
        ZennerClass::HeatCostAllocator => M8_HCA,
    };
    let mut flags = Vec::new();
    let mut covered = 0u16;
    for (mask, name) in map {
        covered |= *mask;
        if raw & *mask != 0 {
            flags.push(*name);
        }
    }
    let undefined_bits = (0..16u8)
        .filter(|b| raw & (1 << b) != 0 && covered & (1 << b) == 0)
        .collect();
    Some(ErrorFlags {
        flags,
        undefined_bits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_water_unit_0x0410_is_block_plus_the_undefined_bit() {
        // 0x0410 = BLOCK (0x0400) + bit 4 (0x0010), which the EDCL enum leaves
        // undefined. This is exactly what 55298170 sends.
        let f = decode_error_flags(ZennerClass::Water, None, 0x0410).unwrap();
        assert_eq!(f.flags, vec!["BLOCK"]);
        assert_eq!(f.undefined_bits, vec![4]);
    }

    #[test]
    fn water_map_names_the_operational_flags() {
        assert_eq!(
            decode_error_flags(ZennerClass::Water, None, 0x0002)
                .unwrap()
                .flags,
            vec!["BATTERY"]
        );
        assert_eq!(
            decode_error_flags(ZennerClass::Water, None, 0x2000)
                .unwrap()
                .flags,
            vec!["LEAK"]
        );
        assert_eq!(
            decode_error_flags(ZennerClass::Water, None, 0x8000)
                .unwrap()
                .flags,
            vec!["BURST"]
        );
    }

    #[test]
    fn the_same_bits_differ_by_class() {
        // Bit 0: REMOVAL on water, TAMPER on an HCA. Bit 4: undefined on water,
        // HCA on an HCA. The whole reason class selection is mandatory.
        assert_eq!(
            decode_error_flags(ZennerClass::Water, None, 0x0001)
                .unwrap()
                .flags,
            vec!["REMOVAL"]
        );
        assert_eq!(
            decode_error_flags(ZennerClass::HeatCostAllocator, None, 0x0001)
                .unwrap()
                .flags,
            vec!["TAMPER"]
        );
        let water_b4 = decode_error_flags(ZennerClass::Water, None, 0x0010).unwrap();
        assert_eq!(water_b4.undefined_bits, vec![4]);
        assert_eq!(
            decode_error_flags(ZennerClass::HeatCostAllocator, None, 0x0010)
                .unwrap()
                .flags,
            vec!["HCA"]
        );
    }

    #[test]
    fn classify_only_commits_to_what_we_have_evidence_for() {
        assert_eq!(classify(0x37), Some(ZennerClass::Water)); // the live unit
        assert_eq!(classify(0x07), Some(ZennerClass::Water)); // standard water
        assert_eq!(classify(0x08), None); // standard HCA — unconfirmed for Zenner
        assert_eq!(classify(0x00), None);
    }

    #[test]
    fn refuses_map_a_for_a_lora_type_code() {
        // A LoRa firmware type (25/29) uses Map B, which we do not implement — so
        // decoding must refuse, never silently apply Map A's lower-bit meanings.
        assert!(decode_error_flags(ZennerClass::Water, Some(29), 0x0001).is_none());
        // Our own type (44) and the unknown case both use Map A.
        assert!(decode_error_flags(ZennerClass::Water, Some(44), 0x0001).is_some());
        assert!(decode_error_flags(ZennerClass::Water, None, 0x0001).is_some());
    }

    #[test]
    fn a_clear_field_decodes_to_nothing() {
        assert!(decode_error_flags(ZennerClass::Water, None, 0x0000)
            .unwrap()
            .is_clear());
    }
}
