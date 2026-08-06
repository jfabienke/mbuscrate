//! Layer 2 — device quirks (`docs/design/vendor-layers.md` §4.2).
//!
//! A quirk is where a device *deviates* from EN 13757/OMS: generic code would be
//! correct per specification but yields the wrong answer for this device. Unlike a
//! [`VendorExtension`](crate::vendors::VendorExtension) — which only adds meaning to
//! bytes the standard leaves undefined and fails safe — a quirk **overrides** what
//! the crate would otherwise conclude, and therefore fails dangerous. That asymmetry
//! is why quirks carry a mandatory evidence manifest (P4), report every application
//! (P5), and are scoped as narrowly as the evidence allows.

use crate::payload::record::MBusRecord;
use crate::vendors::context::DeviceIdentity;
use std::ops::RangeInclusive;

/// Where the knowledge behind a vendor entry comes from (design §4.4). Provenance is
/// part of the entry so a reader can tell a behaviour verified against a capture
/// from one inherited from a datasheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    /// Decoded from a frame this project captured. The gold standard.
    Captured {
        capture: &'static str,
        note: &'static str,
    },
    /// Taken from vendor or standards documentation, not yet seen in our traffic.
    Documented { source: &'static str },
    /// Inferred from behaviour; no authority. Requires review before use.
    Inferred { rationale: &'static str },
}

/// Trust level of an entry. `Verified` quirks apply automatically within their scope;
/// `Provisional` ones require the caller to opt in (P4), and their output is marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuirkStatus {
    Verified,
    Provisional,
}

/// The devices a quirk applies to. Deliberately narrower than "a manufacturer":
/// quirks are usually per model or firmware generation, and a 16-bit-flavoured code
/// like the manufacturer id is shared by wildly different products.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorScope {
    /// Three-letter manufacturer code, e.g. `"QDS"`.
    pub manufacturer: &'static str,
    /// Device versions covered; `None` = any.
    pub versions: Option<RangeInclusive<u8>>,
    /// Device types (media) covered; `None` = any.
    pub device_types: Option<&'static [u8]>,
}

impl VendorScope {
    /// Whether this scope covers the given device.
    pub fn matches(&self, device: &DeviceIdentity) -> bool {
        if !device.manufacturer.eq_ignore_ascii_case(self.manufacturer) {
            return false;
        }
        if let Some(versions) = &self.versions {
            if !versions.contains(&device.version) {
                return false;
            }
        }
        if let Some(types) = self.device_types {
            if !types.contains(&device.device_type) {
                return false;
            }
        }
        true
    }
}

/// The mandatory declaration every quirk ships with (P4): what the standard says,
/// what the device does instead, on whose evidence, and how far that evidence
/// reaches.
#[derive(Debug, Clone, PartialEq)]
pub struct QuirkManifest {
    /// Stable identifier, e.g. `"qds-vif04-date"`. Appears verbatim in decoded
    /// output when the quirk fires.
    pub id: &'static str,
    pub scope: VendorScope,
    /// What the standard says versus what the device does.
    pub deviation: &'static str,
    pub evidence: Evidence,
    pub status: QuirkStatus,
}

/// Record of a quirk having changed a decode outcome (P5). A consumer must be able
/// to tell a standard decode from an overridden one; two gateways that disagree
/// silently cannot be audited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuirkApplied {
    /// The manifest id of the quirk that fired.
    pub quirk_id: &'static str,
    /// Whether the quirk's evidence was merely provisional at the time it fired.
    pub provisional: bool,
}

/// Layer 2 hook set. Every method returns `Some(QuirkApplied)` exactly when it
/// changed the outcome, so application is always observable.
pub trait VendorQuirks: Send + Sync {
    /// The quirk's declaration. Scope-matching and status gating happen against
    /// this, outside the implementation, so a quirk cannot widen its own reach.
    fn manifest(&self) -> &QuirkManifest;

    /// Reinterpret a fully parsed record whose standard reading is wrong for this
    /// device (e.g. a repurposed standard VIF).
    fn reinterpret_record(&self, _record: &mut MBusRecord) -> Option<QuirkApplied> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(mfr: &str, version: u8, device_type: u8) -> DeviceIdentity {
        DeviceIdentity {
            manufacturer: mfr.into(),
            version,
            device_type,
            ..DeviceIdentity::default()
        }
    }

    #[test]
    fn scope_matches_narrowly() {
        let scope = VendorScope {
            manufacturer: "QDS",
            versions: Some(1..=5),
            device_types: Some(&[0x08]),
        };
        assert!(scope.matches(&identity("QDS", 3, 0x08)));
        assert!(
            !scope.matches(&identity("QDS", 6, 0x08)),
            "version out of range"
        );
        assert!(!scope.matches(&identity("QDS", 3, 0x16)), "wrong medium");
        assert!(
            !scope.matches(&identity("KAM", 3, 0x08)),
            "wrong manufacturer"
        );
    }

    #[test]
    fn manufacturer_wide_scope_needs_explicit_nones() {
        let scope = VendorScope {
            manufacturer: "QDS",
            versions: None,
            device_types: None,
        };
        assert!(
            scope.matches(&identity("qds", 200, 0x00)),
            "case-insensitive, any model"
        );
    }
}
