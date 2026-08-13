//! GPS fix source for the gateway — a minimal gpsd client.
//!
//! Talks to a local `gpsd` (default `127.0.0.1:2947`) rather than parsing NMEA
//! directly: gpsd owns the serial device, the NMEA quirks, and the PPS handshake,
//! and exposes clean line-delimited JSON. We consume `TPV` (time-position-velocity)
//! reports for position, and `SKY` reports for the satellite count, keeping the
//! latest trustworthy 2D/3D fix in a shared handle the publisher reads.
//!
//! Time: with PPS the system clock is GPS-disciplined, so we stamp each fix with
//! the system clock (`now_unix`) rather than parsing gpsd's ISO-8601 `time` — one
//! source of truth for time across the gateway.
//!
//! Fix loss is tracked, not ignored: a TPV reporting `mode < 2` marks the held fix
//! invalid (coordinates are kept for the log, but `valid` drops), so a gateway
//! whose antenna dies stops publishing its position instead of freezing the last
//! one forever. The socket carries a read timeout for the same reason — a hung
//! gpsd must look like a reconnect, not a silently stale fix.
//!
//! `mode >= 2` alone is NOT trusted: some receivers (the CASIC AT6558R on the
//! Waveshare SX1262+GNSS HAT, for one) assert a `mode: 3` TPV at a hard-coded
//! **default position** (30.0N/120.0E) with **zero satellites** before they have
//! ever locked. A fix is only marked valid once gpsd's most recent `SKY` reports at
//! least [`MIN_SATS_FOR_FIX`] satellites used in the solution — a real fix cannot
//! exist without them, so this rejects the phantom default without special-casing
//! any one vendor's magic coordinate.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use crate::devices::now_unix;

/// gpsd emits TPV at ~1 Hz while watching; this much silence means the link or
/// the daemon is wedged and the connection should be rebuilt.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Minimum satellites used in the solution for a `mode >= 2` TPV to be trusted as a
/// real fix. Three is the floor for any genuine 2D fix; a "fix" backed by fewer is
/// a receiver default/placeholder (see the module docs) and is not published.
pub const MIN_SATS_FOR_FIX: u32 = 3;

/// A position fix. `valid` is false until gpsd reports a 2D/3D fix, and drops
/// back to false when gpsd reports the fix lost (`mode < 2`).
#[derive(Debug, Clone, PartialEq)]
pub struct GpsFix {
    pub lat: f64,
    pub lon: f64,
    pub alt_m: Option<f64>,
    /// Estimated position error in metres (gpsd `eph`), when the receiver
    /// reports one — the directly useful accuracy figure.
    pub eph_m: Option<f64>,
    /// Horizontal dilution of precision. gpsd carries HDOP on `SKY` reports, not
    /// `TPV`, so this is usually absent — `None` means unknown, never 0.0.
    pub hdop: Option<f64>,
    pub ts: i64,
    pub valid: bool,
}

/// One gpsd `TPV` report, as far as position is concerned.
#[derive(Debug, Clone, PartialEq)]
pub enum TpvReport {
    /// A 2D/3D fix with coordinates.
    Fix(GpsFix),
    /// A TPV explicitly reporting no fix (`mode < 2`) — the held fix is stale.
    NoFix,
}

/// Shared latest fix. Cloneable handle; the reader thread writes, the publisher reads.
#[derive(Clone, Default)]
pub struct GpsHandle(Arc<Mutex<Option<GpsFix>>>);

impl GpsHandle {
    pub fn latest(&self) -> Option<GpsFix> {
        self.0.lock().ok().and_then(|g| g.clone())
    }
    fn set(&self, fix: GpsFix) {
        if let Ok(mut g) = self.0.lock() {
            *g = Some(fix);
        }
    }
    /// Mark the held fix stale (gpsd reported `mode < 2`). Coordinates stay for
    /// the log; `valid` drops so the publisher stops emitting `gw_pos`.
    fn invalidate(&self) {
        if let Ok(mut g) = self.0.lock() {
            if let Some(f) = g.as_mut() {
                f.valid = false;
            }
        }
    }
}

/// Parse one gpsd JSON line. `None` for anything that is not a `TPV` report (or a
/// malformed one); `Some(TpvReport::NoFix)` for a TPV without a 2D/3D fix;
/// `Some(TpvReport::Fix(..))` with `ts` left 0 for the caller to stamp from the
/// (GPS-disciplined) system clock.
pub fn parse_gpsd_tpv(line: &str) -> Option<TpvReport> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("class").and_then(|c| c.as_str()) != Some("TPV") {
        return None;
    }
    // mode: 0/1 = no fix, 2 = 2D, 3 = 3D.
    if v.get("mode").and_then(|m| m.as_u64()).unwrap_or(0) < 2 {
        return Some(TpvReport::NoFix);
    }
    // A 2D/3D TPV without coordinates is malformed; ignore it rather than
    // invalidating a good fix over it.
    Some(TpvReport::Fix(GpsFix {
        lat: v.get("lat").and_then(|x| x.as_f64())?,
        lon: v.get("lon").and_then(|x| x.as_f64())?,
        alt_m: v.get("alt").and_then(|x| x.as_f64()),
        eph_m: v.get("eph").and_then(|x| x.as_f64()),
        hdop: v.get("hdop").and_then(|x| x.as_f64()),
        ts: 0,
        valid: true,
    }))
}

/// Parse a gpsd `SKY` line into the number of satellites **used in the solution**.
/// Returns `None` for anything that is not a usable `SKY` report, so a malformed or
/// summary-less line leaves the last known count untouched rather than resetting it.
///
/// Counts the `used: true` entries in the `satellites` array (present across gpsd
/// versions); falls back to the `uSat` summary field when the array is absent.
pub fn parse_gpsd_sky(line: &str) -> Option<u32> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("class").and_then(|c| c.as_str()) != Some("SKY") {
        return None;
    }
    if let Some(sats) = v.get("satellites").and_then(|s| s.as_array()) {
        let used = sats
            .iter()
            .filter(|s| s.get("used").and_then(|u| u.as_bool()).unwrap_or(false))
            .count();
        return Some(used as u32);
    }
    v.get("uSat").and_then(|u| u.as_u64()).map(|n| n as u32)
}

/// Whether a just-parsed `mode >= 2` TPV fix should be trusted, given how many
/// satellites gpsd's most recent `SKY` reported in the solution. Rejects the
/// zero-satellite default fixes described in the module docs.
pub fn fix_valid_with_sats(sats_used: u32) -> bool {
    sats_used >= MIN_SATS_FOR_FIX
}

/// Connect to gpsd and stream fixes into a fresh [`GpsHandle`] on a background
/// thread. The thread reconnects on error and never blocks the gateway. Returns the
/// handle immediately (initially empty).
pub fn spawn(addr: &str) -> GpsHandle {
    let handle = GpsHandle::default();
    let out = handle.clone();
    let addr = addr.to_string();
    std::thread::spawn(move || loop {
        match read_loop(&addr, &handle) {
            Ok(()) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                log::warn!(
                    "gpsd {addr}: silent for {}s; reconnecting",
                    READ_TIMEOUT.as_secs()
                );
            }
            Err(e) => log::warn!("gpsd {addr}: {e}; reconnecting"),
        }
        std::thread::sleep(std::time::Duration::from_secs(5));
    });
    out
}

fn read_loop(addr: &str, handle: &GpsHandle) -> std::io::Result<()> {
    let stream = TcpStream::connect(addr)?;
    // A wedged gpsd holding the connection open must surface as an error (and a
    // reconnect), not as an eternally-blocked read serving a stale fix.
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    let mut stream = stream;
    // Enable JSON streaming (gpsd stays silent until asked).
    stream.write_all(b"?WATCH={\"enable\":true,\"json\":true};\n")?;
    let reader = BufReader::new(stream);
    // Latest satellites-used count from gpsd SKY reports; gates TPV fixes so a
    // zero-satellite default position (see module docs) is never published.
    let mut sats_used: u32 = 0;
    // Whether we have already logged the current run of satellite-gated rejections,
    // so a sustained phantom fix warns once rather than at the ~1 Hz TPV rate.
    let mut gate_logged = false;
    for line in reader.lines() {
        let line = line?;
        if let Some(used) = parse_gpsd_sky(&line) {
            sats_used = used;
            continue;
        }
        match parse_gpsd_tpv(&line) {
            Some(TpvReport::Fix(mut fix)) if fix_valid_with_sats(sats_used) => {
                fix.ts = now_unix();
                gate_logged = false;
                handle.set(fix);
            }
            Some(TpvReport::Fix(fix)) => {
                // mode >= 2 but not backed by real satellites (e.g. the CASIC
                // AT6558R default 30N/120E with 0 sats). Drop validity via
                // invalidate() — same as a mode<2 loss — so the last known-good
                // position is preserved for the log rather than overwritten with the
                // phantom coordinates. Log the reason once so a genuinely gated lock
                // is diagnosable and not mistaken for dead hardware.
                if !gate_logged {
                    log::warn!(
                        "gpsd: ignoring mode>=2 fix at {:.5},{:.5} — {} satellite(s) used (< {}); receiver default, not a real lock",
                        fix.lat,
                        fix.lon,
                        sats_used,
                        MIN_SATS_FOR_FIX
                    );
                    gate_logged = true;
                }
                handle.invalidate();
            }
            Some(TpvReport::NoFix) => {
                gate_logged = false;
                handle.invalidate();
            }
            None => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(report: Option<TpvReport>) -> GpsFix {
        match report {
            Some(TpvReport::Fix(f)) => f,
            other => panic!("expected a fix, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_3d_tpv_fix_with_eph() {
        // Realistic TPV: carries eph, not hdop (hdop rides on SKY reports).
        let line = r#"{"class":"TPV","mode":3,"lat":56.1629,"lon":10.2039,"alt":42.0,"eph":3.5}"#;
        let f = fix(parse_gpsd_tpv(line));
        assert!(f.valid);
        assert_eq!(f.lat, 56.1629);
        assert_eq!(f.lon, 10.2039);
        assert_eq!(f.alt_m, Some(42.0));
        assert_eq!(f.eph_m, Some(3.5));
        assert_eq!(f.hdop, None, "absent hdop must be None, never 0.0");
    }

    #[test]
    fn carries_hdop_only_when_the_tpv_has_it() {
        let line = r#"{"class":"TPV","mode":2,"lat":1.0,"lon":2.0,"hdop":0.8}"#;
        assert_eq!(fix(parse_gpsd_tpv(line)).hdop, Some(0.8));
    }

    #[test]
    fn distinguishes_no_fix_from_irrelevant_lines() {
        // mode 1 = fix lost: an explicit NoFix, so the held fix can be invalidated.
        assert_eq!(
            parse_gpsd_tpv(r#"{"class":"TPV","mode":1,"lat":1.0,"lon":2.0}"#),
            Some(TpvReport::NoFix)
        );
        // SKY report, not TPV
        assert!(parse_gpsd_tpv(r#"{"class":"SKY","hdop":0.9}"#).is_none());
        // garbage
        assert!(parse_gpsd_tpv("not json").is_none());
        // 3D TPV without coordinates is malformed — ignored, NOT a NoFix, so a
        // glitched report cannot knock out a good fix.
        assert!(parse_gpsd_tpv(r#"{"class":"TPV","mode":3}"#).is_none());
    }

    #[test]
    fn handle_round_trips_latest_fix() {
        let h = GpsHandle::default();
        assert!(h.latest().is_none());
        h.set(GpsFix {
            lat: 1.0,
            lon: 2.0,
            alt_m: None,
            eph_m: None,
            hdop: None,
            ts: 5,
            valid: true,
        });
        assert_eq!(h.latest().unwrap().lat, 1.0);
    }

    #[test]
    fn lost_fix_invalidates_but_keeps_last_position() {
        let h = GpsHandle::default();
        h.invalidate(); // no fix held: a no-op, not a panic
        h.set(GpsFix {
            lat: 56.16,
            lon: 10.20,
            alt_m: None,
            eph_m: Some(3.0),
            hdop: None,
            ts: 100,
            valid: true,
        });
        h.invalidate();
        let f = h.latest().unwrap();
        assert!(!f.valid, "a mode<2 TPV must mark the fix stale");
        assert_eq!(f.lat, 56.16, "last-known position stays for the log");
        assert_eq!(f.ts, 100, "fix_ts keeps the time of the last real fix");
    }

    #[test]
    fn sky_reports_satellites_used_from_the_array() {
        // Three of four satellites flagged used in the solution.
        let line = r#"{"class":"SKY","satellites":[
            {"PRN":1,"used":true},{"PRN":2,"used":true},
            {"PRN":3,"used":false},{"PRN":4,"used":true}]}"#;
        assert_eq!(parse_gpsd_sky(line), Some(3));
    }

    #[test]
    fn sky_falls_back_to_usat_without_an_array() {
        assert_eq!(
            parse_gpsd_sky(r#"{"class":"SKY","uSat":7,"nSat":11}"#),
            Some(7)
        );
        // A SKY with no countable info leaves the count untouched (None), rather
        // than resetting it to zero and knocking out a good fix.
        assert_eq!(parse_gpsd_sky(r#"{"class":"SKY"}"#), None);
        // Not a SKY line.
        assert!(parse_gpsd_sky(r#"{"class":"TPV","mode":3}"#).is_none());
    }

    #[test]
    fn satellite_gate_rejects_below_the_floor() {
        // The AT6558R default fix carries zero satellites; 1 and 2 cannot yield a
        // real 2D fix either. Three is the floor.
        assert!(!fix_valid_with_sats(0));
        assert!(!fix_valid_with_sats(2));
        assert!(fix_valid_with_sats(3));
        assert!(fix_valid_with_sats(9));
    }

    #[test]
    fn the_at6558r_default_fix_is_a_valid_tpv_but_fails_the_satellite_gate() {
        // gpsd relays the receiver's phantom mode-3 fix at 30.0N/120.0E. It parses
        // as a well-formed TPV Fix (so parse_gpsd_tpv alone would trust it)...
        let line = r#"{"class":"TPV","mode":3,"lat":30.000000029,"lon":119.999999994,"eph":188.0}"#;
        let f = fix(parse_gpsd_tpv(line));
        assert!(f.valid, "the TPV itself is well-formed");
        // ...but with gpsd's SKY reporting zero satellites used, the gate rejects it.
        let sats = parse_gpsd_sky(r#"{"class":"SKY","satellites":[]}"#).unwrap();
        assert_eq!(sats, 0);
        assert!(
            !fix_valid_with_sats(sats),
            "a zero-satellite default position must not be published"
        );
    }
}
