//! Vendor conformance harness (`docs/design/vendor-layers.md` §4.6, migration step 7).
//!
//! Machine-checks the three properties the vendor layering rests on, over the
//! registry itself — so registering a new extension or quirk is automatically
//! covered, and a vendor entry that violates its layer's contract fails CI without
//! anyone having to remember to write the test.
//!
//! 1. **Extensions are additive (P2).** For standard records, decoding with a vendor
//!    bound must equal decoding with none — any difference must be attributed to a
//!    named quirk. Silent divergence is exactly the failure the layers exist to
//!    prevent.
//! 2. **Quirks are evidenced and visible (P4, P5).** Every registered quirk carries a
//!    coherent manifest and a demonstration record; applying it must change the
//!    outcome *and* record a `QuirkApplied`.
//! 3. **Identity gating holds (P6).** With a corrupt identity header, decode output
//!    is byte-identical to a vendor-free decode: no hook ran, nothing leaked.

use mbus_rs::payload::record::{parse_variable_record_in_context, MBusRecordValue};
use mbus_rs::vendors::{
    DecodeContext, DeviceIdentity, Evidence, Integrity, QuirkStatus, VendorRegistry,
};
use std::collections::HashSet;

/// A spread of standard records: integer, BCD, temperature, date, LVAR text,
/// DIFE storage chain, and the manufacturer-specific VIF slot.
const CORPUS: &[&[u8]] = &[
    &[0x04, 0x13, 0xD3, 0x63, 0x00, 0x00], // 32-bit volume
    &[0x0C, 0x13, 0x78, 0x56, 0x34, 0x12], // 8-digit BCD volume
    &[0x01, 0x5B, 0x12],                   // 8-bit flow temperature
    &[0x02, 0x6C, 0x1F, 0x2C],             // date type G
    &[0x02, 0x04, 0xF8, 0x10],             // VIF 0x04 — energy per the standard, QDS date by quirk
    &[0x44, 0x13, 0x1F, 0x63, 0x00, 0x00], // storage-1 volume
    &[0x02, 0xFF, 0x20, 0x00, 0x00],       // manufacturer-specific VIF (0xFF 0x20)
];

fn fields(bytes: &[u8], ctx: &DecodeContext) -> (String, String, String, Vec<&'static str>) {
    let (rec, _) = parse_variable_record_in_context(bytes, ctx).expect("corpus record parses");
    let value = match &rec.value {
        MBusRecordValue::Numeric(n) => format!("{n}"),
        MBusRecordValue::String(s) => s.clone(),
    };
    let quirks = rec.applied_quirks.iter().map(|q| q.quirk_id).collect();
    (
        value,
        rec.unit.to_string(),
        rec.quantity.to_string(),
        quirks,
    )
}

fn ctx_for(registry: &VendorRegistry, manufacturer: &str, integrity: Integrity) -> DecodeContext {
    DecodeContext::resolve(
        Some(registry),
        DeviceIdentity {
            manufacturer: manufacturer.to_string(),
            ..DeviceIdentity::default()
        },
        integrity,
    )
}

/// P2: for every registered manufacturer, a bound decode of a standard record either
/// equals the vendor-free decode, or the difference is attributed to a named quirk
/// (or the record sits in a manufacturer-defined slot, which is the extension's to
/// fill). Nothing may diverge silently.
#[test]
fn extensions_are_additive_and_deviations_are_attributed() {
    let registry = VendorRegistry::with_defaults().unwrap();
    let empty = DecodeContext::empty();

    for manufacturer in registry.registered_manufacturers() {
        let bound = ctx_for(&registry, &manufacturer, Integrity::valid());
        for bytes in CORPUS {
            let baseline = fields(bytes, &empty);
            let vendored = fields(bytes, &bound);

            let dif = bytes[0];
            let vif = bytes[1];
            let manufacturer_slot = dif == 0x0F || dif == 0x1F || vif == 0x7F || vif == 0xFF;
            let attributed = !vendored.3.is_empty();

            if (baseline.0, &baseline.1, &baseline.2)
                != (vendored.0.clone(), &vendored.1, &vendored.2)
            {
                assert!(
                    attributed || manufacturer_slot,
                    "{manufacturer}: record {bytes:02X?} diverged from the standard decode \
                     with no quirk attribution and outside any manufacturer slot"
                );
            }
        }
    }
}

/// P4: every registered quirk's manifest is coherent — a non-empty stable id, unique
/// across the registry, a stated deviation, and an evidence/status pairing that does
/// not overclaim (an inference can never be Verified).
#[test]
fn quirk_manifests_are_coherent_and_do_not_overclaim() {
    let registry = VendorRegistry::with_defaults().unwrap();
    let mut ids = HashSet::new();

    for quirk in registry.registered_quirks() {
        let m = quirk.manifest();
        assert!(!m.id.is_empty(), "quirk id must be a stable identifier");
        assert!(
            ids.insert(m.id),
            "duplicate quirk id {:?} — ids appear in decoded output and must be unique",
            m.id
        );
        assert_eq!(
            m.scope.manufacturer.len(),
            3,
            "{}: scope manufacturer must be a 3-letter code",
            m.id
        );
        assert!(
            !m.deviation.trim().is_empty(),
            "{}: the deviation (standard vs. device) must be stated",
            m.id
        );
        match (&m.evidence, m.status) {
            (Evidence::Inferred { .. }, QuirkStatus::Verified) => {
                panic!("{}: an inference cannot be Verified (P4)", m.id)
            }
            (Evidence::Captured { capture, note }, _) => {
                assert!(
                    !capture.trim().is_empty() && !note.trim().is_empty(),
                    "{}: captured evidence must name its capture",
                    m.id
                );
            }
            (Evidence::Documented { source }, _) => {
                assert!(
                    !source.trim().is_empty(),
                    "{}: documented evidence must cite a source",
                    m.id
                );
            }
            (Evidence::Inferred { rationale }, QuirkStatus::Provisional) => {
                assert!(
                    !rationale.trim().is_empty(),
                    "{}: inferred evidence must state its rationale",
                    m.id
                );
            }
        }
    }
}

/// P4 + P5: every registered quirk demonstrates its deviation. Its evidence record,
/// decoded with the quirk in scope, must differ from the standard decode *and* carry
/// the quirk's own id in `applied_quirks`. A quirk that cannot show its deviation
/// cannot justify overriding the standard.
#[test]
fn every_quirk_demonstrates_its_deviation_and_is_visible() {
    let registry = VendorRegistry::with_defaults().unwrap();
    let empty = DecodeContext::empty();

    for quirk in registry.registered_quirks() {
        let m = quirk.manifest();
        let bytes = quirk.evidence_record();
        let bound = ctx_for(&registry, m.scope.manufacturer, Integrity::valid());

        let baseline = fields(bytes, &empty);
        let vendored = fields(bytes, &bound);

        assert_ne!(
            (&baseline.0, &baseline.1, &baseline.2),
            (&vendored.0, &vendored.1, &vendored.2),
            "{}: evidence record {bytes:02X?} decodes identically with and without \
             the quirk — it demonstrates no deviation",
            m.id
        );
        assert!(
            vendored.3.contains(&m.id),
            "{}: the quirk fired but did not attribute itself (P5); applied = {:?}",
            m.id,
            vendored.3
        );
    }
}

/// P6: with a corrupt identity header, decoding is byte-identical to a vendor-free
/// decode for every registered manufacturer and every corpus record — no extension
/// hook, no quirk, no leak.
#[test]
fn corrupt_identity_yields_a_vendor_free_decode() {
    let registry = VendorRegistry::with_defaults().unwrap();
    let empty = DecodeContext::empty();
    let corrupt = Integrity {
        header_valid: false,
        payload_valid: false,
    };

    for manufacturer in registry.registered_manufacturers() {
        let ctx = ctx_for(&registry, &manufacturer, corrupt);
        for bytes in CORPUS {
            assert_eq!(
                fields(bytes, &empty),
                fields(bytes, &ctx),
                "{manufacturer}: corrupt-header decode of {bytes:02X?} was influenced \
                 by vendor code (P6)"
            );
        }
    }
}
