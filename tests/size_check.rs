//! Size guard for `MBusRecord`.
//!
//! Records are moved by value and accumulated in a `Vec`, so growth here is paid on every
//! reading. The fixed-capacity text fields that removed the heap allocations also cost
//! their capacity inline whether used or not — which is precisely why the capacities are
//! chosen per field rather than one global size, and why this test exists to make any
//! future change to them visible.

#[test]
fn mbus_record_stays_within_its_size_budget() {
    use mbus_rs::payload::record::MBusRecord;

    let size = std::mem::size_of::<MBusRecord>();

    // 488 bytes originally, with four `String`s and up to six heap allocations per
    // record; now larger by value and allocation-free, which is what makes the type usable
    // on a target with no allocator. Every inline buffer is paid in every record, so this
    // guard has already earned itself once: setting MAX_APPLIED_QUIRKS to 8 rather than 2
    // added 160 bytes and was caught here rather than in a profile.
    assert!(
        size <= 560,
        "MBusRecord grew to {size} bytes. It is moved by value on every reading, so \
         check what was added — a fixed-capacity field costs its full size in every \
         record, used or not."
    );
    assert!(
        size >= 400,
        "MBusRecord shrank to {size} bytes, which probably means `data: [u8; 252]` \
         changed. That would be good news, but update this guard deliberately."
    );
}

#[test]
fn text_capacities_are_sized_for_their_field_not_uniformly() {
    use mbus_core::payload::text::{QuantityText, UnitText};

    // A unit is `m^3` or `kWh`; a quantity may carry a vendor's ` [ERROR,ALARM]`.
    // Sizing both for the larger case would cost every record 32 bytes it never uses.
    assert!(
        std::mem::size_of::<UnitText>() < std::mem::size_of::<QuantityText>(),
        "units should not be paying for the quantity buffer"
    );
}
