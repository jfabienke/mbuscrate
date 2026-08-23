//! M-Bus data-record parsing — the pure half re-exported from `mbus-core`, plus the
//! vendor layer that stays here.
//!
//! The parser moved to `mbus_core::payload::record`: it is EN 13757-3's application
//! layer, shared byte-for-byte by wired M-Bus and wM-Bus, and after this session's
//! preparation it carried no clock, no allocations and no vendor coupling. The vendor
//! functions below run as a second pass over the finished record — the same shape as
//! `verify_blocks_with_vendor` — and stay where the registry lives.
//!
//! The wrappers preserve this module's `MBusError` signatures so no caller changes:
//! the core reports `ProtocolError`, and `From<ProtocolError> for MBusError` maps
//! variant-for-variant (`UnknownDif(b)` stays `UnknownDif(b)`, and so on).

use crate::error::MBusError;
use crate::vendors;
use mbus_core::payload::text::{QuantityText, UnitText};

pub use mbus_core::payload::record::mbus_data_record_append;
pub use mbus_core::payload::record::{
    parse_fixed_record as core_parse_fixed_record,
    parse_variable_record as core_parse_variable_record,
    parse_variable_record_consumed as core_parse_variable_record_consumed, CustomVif,
    MBusDataInformationBlock, MBusDataRecordHeader, MBusRecord, MBusRecordValue,
    MBusValueInformationBlock, RECORD_TEXT_CAPACITY,
};
pub use mbus_core::payload::record_value::dif_datalength_lookup as core_dif_datalength_lookup;

/// Data-field length implied by a DIF's low nibble.
pub fn mbus_dif_datalength_lookup(dif: u8) -> usize {
    core_dif_datalength_lookup(dif)
}

/// Parse a fixed-format record. See `mbus_core::payload::record::parse_fixed_record`.
pub fn parse_fixed_record(input: &[u8]) -> Result<MBusRecord, MBusError> {
    core_parse_fixed_record(input).map_err(Into::into)
}

/// Parse one variable-data record.
pub fn parse_variable_record(input: &[u8]) -> Result<MBusRecord, MBusError> {
    core_parse_variable_record(input).map_err(Into::into)
}

/// Parse one variable-data record, reporting the exact bytes consumed.
pub fn parse_variable_record_consumed(input: &[u8]) -> Result<(MBusRecord, usize), MBusError> {
    core_parse_variable_record_consumed(input).map_err(Into::into)
}

pub fn parse_variable_record_in_context(
    input: &[u8],
    ctx: &vendors::DecodeContext,
) -> Result<(MBusRecord, usize), MBusError> {
    let (mut record, consumed) = parse_variable_record_consumed(input)?;
    apply_vendor_hooks(&mut record, ctx)?;
    Ok((record, consumed))
}

/// Write a vendor-produced (unit, exponent, quantity, value) into a record.
fn apply_vendor_value(
    record: &mut MBusRecord,
    unit: String,
    exp: i8,
    qty: String,
    var: vendors::VendorVariable,
) {
    // Vendor-supplied text is dynamic, so it takes the owned variant.
    record.unit = UnitText::from_str_truncating(&unit);
    record.quantity = QuantityText::from_str_truncating(&qty);
    record.value = match var {
        vendors::VendorVariable::Numeric(n) => {
            MBusRecordValue::Numeric(n * 10_f64.powi(exp as i32))
        }
        vendors::VendorVariable::String(s) => MBusRecordValue::text(&s),
        _ => MBusRecordValue::text("Vendor specific"),
    };
}

/// Offer a parsed record to the context's vendor binding: first the extension points
/// the standard reserves for manufacturers (DIF 0x0F/0x1F, VIF 0x7F/0xFF, status
/// bits — Layer 1, additive), then the scope-matched quirks (Layer 2, overriding),
/// each of which records its application on the record (P5). No manufacturer name is
/// ever compared here: extensions are keyed by the context's binding and quirks by
/// their own manifests, so generic code stays vendor-blind (P1/P7).
fn apply_vendor_hooks(
    record: &mut MBusRecord,
    ctx: &vendors::DecodeContext,
) -> Result<(), MBusError> {
    let Some(ext) = ctx.extension() else {
        return Ok(());
    };
    let mfr_id = ctx.manufacturer();

    // DIF 0x0F/0x1F: manufacturer data block.
    if record.drh.dib.dif == 0x0F || record.drh.dib.dif == 0x1F {
        if let Some(vendor_records) =
            ext.handle_dif_manufacturer_block(mfr_id, record.drh.dib.dif, &record.data)?
        {
            if let Some(first) = vendor_records.into_iter().next() {
                record.unit = UnitText::from_str_truncating(&first.unit);
                record.quantity = QuantityText::from_str_truncating(&first.quantity);
                record.value = match first.value {
                    vendors::VendorVariable::Numeric(n) => MBusRecordValue::Numeric(n),
                    vendors::VendorVariable::String(s) => MBusRecordValue::text(&s),
                    _ => MBusRecordValue::text("Vendor specific"),
                };
            }
        }
    }

    // VIF 0x7F/0xFF: manufacturer-specific value information.
    if record.drh.vib.vif == 0x7F || record.drh.vib.vif == 0xFF {
        if let Some((unit, exp, qty, var)) =
            ext.parse_vif_manufacturer_specific(mfr_id, record.drh.vib.vif, &record.data)?
        {
            apply_vendor_value(record, unit, exp, qty, var);
        }
    }

    // Vendor-defined status bits [7:5] in the trailing data byte.
    if let Some(&status_byte) = record.data.last() {
        if (status_byte & 0xE0) != 0 {
            if let Some(status_vars) = ext.decode_status_bits(mfr_id, status_byte)? {
                let status_str = status_vars
                    .iter()
                    .filter_map(|v| match v {
                        vendors::VendorVariable::Boolean(true) => Some("ALARM"),
                        vendors::VendorVariable::ErrorFlags { flags } if *flags != 0 => {
                            Some("ERROR")
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                if !status_str.is_empty() {
                    // Vendor annotation: the only place a quantity is built rather than named.
                    record.quantity = QuantityText::from_str_truncating(&format!(
                        "{} [{}]",
                        record.quantity, status_str
                    ));
                }
            }
        }
    }

    // Layer 2: quirks override the standard reading where the device deviates from
    // the specification. Every application is recorded on the record (P5).
    for quirk in ctx.quirks() {
        if let Some(applied) = quirk.reinterpret_record(record) {
            // Full means the audit trail would be incomplete, which is the one thing
            // this vector exists to guarantee — so it is worth a log, not a silent drop.
            if record.applied_quirks.push(applied).is_err() {
                log::warn!(
                    "more than {} quirks fired on one record; audit trail truncated",
                    mbus_core::payload::quirk::MAX_APPLIED_QUIRKS
                );
            }
        }
    }

    Ok(())
}

/// Parse variable record with vendor extension support.
#[deprecated(
    note = "fork of the decode path (vendor-layers P7); use parse_variable_record_in_context             with a DecodeContext — retired in migration step 8"
)]
pub fn parse_variable_record_with_vendor(
    input: &[u8],
    manufacturer_id: Option<&str>,
    registry: Option<&vendors::VendorRegistry>,
) -> Result<MBusRecord, MBusError> {
    // Legacy semantics: no integrity tracking existed, so a valid frame is assumed.
    let ctx = match manufacturer_id {
        Some(mfr) => vendors::DecodeContext::assume_valid(mfr, registry),
        None => vendors::DecodeContext::empty(),
    };
    parse_variable_record_in_context(input, &ctx).map(|(record, _)| record)
}
