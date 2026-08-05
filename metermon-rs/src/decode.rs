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
use mbus_rs::payload::record::{parse_variable_record_consumed, MBusRecord, MBusRecordValue};
use mbus_rs::wmbus::compact_frame::CompactLayoutCache;
use mbus_rs::wmbus::ell;
use mbus_rs::wmbus::frame_decode::FrameType;
use mbus_rs::wmbus::mode_c::decode_mode_c;
use mbus_rs::wmbus::AesKey;
use serde_json::{json, Value};

use crate::config::Config;
use crate::keystore::KeyStore;

/// Decode one normalized wM-Bus frame into a JSON object. Never panics; failures are
/// reported as fields so the A/B diff stays aligned.
pub fn decode_frame(raw: &[u8], cfg: &Config, keys: &KeyStore) -> Value {
    // Stateless convenience wrapper: a throwaway cache cannot expand compact frames,
    // since that needs a full frame seen earlier. Long-running callers should keep a
    // cache and use [`decode_frame_with_cache`].
    decode_frame_with_cache(raw, cfg, keys, &mut CompactLayoutCache::new())
}

/// Decode one frame, using (and updating) a compact-frame layout cache so compact
/// frames from a meter whose full frame was seen earlier decode to real records.
pub fn decode_frame_with_cache(
    raw: &[u8],
    cfg: &Config,
    keys: &KeyStore,
    cache: &mut CompactLayoutCache,
) -> Value {
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
            decode_transport(obj, ci, after_ci, cache, meterid, frame.crc_ok);
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
            obj.insert("encrypted".into(), json!(false));
            decode_transport(obj, ci, after_ci, cache, meterid, frame.crc_ok);
        }
        // Extended Link Layer. Encrypted variants are decrypted in place when a key
        // is held, and the recovered transport payload is dispatched exactly like an
        // unencrypted one, so readings surface identically either way.
        0x8C..=0x8F => {
            obj.insert("ell".into(), json!(true));
            let header = match ell::parse_ell(&frame.payload) {
                Ok(h) => h,
                Err(e) => {
                    obj.insert("ell_error".into(), json!(e.to_string()));
                    return out;
                }
            };
            obj.insert("ell_cc".into(), json!(format!("0x{:02X}", header.cc)));
            obj.insert("ell_acc".into(), json!(format!("0x{:02X}", header.acc)));
            if let Some(sn) = header.session_number {
                obj.insert("ell_sn".into(), json!(format!("0x{sn:08X}")));
            }
            obj.insert("encrypted".into(), json!(header.is_encrypted()));

            if !header.is_encrypted() {
                let inner = &frame.payload[header.header_len..];
                if let Some((&tci, rest)) = inner.split_first() {
                    decode_transport(obj, tci, rest, cache, meterid, frame.crc_ok);
                }
                return out;
            }
            let Some(hexkey) = keys.get(meterid) else {
                obj.insert("ciphertext_hex".into(), json!(hex::encode(after_ci)));
                obj.insert("decrypt".into(), json!("no key for meter"));
                return out;
            };
            match AesKey::from_hex(hexkey)
                .map_err(|e| e.to_string())
                .and_then(|k| {
                    ell::decrypt_ell_payload(&frame.payload, &frame.link_header, &k)
                        .map_err(|e| e.to_string())
                }) {
                Ok(dec) => {
                    obj.insert("decrypted".into(), json!(true));
                    // Not interpreted as a CRC — see the ell module docs.
                    obj.insert(
                        "ell_leading_field".into(),
                        json!(hex::encode(dec.leading_field)),
                    );
                    if let Some((&tci, rest)) = dec.payload.split_first() {
                        decode_transport(obj, tci, rest, cache, meterid, frame.crc_ok);
                    }
                }
                Err(e) => {
                    obj.insert("decrypted".into(), json!(false));
                    obj.insert("ciphertext_hex".into(), json!(hex::encode(after_ci)));
                    obj.insert("decrypt_error".into(), json!(e));
                }
            }
        }
        other => {
            obj.insert("error".into(), json!(format!("unhandled CI 0x{other:02X}")));
            obj.insert("payload_hex".into(), json!(hex::encode(after_ci)));
        }
    }

    out
}

/// Dispatch a transport-layer payload by its CI byte, learning record layouts from
/// full frames and expanding compact frames with them.
fn decode_transport(
    obj: &mut serde_json::Map<String, Value>,
    tpl_ci: u8,
    body: &[u8],
    cache: &mut CompactLayoutCache,
    meter: u32,
    crc_ok: bool,
) {
    obj.insert("tpl_ci".into(), json!(format!("0x{tpl_ci:02X}")));
    match tpl_ci {
        // Full frame: the records are self-describing, so remember their layout for the
        // compact frames that follow.
        0x78 => {
            // Only learn from a frame whose CRC validated: a layout taken from
            // corrupted bytes would be applied to every later compact frame, turning
            // one bad frame into a stream of confidently wrong readings.
            if crc_ok {
                match cache.learn(meter, body) {
                    Ok(sig) => {
                        obj.insert("signature".into(), json!(format!("0x{sig:04X}")));
                    }
                    Err(e) => {
                        obj.insert("layout_error".into(), json!(e.to_string()));
                    }
                }
            }
            insert_records(obj, body);
        }
        // Compact frame: headers omitted; re-interleave them from the cached layout.
        0x79 => {
            obj.insert("compact".into(), json!(true));
            if body.len() >= 2 {
                obj.insert(
                    "signature".into(),
                    json!(format!("0x{:04X}", u16::from_le_bytes([body[0], body[1]]))),
                );
            }
            match cache.expand_compact(meter, body) {
                Ok(expanded) => {
                    obj.insert("expanded".into(), json!(true));
                    insert_records(obj, &expanded);
                }
                Err(e) => {
                    // Typically "no full frame seen yet" — report it rather than guess.
                    obj.insert("compact_error".into(), json!(e.to_string()));
                    obj.insert("payload_hex".into(), json!(hex::encode(body)));
                }
            }
        }
        _ => insert_records(obj, body),
    }
}

/// Decode every DIF/VIF record in `data`, keeping the raw payload for the A/B diff.
///
/// Uses `parse_variable_record_consumed`, which reports how many bytes each record
/// occupied — without that the loop cannot advance, which is why this used to decode
/// only the first record.
fn insert_records(obj: &mut serde_json::Map<String, Value>, data: &[u8]) {
    obj.insert("payload_hex".into(), json!(hex::encode(data)));
    let mut records = Vec::new();
    let mut offset = 0usize;
    while offset < data.len() {
        // 0x2F is the idle filler used to pad to a block boundary.
        if data[offset] == 0x2F {
            offset += 1;
            continue;
        }
        match parse_variable_record_consumed(&data[offset..]) {
            Ok((rec, used)) if used > 0 => {
                records.push(record_to_json(&rec));
                offset += used;
            }
            Ok(_) => break, // no progress: stop rather than spin
            Err(e) => {
                if records.is_empty() {
                    obj.insert("record_error".into(), json!(e.to_string()));
                } else {
                    obj.insert("record_trailing_error".into(), json!(e.to_string()));
                }
                break;
            }
        }
    }
    if !records.is_empty() {
        obj.insert("records".into(), json!(records));
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

    /// End-to-end ELL decrypt on a **synthetic** frame (published test key, built with
    /// an independent AES implementation — see `mbus-rs/tests/wmbus_frames/README.md`).
    /// Real meter keys are secrets and never enter this repository; the live path is
    /// verified on the gateway instead.
    #[test]
    fn decrypts_ell_frame_and_emits_records() {
        let raw =
            hex::decode("3d1b442d2c785634121b168d207e4523012038e54482ea11982e333309").unwrap();
        let mut keys = KeyStore::new();
        keys.install(12345678, "000102030405060708090a0b0c0d0e0f".to_string());
        let v = decode_frame(&raw, &empty_cfg(), &keys);

        assert_eq!(v["ci"], "0x8D");
        assert_eq!(v["ell"], true);
        assert_eq!(v["encrypted"], true);
        assert_eq!(v["decrypted"], true);
        assert_eq!(v["ell_cc"], "0x20");
        assert_eq!(v["tpl_ci"], "0x78", "recovered transport CI: full frame");
        let recs = v["records"].as_array().expect("records decoded");
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0]["vif"], "0x13", "volume");
        assert_eq!(recs[0]["value"], 1.0, "1000 l scaled to 1 m3");
    }

    /// Without a key the frame still parses and reports its ELL header; it simply is
    /// not decrypted. The ciphertext is retained for the A/B diff.
    #[test]
    fn ell_frame_without_key_reports_headers_only() {
        let raw =
            hex::decode("3d1b442d2c785634121b168d207e4523012038e54482ea11982e333309").unwrap();
        let v = decode_frame(&raw, &empty_cfg(), &KeyStore::new());
        assert_eq!(v["ell"], true);
        assert_eq!(v["encrypted"], true);
        assert_eq!(v["decrypt"], "no key for meter");
        assert!(v.get("decrypted").is_none());
        assert!(!v["ciphertext_hex"].as_str().unwrap().is_empty());
    }

    /// A wrong key must be reported as a failed decrypt, never as plausible records.
    #[test]
    fn wrong_key_reports_decrypt_error() {
        let raw =
            hex::decode("3d1b442d2c785634121b168d207e4523012038e54482ea11982e333309").unwrap();
        let mut keys = KeyStore::new();
        keys.install(12345678, "ffeeddccbbaa99887766554433221100".to_string());
        let v = decode_frame(&raw, &empty_cfg(), &keys);
        assert_eq!(v["decrypted"], false);
        assert!(v["decrypt_error"].is_string());
        assert!(v.get("records").is_none());
    }

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
