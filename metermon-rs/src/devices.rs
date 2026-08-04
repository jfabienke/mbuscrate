//! Small on-Pi device manager.
//!
//! Persists per-device information and a significant-event log so device state survives
//! monitor restarts and the history is inspectable. Backed by **redb** — a pure-Rust
//! embedded store (no C dependency, trivial to cross-compile) — with records stored as
//! JSON values under typed tables:
//!   - `devices` — `u32` meter id → [`DeviceRecord`] (identity, running frame counts,
//!     RSSI, key/encryption status, first/last seen).
//!   - `events`  — monotonic `u64` id → [`EventRecord`]: significant transitions plus
//!     sampled frames (`first_seen`, `key`, `silent`, `resumed`, `reading`, 1-in-N
//!     `sample`).
//!
//! redb is single-writer per process: the `monitor` holds the store open, so the
//! standalone `devices` viewer can only open it when the monitor is stopped (or on a
//! copy of the file). While the monitor runs it prints the same report to its log.

use anyhow::{Context, Result};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

const DEVICES: TableDefinition<u32, &str> = TableDefinition::new("devices");
const EVENTS: TableDefinition<u64, &str> = TableDefinition::new("events");
/// Small key/value table for bookkeeping (e.g. the one-time SQLite import marker).
const META: TableDefinition<&str, &str> = TableDefinition::new("meta");
/// META counter of CRC-failed frames whose (untrustworthy) meter id matched no known
/// device — contained at ingestion instead of minting ghost device records.
const UNATTRIBUTED_KEY: &str = "unattributed_crc_fail";
/// Durable AES keys: meter id (decimal device address) -> 32-hex AES-128 key. Persisting
/// keys here lets the gateway load credentials locally at startup — before, and independent
/// of, the MQTT control-topic pull.
const KEYS: TableDefinition<u32, &str> = TableDefinition::new("keys");

/// A well-formed AES-128 key is exactly 32 hex chars (16 bytes). Validated before any key is
/// stored; the key value itself is never logged.
fn is_valid_aes128_hex(key: &str) -> bool {
    key.len() == 32 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Persisted per-device record (JSON-encoded as the `devices` table value).
#[derive(Serialize, Deserialize, Default, Clone)]
struct DeviceRecord {
    manufacturer: String,
    version: u8,
    device_type: u8,
    type_name: String,
    frame_type: String,
    ci: Option<u8>,
    encrypted: bool,
    has_key: bool,
    silent: bool,
    first_seen: i64,
    last_seen: i64,
    frames_total: u64,
    frames_ok: u64,
    frames_fail: u64,
    last_rssi: i16,
    last_reading: Option<String>,
    /// wM-Bus radio mode this device is received on ("C"/"T"/"S"; "" for records
    /// written before mode tracking existed).
    #[serde(default)]
    mode: String,
    /// Last AFC-measured carrier offset from the 868.95 MHz center, in Hz.
    #[serde(default)]
    last_freq_offset_hz: i32,
}

/// Persisted event record (JSON-encoded as the `events` table value).
#[derive(Serialize, Deserialize)]
struct EventRecord {
    ts: i64,
    meter_id: u32,
    kind: String,
    detail: String,
}

/// Seconds since the Unix epoch (wall clock on the Pi).
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Map an EN 13757-3 / OMS device-type (medium) byte to a human name.
pub fn device_type_name(t: u8) -> &'static str {
    match t {
        0x00 => "Other",
        0x01 => "Oil",
        0x02 => "Electricity",
        0x03 => "Gas",
        0x04 => "Heat",
        0x05 => "Steam",
        0x06 => "Warm Water",
        0x07 => "Water",
        0x08 => "Heat Cost Alloc",
        0x0A | 0x0B => "Cooling",
        0x0C => "Heat/Cooling",
        0x15 => "Hot Water",
        0x16 => "Cold Water",
        0x17 => "Dual Water",
        _ => "Unknown",
    }
}

/// One decoded-frame observation fed to the manager.
pub struct Observation {
    pub meter_id: u32,
    pub manufacturer: String,
    pub version: u8,
    pub device_type: u8,
    pub frame_type: String,
    pub ci: Option<u8>,
    pub encrypted: bool,
    pub crc_ok: bool,
    pub rssi: i16,
    pub has_key: bool,
    /// A decoded human reading (e.g. "25.340 m3"), once decryption is available.
    pub reading: Option<String>,
    /// Radio mode this frame was received on ("C" today — single-channel mode-C RX).
    pub mode: String,
    /// AFC-measured carrier offset from the 868.95 MHz center for this frame, in Hz.
    pub freq_offset_hz: i32,
}

pub struct DeviceManager {
    db: Database,
    /// Emit a `sample` event every Nth frame per device (1 = every frame).
    sample_every: u64,
    /// Flag a device `silent` after this many seconds without an identified frame.
    silence_after: i64,
}

impl DeviceManager {
    /// Open (creating if needed) the store for read/write use by the monitor.
    pub fn open(path: &str, sample_every: u64, silence_after_secs: i64) -> Result<Self> {
        let db = Database::create(path).with_context(|| format!("open device store {path}"))?;
        // Ensure both tables exist.
        let w = db.begin_write()?;
        {
            w.open_table(DEVICES)?;
            w.open_table(EVENTS)?;
            w.open_table(META)?;
            w.open_table(KEYS)?; // initialize the keys table on first database creation
        }
        w.commit()?;
        Ok(Self {
            db,
            sample_every: sample_every.max(1),
            silence_after: silence_after_secs,
        })
    }

    /// Open an existing store for the `devices` viewer.
    ///
    /// redb opens the file read-write and takes a single-writer lock, so this fails if
    /// the monitor is still running (stop it first) or if the file is owned by another
    /// user (the monitor runs as root under systemd, so run `devices` with `sudo`).
    pub fn open_readonly(path: &str) -> Result<Self> {
        let db = Database::open(path).with_context(|| {
            format!("open device store {path} — stop the monitor first (redb is single-writer), and run with sudo if the monitor created the file as root")
        })?;
        Ok(Self {
            db,
            sample_every: 1,
            silence_after: 0,
        })
    }

    /// Durably store an AES key for a meter (write-through when a key is pulled upstream).
    ///
    /// Validates the key is well-formed hex before writing (a malformed key is rejected, not
    /// stored), and the key value itself is never logged. The write is a single committed redb
    /// transaction, so persistence is atomic — a caller can treat success as durable.
    pub fn store_key(&self, meterid: u32, hexkey: &str) -> Result<()> {
        if meterid == 0 || !is_valid_aes128_hex(hexkey) {
            anyhow::bail!("refusing to store malformed AES key for meter {meterid}");
        }
        let w = self.db.begin_write()?;
        {
            let mut t = w.open_table(KEYS)?;
            t.insert(meterid, hexkey)?;
        }
        w.commit()?;
        Ok(())
    }

    /// Load all persisted `(meterid, hexkey)` pairs, to seed the keystore at startup before
    /// MQTT/radio initialization.
    pub fn load_keys(&self) -> Result<Vec<(u32, String)>> {
        let r = self.db.begin_read()?;
        let t = match r.open_table(KEYS) {
            Ok(t) => t,
            // A store created before the keys table existed: nothing to load.
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        for row in t.iter()? {
            let (k, v) = row?;
            out.push((k.value(), v.value().to_string()));
        }
        Ok(out)
    }

    /// Append events under fresh monotonic ids within an existing write transaction.
    fn append_events(
        txn: &redb::WriteTransaction,
        now: i64,
        meter: u32,
        events: &[(&str, String)],
    ) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        let mut etab = txn.open_table(EVENTS)?;
        let base = etab.last()?.map(|(k, _)| k.value() + 1).unwrap_or(0);
        for (i, (kind, detail)) in events.iter().enumerate() {
            let ev = EventRecord {
                ts: now,
                meter_id: meter,
                kind: (*kind).to_string(),
                detail: detail.clone(),
            };
            etab.insert(base + i as u64, serde_json::to_string(&ev)?.as_str())?;
        }
        Ok(())
    }

    /// Record one decoded frame: upsert the device record and emit significant events.
    ///
    /// Ghost containment: every field of a CRC-failed frame — including the meter id —
    /// is untrustworthy, so such a frame can never *create* a device (it is counted in
    /// the global [`UNATTRIBUTED_KEY`] tally instead, returning `false`), and for a
    /// known device it only bumps the failure counters: garbled contents must not
    /// rewrite identity, reset silence, or emit events.
    pub fn record_frame(&self, o: &Observation) -> Result<bool> {
        let now = now_unix();
        let txn = self.db.begin_write()?;
        let mut events: Vec<(&str, String)> = Vec::new();
        let mut attributed = true;
        {
            let mut dtab = txn.open_table(DEVICES)?;
            let prev: Option<DeviceRecord> = dtab
                .get(o.meter_id)?
                .map(|g| serde_json::from_str(g.value()))
                .transpose()?;

            match (prev, o.crc_ok) {
                (None, false) => {
                    drop(dtab);
                    let mut mtab = txn.open_table(META)?;
                    let n: u64 = mtab
                        .get(UNATTRIBUTED_KEY)?
                        .map(|g| g.value().parse().unwrap_or(0))
                        .unwrap_or(0);
                    mtab.insert(UNATTRIBUTED_KEY, (n + 1).to_string().as_str())?;
                    attributed = false;
                }
                (Some(mut rec), false) => {
                    rec.frames_total += 1;
                    rec.frames_fail += 1;
                    rec.last_rssi = o.rssi;
                    let total = rec.frames_total;
                    dtab.insert(o.meter_id, serde_json::to_string(&rec)?.as_str())?;
                    if total.is_multiple_of(self.sample_every) {
                        events.push(("sample", format!("rssi={} crc_ok=false", o.rssi)));
                    }
                }
                (prev, true) => {
                    let is_new = prev.is_none();
                    let prev_has_key = prev.as_ref().map(|r| r.has_key).unwrap_or(false);
                    let was_silent = prev.as_ref().map(|r| r.silent).unwrap_or(false);
                    let prev_reading = prev.as_ref().and_then(|r| r.last_reading.clone());

                    let type_name = device_type_name(o.device_type).to_string();
                    let mut rec = prev.unwrap_or_default();
                    rec.manufacturer = o.manufacturer.clone();
                    rec.version = o.version;
                    rec.device_type = o.device_type;
                    rec.type_name = type_name.clone();
                    rec.frame_type = o.frame_type.clone();
                    rec.ci = o.ci;
                    rec.encrypted = o.encrypted;
                    rec.has_key = o.has_key;
                    rec.mode = o.mode.clone();
                    rec.last_freq_offset_hz = o.freq_offset_hz;
                    rec.silent = false;
                    if is_new {
                        rec.first_seen = now;
                    }
                    rec.last_seen = now;
                    rec.frames_total += 1;
                    rec.frames_ok += 1;
                    rec.last_rssi = o.rssi;
                    if let Some(r) = &o.reading {
                        rec.last_reading = Some(r.clone());
                    }
                    let total = rec.frames_total;
                    dtab.insert(o.meter_id, serde_json::to_string(&rec)?.as_str())?;

                    if is_new {
                        events.push((
                            "first_seen",
                            format!(
                                "{} {} v{} type{}",
                                o.manufacturer, type_name, o.version, o.frame_type
                            ),
                        ));
                    }
                    if o.has_key && !prev_has_key {
                        events.push(("key", "AES key available".to_string()));
                    }
                    if was_silent {
                        events.push(("resumed", format!("rssi={}", o.rssi)));
                    }
                    if let Some(r) = &o.reading {
                        if prev_reading.as_deref() != Some(r.as_str()) {
                            events.push(("reading", r.clone()));
                        }
                    }
                    if total.is_multiple_of(self.sample_every) {
                        events.push(("sample", format!("rssi={} crc_ok=true", o.rssi)));
                    }
                }
            }
        }
        Self::append_events(&txn, now, o.meter_id, &events)?;
        txn.commit()?;
        Ok(attributed)
    }

    /// Total CRC-failed frames contained at ingestion because their id matched no
    /// known device.
    pub fn unattributed_crc_fail(&self) -> Result<u64> {
        let r = self.db.begin_read()?;
        let t = r.open_table(META)?;
        Ok(t.get(UNATTRIBUTED_KEY)?
            .map(|g| g.value().parse().unwrap_or(0))
            .unwrap_or(0))
    }

    /// Remove devices that never produced a CRC-valid frame (`frames_ok == 0`), and
    /// their events — garbled-id ghosts minted before ingestion containment existed.
    /// A real meter caught by this re-registers on its first valid frame. Returns
    /// `(devices_removed, events_removed)`.
    pub fn prune_ghosts(&self) -> Result<(usize, usize)> {
        let txn = self.db.begin_write()?;
        let mut removed_events = 0usize;
        let ghosts: Vec<u32>;
        {
            let mut dtab = txn.open_table(DEVICES)?;
            ghosts = dtab
                .iter()?
                .filter_map(|e| e.ok())
                .filter_map(|(k, v)| {
                    let rec: DeviceRecord = serde_json::from_str(v.value()).ok()?;
                    (rec.frames_ok == 0).then(|| k.value())
                })
                .collect();
            for id in &ghosts {
                dtab.remove(*id)?;
            }
            let ghost_set: std::collections::HashSet<u32> = ghosts.iter().copied().collect();
            let mut etab = txn.open_table(EVENTS)?;
            let doomed: Vec<u64> = etab
                .iter()?
                .filter_map(|e| e.ok())
                .filter_map(|(k, v)| {
                    let ev: EventRecord = serde_json::from_str(v.value()).ok()?;
                    ghost_set.contains(&ev.meter_id).then(|| k.value())
                })
                .collect();
            for k in doomed {
                etab.remove(k)?;
                removed_events += 1;
            }
        }
        txn.commit()?;
        Ok((ghosts.len(), removed_events))
    }

    /// Record a periodic airwave-sweep discovery summary as a global event
    /// (meter_id 0), e.g. `C:[63398862,74644444] T-bursts:0 S-bursts:0`.
    pub fn record_discovery(&self, summary: &str) -> Result<()> {
        let txn = self.db.begin_write()?;
        Self::append_events(&txn, now_unix(), 0, &[("discovery", summary.to_string())])?;
        txn.commit()?;
        Ok(())
    }

    /// Emit `silent` events for devices with no identified frame within `silence_after`.
    pub fn check_silence(&self) -> Result<()> {
        if self.silence_after <= 0 {
            return Ok(());
        }
        let now = now_unix();
        let cutoff = now - self.silence_after;
        let txn = self.db.begin_write()?;
        // (meter, updated_json, event_detail) for devices that just went silent.
        let mut newly_silent: Vec<(u32, String, String)> = Vec::new();
        {
            let mut dtab = txn.open_table(DEVICES)?;
            for entry in dtab.iter()? {
                let (k, v) = entry?;
                let mut rec: DeviceRecord = serde_json::from_str(v.value())?;
                if !rec.silent && rec.last_seen < cutoff {
                    let mins = (now - rec.last_seen) / 60;
                    rec.silent = true;
                    newly_silent.push((
                        k.value(),
                        serde_json::to_string(&rec)?,
                        format!("no frame for {mins}m"),
                    ));
                }
            }
            for (meter, json, _) in &newly_silent {
                dtab.insert(*meter, json.as_str())?;
            }
        }
        for (meter, _, detail) in newly_silent {
            Self::append_events(&txn, now, meter, &[("silent", detail)])?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Print the device table and the most recent events.
    pub fn print_report(&self, recent_events: usize) -> Result<()> {
        let txn = self.db.begin_read()?;

        println!(
            "{:>10}  {:>3}  {:<15}  {:>4}  {:>7}  {:>11}  {:>6}  {:>7}  {:>3}",
            "meter", "mfr", "type", "mode", "seen", "frames(ok)", "rssi", "off(Hz)", "key"
        );
        if let Ok(dtab) = txn.open_table(DEVICES) {
            for entry in dtab.iter()? {
                let (k, v) = entry?;
                let id = k.value();
                let r: DeviceRecord = serde_json::from_str(v.value())?;
                let frames = format!("{}/{}", r.frames_total, r.frames_ok);
                let seen = if r.silent {
                    format!("{}!", ago(r.last_seen))
                } else {
                    ago(r.last_seen)
                };
                let mode = if r.mode.is_empty() {
                    "-"
                } else {
                    r.mode.as_str()
                };
                println!(
                    "{id:>10}  {:>3}  {:<15}  {mode:>4}  {seen:>7}  {frames:>11}  {:>4}dBm  {:>+7}  {:>3}",
                    r.manufacturer,
                    r.type_name,
                    r.last_rssi,
                    r.last_freq_offset_hz,
                    if r.has_key { "yes" } else { "-" },
                );
            }
        }

        if let Ok(n) = self.unattributed_crc_fail() {
            if n > 0 {
                println!("({n} unattributed CRC-fail frame(s) contained — no device minted)");
            }
        }

        println!("\nrecent events:");
        if let Ok(etab) = txn.open_table(EVENTS) {
            // Sort by timestamp, not insertion id: after a migration, imported events
            // carry old timestamps under fresh (higher) ids, so id order != time order.
            let mut list: Vec<EventRecord> = Vec::new();
            for entry in etab.iter()? {
                let (_, v) = entry?;
                list.push(serde_json::from_str(v.value())?);
            }
            list.sort_by_key(|e| e.ts);
            let start = list.len().saturating_sub(recent_events);
            for e in &list[start..] {
                println!(
                    "  {:>6} ago  {:<10} {}  {}",
                    ago(e.ts),
                    e.kind,
                    e.meter_id,
                    e.detail
                );
            }
        }
        Ok(())
    }

    /// Merge a dump (from the old SQLite store) into this redb store. Device frame
    /// counts are summed, `first_seen` keeps the earliest, `last_seen` the latest, and
    /// events are appended. A one-time marker prevents accidental double-import.
    pub fn import_dump(&self, dump: &ImportDump) -> Result<(usize, usize)> {
        let txn = self.db.begin_write()?;
        {
            let mtab = txn.open_table(META)?;
            if mtab.get("sqlite_imported")?.is_some() {
                anyhow::bail!(
                    "store already carries an import marker — refusing to import again (would double-count)"
                );
            }
        }
        {
            let mut dtab = txn.open_table(DEVICES)?;
            for od in &dump.devices {
                let current: Option<DeviceRecord> = dtab
                    .get(od.meter_id)?
                    .map(|g| serde_json::from_str(g.value()))
                    .transpose()?;
                let merged = match current {
                    Some(mut cur) => {
                        cur.frames_total += od.frames_total;
                        cur.frames_ok += od.frames_ok;
                        cur.frames_fail += od.frames_fail;
                        cur.first_seen = min_nonzero(cur.first_seen, od.first_seen);
                        cur.last_seen = cur.last_seen.max(od.last_seen);
                        if cur.manufacturer.is_empty() {
                            cur.manufacturer = od.manufacturer.clone();
                        }
                        if cur.type_name.is_empty() {
                            cur.type_name = od.type_name.clone();
                        }
                        if cur.frame_type.is_empty() {
                            cur.frame_type = od.frame_type.clone();
                        }
                        cur
                    }
                    None => DeviceRecord {
                        manufacturer: od.manufacturer.clone(),
                        version: od.version,
                        device_type: od.device_type,
                        type_name: od.type_name.clone(),
                        frame_type: od.frame_type.clone(),
                        ci: od.ci,
                        encrypted: od.encrypted,
                        has_key: od.has_key,
                        silent: od.silent,
                        first_seen: od.first_seen,
                        last_seen: od.last_seen,
                        frames_total: od.frames_total,
                        frames_ok: od.frames_ok,
                        frames_fail: od.frames_fail,
                        last_rssi: od.last_rssi,
                        last_reading: od.last_reading.clone(),
                        // Old SQLite dumps predate mode/frequency tracking.
                        mode: String::new(),
                        last_freq_offset_hz: 0,
                    },
                };
                dtab.insert(od.meter_id, serde_json::to_string(&merged)?.as_str())?;
            }
        }
        {
            let mut etab = txn.open_table(EVENTS)?;
            let base = etab.last()?.map(|(k, _)| k.value() + 1).unwrap_or(0);
            for (i, oe) in dump.events.iter().enumerate() {
                let ev = EventRecord {
                    ts: oe.ts,
                    meter_id: oe.meter_id,
                    kind: oe.kind.clone(),
                    detail: oe.detail.clone(),
                };
                etab.insert(base + i as u64, serde_json::to_string(&ev)?.as_str())?;
            }
        }
        {
            let mut mtab = txn.open_table(META)?;
            mtab.insert("sqlite_imported", "1")?;
        }
        txn.commit()?;
        Ok((dump.devices.len(), dump.events.len()))
    }
}

/// A device/event dump to merge into the store (e.g. exported from the old SQLite DB).
#[derive(Deserialize, Default)]
pub struct ImportDump {
    #[serde(default)]
    devices: Vec<ImportDevice>,
    #[serde(default)]
    events: Vec<ImportEvent>,
}

#[derive(Deserialize)]
struct ImportDevice {
    meter_id: u32,
    #[serde(default)]
    manufacturer: String,
    #[serde(default)]
    version: u8,
    #[serde(default)]
    device_type: u8,
    #[serde(default)]
    type_name: String,
    #[serde(default)]
    frame_type: String,
    #[serde(default)]
    ci: Option<u8>,
    #[serde(default)]
    encrypted: bool,
    #[serde(default)]
    has_key: bool,
    #[serde(default)]
    silent: bool,
    #[serde(default)]
    first_seen: i64,
    #[serde(default)]
    last_seen: i64,
    #[serde(default)]
    frames_total: u64,
    #[serde(default)]
    frames_ok: u64,
    #[serde(default)]
    frames_fail: u64,
    #[serde(default)]
    last_rssi: i16,
    #[serde(default)]
    last_reading: Option<String>,
}

#[derive(Deserialize)]
struct ImportEvent {
    ts: i64,
    meter_id: u32,
    kind: String,
    #[serde(default)]
    detail: String,
}

/// Smallest of two Unix timestamps, ignoring a zero (missing) value.
fn min_nonzero(a: i64, b: i64) -> i64 {
    match (a, b) {
        (0, x) | (x, 0) => x,
        (a, b) => a.min(b),
    }
}

/// Compact relative-age string for a Unix-seconds timestamp.
fn ago(ts: i64) -> String {
    let d = (now_unix() - ts).max(0);
    if d < 90 {
        format!("{d}s")
    } else if d < 5400 {
        format!("{}m", d / 60)
    } else if d < 172_800 {
        format!("{}h", d / 3600)
    } else {
        format!("{}d", d / 86_400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_db() -> String {
        // Unique-enough path without Date/rand: use the test thread name + a static bump.
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir();
        dir.join(format!("metermon-devtest-{}-{n}.redb", std::process::id()))
            .to_string_lossy()
            .into_owned()
    }

    fn obs(meter: u32, crc_ok: bool, has_key: bool) -> Observation {
        Observation {
            meter_id: meter,
            manufacturer: "KAM".into(),
            version: 0x1B,
            device_type: 0x06,
            frame_type: "B".into(),
            ci: Some(0x8D),
            encrypted: true,
            crc_ok,
            rssi: -90,
            has_key,
            reading: None,
            mode: "C".into(),
            freq_offset_hz: 0,
        }
    }

    #[test]
    fn persists_mode_and_freq_offset() {
        let dm = DeviceManager::open(&tmp_db(), 20, 600).unwrap();
        let mut o = obs(74644444, true, false);
        o.mode = "C".into();
        o.freq_offset_hz = 305;
        dm.record_frame(&o).unwrap();

        let txn = dm.db.begin_read().unwrap();
        let dtab = txn.open_table(DEVICES).unwrap();
        let raw = dtab.get(74644444u32).unwrap().unwrap();
        let rec: DeviceRecord = serde_json::from_str(raw.value()).unwrap();
        assert_eq!(rec.mode, "C");
        assert_eq!(rec.last_freq_offset_hz, 305);
    }

    /// Count events of a given kind for a meter (test helper).
    fn count_events(dm: &DeviceManager, meter: u32, kind: &str) -> usize {
        let txn = dm.db.begin_read().unwrap();
        let etab = txn.open_table(EVENTS).unwrap();
        etab.iter()
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|(_, v)| {
                let ev: EventRecord = serde_json::from_str(v.value()).unwrap();
                ev.meter_id == meter && ev.kind == kind
            })
            .count()
    }

    fn device_counts(dm: &DeviceManager, meter: u32) -> (u64, u64, u64, String) {
        let txn = dm.db.begin_read().unwrap();
        let dtab = txn.open_table(DEVICES).unwrap();
        let g = dtab.get(meter).unwrap().unwrap();
        let r: DeviceRecord = serde_json::from_str(g.value()).unwrap();
        (r.frames_total, r.frames_ok, r.frames_fail, r.type_name)
    }

    #[test]
    fn upserts_and_counts_frames() {
        let path = tmp_db();
        let dm = DeviceManager::open(&path, 20, 600).unwrap();
        dm.record_frame(&obs(63398862, true, false)).unwrap();
        dm.record_frame(&obs(63398862, false, false)).unwrap();
        dm.record_frame(&obs(63398862, true, false)).unwrap();
        assert_eq!(
            device_counts(&dm, 63398862),
            (3, 2, 1, "Warm Water".to_string())
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn emits_first_seen_and_key_events_once() {
        let path = tmp_db();
        let dm = DeviceManager::open(&path, 1000, 600).unwrap();
        dm.record_frame(&obs(74644444, true, false)).unwrap(); // first_seen
        dm.record_frame(&obs(74644444, true, false)).unwrap(); // nothing new
        dm.record_frame(&obs(74644444, true, true)).unwrap(); // key installed
        dm.record_frame(&obs(74644444, true, true)).unwrap(); // key already known
        assert_eq!(count_events(&dm, 74644444, "first_seen"), 1);
        assert_eq!(count_events(&dm, 74644444, "key"), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn samples_every_nth_frame() {
        let path = tmp_db();
        let dm = DeviceManager::open(&path, 5, 600).unwrap();
        for _ in 0..12 {
            dm.record_frame(&obs(80504381, true, false)).unwrap();
        }
        assert_eq!(count_events(&dm, 80504381, "sample"), 2); // at 5 and 10
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn imports_merges_and_is_idempotent() {
        let path = tmp_db();
        let dm = DeviceManager::open(&path, 20, 600).unwrap();
        dm.record_frame(&obs(63398862, true, false)).unwrap(); // live: total 1, ok 1

        let dump: ImportDump = serde_json::from_str(
            r#"{
              "devices":[
                {"meter_id":63398862,"manufacturer":"KAM","type_name":"Warm Water",
                 "frames_total":10,"frames_ok":4,"frames_fail":6,"first_seen":100,"last_seen":200},
                {"meter_id":99999999,"manufacturer":"XYZ","type_name":"Heat",
                 "frames_total":5,"frames_ok":5,"first_seen":150,"last_seen":250}
              ],
              "events":[{"ts":50,"meter_id":63398862,"kind":"first_seen","detail":"old"}]
            }"#,
        )
        .unwrap();

        let (d, e) = dm.import_dump(&dump).unwrap();
        assert_eq!((d, e), (2, 1));
        // merged counts: 1+10 total, 1+4 ok, 0+6 fail
        assert_eq!(
            device_counts(&dm, 63398862),
            (11, 5, 6, "Warm Water".to_string())
        );
        // device only in the dump is inserted
        assert_eq!(device_counts(&dm, 99999999).0, 5);
        // old event preserved
        assert_eq!(count_events(&dm, 63398862, "first_seen"), 2);
        // idempotency guard: a second import is refused
        assert!(dm.import_dump(&dump).is_err());
        assert_eq!(
            device_counts(&dm, 63398862).0,
            11,
            "counts unchanged after refused re-import"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persists_across_reopen() {
        let path = tmp_db();
        {
            let dm = DeviceManager::open(&path, 20, 600).unwrap();
            dm.record_frame(&obs(63398862, true, false)).unwrap();
        } // drop -> releases the file
        let dm = DeviceManager::open(&path, 20, 600).unwrap();
        dm.record_frame(&obs(63398862, true, false)).unwrap();
        assert_eq!(device_counts(&dm, 63398862).0, 2, "count survived reopen");
        let _ = std::fs::remove_file(&path);
    }

    fn has_device(dm: &DeviceManager, meter: u32) -> bool {
        let txn = dm.db.begin_read().unwrap();
        let dtab = txn.open_table(DEVICES).unwrap();
        dtab.get(meter).unwrap().is_some()
    }

    #[test]
    fn crc_failed_frames_never_create_devices() {
        let path = tmp_db();
        let dm = DeviceManager::open(&path, 1000, 600).unwrap();

        // Unknown id + bad CRC: contained, not minted.
        assert!(!dm.record_frame(&obs(12345678, false, false)).unwrap());
        assert!(!dm.record_frame(&obs(12345678, false, false)).unwrap());
        assert!(!has_device(&dm, 12345678));
        assert_eq!(dm.unattributed_crc_fail().unwrap(), 2);
        assert_eq!(count_events(&dm, 12345678, "first_seen"), 0);

        // A valid frame mints the device as usual.
        assert!(dm.record_frame(&obs(12345678, true, false)).unwrap());
        assert!(has_device(&dm, 12345678));

        // Bad-CRC frame for a known device: counts the failure, but its garbled
        // contents must not rewrite identity.
        let mut garbled = obs(12345678, false, false);
        garbled.manufacturer = "XXX".into();
        garbled.device_type = 0x04;
        assert!(dm.record_frame(&garbled).unwrap());
        let (total, ok, fail, type_name) = device_counts(&dm, 12345678);
        assert_eq!((total, ok, fail), (2, 1, 1));
        assert_eq!(
            type_name, "Warm Water",
            "garbled frame must not rewrite identity"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn prune_ghosts_removes_never_validated_devices() {
        let path = tmp_db();
        let dm = DeviceManager::open(&path, 1000, 600).unwrap();
        dm.record_frame(&obs(74644444, true, false)).unwrap(); // real device

        // A pre-containment ghost: never CRC-valid, with an event trail.
        let dump: ImportDump = serde_json::from_str(
            r#"{
              "devices":[{"meter_id":63398870,"manufacturer":"KAM","type_name":"Warm Water",
                          "frames_total":3,"frames_fail":3,"first_seen":100,"last_seen":200}],
              "events":[{"ts":150,"meter_id":63398870,"kind":"first_seen","detail":"ghost"},
                        {"ts":180,"meter_id":63398870,"kind":"silent","detail":"no frame"}]
            }"#,
        )
        .unwrap();
        dm.import_dump(&dump).unwrap();
        assert!(has_device(&dm, 63398870));

        let (d, e) = dm.prune_ghosts().unwrap();
        assert_eq!((d, e), (1, 2));
        assert!(!has_device(&dm, 63398870), "ghost removed");
        assert!(has_device(&dm, 74644444), "validated device kept");
        assert_eq!(count_events(&dm, 74644444, "first_seen"), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stores_and_loads_keys_rejecting_malformed_and_surviving_reopen() {
        let path = tmp_db();
        let key = "000102030405060708090a0b0c0d0e0f";
        {
            let dm = DeviceManager::open(&path, 20, 600).unwrap();
            dm.store_key(74644444, key).unwrap();
            // Malformed keys are refused, not stored.
            assert!(dm.store_key(74644444, "abcd").is_err()); // too short
            assert!(dm
                .store_key(74644444, "zz0102030405060708090a0b0c0d0e0f")
                .is_err()); // non-hex
            assert!(dm.store_key(0, key).is_err()); // meter 0
        } // drop -> releases the file
          // Durable across reopen; the rejected writes left no trace.
        let dm = DeviceManager::open(&path, 20, 600).unwrap();
        assert_eq!(dm.load_keys().unwrap(), vec![(74644444, key.to_string())]);
        let _ = std::fs::remove_file(&path);
    }
}
