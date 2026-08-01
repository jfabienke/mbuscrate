//! Platform-independent decode core: raw wM-Bus frame bytes -> JSON.
//!
//! This is the heart of the A/B test. It takes the exact bytes a radio (or a capture
//! file) delivers and produces a decoded JSON object shaped to line up with what the
//! C++ `metermon` publishes, so the two can be diffed.
//!
//! The mode-C link-layer decode (Type A / Type B block framing, per-block CRC, BCD
//! address) now lives in the crate as [`mbus_rs::wmbus::mode_c::decode_mode_c`]; this
//! module is only the metermon-shaped JSON glue on top of it, plus the CI-byte
//! application-layer dispatch.

use mbus_rs::id_to_manufacturer;
use mbus_rs::payload::record::{parse_variable_record, MBusRecord, MBusRecordValue};
use mbus_rs::wmbus::frame_decode::FrameType;
use mbus_rs::wmbus::mode_c::decode_mode_c;
use serde_json::{json, Value};

use crate::config::Config;
use crate::keystore::KeyStore;

/// Decode one normalized wM-Bus frame into a JSON object. Never panics; failures are
/// reported as fields so the A/B diff stays aligned.
pub fn decode_frame(raw: &[u8], cfg: &Config, keys: &KeyStore) -> Value {
    let frame = match decode_mode_c(raw) {
        Ok(f) => f,
        Err(e) => {
            return json!({
                "gwid": cfg.gwid,
                "raw_hex": hex::encode(raw),
                "decode_error": e.to_string(),
            });
        }
    };

    let meterid = frame.device_address;
    let frame_type = match frame.frame_type {
        FrameType::TypeA => "A",
        FrameType::TypeB => "B",
        FrameType::Unknown => "?",
    };
    let mut out = json!({
        "gwid": cfg.gwid,
        "meterid": meterid,
        "manufacturer": id_to_manufacturer(frame.manufacturer_id),
        "address": format!("{meterid:08}"),
        "version": frame.version,
        "type": frame.device_type,
        "frame_type": frame_type,
        "c_field": format!("0x{:02X}", frame.control_field),
        "crc_ok": frame.crc_ok,
    });
    let obj = out.as_object_mut().unwrap();

    // No application payload (e.g. ACC-NR / SND-NKE short frames).
    let ci = match frame.ci() {
        Some(ci) => ci,
        None => {
            obj.insert("ci".into(), json!(null));
            return out;
        }
    };
    obj.insert("ci".into(), json!(format!("0x{ci:02X}")));
    let after_ci = frame.application_data();

    match ci {
        // No TPL header — plaintext records follow directly.
        0x78 => {
            obj.insert("encrypted".into(), json!(false));
            insert_records(obj, after_ci);
        }
        // Short TPL header (OMS 7.2.4): ACC, STS, Configuration Word, then payload.
        0x7A => {
            if after_ci.len() < 4 {
                obj.insert("error".into(), json!("short header truncated"));
                return out;
            }
            let sts = after_ci[1];
            let cw = u16::from_le_bytes([after_ci[2], after_ci[3]]);
            let mode = (cw >> 8) & 0x1F; // mode from CW, not CI (matches epulse)
            obj.insert("status".into(), json!(sts));
            obj.insert("mode".into(), json!(mode));
            let ciphertext = &after_ci[4..];
            if mode == 0 {
                obj.insert("encrypted".into(), json!(false));
                insert_records(obj, ciphertext);
            } else {
                obj.insert("encrypted".into(), json!(true));
                obj.insert("ciphertext_hex".into(), json!(hex::encode(ciphertext)));
                obj.insert(
                    "decrypt".into(),
                    json!(if keys.get(meterid).is_some() {
                        format!("mode {mode} decrypt pending Phase 1.4")
                    } else {
                        "no key for meter".into()
                    }),
                );
            }
        }
        // Compact frame — records keyed by a format signature learned elsewhere.
        0x79 => {
            if after_ci.len() >= 2 {
                let sig = u16::from_le_bytes([after_ci[0], after_ci[1]]);
                obj.insert("signature".into(), json!(format!("0x{sig:04X}")));
            }
            obj.insert("compact".into(), json!(true));
        }
        // Extended Link Layer (encrypted, AES-CTR). Header parse only for now;
        // the CTR decrypt is Phase 1.4 crypto work.
        0x8D => {
            obj.insert("ell".into(), json!(true));
            obj.insert("encrypted".into(), json!(true));
            obj.insert("ciphertext_hex".into(), json!(hex::encode(after_ci)));
            obj.insert(
                "decrypt".into(),
                json!(if keys.get(meterid).is_some() {
                    "ELL AES-CTR decrypt pending Phase 1.4"
                } else {
                    "no key for meter"
                }),
            );
        }
        other => {
            obj.insert("error".into(), json!(format!("unhandled CI 0x{other:02X}")));
            obj.insert("payload_hex".into(), json!(hex::encode(after_ci)));
        }
    }

    out
}

/// Best-effort record decode. `parse_variable_record` decodes one record but does not
/// report bytes consumed, so a full multi-record loop is a follow-up. We decode the
/// first record and always keep the raw payload for the diff.
fn insert_records(obj: &mut serde_json::Map<String, Value>, data: &[u8]) {
    obj.insert("payload_hex".into(), json!(hex::encode(data)));
    match parse_variable_record(data) {
        Ok(rec) => {
            obj.insert("records".into(), json!([record_to_json(&rec)]));
        }
        Err(e) => {
            obj.insert("record_error".into(), json!(e.to_string()));
        }
    }
}

fn record_to_json(rec: &MBusRecord) -> Value {
    let value = match &rec.value {
        MBusRecordValue::Numeric(n) => json!(n),
        MBusRecordValue::String(s) => json!(s),
    };
    json!({
        "dif": format!("0x{:02X}", rec.drh.dib.dif),
        "vif": format!("0x{:02X}", rec.drh.vib.vif),
        "value": value,
        "unit": rec.unit,
        "quantity": rec.quantity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_cfg() -> Config {
        serde_json::from_str(
            r#"{"gwid":"6543","mqtt":{"host":"h","clientid":"c","data-topic":"t"}}"#,
        )
        .unwrap()
    }

    #[test]
    fn decodes_real_type_b_frame() {
        // A real, complete Type B frame captured from meter 74644444 (KAM), CI=0x8D.
        let raw = hex::decode(
            "3d25442d2c444464741b168d208d3048a121f6597959d56873b609a439b99d58531a8a726d9f0c",
        )
        .unwrap();
        let v = decode_frame(&raw, &empty_cfg(), &KeyStore::new());
        assert_eq!(v["meterid"], 74644444u32); // BCD-decoded, matches metermon
        assert_eq!(v["manufacturer"], "KAM");
        assert_eq!(v["frame_type"], "B");
        assert_eq!(v["c_field"], "0x44");
        assert_eq!(v["ci"], "0x8D");
        // Real-frame CRC verification (Phase 1.3): the canonical CRC validates it.
        assert_eq!(v["crc_ok"], true);
    }

    #[test]
    fn reports_decode_error_without_panicking() {
        let v = decode_frame(&[0x00, 0x01, 0x02], &empty_cfg(), &KeyStore::new());
        assert!(v.get("decode_error").is_some());
    }
}
