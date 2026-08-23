//! Audit trail for vendor quirks that changed a decode.
//!
//! The type lives here, not with the vendor registry, because it is a *field of a record*
//! — a record must be able to say how it was decoded even on a target that has no vendor
//! registry linked at all. The quirks themselves, and the logic that decides when one
//! fires, stay in `mbus-rs`.

/// Record of a quirk having changed a decode outcome.
///
/// A consumer must be able to tell a standard decode from an overridden one; two gateways
/// that disagree silently cannot be audited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct QuirkApplied {
    /// The manifest id of the quirk that fired.
    pub quirk_id: &'static str,
    /// Whether the quirk's evidence was merely provisional at the time it fired.
    pub provisional: bool,
}

/// How many quirk applications one record can record.
///
/// Two, not eight. `QuirkApplied` is 24 bytes, so the vector costs `24 * N + 8` inline in
/// *every* record whether a quirk fires or not — measured, raising this from 2 to 8 grew
/// `MBusRecord` by 160 bytes, which the size guard caught. Exactly one `VendorQuirks`
/// implementation exists today and a record realistically attracts one.
///
/// Raising it is a one-line change, and the size guard makes the cost visible when you do.
/// A full buffer drops audit information — the one thing this type exists to guarantee —
/// so the caller logs rather than ignoring the failed push.
pub const MAX_APPLIED_QUIRKS: usize = 2;

/// The quirks that changed one record.
pub type AppliedQuirks = heapless::Vec<QuirkApplied, MAX_APPLIED_QUIRKS>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quirk_record_is_copy_and_allocation_free() {
        let q = QuirkApplied {
            quirk_id: "qundis.date.g-vs-f",
            provisional: true,
        };
        let copied = q; // Copy, not a move — so recording one costs nothing.
        assert_eq!(q, copied);
    }

    #[test]
    fn the_audit_vector_holds_more_than_any_record_needs() {
        let mut v = AppliedQuirks::new();
        for i in 0..MAX_APPLIED_QUIRKS {
            assert!(
                v.push(QuirkApplied {
                    quirk_id: "x",
                    provisional: i % 2 == 0
                })
                .is_ok(),
                "capacity {MAX_APPLIED_QUIRKS} should hold {i}"
            );
        }
        // Beyond capacity the push fails rather than silently dropping — the caller can
        // see that the audit trail is incomplete.
        assert!(v
            .push(QuirkApplied {
                quirk_id: "overflow",
                provisional: false
            })
            .is_err());
    }
}
