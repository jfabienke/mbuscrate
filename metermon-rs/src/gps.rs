//! GPS fix source for the gateway — a minimal gpsd client.
//!
//! Talks to a local `gpsd` (default `127.0.0.1:2947`) rather than parsing NMEA
//! directly: gpsd owns the serial device, the NMEA quirks, and the PPS handshake,
//! and exposes clean line-delimited JSON. We consume `TPV` (time-position-velocity)
//! reports and keep the latest 2D/3D fix in a shared handle the publisher reads.
//!
//! Time: with PPS the system clock is GPS-disciplined, so we stamp each fix with
//! the system clock (`now_unix`) rather than parsing gpsd's ISO-8601 `time` — one
//! source of truth for time across the gateway.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::devices::now_unix;

/// A position fix. `valid` is false until gpsd reports a 2D/3D fix.
#[derive(Debug, Clone, PartialEq)]
pub struct GpsFix {
    pub lat: f64,
    pub lon: f64,
    pub alt_m: Option<f64>,
    pub hdop: f64,
    pub ts: i64,
    pub valid: bool,
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
}

/// Parse one gpsd JSON line into a fix. Returns `Some` only for a `TPV` report with
/// a 2D/3D mode and coordinates; `ts` is left 0 for the caller to stamp from the
/// (GPS-disciplined) system clock.
pub fn parse_gpsd_tpv(line: &str) -> Option<GpsFix> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("class").and_then(|c| c.as_str()) != Some("TPV") {
        return None;
    }
    // mode: 0/1 = no fix, 2 = 2D, 3 = 3D.
    if v.get("mode").and_then(|m| m.as_u64()).unwrap_or(0) < 2 {
        return None;
    }
    Some(GpsFix {
        lat: v.get("lat").and_then(|x| x.as_f64())?,
        lon: v.get("lon").and_then(|x| x.as_f64())?,
        alt_m: v.get("alt").and_then(|x| x.as_f64()),
        // hdop rides on gpsd SKY reports, not TPV; leave 0 unless TPV carries it.
        hdop: v.get("hdop").and_then(|x| x.as_f64()).unwrap_or(0.0),
        ts: 0,
        valid: true,
    })
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
            Err(e) => log::warn!("gpsd {addr}: {e}; reconnecting"),
        }
        std::thread::sleep(std::time::Duration::from_secs(5));
    });
    out
}

fn read_loop(addr: &str, handle: &GpsHandle) -> std::io::Result<()> {
    let mut stream = TcpStream::connect(addr)?;
    // Enable JSON streaming (gpsd stays silent until asked).
    stream.write_all(b"?WATCH={\"enable\":true,\"json\":true};\n")?;
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let line = line?;
        if let Some(mut fix) = parse_gpsd_tpv(&line) {
            fix.ts = now_unix();
            handle.set(fix);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_3d_tpv_fix() {
        let line = r#"{"class":"TPV","mode":3,"lat":56.1629,"lon":10.2039,"alt":42.0,"hdop":0.8}"#;
        let f = parse_gpsd_tpv(line).unwrap();
        assert!(f.valid);
        assert_eq!(f.lat, 56.1629);
        assert_eq!(f.lon, 10.2039);
        assert_eq!(f.alt_m, Some(42.0));
        assert_eq!(f.hdop, 0.8);
    }

    #[test]
    fn rejects_no_fix_and_non_tpv() {
        // mode 1 = no fix
        assert!(parse_gpsd_tpv(r#"{"class":"TPV","mode":1,"lat":1.0,"lon":2.0}"#).is_none());
        // SKY report, not TPV
        assert!(parse_gpsd_tpv(r#"{"class":"SKY","hdop":0.9}"#).is_none());
        // garbage
        assert!(parse_gpsd_tpv("not json").is_none());
        // TPV without coordinates
        assert!(parse_gpsd_tpv(r#"{"class":"TPV","mode":3}"#).is_none());
    }

    #[test]
    fn handle_round_trips_latest_fix() {
        let h = GpsHandle::default();
        assert!(h.latest().is_none());
        h.set(GpsFix { lat: 1.0, lon: 2.0, alt_m: None, hdop: 0.0, ts: 5, valid: true });
        assert_eq!(h.latest().unwrap().lat, 1.0);
    }
}
