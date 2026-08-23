//! M-Bus data-record parsing (EN 13757-3 application layer).
//!
//! Shared by wired M-Bus and wM-Bus: both transports carry the same DIF/VIF record
//! structures, which is why this lives under `payload` and not under either link layer —
//! and why porting it serves a wired optical-head reader and a wireless receiver alike.
//!
//! Pure parsing only. The vendor layer — quirks, CI hooks, per-manufacturer overrides —
//! stays in `mbus-rs` and runs as a separate pass over the finished record
//! (`parse_variable_record_in_context` there), exactly as block-CRC vendor tolerance
//! does. The record itself carries no clock, no heap and no vendor coupling.

use crate::constants::*;
use crate::error::ProtocolError;
use crate::payload::quirk::AppliedQuirks;
use crate::payload::record_value::{bcd_le, dif_datalength_lookup, int_le};
use crate::payload::text::{QuantityText, UnitText};
use nom::{bytes::complete::take, number::complete::be_u8, IResult};

/// `"{a}, {b}"` into a fixed buffer, clipping at capacity — without `core::fmt`, whose
/// machinery carries panic paths the linker cannot eliminate.
fn compose<const N: usize>(a: &str, b: &str) -> heapless::String<N> {
    let mut out = heapless::String::new();
    for part in [a, ", ", b] {
        for c in part.chars() {
            if out.push(c).is_err() {
                return out;
            }
        }
    }
    out
}

/// Data length described by an LVAR byte, reporting the offending byte on failure.
fn parse_variable_data_length(input: u8) -> Result<usize, ProtocolError> {
    crate::payload::record_value::variable_data_length(input)
        .map_err(|_| ProtocolError::UnknownDif(input))
}

/// Plain-text VIF storage, bounded by `MBUS_VALUE_INFO_BLOCK_CUSTOM_VIF_SIZE`.
pub type CustomVif = heapless::String<16>;

/// Represents an M-Bus data record.
///
/// Carries no timestamp. It used to hold a `SystemTime` stamped by the parser with
/// `SystemTime::now()` — which is the wrong place: a decoder is a pure function of bytes,
/// and the instant that matters is when the frame was *received*, not when it happened to
/// be parsed. The receiving layer knows that instant; the parser does not, and on a
/// microcontroller there may be no wall clock at all. This is the rule `mbus-core`'s own
/// documentation states: if a function needs to know the time, it takes it as an argument.
#[derive(Debug)]
pub struct MBusRecord {
    pub storage_number: u32,
    pub tariff: i32,
    pub device: i32,
    pub is_numeric: bool,
    pub value: MBusRecordValue,
    /// Unit of the reading, e.g. `m^3`. Usually a pointer into the const VIF tables.
    pub unit: UnitText,
    /// What is being measured, e.g. `Volume`. Always static — the medium and function
    /// names come from fixed tables — so this needs no storage at all.
    pub function_medium: &'static str,
    /// Quantity name, e.g. `Volume`, possibly annotated by a vendor extension.
    pub quantity: QuantityText,
    pub drh: MBusDataRecordHeader,
    pub data_len: usize,
    pub data: [u8; 256],
    /// Set when the meter signalled that another record follows (DIF 0x1F). Consumers
    /// keep parsing while it is true.
    pub more_records_follow: bool,
    /// Quirks that changed this record's interpretation (vendor-layers P5). Empty for
    /// a purely standard decode; a consumer can always tell an overridden reading.
    pub applied_quirks: AppliedQuirks,
}

/// Represents the M-Bus data record header.
#[derive(Debug, Default)]
pub struct MBusDataRecordHeader {
    pub dib: MBusDataInformationBlock,
    pub vib: MBusValueInformationBlock,
}

/// Represents the M-Bus data information block.
#[derive(Debug, Default)]
pub struct MBusDataInformationBlock {
    pub dif: u8,
    pub ndife: usize,
    pub dife: [u8; 10],
}

/// Represents the M-Bus value information block.
#[derive(Debug, Default)]
pub struct MBusValueInformationBlock {
    pub vif: u8,
    pub nvife: usize,
    pub vife: [u8; 10],
    /// Plain-text VIF (`0x7C`) content. Capped at
    /// `MBUS_VALUE_INFO_BLOCK_CUSTOM_VIF_SIZE` (16) by the standard and by the parser,
    /// so it needs no allocator.
    pub custom_vif: CustomVif,
}

/// Text capacity for a record's decoded string value.
///
/// LVAR text can in principle reach 1130 bytes, but a record's raw bytes are already kept
/// in [`MBusRecord::data`] — this is a *decoded* convenience copy, reversed into reading
/// order. So a clipped value costs nothing recoverable: the caller reads `data` for the
/// full content. 32 covers the device-identification strings that occur in practice
/// without charging every record for the pathological case.
pub const RECORD_TEXT_CAPACITY: usize = 32;

/// A record's decoded value.
#[derive(Debug, Clone, PartialEq)]
pub enum MBusRecordValue {
    Numeric(f64),
    String(heapless::String<RECORD_TEXT_CAPACITY>),
}

impl MBusRecordValue {
    /// Build a string value, clipping at [`RECORD_TEXT_CAPACITY`].
    pub fn text(s: &str) -> Self {
        let mut out = heapless::String::new();
        for c in s.chars() {
            if out.push(c).is_err() {
                break;
            }
        }
        MBusRecordValue::String(out)
    }
}

// Constants for fixed-length medium units (based on M-Bus spec)
#[allow(dead_code)]
const FIXED_MEDIUM_UNITS: &[(u8, &str, f64, &str)] = &[
    (0x00, "Wh", 1e-3, "Energy"),
    (0x01, "10^-1 Wh", 1e-4, "Energy"),
    (0x02, "10^-2 Wh", 1e-5, "Energy"),
    (0x03, "10^-3 Wh", 1e-6, "Energy"),
    (0x04, "10^-4 Wh", 1e-7, "Energy"),
    (0x05, "10^-5 Wh", 1e-8, "Energy"),
    (0x06, "10^-6 Wh", 1e-9, "Energy"),
    (0x07, "10^-7 Wh", 1e-10, "Energy"),
    (0x08, "J", 1e0, "Energy"),
    (0x09, "10^-1 J", 1e-1, "Energy"),
    (0x0A, "10^-2 J", 1e-2, "Energy"),
    (0x0B, "10^-3 J", 1e-3, "Energy"),
    (0x0C, "10^-4 J", 1e-4, "Energy"),
    (0x0D, "10^-5 J", 1e-5, "Energy"),
    (0x0E, "10^-6 J", 1e-6, "Energy"),
    (0x0F, "10^-7 J", 1e-7, "Energy"),
    (0x10, "m^3", 1e-6, "Volume"),
    (0x11, "10^-1 m^3", 1e-7, "Volume"),
    (0x12, "10^-2 m^3", 1e-8, "Volume"),
    (0x13, "10^-3 m^3", 1e-9, "Volume"),
    (0x14, "10^-4 m^3", 1e-10, "Volume"),
    (0x15, "10^-5 m^3", 1e-11, "Volume"),
    (0x16, "10^-6 m^3", 1e-12, "Volume"),
    (0x17, "10^-7 m^3", 1e-13, "Volume"),
    (0x18, "kg", 1e-3, "Mass"),
    (0x19, "10^-1 kg", 1e-4, "Mass"),
    (0x1A, "10^-2 kg", 1e-5, "Mass"),
    (0x1B, "10^-3 kg", 1e-6, "Mass"),
    (0x1C, "10^-4 kg", 1e-7, "Mass"),
    (0x1D, "10^-5 kg", 1e-8, "Mass"),
    (0x1E, "10^-6 kg", 1e-9, "Mass"),
    (0x1F, "10^-7 kg", 1e-10, "Mass"),
    (0x20, "s", 1.0, "On time"),
    (0x21, "10^-1 s", 1e-1, "On time"),
    (0x22, "10^-2 s", 1e-2, "On time"),
    (0x23, "10^-3 s", 1e-3, "On time"),
    (0x24, "s", 1.0, "Operating time"),
    (0x25, "10^-1 s", 1e-1, "Operating time"),
    (0x26, "10^-2 s", 1e-2, "Operating time"),
    (0x27, "10^-3 s", 1e-3, "Operating time"),
    (0x28, "W", 1e-3, "Power"),
    (0x29, "10^-1 W", 1e-4, "Power"),
    (0x2A, "10^-2 W", 1e-5, "Power"),
    (0x2B, "10^-3 W", 1e-6, "Power"),
    (0x2C, "10^-4 W", 1e-7, "Power"),
    (0x2D, "10^-5 W", 1e-8, "Power"),
    (0x2E, "10^-6 W", 1e-9, "Power"),
    (0x2F, "10^-7 W", 1e-10, "Power"),
    (0x30, "J/h", 1e0, "Power"),
    (0x31, "10^-1 J/h", 1e-1, "Power"),
    (0x32, "10^-2 J/h", 1e-2, "Power"),
    (0x33, "10^-3 J/h", 1e-3, "Power"),
    (0x34, "10^-4 J/h", 1e-4, "Power"),
    (0x35, "10^-5 J/h", 1e-5, "Power"),
    (0x36, "10^-6 J/h", 1e-6, "Power"),
    (0x37, "10^-7 J/h", 1e-7, "Power"),
    (0x38, "m^3/h", 1e-6, "Volume flow"),
    (0x39, "10^-1 m^3/h", 1e-7, "Volume flow"),
    (0x3A, "10^-2 m^3/h", 1e-8, "Volume flow"),
    (0x3B, "10^-3 m^3/h", 1e-9, "Volume flow"),
    (0x3C, "10^-4 m^3/h", 1e-10, "Volume flow"),
    (0x3D, "10^-5 m^3/h", 1e-11, "Volume flow"),
    (0x3E, "10^-6 m^3/h", 1e-12, "Volume flow"),
    (0x3F, "10^-7 m^3/h", 1e-13, "Volume flow"),
    (0x40, "m^3/min", 1e-7, "Volume flow"),
    (0x41, "10^-1 m^3/min", 1e-8, "Volume flow"),
    (0x42, "10^-2 m^3/min", 1e-9, "Volume flow"),
    (0x43, "10^-3 m^3/min", 1e-10, "Volume flow"),
    (0x44, "10^-4 m^3/min", 1e-11, "Volume flow"),
    (0x45, "10^-5 m^3/min", 1e-12, "Volume flow"),
    (0x46, "10^-6 m^3/min", 1e-13, "Volume flow"),
    (0x47, "10^-7 m^3/min", 1e-14, "Volume flow"),
    (0x48, "m^3/s", 1e-9, "Volume flow"),
    (0x49, "10^-1 m^3/s", 1e-10, "Volume flow"),
    (0x4A, "10^-2 m^3/s", 1e-11, "Volume flow"),
    (0x4B, "10^-3 m^3/s", 1e-12, "Volume flow"),
    (0x4C, "10^-4 m^3/s", 1e-13, "Volume flow"),
    (0x4D, "10^-5 m^3/s", 1e-14, "Volume flow"),
    (0x4E, "10^-6 m^3/s", 1e-15, "Volume flow"),
    (0x4F, "10^-7 m^3/s", 1e-16, "Volume flow"),
    (0x50, "kg/h", 1e-3, "Mass flow"),
    (0x51, "10^-1 kg/h", 1e-4, "Mass flow"),
    (0x52, "10^-2 kg/h", 1e-5, "Mass flow"),
    (0x53, "10^-3 kg/h", 1e-6, "Mass flow"),
    (0x54, "10^-4 kg/h", 1e-7, "Mass flow"),
    (0x55, "10^-5 kg/h", 1e-8, "Mass flow"),
    (0x56, "10^-6 kg/h", 1e-9, "Mass flow"),
    (0x57, "10^-7 kg/h", 1e-10, "Mass flow"),
    (0x58, "°C", 1e-3, "Flow temperature"),
    (0x59, "10^-1 °C", 1e-4, "Flow temperature"),
    (0x5A, "10^-2 °C", 1e-5, "Flow temperature"),
    (0x5B, "10^-3 °C", 1e-6, "Flow temperature"),
    (0x5C, "°C", 1e-3, "Return temperature"),
    (0x5D, "10^-1 °C", 1e-4, "Return temperature"),
    (0x5E, "10^-2 °C", 1e-5, "Return temperature"),
    (0x5F, "10^-3 °C", 1e-6, "Return temperature"),
    (0x60, "K", 1e-3, "Temperature difference"),
    (0x61, "10^-1 K", 1e-4, "Temperature difference"),
    (0x62, "10^-2 K", 1e-5, "Temperature difference"),
    (0x63, "10^-3 K", 1e-6, "Temperature difference"),
    (0x64, "°C", 1e-3, "External temperature"),
    (0x65, "10^-1 °C", 1e-4, "External temperature"),
    (0x66, "10^-2 °C", 1e-5, "External temperature"),
    (0x67, "10^-3 °C", 1e-6, "External temperature"),
    (0x68, "bar", 1e-3, "Pressure"),
    (0x69, "10^-1 bar", 1e-4, "Pressure"),
    (0x6A, "10^-2 bar", 1e-5, "Pressure"),
    (0x6B, "10^-3 bar", 1e-6, "Pressure"),
    (0x6C, "-", 1.0, "Time point (date)"),
    (0x6D, "-", 1.0, "Time point (date & time)"),
    (0x6E, "Units for H.C.A.", 1.0, "H.C.A."),
    (0x6F, "Reserved", 0.0, "Reserved"),
    (0x70, "s", 1.0, "Averaging Duration"),
    (0x71, "10^-1 s", 1e-1, "Averaging Duration"),
    (0x72, "10^-2 s", 1e-2, "Averaging Duration"),
    (0x73, "10^-3 s", 1e-3, "Averaging Duration"),
    (0x74, "s", 1.0, "Actuality Duration"),
    (0x75, "10^-1 s", 1e-1, "Actuality Duration"),
    (0x76, "10^-2 s", 1e-2, "Actuality Duration"),
    (0x77, "10^-3 s", 1e-3, "Actuality Duration"),
    (0x78, "", 1.0, "Fabrication No"),
    (0x79, "", 1.0, "(Enhanced) Identification"),
    (0x7A, "", 1.0, "Bus Address"),
    (0x7B, "", 1.0, "Any VIF"),
    (0x7C, "", 1.0, "Any VIF"),
    (0x7D, "", 1.0, "Any VIF"),
    (0x7E, "", 1.0, "Any VIF"),
    (0x7F, "", 1.0, "Manufacturer specific"),
];

/// Parses a fixed-length M-Bus data record.
pub fn parse_fixed_record(input: &[u8]) -> Result<MBusRecord, ProtocolError> {
    if input.len() < crate::constants::MBUS_DATA_FIXED_LENGTH {
        return Err(ProtocolError::InvalidField("Fixed data too short"));
    }

    let device_id_bcd = match crate::payload::data_encoding::decode_bcd(&input[0..4]) {
        Ok((_, val)) => val,
        Err(_) => return Err(ProtocolError::InvalidField("Invalid BCD device ID")),
    };
    let manufacturer_val = u16::from_be_bytes([input[4], input[5]]);
    if !(0x0421..=0x6B5A).contains(&manufacturer_val) {
        return Err(ProtocolError::InvalidField("Invalid manufacturer"));
    }
    let _manufacturer = manufacturer_val as i32;
    let _version = input[6];
    let medium = input[7];
    let _access_number = input[8];
    let status = input[9];
    let _signature = match crate::payload::data_encoding::decode_int(&input[10..12], 2) {
        Ok((_, val)) => val,
        Err(_) => return Err(ProtocolError::InvalidField("Invalid signature")),
    };
    let counter1 = if (status & crate::constants::MBUS_DATA_FIXED_STATUS_FORMAT_MASK)
        == crate::constants::MBUS_DATA_FIXED_STATUS_FORMAT_BCD
    {
        match crate::payload::data_encoding::decode_bcd(&input[12..16]) {
            Ok((_, val)) => val as i32,
            Err(_) => return Err(ProtocolError::InvalidField("Invalid BCD counter")),
        }
    } else {
        match crate::payload::data_encoding::decode_int(&input[12..16], 4) {
            Ok((_, val)) => val,
            Err(_) => return Err(ProtocolError::InvalidField("Invalid int counter")),
        }
    };
    let counter2 = 0; // Assuming no second counter for simplicity

    let (unit1, value1, quantity1) = normalize_fixed_unit(medium, counter1 as f64)?;
    let (unit2, value2, quantity2) = normalize_fixed_unit(medium, counter2 as f64)?;

    let record = MBusRecord {
        storage_number: device_id_bcd,
        tariff: -1,
        device: -1,
        is_numeric: true,
        value: MBusRecordValue::Numeric(value1 + value2),
        // Composed by pushing chars, clipping at capacity exactly as the old
        // `format!` composition did. Not `write!`: core::fmt's machinery carries panic
        // paths the linker cannot eliminate, and this module is under the panic ratchet.
        unit: UnitText::Owned(compose(unit1, unit2)),
        function_medium: "Fixed",
        quantity: QuantityText::Owned(compose(quantity1, quantity2)),
        drh: MBusDataRecordHeader {
            dib: MBusDataInformationBlock {
                dif: 0,
                ndife: 0,
                dife: [0; 10],
            },
            vib: MBusValueInformationBlock {
                vif: medium,
                nvife: 0,
                vife: [0; 10],
                custom_vif: CustomVif::new(),
            },
        },
        data_len: input.len(),
        data: {
            let mut data = [0; 256];
            data[..input.len()].copy_from_slice(input);
            data
        },
        more_records_follow: false,
        applied_quirks: AppliedQuirks::new(),
    };

    Ok(record)
}

/// Parses a variable-length M-Bus data record.
/// Parse one variable-data record AND report the exact number of input bytes it consumed
/// (DRH incl. any DIFE/VIFE chain, the optional variable-length byte, and the data). Use
/// this — not an estimate — to walk a multi-record payload without misaligning on records
/// with DIFE/VIFE chains or variable-length data.
pub fn parse_variable_record_consumed(input: &[u8]) -> Result<(MBusRecord, usize), ProtocolError> {
    let (mut remaining, mut record) = parse_variable_record_inner(input)
        // The nom error's debug detail cannot be carried without an allocator; the
        // static description is what a caller can act on either way.
        .map_err(|_| ProtocolError::InvalidField("malformed record structure"))?;
    // The nom parser already consumed the DRH (DIF + DIFEs + VIF + VIFEs).
    let mut consumed = input.len() - remaining.len();

    // For manufacturer-specific or more-records-follow, data is already populated
    if record.drh.dib.dif != MBUS_DIB_DIF_MANUFACTURER_SPECIFIC
        && record.drh.dib.dif != MBUS_DIB_DIF_MORE_RECORDS_FOLLOW
    {
        // re-calculate data length, if of variable length type
        if (record.drh.dib.dif & MBUS_DATA_RECORD_DIF_MASK_DATA) == 0x0D {
            // The LVAR byte must actually be present. `first()` tolerated an empty slice
            // and then the next line sliced `[1..]` on it, which panics — reachable from
            // any record that announces a variable length at the very end of the input.
            let Some(&lvar) = remaining.first() else {
                return Err(ProtocolError::PrematureEnd);
            };
            record.data_len = parse_variable_data_length(lvar)?;
            remaining = &remaining[1..];
            consumed += 1; // the variable-length byte
        }

        if record.data_len > remaining.len() {
            return Err(ProtocolError::PrematureEnd);
        }

        // EN 13757-3 §6.4.3 lets an LVAR code describe up to 1130 bytes, but
        // `MBusRecord::data` is a fixed 256-byte array. The check above only compares
        // against the *input*, so a long enough input walked off the end of the
        // destination — an out-of-bounds panic on attacker-supplied bytes, which for a
        // gateway parsing untrusted meter frames is a denial of service.
        if record.data_len > record.data.len() {
            return Err(ProtocolError::InvalidField(
                "record data length exceeds the record buffer",
            ));
        }

        for j in 0..record.data_len {
            record.data[j] = *remaining.get(j).unwrap_or(&0);
        }
        consumed += record.data_len; // the data bytes
    }

    decode_record_value(&mut record);
    Ok((record, consumed))
}

/// Decode a record's data bytes into its value, unit and quantity.
///
/// Parsing the DRH only tells you *how* to read the data; without this step a record
/// carries a header and a zero. The DIF's low nibble selects the encoding, and the
/// VIB supplies the decimal exponent the raw integer is scaled by (e.g. VIF 0x13 is
/// volume in 10⁻³ m³, so a raw 25555 becomes 25.555 m³).
fn decode_record_value(record: &mut MBusRecord) {
    // Manufacturer-specific and more-records-follow markers carry no scalar value.
    if record.drh.dib.dif == MBUS_DIB_DIF_MANUFACTURER_SPECIFIC
        || record.drh.dib.dif == MBUS_DIB_DIF_MORE_RECORDS_FOLLOW
    {
        return;
    }

    let vib = {
        let v = &record.drh.vib;
        // 1 primary + at most 10 VIFEs: exactly `Vib`'s capacity, so no push can fail.
        let mut infos = crate::payload::vif::Vib::new();
        // 0xFD and 0xFB are not units: they are escapes saying "the meaning is in
        // the next byte". Looking only at the primary VIF leaves every extended
        // quantity — voltage, current, and the rest — decoded to the right number
        // with no unit and no name.
        match v.vif {
            0xFD if v.nvife > 0 => {
                if let Some(ext) = crate::payload::vif_maps::lookup_vife_fd(v.vife[0]) {
                    let _ = infos.push(ext);
                }
            }
            0xFB if v.nvife > 0 => {
                if let Some(ext) = crate::payload::vif_maps::lookup_vife_fb(v.vife[0]) {
                    let _ = infos.push(ext);
                }
            }
            _ => {
                if let Some(primary) = crate::payload::vif_maps::lookup_primary_vif(v.vif) {
                    let _ = infos.push(primary);
                }
            }
        }
        infos
    };
    // The core returns `&'static str`; this module's record fields are still owned
    // strings, so it converts here. When `MBusRecord` moves to fixed-capacity fields the
    // conversion disappears rather than moving.
    // `normalize_vib` returns `&'static str` straight off the const tables, so these go
    // into the record with no allocation and no copy.
    let (unit, exponent, quantity) = match crate::payload::vif::normalize_vib(&vib) {
        Ok((u, e, q)) => (UnitText::Static(u), e, QuantityText::Static(q)),
        // Unknown VIF: leave the raw bytes for the caller rather than inventing a unit.
        Err(_) => (UnitText::new(), 1.0, QuantityText::new()),
    };

    let data = &record.data[..record.data_len];
    let coding = record.drh.dib.dif & MBUS_DATA_RECORD_DIF_MASK_DATA;
    let raw: Option<f64> = match coding {
        0x00 => Some(0.0),                                      // no data
        0x01..=0x04 | 0x06 | 0x07 => Some(int_le(data) as f64), // 8/16/24/32/48/64-bit int
        0x05 => (data.len() >= 4)
            .then(|| f32::from_le_bytes([data[0], data[1], data[2], data[3]]) as f64),
        0x09..=0x0C | 0x0E => bcd_le(data), // 2/4/6/8/12-digit BCD
        0x0D => {
            // Variable length (LVAR): text, kept verbatim.
            record.is_numeric = false;
            // Reversed into reading order, straight into the fixed buffer — the previous
            // version collected a Vec and then allocated a String from it, two allocations
            // to re-present bytes that are already sitting in `record.data`.
            let mut text = heapless::String::<RECORD_TEXT_CAPACITY>::new();
            for &b in data.iter().rev() {
                if text.push(b as char).is_err() {
                    break;
                }
            }
            record.value = MBusRecordValue::String(text);
            record.unit = unit;
            record.quantity = quantity;
            return;
        }
        _ => None, // selection for readout / special functions
    };

    if let Some(raw) = raw {
        record.is_numeric = true;
        record.value = MBusRecordValue::Numeric(raw * exponent);
    }
    record.unit = unit;
    record.quantity = quantity;
}

/// Parse one variable-data record. See [`parse_variable_record_consumed`] when you need the
/// exact bytes consumed (e.g. to advance through a multi-record payload).
pub fn parse_variable_record(input: &[u8]) -> Result<MBusRecord, ProtocolError> {
    parse_variable_record_consumed(input).map(|(record, _)| record)
}

fn parse_variable_record_inner(input: &[u8]) -> IResult<&[u8], MBusRecord> {
    let mut record = MBusRecord {
        storage_number: 0,
        tariff: -1,
        device: -1,
        is_numeric: true,
        value: MBusRecordValue::Numeric(0.0),
        unit: UnitText::new(),
        function_medium: "",
        quantity: QuantityText::new(),
        drh: MBusDataRecordHeader {
            dib: MBusDataInformationBlock {
                dif: 0,
                ndife: 0,
                dife: [0; 10],
            },
            vib: MBusValueInformationBlock {
                vif: 0,
                nvife: 0,
                vife: [0; 10],
                custom_vif: CustomVif::new(),
            },
        },
        data_len: 0,
        data: [0; 256],
        more_records_follow: false,
        applied_quirks: AppliedQuirks::new(),
    };

    // Skip idle filler bytes if present (they are optional)
    let i = input;
    let (i, _) = nom::bytes::complete::take_while(|b| b == MBUS_DIB_DIF_IDLE_FILLER)(i)?;

    let (i, dif) = be_u8(i)?;
    record.drh.dib.dif = dif;

    if record.drh.dib.dif == MBUS_DIB_DIF_MANUFACTURER_SPECIFIC
        || record.drh.dib.dif == MBUS_DIB_DIF_MORE_RECORDS_FOLLOW
    {
        if record.drh.dib.dif == MBUS_DIB_DIF_MORE_RECORDS_FOLLOW {
            record.more_records_follow = true;
        }

        // For manufacturer-specific or more-records-follow,
        // all remaining data belongs to this record
        record.data_len = i.len();
        record.data[..i.len()].copy_from_slice(i);

        mbus_data_record_append(&mut record);
        return Ok((&[], record));
    }

    record.data_len = dif_datalength_lookup(record.drh.dib.dif);

    // Parse DIF extensions if DIF has extension bit set
    let mut i_temp = i;
    if (record.drh.dib.dif & MBUS_DIB_DIF_EXTENSION_BIT) != 0 {
        let mut dife_count = 0;
        loop {
            // Refuse a chain that runs off the end or exceeds the standard's ten DIFEs,
            // rather than stopping and carrying on. Breaking here meant the next byte —
            // the 11th DIFE — was read as the VIF, and every field after it shifted by
            // one: the parser returned Ok with a confidently wrong record. This is the
            // error handling the (unused) parser in data.rs had and this one lacked.
            if i_temp.is_empty() {
                return Err(nom::Err::Error(nom::error::Error::new(
                    i_temp,
                    nom::error::ErrorKind::Eof,
                )));
            }
            if dife_count >= 10 {
                return Err(nom::Err::Error(nom::error::Error::new(
                    i_temp,
                    nom::error::ErrorKind::TooLarge,
                )));
            }
            let dife = i_temp[0];
            record.drh.dib.dife[dife_count] = dife;
            dife_count += 1;
            i_temp = &i_temp[1..];
            // Continue if this DIFE also has extension bit
            if (dife & MBUS_DIB_DIF_EXTENSION_BIT) == 0 {
                break;
            }
        }
        record.drh.dib.ndife = dife_count;
    }
    let i = i_temp;

    let (i, vif) = be_u8(i)?;
    record.drh.vib.vif = vif;

    // Custom (plain-text) VIF 0x7C: length byte + ASCII text. Advance the cursor PAST the
    // length and text — previously the cursor from `take()` was discarded, so parsing
    // resumed at the length byte and every downstream offset (data + next record) was wrong.
    let i = if (vif & MBUS_DIB_VIF_WITHOUT_EXTENSION) == 0x7C {
        let (i, var_vif_len) = be_u8(i)?;
        if var_vif_len > MBUS_VALUE_INFO_BLOCK_CUSTOM_VIF_SIZE {
            return Err(nom::Err::Error(nom::error::Error::new(
                i,
                nom::error::ErrorKind::Tag,
            )));
        }
        let (i, custom_vif) = take(var_vif_len)(i)?;
        // Reversed, as the wire carries it, straight into the fixed buffer. The parser
        // already rejects a length above MBUS_VALUE_INFO_BLOCK_CUSTOM_VIF_SIZE above, so
        // the pushes cannot overflow; discarded rather than unwrapped all the same.
        record.drh.vib.custom_vif.clear();
        for byte in custom_vif.iter().rev() {
            let _ = record.drh.vib.custom_vif.push(*byte as char);
        }
        i
    } else {
        i
    };

    // Parse VIF extensions if VIF has extension bit set
    let mut i_temp = i;
    if (record.drh.vib.vif & MBUS_DIB_VIF_EXTENSION_BIT) != 0 {
        let mut vife_count = 0;
        loop {
            // Same reasoning as the DIFE chain above: stopping silently made the next
            // byte — the 11th VIFE — the first data byte, shifting the value.
            if i_temp.is_empty() {
                return Err(nom::Err::Error(nom::error::Error::new(
                    i_temp,
                    nom::error::ErrorKind::Eof,
                )));
            }
            if vife_count >= 10 {
                return Err(nom::Err::Error(nom::error::Error::new(
                    i_temp,
                    nom::error::ErrorKind::TooLarge,
                )));
            }
            let vife = i_temp[0];
            record.drh.vib.vife[vife_count] = vife;
            vife_count += 1;
            i_temp = &i_temp[1..];
            // Continue if this VIFE also has extension bit
            if (vife & MBUS_DIB_VIF_EXTENSION_BIT) == 0 {
                break;
            }
        }
        record.drh.vib.nvife = vife_count;
    }
    let i = i_temp;

    accumulate_dib_fields(&mut record);

    Ok((i, record))
}

/// Accumulate storage number, tariff, and subunit (device) from the DIF + DIFE chain
/// per EN 13757-3 §6.3.2.
///
/// The DIF's bit 6 (`0x40`) carries the storage-number LSB — present on *every* record,
/// so it is always applied. Each DIFE then extends the fields, least-significant first:
/// its low nibble (`0x0F`) adds 4 more storage bits, bits `0x30` add 2 tariff bits, and
/// bit `0x40` adds 1 subunit bit.
///
/// Tariff and subunit keep the `-1` "absent" sentinel unless at least one DIFE is present,
/// so a plain record still reports "no tariff / no subunit" (downstream instrumentation
/// maps `< 0` to `None`). Storage has no sentinel — `0` is a valid storage number — so it
/// is always written.
///
/// This is what surfaces a Techem/OMS storage-indexed history run (e.g. `mkradio3a`'s
/// `82 xx FD3A` slots): each slot's DIFE nibble becomes a distinct `storage_number`, which
/// the vendor layer maps to a billing date.
fn accumulate_dib_fields(record: &mut MBusRecord) {
    let dif = record.drh.dib.dif;
    let ndife = record.drh.dib.ndife;

    // Storage-number LSB from DIF bit 6 — always present.
    let mut storage: u32 = ((dif & MBUS_DATA_RECORD_DIF_MASK_STORAGE_NO) >> 6) as u32;

    if ndife > 0 {
        let mut tariff: u32 = 0;
        let mut device: u32 = 0;
        for (idx, &dife) in record.drh.dib.dife[..ndife].iter().enumerate() {
            // Storage bits are 4 per DIFE, starting at bit 1 (LSB is the DIF bit above).
            // Guard the shift: a pathological chain (>7 DIFEs) would exceed u32 — those
            // high bits simply don't fit and are dropped, matching libmbus behaviour.
            let storage_shift = 1 + 4 * idx;
            if storage_shift < 32 {
                storage |= ((dife & MBUS_DATA_RECORD_DIFE_MASK_STORAGE_NO) as u32) << storage_shift;
            }
            tariff |= (((dife & MBUS_DATA_RECORD_DIFE_MASK_TARIFF) >> 4) as u32) << (2 * idx);
            device |= (((dife & MBUS_DATA_RECORD_DIFE_MASK_DEVICE) >> 6) as u32) << idx;
        }
        record.tariff = tariff as i32;
        record.device = device as i32;
    }

    record.storage_number = storage;
}

/// Normalizes a single fixed-length M-Bus data record unit.
///
/// Returns `&'static str` straight off `FIXED_MEDIUM_UNITS`, which is a `const` table —
/// the previous signature called `.to_string()` on both, the same needless allocation
/// `normalize_vib` had.
fn normalize_fixed_unit(
    medium_unit: u8,
    value: f64,
) -> Result<(&'static str, f64, &'static str), ProtocolError> {
    if let Some((_, unit, exponent, quantity)) = FIXED_MEDIUM_UNITS
        .iter()
        .find(|(code, _, _, _)| *code == medium_unit)
    {
        Ok((unit, value * exponent, quantity))
    } else {
        Err(ProtocolError::UnknownVif(medium_unit))
    }
}

pub fn mbus_data_record_append(record: &mut MBusRecord) {
    // For manufacturer-specific or more records follow, set appropriate fields
    if record.drh.dib.dif == MBUS_DIB_DIF_MANUFACTURER_SPECIFIC {
        record.quantity = QuantityText::Static("Manufacturer specific");
    }
    if record.drh.dib.dif == MBUS_DIB_DIF_MORE_RECORDS_FOLLOW {
        record.more_records_follow = true;
    }
    // Additional logic can be added here as needed
}

/// Parse one variable-data record under a [`DecodeContext`] — **the** decode path
/// (vendor-layers P7): plain parsing is this with an empty context, and every vendor
/// hook fires only through the context's binding, which exists only for frames whose
/// identity header validated (P6).
#[cfg(test)]
mod tests {

    // --- DIF/DIFE storage-number / tariff / subunit accumulation (EN 13757-3 §6.3.2) ---

    /// A plain record with no DIFE and no DIF storage bit: storage 0, tariff/subunit absent.
    #[test]
    fn plain_record_has_zero_storage_and_absent_tariff_subunit() {
        // DIF 0x04 (32-bit int, bit 6 clear), VIF 0x13, 4 data bytes.
        let (rec, _) =
            parse_variable_record_consumed(&[0x04, 0x13, 0x00, 0x00, 0x00, 0x00]).unwrap();
        assert_eq!(rec.storage_number, 0);
        assert_eq!(rec.tariff, -1, "no DIFE -> tariff absent");
        assert_eq!(rec.device, -1, "no DIFE -> subunit absent");
    }

    /// DIF bit 6 alone (no DIFE) is the storage-number LSB -> storage 1.
    #[test]
    fn dif_bit6_sets_storage_lsb() {
        // DIF 0x41 = 32-bit... actually 0x01 (8-bit int) | 0x40 storage bit.
        let (rec, _) = parse_variable_record_consumed(&[0x41, 0x13, 0x07]).unwrap();
        assert_eq!(rec.storage_number, 1);
        assert_eq!(rec.tariff, -1);
    }

    /// One DIFE carrying a storage nibble: storage = DIF-LSB | (nibble << 1).
    #[test]
    fn single_dife_extends_storage_number() {
        // DIF 0x84 (32-bit int + extension), DIFE 0x03 (storage nibble 3, no tariff/subunit).
        // storage = 0 (DIF bit6 clear) | (3 << 1) = 6.
        let (rec, _) =
            parse_variable_record_consumed(&[0x84, 0x03, 0x13, 0x00, 0x00, 0x00, 0x00]).unwrap();
        assert_eq!(rec.storage_number, 6);
        assert_eq!(rec.tariff, 0, "DIFE present, tariff bits 0");
        assert_eq!(rec.device, 0, "DIFE present, subunit bit 0");
    }

    /// DIFE tariff and subunit bits decode independently of storage.
    #[test]
    fn dife_tariff_and_subunit_bits() {
        // DIF 0x84, DIFE 0x50 = tariff (0x30>>4 = 1) ... 0x50 & 0x30 = 0x10 -> tariff 1,
        // 0x50 & 0x40 = 0x40 -> subunit 1, storage nibble 0.
        let (rec, _) =
            parse_variable_record_consumed(&[0x84, 0x50, 0x13, 0x00, 0x00, 0x00, 0x00]).unwrap();
        assert_eq!(rec.tariff, 1);
        assert_eq!(rec.device, 1);
        assert_eq!(rec.storage_number, 0);
    }

    /// A storage-indexed history run: successive DIFE nibbles yield distinct storage
    /// numbers — the mechanism the Techem/OMS almanac decode depends on.
    #[test]
    fn history_run_yields_distinct_storage_numbers() {
        // Two DIFEs: low nibble of DIFE0 = bits [4:1], low nibble of DIFE1 = bits [8:5].
        // DIFE0 = 0x08 (storage nibble 8), DIFE1 = 0x00 -> storage = 8 << 1 = 16.
        let (rec, _) =
            parse_variable_record_consumed(&[0x84, 0x88, 0x00, 0x13, 0x00, 0x00, 0x00, 0x00])
                .unwrap();
        // DIFE0 = 0x88: extension bit set + storage nibble 8. DIFE1 = 0x00.
        // storage = (8 << 1) | (0 << 5) = 16.
        assert_eq!(rec.storage_number, 16);
    }

    /// Parsing a record header is not the same as reading a meter: these pin the
    /// data-byte decoding and VIF scaling that turn a DRH plus bytes into a value.
    #[test]
    fn decodes_32bit_integer_with_vif_exponent() {
        // DIF 0x04 (32-bit int), VIF 0x13 (volume, 10^-3 m3), raw 25555 -> 25.555 m3.
        let (rec, used) = parse_variable_record_consumed(&[0x04, 0x13, 0xD3, 0x63, 0x00, 0x00])
            .expect("record parses");
        assert_eq!(used, 6);
        match rec.value {
            MBusRecordValue::Numeric(v) => assert!((v - 25.555).abs() < 1e-9, "got {v}"),
            other => panic!("expected numeric, got {other:?}"),
        }
        assert_eq!(rec.unit, "m^3");
    }

    #[test]
    fn decodes_8bit_integer_temperature() {
        // DIF 0x01 (8-bit int), VIF 0x5B (flow temperature, degrees C), raw 18.
        let (rec, _) = parse_variable_record_consumed(&[0x01, 0x5B, 0x12]).unwrap();
        match rec.value {
            MBusRecordValue::Numeric(v) => assert!((v - 18.0).abs() < 1e-9, "got {v}"),
            other => panic!("expected numeric, got {other:?}"),
        }
    }

    #[test]
    fn decodes_packed_bcd_including_negative_sign_nibble() {
        // DIF 0x0C (8-digit BCD), VIF 0x13: 0x12345678 little-endian BCD = 12345678.
        let (rec, _) =
            parse_variable_record_consumed(&[0x0C, 0x13, 0x78, 0x56, 0x34, 0x12]).unwrap();
        match rec.value {
            MBusRecordValue::Numeric(v) => assert!((v - 12345.678).abs() < 1e-9, "got {v}"),
            other => panic!("expected numeric, got {other:?}"),
        }
        // 0xF in the top nibble of the last byte marks a negative magnitude.
        assert_eq!(super::bcd_le(&[0x34, 0x12]), Some(1234.0));
        assert_eq!(super::bcd_le(&[0x34, 0xF2]), Some(-234.0));
        assert_eq!(
            super::bcd_le(&[0x3A, 0x12]),
            None,
            "non-BCD nibble rejected"
        );
    }

    #[test]
    fn sign_extends_negative_integers() {
        assert_eq!(super::int_le(&[0xFF]), -1);
        assert_eq!(super::int_le(&[0x00, 0x80]), -32768);
        assert_eq!(super::int_le(&[0xD3, 0x63, 0x00, 0x00]), 25555);
    }
    use super::*;
    use crate::error::ProtocolError;

    #[test]
    fn test_mbus_dif_datalength_lookup_all_cases() {
        // Table-driven test for all DIF values
        let test_cases = vec![
            (0x00, 0),
            (0x01, 1),
            (0x02, 2),
            (0x03, 3),
            (0x04, 4),
            (0x05, 4), // 32-bit real
            (0x06, 6), // 48-bit int
            (0x07, 8), // 64-bit int
            (0x08, 0), // selection for readout
            (0x09, 1),
            (0x0A, 2),
            (0x0B, 3),
            (0x0C, 4),
            (0x0D, 0), // variable length
            (0x0E, 6),
            (0x0F, 0), // special functions
            (0x10, 0), // Out of range, defaults to 0
        ];
        for (dif, expected) in test_cases {
            assert_eq!(dif_datalength_lookup(dif), expected);
        }
    }

    #[test]
    fn test_parse_variable_data_length_edge_cases() -> Result<(), ProtocolError> {
        // Direct length
        assert_eq!(parse_variable_data_length(0xBF)?, 191);

        // Even lengths (C0-CF)
        assert_eq!(parse_variable_data_length(0xC0)?, 0);
        assert_eq!(parse_variable_data_length(0xCF)?, 30);

        // Odd lengths (D0-DF)
        assert_eq!(parse_variable_data_length(0xD0)?, 1);
        assert_eq!(parse_variable_data_length(0xDF)?, 31);

        // Large even (E0-EF)
        assert_eq!(parse_variable_data_length(0xE0)?, 64);
        assert_eq!(parse_variable_data_length(0xEF)?, 79);

        // Large odd (F0-FA)
        assert_eq!(parse_variable_data_length(0xF0)?, 1120);
        assert_eq!(parse_variable_data_length(0xFA)?, 1130);

        // Invalid
        assert!(matches!(
            parse_variable_data_length(0xFB),
            Err(ProtocolError::UnknownDif(0xFB))
        ));
        assert!(matches!(
            parse_variable_data_length(0xFF),
            Err(ProtocolError::UnknownDif(0xFF))
        ));

        Ok(())
    }

    #[test]
    fn test_normalize_fixed_unit_all_cases() -> Result<(), ProtocolError> {
        // Test all defined units
        for (code, unit, exponent, quantity) in FIXED_MEDIUM_UNITS.iter() {
            let result = normalize_fixed_unit(*code, 100.0)?;
            assert_eq!(result.0, unit.to_string());
            assert_eq!(result.1, 100.0 * *exponent);
            assert_eq!(result.2, quantity.to_string());
        }

        // Test unknown unit
        assert!(matches!(
            normalize_fixed_unit(0xFF, 100.0),
            Err(ProtocolError::UnknownVif(0xFF))
        ));

        Ok(())
    }

    #[test]
    fn test_parse_fixed_record_invalid_cases() {
        // Too short input
        let short_input = [0u8; 11];
        assert!(matches!(
            parse_fixed_record(&short_input),
            Err(ProtocolError::InvalidField(_))
        ));

        // Invalid BCD device ID
        let invalid_bcd = [
            0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ];
        assert!(matches!(
            parse_fixed_record(&invalid_bcd),
            Err(ProtocolError::InvalidField(_))
        ));

        // Invalid manufacturer
        let invalid_man = [
            0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ];
        assert!(matches!(
            parse_fixed_record(&invalid_man),
            Err(ProtocolError::InvalidField(_))
        ));

        // Invalid signature
        let invalid_sig = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00,
            0x00, 0x00,
        ];
        assert!(matches!(
            parse_fixed_record(&invalid_sig),
            Err(ProtocolError::InvalidField(_))
        ));

        // Invalid BCD counter
        let invalid_bcd_counter = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0xFF, 0xFF,
            0xFF, 0xFF,
        ]; // Status for BCD
        assert!(matches!(
            parse_fixed_record(&invalid_bcd_counter),
            Err(ProtocolError::InvalidField(_))
        ));

        // Invalid int counter
        let invalid_int_counter = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF,
            0xFF, 0xFF,
        ]; // Status for int
        assert!(matches!(
            parse_fixed_record(&invalid_int_counter),
            Err(ProtocolError::InvalidField(_))
        ));
    }

    #[test]
    fn test_mbus_data_record_append() {
        let mut record = MBusRecord {
            // Minimal record
            storage_number: 0,
            tariff: -1,
            device: -1,
            is_numeric: true,
            value: MBusRecordValue::Numeric(0.0),
            unit: UnitText::new(),
            function_medium: "",
            quantity: QuantityText::new(),
            drh: MBusDataRecordHeader {
                dib: MBusDataInformationBlock {
                    dif: MBUS_DIB_DIF_MANUFACTURER_SPECIFIC,
                    ndife: 0,
                    dife: [0; 10],
                },
                vib: MBusValueInformationBlock {
                    vif: 0,
                    nvife: 0,
                    vife: [0; 10],
                    custom_vif: CustomVif::new(),
                },
            },
            data_len: 0,
            data: [0; 256],
            more_records_follow: false,
            applied_quirks: AppliedQuirks::new(),
        };
        mbus_data_record_append(&mut record);
        assert_eq!(record.quantity, "Manufacturer specific");

        // Test more records follow
        record.drh.dib.dif = MBUS_DIB_DIF_MORE_RECORDS_FOLLOW;
        mbus_data_record_append(&mut record);
        assert!(record.more_records_follow);
    }

    #[test]
    fn custom_vif_record_reports_exact_consumption() {
        // Regression for fix #8: DIF=0x01 (1-byte int), VIF=0x7C (plain text) len=5 "Test1",
        // data=0x42, then a following record byte 0xEE. Consumption must be 9 (DIF+VIF+len+
        // 5 text + 1 data) and the data byte must be 0x42 — previously the custom-VIF cursor
        // was discarded, so consumption was ~3 and 0x05 was mistaken for the data.
        let data = [0x01, 0x7C, 0x05, b'T', b'e', b's', b't', b'1', 0x42, 0xEE];
        let (record, consumed) = parse_variable_record_consumed(&data).unwrap();
        assert_eq!(consumed, 9, "DIF+VIF+len+5 text+1 data = 9 bytes");
        assert_eq!(record.data_len, 1);
        assert_eq!(
            record.data[0], 0x42,
            "data must be 0x42, not the VIF length byte"
        );
        assert_eq!(&data[consumed..], &[0xEE], "next record stays aligned");
    }
}

#[cfg(test)]
mod vif_extension_tests {
    use super::*;

    /// A battery-voltage record as a real meter sends it: DIF 0x02 (16-bit int),
    /// VIF 0xFD (escape), VIFE 0x46 (voltage, 10^-3 V), value 4137 mV.
    #[test]
    fn extended_vif_voltage_gets_unit_and_quantity() {
        let raw = [0x02u8, 0xFD, 0x46, 0x29, 0x10];
        let (rec, used) = parse_variable_record_consumed(&raw).expect("record parses");
        assert_eq!(used, raw.len());
        assert_eq!(rec.quantity, "Voltage");
        assert_eq!(rec.unit, "V");
        match rec.value {
            MBusRecordValue::Numeric(v) => {
                // 4137 raw x 10^-3 V
                assert!((v - 4.137).abs() < 1e-9, "got {v}");
            }
            other => panic!("expected a numeric voltage, got {other:?}"),
        }
    }
}
