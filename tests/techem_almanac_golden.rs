//! End-to-end golden: a Techem fhkvdataiv HCA telegram decrypts and its storage-indexed
//! billing history ("almanac") decodes into distinct, dated periods.
//!
//! This exercises the whole chain the almanac decode depends on:
//!   1. OMS mode-5 (AES-128-CBC) decryption,
//!   2. the generic DIF/VIF record walk with **DIFE storage-number accumulation**
//!      (`record.rs`) — without it the storage-8 period would collapse onto storage 0,
//!   3. `techem::extract_oms_history` pairing each period's value + Type-G date.
//!
//! Vector + key are the `wmbusmeters` GPL reference telegram documented in
//! `docs/TECHEM_RESEARCH.md` (id 14542076); values validated independently here.
//! Clean-room: only format facts (offsets, DIF/VIF, Type-G date math) are used.
#![cfg(feature = "crypto")]

use mbus_rs::payload::record::{parse_variable_record_consumed, MBusRecordValue};
use mbus_rs::vendors::techem::extract_oms_history;
use mbus_rs::wmbus::crypto::AesKey;
use mbus_rs::wmbus::oms::decrypt_mode5_cbc;

fn hx(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

const FHKVDATAIV: &str = "4E4468507620541494087AAD004005089D86B62A329B3439873999738F82461ABDE3C7AC78692B363F3B41EB68607F9C9160F550769B065B6EA00A2E44346E29FF5DC5CB86283C69324AD33D137F6F";
const KEY: &str = "FCF41938F63432975B52505F547FCEDF";

/// Decrypt the telegram and return its plaintext record area (after the 2F2F idle-fill).
fn decrypt_record_area() -> Vec<u8> {
    let tg = hx(FHKVDATAIV);
    // Link frame: L C M M A A A A V T | CI ACC STATUS CFG CFG | ciphertext…
    let link_address: [u8; 8] = tg[2..10].try_into().unwrap();
    let acc = tg[11];
    let ct = &tg[15..];
    let key = AesKey::from_hex(KEY).unwrap();
    let pt = decrypt_mode5_cbc(ct, &link_address, acc, &key).unwrap();

    // Strip the leading OMS idle-fill (2F).
    let start = pt.iter().position(|&b| b != 0x2F).unwrap();
    pt[start..].to_vec()
}

fn parse_all(mut buf: &[u8]) -> Vec<mbus_rs::payload::record::MBusRecord> {
    let mut out = Vec::new();
    while !buf.is_empty() && buf[0] != 0x2F {
        match parse_variable_record_consumed(buf) {
            Ok((rec, used)) if used > 0 => {
                out.push(rec);
                buf = &buf[used..];
            }
            _ => break,
        }
    }
    out
}

#[test]
fn storage_numbers_separate_current_setdate_and_prior_period() {
    let records = parse_all(&decrypt_record_area());

    // Collect (storage, vif, value) for the numeric records.
    let mut current = None;
    let mut set_date_val = None;
    let mut prior_val = None;
    for r in &records {
        // HCA consumption readings are integer-coded, so they are `Scaled` now — read the
        // scalar via `as_f64` (small counts, no precision concern) and skip text records.
        if !matches!(r.value, MBusRecordValue::String(_)) {
            let v = r.value.as_f64();
            // VIF 0x6E = HCA units (the consumption reading).
            if r.drh.vib.vif & 0x7F == 0x6E {
                match r.storage_number {
                    0 => current = Some(v),
                    1 => set_date_val = Some(v),
                    8 => prior_val = Some(v),
                    _ => {}
                }
            }
        }
    }

    // The point of the DIFE fix: three HCA readings at three distinct storage numbers.
    // Before the fix, the storage-8 reading reported storage 0 and collided with current.
    assert_eq!(current, Some(2.0), "current reading (storage 0)");
    assert_eq!(set_date_val, Some(25.0), "set-date reading (storage 1)");
    assert_eq!(prior_val, Some(0.0), "prior-period reading (storage 8)");
}

#[test]
fn almanac_decodes_dated_billing_history() {
    let records = parse_all(&decrypt_record_area());
    let history = extract_oms_history(&records);

    assert_eq!(history.len(), 2, "two historical periods: {history:#?}");

    assert_eq!(history[0].storage, 1);
    assert_eq!(history[0].date.as_deref(), Some("2020-12-31"));
    assert_eq!(history[0].value, Some(25.0));

    assert_eq!(history[1].storage, 8);
    assert_eq!(history[1].date.as_deref(), Some("2019-10-31"));
    assert_eq!(history[1].value, Some(0.0));
}
