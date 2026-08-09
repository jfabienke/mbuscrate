//! Gateway-side `op:observed` up-sync payload builder. See
//! docs/OP_OBSERVED_UPSYNC.md for the wire schema and ownership model.
//!
//! Pure: it turns device-store records + the current GPS fix into the message the
//! gateway publishes. The I/O — reading the watermark, publishing, advancing on
//! confirmation — lives in the monitor loop; keeping the build pure makes the
//! schema unit-testable without a broker or a radio.

use serde_json::{json, Value};

use crate::devices::ObservedRecord;
use crate::gps::GpsFix;

/// Build one `op:observed` message from records changed since the watermark.
///
/// Returns `(message, new_hwm)`. `new_hwm` is the max `last_seen` in the batch
/// (never below `prev_hwm`); the caller persists it via
/// `DeviceStore::advance_observed` **only after** the sync is confirmed, so an
/// unacked batch re-sends rather than being lost.
///
/// Carries gateway-owned observation fields only — never keys or backend-owned
/// profile fields. `gw_pos` is included only when the fix is valid.
pub fn build_observed(
    gw_id: &str,
    gw_fix: Option<&GpsFix>,
    records: &[ObservedRecord],
    seq: u64,
    prev_hwm: i64,
    ts: i64,
) -> (Value, i64) {
    let new_hwm = records
        .iter()
        .map(|r| r.last_seen)
        .max()
        .unwrap_or(prev_hwm)
        .max(prev_hwm);

    let meters: Vec<Value> = records
        .iter()
        .map(|r| {
            json!({
                "serial": r.serial,
                "mfr": r.mfr,
                "device_type": r.device_type,
                "mode": r.mode,
                "encrypted": r.encrypted,
                "has_key": r.has_key,
                "silent": r.silent,
                "first_seen": r.first_seen,
                "last_seen": r.last_seen,
                "frames_ok": r.frames_ok,
                "frames_total": r.frames_total,
                "last_rssi": r.last_rssi,
            })
        })
        .collect();

    let mut msg = json!({
        "op": "observed",
        "ts": ts,
        "gw": gw_id,
        "seq": seq,
        "meters": meters,
    });
    if let Some(f) = gw_fix.filter(|f| f.valid) {
        msg["gw_pos"] = json!({ "lat": f.lat, "lon": f.lon, "hdop": f.hdop, "fix_ts": f.ts });
    }
    (msg, new_hwm)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(serial: u32, last_seen: i64, rssi: i16) -> ObservedRecord {
        ObservedRecord {
            serial,
            mfr: "KAM".into(),
            device_type: 4,
            mode: "C".into(),
            encrypted: true,
            has_key: false,
            silent: false,
            first_seen: 1,
            last_seen,
            frames_ok: 10,
            frames_total: 12,
            last_rssi: rssi,
        }
    }

    #[test]
    fn builds_message_and_advances_watermark_to_max_last_seen() {
        let recs = [rec(1, 100, -60), rec(2, 250, -70), rec(3, 180, -80)];
        let (msg, new_hwm) = build_observed("6543", None, &recs, 7, 50, 1_000);
        assert_eq!(msg["op"], "observed");
        assert_eq!(msg["gw"], "6543");
        assert_eq!(msg["seq"], 7);
        assert_eq!(msg["meters"].as_array().unwrap().len(), 3);
        assert_eq!(new_hwm, 250); // max last_seen in the batch
        assert!(msg.get("gw_pos").is_none()); // no fix supplied
    }

    #[test]
    fn empty_batch_keeps_the_previous_watermark() {
        let (_msg, new_hwm) = build_observed("6543", None, &[], 1, 999, 1_000);
        assert_eq!(new_hwm, 999);
    }

    #[test]
    fn watermark_never_regresses_below_prev() {
        // A record older than prev_hwm shouldn't drag the mark backwards.
        let recs = [rec(1, 40, -60)];
        let (_m, new_hwm) = build_observed("6543", None, &recs, 1, 100, 1_000);
        assert_eq!(new_hwm, 100);
    }

    #[test]
    fn includes_gw_pos_only_for_a_valid_fix() {
        let fix = GpsFix { lat: 56.16, lon: 10.20, alt_m: Some(42.0), hdop: 0.8, ts: 5, valid: true };
        let (msg, _) = build_observed("6543", Some(&fix), &[rec(1, 100, -60)], 1, 0, 1_000);
        assert_eq!(msg["gw_pos"]["lat"], 56.16);
        assert_eq!(msg["gw_pos"]["fix_ts"], 5);

        let invalid = GpsFix { valid: false, ..fix };
        let (msg2, _) = build_observed("6543", Some(&invalid), &[rec(1, 100, -60)], 1, 0, 1_000);
        assert!(msg2.get("gw_pos").is_none());
    }

    #[test]
    fn payload_carries_no_key_or_profile_fields() {
        let (msg, _) = build_observed("6543", None, &[rec(1, 100, -60)], 1, 0, 1_000);
        let m = &msg["meters"][0];
        assert!(m.get("key").is_none());
        assert!(m.get("model").is_none());
        assert!(m.get("firmware").is_none());
    }
}
