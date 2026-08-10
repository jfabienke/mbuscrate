//! Techem manufacturer-specific wM-Bus decoders — legacy *positional* telegrams.
//!
//! Techem's legacy telegrams carry a non-OMS, positional payload under a
//! manufacturer CI (`0xA0`/`0xA1`/`0xA2`). This module **translates** them into
//! standard [`VendorDataRecord`]s (dates unpacked, units scaled, tagged with the
//! documented DIF/VIF), so downstream quantity/unit mapping stays generic and
//! never learns Techem exists. Techem's *newer* cells are standard OMS and decode
//! via the generic DIF/VIF path — they are intentionally **not** handled here.
//!
//! See `docs/TECHEM_RESEARCH.md` (variant matrix, field maps) and
//! `docs/TECHEM_DESIGN.md` (the OMS-first / manufacturer-isolated design).
//!
//! NOTE: these pure decoders are not yet wired into the decode path — the
//! manufacturer-CI seam is a separate step (see `docs/TECHEM_DESIGN.md`, Phase 1).
//! They are decision-independent of where that seam lands; the golden tests below
//! exercise them directly.
#![allow(dead_code)]

use super::{VendorDataRecord, VendorDeviceInfo, VendorExtension, VendorVariable};
use crate::error::MBusError;
use serde_json::Value;

/// Techem `VendorExtension` — registered under `"TCH"`. Decodes the legacy
/// positional telegrams (manufacturer CI) and names the device from its
/// `(version, device_type)`; the newer OMS cells need no decode code (the generic
/// DIF/VIF path handles them), only the naming.
pub struct TechemExtension;

impl VendorExtension for TechemExtension {
    fn handle_ci_manufacturer_range(
        &self,
        manufacturer_id: &str,
        version: u8,
        device_type: u8,
        _ci: u8,
        payload: &[u8],
    ) -> Result<Option<Vec<VendorDataRecord>>, MBusError> {
        if manufacturer_id != "TCH" {
            return Ok(None);
        }
        // Newer OMS cells select to None here and fall through to the generic path.
        Ok(Variant::select(version, device_type).and_then(|v| v.decode(payload)))
    }

    fn enrich_device_header(
        &self,
        manufacturer_id: &str,
        mut info: VendorDeviceInfo,
    ) -> Result<Option<VendorDeviceInfo>, MBusError> {
        if manufacturer_id != "TCH" {
            return Ok(None);
        }
        match identify(info.version, info.device_type) {
            Some((model, media)) => {
                info.model = Some(model.to_string());
                info.additional_info
                    .insert("media".to_string(), Value::String(media.to_string()));
                Ok(Some(info))
            }
            None => Ok(None),
        }
    }
}

/// Human-readable `(model, media)` for a Techem `(version, device_type)` — covering
/// both legacy and OMS cells. Identity only; decoding is [`Variant`].
pub fn identify(version: u8, device_type: u8) -> Option<(&'static str, &'static str)> {
    Some(match (version, device_type) {
        (0x69 | 0x94, 0x80) => ("Techem FHKV data III", "heat cost allocator"),
        (0x69 | 0x94, 0x08) => ("Techem FHKV data IV", "heat cost allocator"),
        (0x6A, 0x08) => ("Techem FHKV radio 4", "heat cost allocator"),
        (0x70 | 0x95, 0x62) => ("Techem mk-radio (warm water)", "warm water"),
        (0x70 | 0x95, 0x72) => ("Techem mk-radio (cold water)", "water"),
        (0x50, 0x72) => ("Techem mk-radio 3a", "water"),
        (0x95, 0x37) => ("Techem mk-radio 4a", "water"),
        (0x22 | 0x39 | 0x45, 0x04 | 0x43 | 0xC3) => ("Techem compact V", "heat"),
        (0x27, 0x04 | 0xC3) => ("Techem vario 4", "heat"),
        (0x28, 0x04) => ("Techem vario 411", "heat"),
        (0x17, 0x04) => ("Techem vario 451 MID", "heat"),
        _ => return None,
    })
}

/// Techem device category (from the wM-Bus device-type byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Hca,
    Water,
    Heat,
}

/// A legacy positional Techem telegram variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// FHKV data III heat-cost allocator (unencrypted, CI 0xA0/0xA2).
    FhkvIii,
    /// mk-radio 3/4 water meter (legacy, CI 0xA2).
    MkRadio,
    /// compact5 heat meter (legacy, CI 0xA1/0xA2).
    Compact5,
    /// vario451 heat meter (legacy, CI 0xA2, raw mGJ).
    Vario451,
}

impl Variant {
    /// Select a legacy positional variant from the link-header selector.
    ///
    /// Keyed on `(version, device_type)` — the CI byte is *not* a reliable
    /// discriminator (FHKV III appears under both 0xA0 and 0xA2). Returns `None`
    /// for the newer OMS cells (decoded by the generic path) and anything
    /// unrecognized.
    pub fn select(version: u8, device_type: u8) -> Option<Variant> {
        match (version, device_type) {
            (0x69 | 0x94, 0x80) => Some(Variant::FhkvIii),
            (0x70 | 0x95, 0x62 | 0x72) => Some(Variant::MkRadio),
            (0x22 | 0x39 | 0x45, 0x04 | 0x43 | 0xC3) => Some(Variant::Compact5),
            (0x27, 0x04 | 0xC3) => Some(Variant::Vario451),
            _ => None,
        }
    }

    pub fn category(self) -> Category {
        match self {
            Variant::FhkvIii => Category::Hca,
            Variant::MkRadio => Category::Water,
            Variant::Compact5 | Variant::Vario451 => Category::Heat,
        }
    }

    /// Decode the application payload (bytes after the CI) into normalized records.
    pub fn decode(self, app_data: &[u8]) -> Option<Vec<VendorDataRecord>> {
        match self {
            Variant::FhkvIii => decode_fhkv_iii(app_data),
            Variant::MkRadio => decode_mkradio(app_data),
            Variant::Compact5 => decode_compact5(app_data),
            Variant::Vario451 => decode_vario451(app_data),
        }
    }
}

// ---- little helpers -------------------------------------------------------

fn le16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o + 1)?]))
}

fn le24(b: &[u8], o: usize) -> Option<u32> {
    Some((*b.get(o)? as u32) | ((*b.get(o + 1)? as u32) << 8) | ((*b.get(o + 2)? as u32) << 16))
}

fn num(dif: u8, vif: u8, quantity: &str, unit: &str, value: f64) -> VendorDataRecord {
    VendorDataRecord {
        dif,
        vif,
        unit: unit.to_string(),
        value: VendorVariable::Numeric(value),
        quantity: quantity.to_string(),
    }
}

fn text(dif: u8, vif: u8, quantity: &str, value: String) -> VendorDataRecord {
    VendorDataRecord {
        dif,
        vif,
        unit: String::new(),
        value: VendorVariable::String(value),
        quantity: quantity.to_string(),
    }
}

// ---- Techem bit-packed dates ---------------------------------------------
//
// Two encodings within one telegram. "previous" (billing) and "current"
// (readout) dates pack day/month/year into a u16 with different bit positions.
// Epoch is year 2000. Verified against the fhkvdataiii golden telegrams.

/// Previous/billing date: `day = raw & 0x1F`, `month = (raw>>5) & 0x0F`,
/// `year = 2000 + (raw>>9)`.
fn date_prev(raw: u16) -> (u16, u8, u8) {
    let day = (raw & 0x1F) as u8;
    let month = ((raw >> 5) & 0x0F) as u8;
    let year = 2000 + (raw >> 9);
    (year, month, day)
}

/// Current/readout date: `day = (raw>>4) & 0x1F`, `month = (raw>>9) & 0x0F`.
/// The year is the previous-billing year unless the month/day has rolled past it
/// (Techem stores only the previous year explicitly).
fn date_curr(raw: u16, prev_year: u16, prev_month: u8, prev_day: u8) -> (u16, u8, u8) {
    let day = ((raw >> 4) & 0x1F) as u8;
    let month = ((raw >> 9) & 0x0F) as u8;
    let rolled = month < prev_month || (month == prev_month && day <= prev_day);
    let year = prev_year + rolled as u16;
    (year, month, day)
}

fn iso(y: u16, m: u8, d: u8) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

// ---- positional decoders --------------------------------------------------

/// FHKV data III HCA: `[tag] PrevDate PrevHca CurrDate CurrHca [extra] TempRoom TempRad`.
/// Tag `01`/`11` have no extra byte; `0F` inserts one before the temperatures.
fn decode_fhkv_iii(d: &[u8]) -> Option<Vec<VendorDataRecord>> {
    let extra = match *d.get(0)? {
        0x01 | 0x11 => 0usize,
        0x0F => 1usize,
        _ => return None,
    };
    let prev_date = le16(d, 1)?;
    let prev_hca = le16(d, 3)?;
    let curr_date = le16(d, 5)?;
    let curr_hca = le16(d, 7)?;
    let t = 9 + extra;
    let temp_room = le16(d, t)?;
    let temp_rad = le16(d, t + 2)?;

    let (py, pm, pd) = date_prev(prev_date);
    let (cy, cm, cd) = date_curr(curr_date, py, pm, pd);

    Some(vec![
        num(0x02, 0x6E, "current_hca", "", curr_hca as f64),
        num(0x42, 0x6E, "previous_hca", "", prev_hca as f64),
        text(0x42, 0x6C, "current_date", iso(cy, cm, cd)),
        text(0x02, 0x6C, "previous_date", iso(py, pm, pd)),
        num(0x02, 0x65, "temp_room_c", "°C", temp_room as f64 / 100.0),
        num(0x02, 0x5D, "temp_radiator_c", "°C", temp_rad as f64 / 100.0),
    ])
}

/// mk-radio water (legacy): `hdr(3) Prev(u16)@4215 b b Curr(u16)@0215`. VIF 0x15 =
/// volume × 0.1 m³.
fn decode_mkradio(d: &[u8]) -> Option<Vec<VendorDataRecord>> {
    let prev = le16(d, 3)? as f64 * 0.1;
    let curr = le16(d, 7)? as f64 * 0.1;
    Some(vec![
        num(0x02, 0x15, "current_volume_m3", "m3", curr),
        num(0x42, 0x15, "previous_volume_m3", "m3", prev),
        num(0x0C, 0x15, "total_volume_m3", "m3", prev + curr),
    ])
}

/// compact5 heat (legacy): `hdr(3) Prev(u24)@037E b Curr(u24)@037F`. Raw kWh.
fn decode_compact5(d: &[u8]) -> Option<Vec<VendorDataRecord>> {
    let prev = le24(d, 3)? as f64;
    let curr = le24(d, 7)? as f64;
    Some(vec![
        num(0x03, 0x7F, "current_kwh", "kWh", curr),
        num(0x03, 0x7E, "previous_kwh", "kWh", prev),
        num(0x0C, 0x00, "total_kwh", "kWh", prev + curr),
    ])
}

/// vario451 heat (legacy): `hdr(3) Prev(u16)@027E bb Curr(u16)@027F`. Raw mGJ
/// (1/1000 GJ); kWh = raw / 3.6.
fn decode_vario451(d: &[u8]) -> Option<Vec<VendorDataRecord>> {
    let prev = le16(d, 3)? as f64 / 3.6;
    let curr = le16(d, 7)? as f64 / 3.6;
    Some(vec![
        num(0x02, 0x7F, "current_kwh", "kWh", curr),
        num(0x02, 0x7E, "previous_kwh", "kWh", prev),
        num(0x0C, 0x00, "total_kwh", "kWh", prev + curr),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hx(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Extract the application payload (after the CI byte) from a full telegram.
    /// Layout: L(1) C(1) M(2) A(4) V(1) T(1) CI(1) then payload.
    fn app(telegram: &str) -> Vec<u8> {
        hx(&telegram.replace('_', ""))[11..].to_vec()
    }

    fn n(recs: &[VendorDataRecord], q: &str) -> f64 {
        match &recs.iter().find(|r| r.quantity == q).unwrap().value {
            VendorVariable::Numeric(v) => *v,
            _ => panic!("not numeric: {q}"),
        }
    }
    fn s(recs: &[VendorDataRecord], q: &str) -> String {
        match &recs.iter().find(|r| r.quantity == q).unwrap().value {
            VendorVariable::String(v) => v.clone(),
            _ => panic!("not string: {q}"),
        }
    }

    #[test]
    fn registered_in_default_registry_and_dispatches() {
        // Integration: with_defaults registers "TCH", and the CI-range dispatch
        // routes a positional payload to the decoder — the seam, end to end.
        let reg = crate::vendors::VendorRegistry::with_defaults().unwrap();
        let payload = hx("119F27020480048300C408F70914");
        let recs = crate::vendors::dispatch_ci_hook(&reg, "TCH", 0x69, 0x80, 0xA0, &payload)
            .unwrap()
            .expect("TCH handler returns records");
        assert!(recs.iter().any(|r| r.quantity == "current_hca"));
        // A newer OMS cell (device_type 0x08) returns None so the generic path takes it.
        let oms = crate::vendors::dispatch_ci_hook(&reg, "TCH", 0x69, 0x08, 0xA0, &payload).unwrap();
        assert!(oms.is_none());
    }

    #[test]
    fn identifies_models_across_legacy_and_oms_cells() {
        assert_eq!(
            identify(0x69, 0x80),
            Some(("Techem FHKV data III", "heat cost allocator"))
        );
        assert_eq!(
            identify(0x69, 0x08),
            Some(("Techem FHKV data IV", "heat cost allocator"))
        );
        assert_eq!(identify(0x17, 0x04), Some(("Techem vario 451 MID", "heat")));
        assert_eq!(identify(0x95, 0x37), Some(("Techem mk-radio 4a", "water")));
        assert_eq!(identify(0xFF, 0xFF), None);
    }

    #[test]
    fn enrich_names_device_via_registry() {
        let reg = crate::vendors::VendorRegistry::with_defaults().unwrap();
        let info = VendorDeviceInfo {
            manufacturer_id: 0x5068,
            device_id: 11_776_622,
            version: 0x69,
            device_type: 0x80,
            model: None,
            serial_number: None,
            firmware_version: None,
            additional_info: Default::default(),
        };
        let out = crate::vendors::dispatch_header_hook(&reg, "TCH", info)
            .unwrap()
            .expect("TCH names its device");
        assert_eq!(out.model.as_deref(), Some("Techem FHKV data III"));
        assert_eq!(
            out.additional_info.get("media").unwrap(),
            "heat cost allocator"
        );
    }

    #[test]
    fn selects_variants_and_skips_oms_cells() {
        assert_eq!(Variant::select(0x69, 0x80), Some(Variant::FhkvIii));
        assert_eq!(Variant::select(0x94, 0x80), Some(Variant::FhkvIii));
        assert_eq!(Variant::select(0x95, 0x72), Some(Variant::MkRadio));
        assert_eq!(Variant::select(0x45, 0x04), Some(Variant::Compact5));
        assert_eq!(Variant::select(0x27, 0x04), Some(Variant::Vario451));
        // OMS cells return None (generic path handles them):
        assert_eq!(Variant::select(0x69, 0x08), None); // fhkvdataiv
        assert_eq!(Variant::select(0x17, 0x04), None); // vario451mid
        assert_eq!(Variant::select(0x50, 0x72), None); // mkradio3a
    }

    #[test]
    fn techem_dates_match_golden() {
        // 0x279F previous -> 2019-12-31; 0x0480 current in that year -> 2020-02-08.
        let (py, pm, pd) = date_prev(0x279F);
        assert_eq!((py, pm, pd), (2019, 12, 31));
        assert_eq!(date_curr(0x0480, py, pm, pd), (2020, 2, 8));
    }

    #[test]
    fn fhkvdataiii_tag11_golden() {
        // wmbusmeters fhkvdataiii "Room" 11776622 (GPL test vector, reference).
        let d = app("34446850226677116980A0119F27020480048300C408F70914");
        let r = Variant::FhkvIii.decode(&d).unwrap();
        assert_eq!(n(&r, "current_hca"), 131.0);
        assert_eq!(n(&r, "previous_hca"), 1026.0);
        assert_eq!(s(&r, "current_date"), "2020-02-08");
        assert_eq!(s(&r, "previous_date"), "2019-12-31");
        assert!((n(&r, "temp_room_c") - 22.44).abs() < 1e-9);
        assert!((n(&r, "temp_radiator_c") - 25.51).abs() < 1e-9);
    }

    #[test]
    fn fhkvdataiii_tag0f_golden() {
        // "Rooom" 11111234 — the 0x0F variant with the extra byte before temps.
        let d = app("33446850341211119480A20F9F292D005024040011BD083809");
        let r = Variant::FhkvIii.decode(&d).unwrap();
        assert_eq!(n(&r, "current_hca"), 4.0);
        assert_eq!(n(&r, "previous_hca"), 45.0);
        assert_eq!(s(&r, "current_date"), "2021-02-05");
        assert_eq!(s(&r, "previous_date"), "2020-12-31");
        assert!((n(&r, "temp_room_c") - 22.37).abs() < 1e-9);
        assert!((n(&r, "temp_radiator_c") - 23.60).abs() < 1e-9);
    }

    #[test]
    fn compact5_golden() {
        // "Heating" 62626262 -> total 495 kWh (current 120, previous 375).
        let d = app("36446850626262624543A1009F2777010060780000000A00");
        let r = Variant::Compact5.decode(&d).unwrap();
        assert_eq!(n(&r, "current_kwh"), 120.0);
        assert_eq!(n(&r, "previous_kwh"), 375.0);
        assert_eq!(n(&r, "total_kwh"), 495.0);
    }

    #[test]
    fn vario451_golden() {
        // "HeatMeter" 58234965 -> total 6371.67 kWh (mGJ raw / 3.6).
        let d = app("374468506549235827C3A2129F25383300A862260000");
        let r = Variant::Vario451.decode(&d).unwrap();
        assert!((n(&r, "current_kwh") - 2729.444444).abs() < 1e-3);
        assert!((n(&r, "previous_kwh") - 3642.222222).abs() < 1e-3);
        assert!((n(&r, "total_kwh") - 6371.666667).abs() < 1e-3);
    }

    #[test]
    fn mkradio_golden() {
        // "Duschagain" 02410120 -> total 0.4 m³, target(previous) 0.1 m³.
        let d = app("2F446850200141029562A206702901006017030004");
        let r = Variant::MkRadio.decode(&d).unwrap();
        assert!((n(&r, "previous_volume_m3") - 0.1).abs() < 1e-9);
        assert!((n(&r, "current_volume_m3") - 0.3).abs() < 1e-9);
        assert!((n(&r, "total_volume_m3") - 0.4).abs() < 1e-9);
    }
}
