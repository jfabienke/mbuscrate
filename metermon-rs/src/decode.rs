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
use mbus_rs::payload::record::{parse_variable_record_in_context, MBusRecord, MBusRecordValue};
use mbus_rs::vendors::{DecodeContext, DeviceIdentity, Integrity, VendorRegistry};
use mbus_rs::wmbus::compact_frame::CompactLayoutCache;
use mbus_rs::wmbus::ell;
use mbus_rs::wmbus::frame_decode::FrameType;
use mbus_rs::wmbus::mode_c::decode_mode_c;
use mbus_rs::wmbus::oms;
use mbus_rs::wmbus::AesKey;
use serde_json::{json, Value};
use std::sync::OnceLock;

use crate::config::Config;
use crate::keystore::KeyStore;
use crate::profiles::ProfileStore;

/// Process-wide vendor registry (QUNDIS today, KAM when its interpretation lands).
/// Threading this through the decode context is what makes the crate's vendor layer
/// reachable at runtime — before this, no live path ever constructed a registry.
fn vendor_registry() -> &'static VendorRegistry {
    static REGISTRY: OnceLock<VendorRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| VendorRegistry::with_defaults().expect("default vendor registry"))
}

/// Decode one normalized wM-Bus frame into a JSON object. Never panics; failures are
/// reported as fields so the A/B diff stays aligned.
pub fn decode_frame(raw: &[u8], cfg: &Config, keys: &KeyStore) -> Value {
    // Stateless convenience wrapper: a throwaway cache cannot expand compact frames,
    // since that needs a full frame seen earlier, and no profiles are available.
    // Long-running callers should keep both and use [`decode_frame_with_cache`].
    decode_frame_with_cache(
        raw,
        cfg,
        keys,
        &mut CompactLayoutCache::new(),
        &ProfileStore::new(),
    )
}

/// Decode one frame, using (and updating) a compact-frame layout cache so compact
/// frames from a meter whose full frame was seen earlier decode to real records, and
/// consulting the backend-supplied profile store to resolve the device model
/// (vendor-layers §7.2).
pub fn decode_frame_with_cache(
    raw: &[u8],
    cfg: &Config,
    keys: &KeyStore,
    cache: &mut CompactLayoutCache,
    profiles: &ProfileStore,
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
    // Vendor binding for this frame (vendor-layers P6/P9): resolved once, and only
    // when the frame's CRCs validated — mode_c reports one aggregate flag today, so
    // a failure in ANY block conservatively blocks vendor dispatch, which errs on
    // exactly the safe side until per-region integrity is threaded out of mode_c.
    let ctx = DecodeContext::resolve(
        Some(vendor_registry()),
        DeviceIdentity {
            manufacturer: id_to_manufacturer(frame.manufacturer_id).to_string(),
            version: frame.version,
            device_type: frame.device_type,
            address: meterid,
            // The Device Manager's answer for this meter, when the gateway holds one
            // (migration step 4). Selects model-specific interpretation in the crate;
            // absent, decode falls back to raw for anything the frame cannot settle.
            profile: profiles.get(meterid).cloned(),
        },
        Integrity {
            header_valid: frame.crc_ok,
            payload_valid: frame.crc_ok,
        },
    );
    let frame_type = match frame.frame_type {
        FrameType::TypeA => "A",
        FrameType::TypeB => "B",
        FrameType::Unknown => "?",
    };
    let resolved_model = ctx.device.profile.as_ref().map(|p| p.model.clone());
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
    // Make the profile channel observable end to end (§7.2), even before anything
    // interprets the model (that is migration step 5).
    if let Some(model) = resolved_model {
        obj.insert("profile".into(), json!(model));
    }

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
            decode_transport(obj, ci, after_ci, cache, meterid, frame.crc_ok, &ctx);
        }
        // Short TPL header (OMS 7.2.4): ACC, STS, Configuration Word, then payload.
        0x7A => decode_tpl(obj, after_ci, 0, &frame, keys, meterid, &ctx),
        // Long TPL header: an 8-byte address repeat precedes ACC/STS/CW. Same body,
        // four bytes further in — the offset the crate's find_ci_offset never handled.
        0x72 => decode_tpl(obj, after_ci, 8, &frame, keys, meterid, &ctx),
        // Compact frame — records keyed by a format signature learned elsewhere.
        0x79 => {
            obj.insert("encrypted".into(), json!(false));
            decode_transport(obj, ci, after_ci, cache, meterid, frame.crc_ok, &ctx);
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
                    decode_transport(obj, tci, rest, cache, meterid, frame.crc_ok, &ctx);
                }
                return out;
            }
            let mfr = id_to_manufacturer(frame.manufacturer_id);
            let Some((hexkey, key_source)) = keys.resolve(meterid, &mfr) else {
                obj.insert("ciphertext_hex".into(), json!(hex::encode(after_ci)));
                obj.insert("decrypt".into(), json!("no key for meter"));
                return out;
            };
            // Record which key opened it: a reading that only decrypts under a
            // published factory default says the device was never given a unique
            // one, which an operator should be able to see rather than infer.
            obj.insert("key_source".into(), json!(key_source.as_str()));
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
                        decode_transport(obj, tci, rest, cache, meterid, frame.crc_ok, &ctx);
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

/// Decode an OMS TPL-header payload (short CI 0x7A or long CI 0x72), decrypting a
/// mode-5 body when a key is held.
///
/// `addr_prefix` is the length of the address repeat before ACC — 0 for the short
/// header, 8 for the long. The IV always uses the *link-layer* address
/// (`frame.link_header`), not the TPL repeat, matching the epulse reference.
#[allow(clippy::too_many_arguments)]
fn decode_tpl(
    obj: &mut serde_json::Map<String, Value>,
    after_ci: &[u8],
    addr_prefix: usize,
    frame: &mbus_rs::wmbus::mode_c::WMBusLinkFrame,
    keys: &KeyStore,
    meterid: u32,
    ctx: &DecodeContext,
) {
    // ACC, STS, CW(2) follow the optional address prefix.
    if after_ci.len() < addr_prefix + 4 {
        obj.insert("error".into(), json!("TPL header truncated"));
        return;
    }
    let acc = after_ci[addr_prefix];
    let sts = after_ci[addr_prefix + 1];
    let cw = u16::from_le_bytes([after_ci[addr_prefix + 2], after_ci[addr_prefix + 3]]);
    let mode = (cw >> 8) & 0x1F; // mode is in the config word, never the CI byte
    obj.insert("status".into(), json!(sts));
    obj.insert("mode".into(), json!(mode));
    let body = &after_ci[addr_prefix + 4..];

    if mode == 0 {
        obj.insert("encrypted".into(), json!(false));
        insert_records(obj, body, ctx);
        return;
    }
    obj.insert("encrypted".into(), json!(true));
    if mode != 5 {
        // Only Security Profile A (mode 5) is implemented; 7/9/13 would go here.
        obj.insert("ciphertext_hex".into(), json!(hex::encode(body)));
        obj.insert("decrypt".into(), json!(format!("unsupported mode {mode}")));
        return;
    }

    // The config word's block count bounds the ciphertext; trailing bytes, if any,
    // are unencrypted. Fall back to the whole body rounded to a block boundary.
    let num_blocks = ((cw >> 4) & 0x0F) as usize;
    let enc_len = if num_blocks > 0 {
        (num_blocks * 16).min(body.len())
    } else {
        body.len() - (body.len() % 16)
    };
    let ciphertext = &body[..enc_len];

    let mfr = id_to_manufacturer(frame.manufacturer_id);
    let Some((hexkey, key_source)) = keys.resolve(meterid, &mfr) else {
        obj.insert("ciphertext_hex".into(), json!(hex::encode(ciphertext)));
        obj.insert("decrypt".into(), json!("no key for meter"));
        return;
    };
    let key = match AesKey::from_hex(hexkey) {
        Ok(k) => k,
        Err(e) => {
            obj.insert("decrypt".into(), json!(format!("bad key: {e}")));
            return;
        }
    };
    match oms::decrypt_mode5_cbc(ciphertext, &frame.link_header, acc, &key) {
        Ok(plain) if oms::decrypted_ok(&plain) => {
            obj.insert("decrypted".into(), json!(true));
            obj.insert("key_source".into(), json!(key_source.as_str()));
            // A frame that only opens under a published factory default is a device
            // that was deployed but never personalised — worth flagging plainly, not
            // burying, so an operator can find exposed meters.
            if key_source == crate::keystore::KeySource::FactoryDefault {
                obj.insert(
                    "provisioning".into(),
                    json!("UNPROVISIONED: factory-default key"),
                );
            }
            insert_records(obj, &plain, ctx);
        }
        Ok(_) => {
            // Decrypted cleanly but no idle-fill marker: the key was wrong. Never
            // surfaced as records — a wrong-key plaintext is plausible garbage.
            obj.insert("decrypted".into(), json!(false));
            obj.insert("ciphertext_hex".into(), json!(hex::encode(ciphertext)));
            obj.insert(
                "decrypt".into(),
                json!(format!(
                    "wrong key ({}): no 2F 2F marker",
                    key_source.as_str()
                )),
            );
        }
        Err(e) => {
            obj.insert("decrypted".into(), json!(false));
            obj.insert("ciphertext_hex".into(), json!(hex::encode(ciphertext)));
            obj.insert("decrypt_error".into(), json!(e.to_string()));
        }
    }
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
    ctx: &DecodeContext,
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
            insert_records(obj, body, ctx);
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
                    insert_records(obj, &expanded, ctx);
                }
                Err(e) => {
                    // Typically "no full frame seen yet" — report it rather than guess.
                    obj.insert("compact_error".into(), json!(e.to_string()));
                    obj.insert("payload_hex".into(), json!(hex::encode(body)));
                }
            }
        }
        _ => insert_records(obj, body, ctx),
    }
}

/// Decode every DIF/VIF record in `data`, keeping the raw payload for the A/B diff.
///
/// Uses `parse_variable_record_consumed`, which reports how many bytes each record
/// occupied — without that the loop cannot advance, which is why this used to decode
/// only the first record.
fn insert_records(obj: &mut serde_json::Map<String, Value>, data: &[u8], ctx: &DecodeContext) {
    obj.insert("payload_hex".into(), json!(hex::encode(data)));
    let mut records = Vec::new();
    let mut frame_quirks: Vec<&'static str> = Vec::new();
    let mut offset = 0usize;
    while offset < data.len() {
        // 0x2F is the idle filler used to pad to a block boundary.
        if data[offset] == 0x2F {
            offset += 1;
            continue;
        }
        match parse_variable_record_in_context(&data[offset..], ctx) {
            Ok((rec, used)) if used > 0 => {
                for q in &rec.applied_quirks {
                    if !frame_quirks.contains(&q.quirk_id) {
                        frame_quirks.push(q.quirk_id);
                    }
                }
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
    // Frame-level union of fired quirks (P5): what the device store persists, and
    // what a downstream consumer checks without walking every record.
    if !frame_quirks.is_empty() {
        obj.insert("applied_quirks".into(), json!(frame_quirks));
    }
}

fn record_to_json(rec: &MBusRecord) -> Value {
    let raw_hex = hex::encode(&rec.data[..rec.data_len]);
    let mut obj = json!({
        "dif": format!("0x{:02X}", rec.drh.dib.dif),
        "vif": format!("0x{:02X}", rec.drh.vib.vif),
        "raw_hex": raw_hex,
    });

    // An unknown VIF means we do not know the quantity, the unit, or even how to
    // scale the bytes — so any "value" would be fabricated. This is precisely how
    // the Zenner vendor field (DIF 0x55 / VIF 0xE7 chain) was reported as a
    // nonsense 6.29e26 IEEE float: the data-field said "32-bit real", the VIF was
    // unrecognised, and we decoded and emitted it anyway. Present the raw bytes and
    // say plainly that the VIF is unknown, rather than dressing them as a reading.
    if rec.quantity.is_empty() {
        obj["unknown_vif"] = json!(true);
        // A vendor field pending identification — the honest state until Zenner
        // documentation or a non-zero capture pins its meaning (Evidence::Inferred).
        if !rec.applied_quirks.is_empty() {
            obj["quirks"] = json!(rec
                .applied_quirks
                .iter()
                .map(|q| q.quirk_id)
                .collect::<Vec<_>>());
        }
        return obj;
    }

    // Known VIF. A String value from a variable-length (LVAR) block is opaque
    // vendor content, not text, unless it is actually printable — the 44-byte
    // Zenner volume-history block otherwise renders as reversed mojibake. Keep the
    // quantity/unit the VIF gives us, but let raw_hex carry the bytes.
    match &rec.value {
        MBusRecordValue::Numeric(n) => {
            obj["value"] = json!(n);
        }
        MBusRecordValue::String(txt) => {
            if txt.bytes().all(|b| (0x20..0x7F).contains(&b)) {
                obj["value"] = json!(txt);
            } else {
                obj["value_opaque"] = json!(true);
            }
        }
    }
    obj["unit"] = json!(rec.unit);
    obj["quantity"] = json!(rec.quantity);
    // A quirk-overridden reading is not the same fact as a standard one (P5): name
    // the quirk so two gateways that disagree can be audited.
    if !rec.applied_quirks.is_empty() {
        obj["quirks"] = json!(rec
            .applied_quirks
            .iter()
            .map(|q| q.quirk_id)
            .collect::<Vec<_>>());
        if rec.applied_quirks.iter().any(|q| q.provisional) {
            obj["quirk_provisional"] = json!(true);
        }
    }
    obj
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Step 4 observable end to end: with a backend-supplied profile installed, the
    /// decode carries the resolved model — the crate's DeviceIdentity.profile seam is
    /// fed, even though nothing interprets it yet (that is step 5).
    #[test]
    fn resolved_profile_appears_in_decode_output() {
        use mbus_rs::vendors::DeviceProfile;
        let raw = hex::decode("3d12442d2c785634121b16780413e80300009a1f").unwrap();
        let mut profiles = ProfileStore::new();
        profiles.install(
            12345678,
            DeviceProfile {
                model: "MULTICAL 21".into(),
                firmware: None,
            },
        );
        let v = decode_frame_with_cache(
            &raw,
            &empty_cfg(),
            &KeyStore::new(),
            &mut mbus_rs::wmbus::compact_frame::CompactLayoutCache::new(),
            &profiles,
        );
        assert_eq!(v["profile"], "MULTICAL 21");
        // Without a profile the field is absent, not invented.
        let v2 = decode_frame(&raw, &empty_cfg(), &KeyStore::new());
        assert!(v2.get("profile").is_none());
    }

    /// The vendor layer is live: a QUNDIS frame decoded through the ordinary path
    /// gets its repurposed VIF 0x04 interpreted as a date, because decode_frame now
    /// resolves a DecodeContext (registry + identity + integrity) per frame. Before
    /// migration step 2 no runtime path constructed a registry at all.
    #[test]
    fn qundis_date_decodes_through_the_live_path() {
        // Synthetic mode-C type B: QDS (0x4493), id 12345678, HCA, CI 0x78,
        // record DIF 02 VIF 04 data F8 10 (QUNDIS-encoded December 2015).
        let raw = hex::decode("3d10449344785634120108780204f810a407").unwrap();
        let v = decode_frame(&raw, &empty_cfg(), &KeyStore::new());
        assert_eq!(v["manufacturer"], "QDS");
        assert_eq!(v["crc_ok"], true);
        let recs = v["records"].as_array().expect("records decoded");
        let date = recs[0]["value"].as_str().expect("QUNDIS date is a string");
        assert!(date.contains("2015") && date.contains("12"), "got {date}");
        assert_eq!(recs[0]["unit"], "Date");
        // P5: the overriding quirk is named per record and at frame level.
        assert_eq!(recs[0]["quirks"][0], "qds-vif04-date");
        assert_eq!(v["applied_quirks"][0], "qds-vif04-date");
    }

    /// P6 end to end: the same QUNDIS frame with a corrupted CRC decodes with NO
    /// vendor interpretation — a corrupt manufacturer code must never select vendor
    /// code, so the record falls back to the standard (energy) reading.
    #[test]
    fn corrupt_qundis_frame_gets_no_vendor_interpretation() {
        let mut raw = hex::decode("3d10449344785634120108780204f810a407").unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 0xFF; // break the CRC
        let v = decode_frame(&raw, &empty_cfg(), &KeyStore::new());
        assert_eq!(v["crc_ok"], false);
        let recs = v["records"]
            .as_array()
            .expect("records still parse (A/B diff)");
        assert!(
            recs[0]["value"].is_number(),
            "vendor date interpretation must not fire on a corrupt frame: {:?}",
            recs[0]
        );
    }

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

    /// A record whose VIF we cannot resolve must present raw bytes and say so —
    /// never a fabricated scalar. This is the defect the Zenner vendor field
    /// exposed: DIF 0x55 (data field 5 = "32-bit real") with an unrecognised VIF
    /// chain was decoded as an IEEE float and published as a nonsense 6.29e26.
    #[test]
    fn unknown_vif_record_is_raw_not_a_fabricated_reading() {
        use mbus_rs::payload::record::parse_variable_record_consumed;
        // DIF 0x05 (32-bit real), VIF chain 0xE7 0xC6 0x40 (unknown), 4 data bytes.
        let (rec, _) =
            parse_variable_record_consumed(&[0x05, 0xE7, 0xC6, 0x40, 0x11, 0x22, 0x33, 0x44])
                .unwrap();
        let j = record_to_json(&rec);
        assert_eq!(j["unknown_vif"], json!(true));
        assert_eq!(j["raw_hex"], json!("11223344"));
        assert!(
            j.get("value").is_none(),
            "an unknown VIF must not emit a decoded value"
        );

        // A known VIF (volume, 8-digit BCD) still decodes to a real reading.
        let (vol, _) =
            parse_variable_record_consumed(&[0x0C, 0x13, 0x34, 0x12, 0x00, 0x00]).unwrap();
        let jv = record_to_json(&vol);
        assert_eq!(jv["quantity"], json!("Volume"));
        assert!(jv.get("value").is_some());
        assert!(jv.get("unknown_vif").is_none());
    }
}
