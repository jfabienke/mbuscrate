//! metermon-rs — a Rust reimplementation of the epulse `metermon` wM-Bus gateway,
//! built on the `mbus-rs` crate, for A/B validation against the deployed C++ gateway.
//!
//! Subcommands:
//!   replay <capture> [--config metermon.conf]   decode a capture file to JSON (A/B input)
//!   run    [--config metermon.conf] [--shadow]   live: RFM69 -> decode -> MQTT (needs `radio` feature)
//!
//! The `replay` path builds and runs on any host and is what the capture-replay A/B
//! uses. `run` needs the `radio` feature (Raspberry Pi + RFM69) and frees/needs the
//! SPI bus, so it only makes sense with metermon stopped.

// The MQTT publisher and parts of the config are consumed only by the live radio
// path (`run_live`, behind the `radio` feature). Without it, the replay-only build
// legitimately doesn't read them — silence dead-code noise rather than the real signal.
#![cfg_attr(not(feature = "radio"), allow(dead_code))]

mod config;
mod decode;
mod devices;
mod health;
mod keystore;
mod mock_backend;
mod profiles;
mod publish;
mod source;
mod sweep;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::Config;
use keystore::KeyStore;
use source::{FileReplaySource, FrameSource};

#[derive(Parser)]
#[command(
    name = "metermon-rs",
    about = "Rust wM-Bus gateway (mbus-rs) — A/B target for metermon"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Decode a capture file (one hex frame per line) to JSON lines on stdout.
    Replay {
        capture: String,
        #[arg(long, default_value = "metermon.conf")]
        config: String,
        /// AES keys captured from the control topic (JSON map or op:key lines).
        /// Without keys, encrypted frames decode only to the header.
        #[arg(long)]
        keys: Option<String>,
    },
    /// Live receive from the RFM69 radio and publish to MQTT (requires `radio` feature).
    Run {
        #[arg(long, default_value = "metermon.conf")]
        config: String,
        /// Publish to `<data-topic>-rust` instead of the live topic.
        #[arg(long)]
        shadow: bool,
    },
    /// Capture raw wM-Bus frames from the radio to a hex file (requires `radio` feature).
    /// Non-decoding: just records what the radio delivers, for offline A/B replay.
    Capture {
        /// Output file (one hex frame per line; `#` header line included).
        out: String,
        #[arg(long, default_value = "metermon.conf")]
        config: String,
        /// Stop after this many seconds.
        #[arg(long, default_value_t = 120)]
        seconds: u64,
        /// Stop after this many frames (whichever limit is hit first).
        #[arg(long, default_value_t = 50)]
        count: usize,
    },
    /// Read-only RFM69 register dump (requires `radio` feature).
    ///
    /// Opens SPI WITHOUT resetting or reconfiguring the chip, so if run right after
    /// stopping metermon it reveals metermon's working register config for comparison.
    DumpRegs {
        #[arg(long, default_value = "metermon.conf")]
        config: String,
    },
    /// Capture the raw 868.95 MHz bitstream with HW sync DISABLED (mode-agnostic).
    /// Streams demodulated bytes to a binary file for offline C/T-mode analysis.
    /// Run with the monitor stopped. (requires `radio` feature)
    ProbeRaw {
        /// Output binary file (raw demodulated byte stream).
        out: String,
        #[arg(long, default_value = "metermon.conf")]
        config: String,
        /// Capture duration in seconds.
        #[arg(long, default_value_t = 180)]
        seconds: u64,
        /// RX centre frequency in Hz (868950000 for C/T, 868300000 for S).
        #[arg(long, default_value_t = 868_950_000)]
        freq_hz: u64,
        /// Chip/bit rate in bps (100000 for C/T, 32768 for S).
        #[arg(long, default_value_t = 100_000)]
        bitrate_bps: u32,
    },
    /// Run continuously, decoding frames and accumulating per-meter statistics
    /// (count, CRC pass/fail, RSSI). Prints a stats table periodically. (`radio` feature.)
    ///
    /// AES keys are pulled live from the MQTT control topic (as metermon does); an
    /// optional `--keys` file seeds the store for offline/immediate decryption.
    Monitor {
        #[arg(long, default_value = "metermon.conf")]
        config: String,
        /// Seconds between stats reports.
        #[arg(long, default_value_t = 60)]
        report: u64,
        /// AES key file to seed the keystore (JSON map or op:key lines), in addition
        /// to keys pulled live from the control topic.
        #[arg(long)]
        keys: Option<String>,
        /// redb device-manager store (device info + event history, persists across
        /// restarts). Inspect with `metermon-rs devices --db <path>` (monitor stopped).
        #[arg(long, default_value = "metermon-devices.redb")]
        db: String,
        /// Hours between airwave discovery sweeps (C/T/S mode scan); 0 disables.
        /// Each sweep pauses C reception for ~2 min and records a `discovery` event.
        #[arg(long, default_value_t = 12)]
        sweep_hours: u64,
    },
    /// Show the device manager's stored device table and recent events (no radio).
    Devices {
        /// redb device-manager store written by `monitor` (open it with the monitor stopped).
        #[arg(long, default_value = "metermon-devices.redb")]
        db: String,
        /// Number of recent events to show.
        #[arg(long, default_value_t = 20)]
        events: usize,
        /// Remove devices that never produced a CRC-valid frame (garbled-id ghosts),
        /// together with their events, before printing the report. A real meter caught
        /// by this re-registers on its first valid frame.
        #[arg(long)]
        prune_ghosts: bool,
    },
    /// Run one airwave discovery sweep now (C/T/S mode scan) and record it to the store.
    /// Run with the monitor stopped. (requires `radio` feature)
    Sweep {
        #[arg(long, default_value = "metermon.conf")]
        config: String,
        #[arg(long, default_value = "metermon-devices.redb")]
        db: String,
        /// Capture seconds per band.
        #[arg(long, default_value_t = 60)]
        seconds: u64,
    },
    /// Import a JSON device dump into the redb store (merges counts, appends events).
    /// Run with the monitor stopped. Use to migrate the old SQLite store.
    Import {
        /// redb store to merge into.
        #[arg(long, default_value = "metermon-devices.redb")]
        db: String,
        /// JSON dump: {"devices":[...],"events":[...]} (e.g. exported from the old SQLite DB).
        #[arg(long)]
        from: String,
    },
    /// Import the `keys` map of a metermon.conf-format file into the redb store, so
    /// the monitor holds them from startup without any broker. Keys are validated
    /// before storage and their values are never printed. Run with the monitor
    /// stopped (redb is single-writer).
    ImportKeys {
        /// redb store to write the keys into.
        #[arg(long, default_value = "metermon-devices.redb")]
        db: String,
        /// metermon.conf-format JSON whose `keys` map to import (e.g. a retired
        /// gateway config).
        #[arg(long)]
        from_config: String,
    },
    /// Run the mock backend Device Manager: watch the gateway's data topic and answer
    /// op:startup / op:profile_request with op:profile messages from a catalog file.
    /// Stand-in for the real Device Manager (vendor-layers §7.2); serves model names
    /// only, never key material.
    MockBackend {
        #[arg(long, default_value = "metermon.conf")]
        config: String,
        /// Device catalog: JSON {"<meterid>": {"model": "...", "firmware": null}}.
        #[arg(long)]
        catalog: String,
    },
    /// Remove a persisted AES key from the redb store (e.g. one bound to the wrong
    /// meter id). Run with the monitor stopped (redb is single-writer).
    RemoveKey {
        /// redb store to remove the key from.
        #[arg(long, default_value = "metermon-devices.redb")]
        db: String,
        /// Meter id (decimal device address) whose key to remove.
        #[arg(long)]
        meterid: u32,
    },
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Replay {
            capture,
            config,
            keys,
        } => run_replay(&capture, &config, keys.as_deref()),
        Cmd::Run { config, shadow } => run_live(&config, shadow),
        Cmd::Capture {
            out,
            config,
            seconds,
            count,
        } => run_capture(&config, &out, seconds, count),
        Cmd::DumpRegs { config } => run_dumpregs(&config),
        Cmd::ProbeRaw {
            out,
            config,
            seconds,
            freq_hz,
            bitrate_bps,
        } => run_probe_raw(&config, &out, seconds, freq_hz, bitrate_bps),
        Cmd::Monitor {
            config,
            report,
            keys,
            db,
            sweep_hours,
        } => run_monitor(&config, report, keys.as_deref(), &db, sweep_hours),
        Cmd::Devices {
            db,
            events,
            prune_ghosts,
        } => run_devices(&db, events, prune_ghosts),
        Cmd::Sweep {
            config,
            db,
            seconds,
        } => run_sweep(&config, &db, seconds),
        Cmd::Import { db, from } => run_import(&db, &from),
        Cmd::ImportKeys { db, from_config } => run_import_keys(&db, &from_config),
        Cmd::MockBackend { config, catalog } => mock_backend::run(&config, &catalog),
        Cmd::RemoveKey { db, meterid } => {
            let dm = devices::DeviceManager::open(&db, 20, 600)?;
            if dm.remove_key(meterid)? {
                println!("removed AES key for meter {meterid}");
            } else {
                println!("no AES key stored for meter {meterid}");
            }
            Ok(())
        }
    }
}

/// Import a config file's `keys` map into the redb store (monitor stopped). The
/// monitor then loads them at startup like any control-topic-pulled key. Key values
/// are never printed; malformed entries are rejected by `store_key`.
fn run_import_keys(db: &str, from_config: &str) -> Result<()> {
    let cfg = Config::load(from_config)?;
    if cfg.keys.is_empty() {
        anyhow::bail!("no `keys` map in {from_config}");
    }
    let dm = devices::DeviceManager::open(db, 20, 600)?;
    let (mut stored, mut rejected) = (0u32, 0u32);
    for (id_str, hexkey) in &cfg.keys {
        let Ok(id) = id_str.parse::<u32>() else {
            log::warn!("skipping non-numeric meterid {id_str:?}");
            rejected += 1;
            continue;
        };
        match dm.store_key(id, hexkey) {
            Ok(()) => {
                println!("stored AES key for meter {id}");
                stored += 1;
            }
            Err(e) => {
                log::warn!("rejected key for meter {id}: {e}");
                rejected += 1;
            }
        }
    }
    println!("imported {stored} key(s) into {db} ({rejected} rejected)");
    Ok(())
}

/// The `op:startup` announcement the C++ gateway publishes on its data topic at boot
/// (metermon.cc:207). The upstream backend responds by pushing `op:key` control
/// messages to the `control` topic named here. Unlike the C++ version, the raw config
/// file is deliberately NOT included — it may contain a seeded `keys` map.
#[cfg(feature = "radio")]
fn startup_announcement(gwid: &str, control_topic: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "op": "startup",
        "ts": devices::now_unix(),
        "gw": gwid,
        "app": "metermon-rs",
        "version": env!("CARGO_PKG_VERSION"),
        "control": control_topic,
        "pid": std::process::id(),
    })
}

/// Persist-then-install handler for `op:key` control messages, shared by the primary
/// and the dedicated key broker. The redb write happens outside the keystore lock and
/// the key only becomes usable after the write commits, so a key is never live before
/// it is durable. Key values are never logged; `store_key` validates and rejects
/// malformed keys.
#[cfg(feature = "radio")]
fn key_install_handler(
    keys: std::sync::Arc<std::sync::Mutex<KeyStore>>,
    dm: std::sync::Arc<devices::DeviceManager>,
) -> impl Fn(&serde_json::Value) + Send + 'static {
    move |msg| {
        let Some((id, hexkey)) = KeyStore::parse_key_message(msg) else {
            return;
        };
        match dm.store_key(id, &hexkey) {
            Ok(()) => {
                keys.lock().unwrap().install(id, hexkey);
                log::info!("installed + persisted AES key for meter {id} (control topic)");
            }
            Err(e) => log::warn!("rejected AES key for meter {id}: {e}"),
        }
    }
}

/// One-shot airwave discovery sweep (monitor must be stopped). Records to redb.
#[cfg(feature = "radio")]
fn run_sweep(config_path: &str, db: &str, seconds: u64) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let spidev = cfg
        .devices
        .values()
        .find(|d| d.dev_type.eq_ignore_ascii_case("WMBUS"))
        .and_then(|d| d.spidev.clone())
        .ok_or_else(|| anyhow::anyhow!("no WMBUS device with a spidev in config"))?;
    log::info!("airwave sweep: scanning C/T (868.95) then S (868.30), {seconds}s each");
    let (meters, t, s) = sweep::sweep_once(&spidev, seconds)?;
    let summary = format!("C:{meters:?} T-bursts:{t} S-bursts:{s}");
    let dm = devices::DeviceManager::open(db, 20, 600)?;
    dm.record_discovery(&summary)?;
    println!("airwave sweep: C meters {meters:?}, T-bursts {t}, S-bursts {s}");
    Ok(())
}

#[cfg(not(feature = "radio"))]
fn run_sweep(_config_path: &str, _db: &str, _seconds: u64) -> Result<()> {
    bail_no_radio("sweep")
}

/// Show the device manager store (device table + recent events). Host-independent.
fn run_devices(db: &str, events: usize, prune_ghosts: bool) -> Result<()> {
    let dm = devices::DeviceManager::open_readonly(db)?;
    if prune_ghosts {
        let (d, e) = dm.prune_ghosts()?;
        println!("pruned {d} ghost device(s) and {e} associated event(s)\n");
    }
    dm.print_report(events)
}

/// Merge a JSON device dump into the redb store (monitor must be stopped).
fn run_import(db: &str, from: &str) -> Result<()> {
    let text = std::fs::read_to_string(from)
        .map_err(|e| anyhow::anyhow!("cannot read dump {from}: {e}"))?;
    let dump: devices::ImportDump = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("invalid dump JSON in {from}: {e}"))?;
    let dm = devices::DeviceManager::open(db, 20, 600)?;
    let (devices_n, events_n) = dm.import_dump(&dump)?;
    println!("imported {devices_n} device row(s) and {events_n} event(s) into {db}");
    Ok(())
}

/// Per-meter running statistics for the monitor.
#[derive(Default)]
struct MeterStat {
    total: u64,
    ok: u64,
    fail: u64,
    last_rssi: i16,
    sum_rssi: i64,
    /// Whether an AES key is currently held for this meter (from config seed,
    /// `--keys` file, or pulled live off the control topic).
    has_key: bool,
}

#[cfg(feature = "radio")]
fn run_monitor(
    config_path: &str,
    report_secs: u64,
    keys_path: Option<&str>,
    db_path: &str,
    sweep_hours: u64,
) -> Result<()> {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    let cfg = Config::load(config_path)?;
    let spidev = cfg
        .devices
        .values()
        .find(|d| d.dev_type.eq_ignore_ascii_case("WMBUS"))
        .and_then(|d| d.spidev.clone())
        .ok_or_else(|| anyhow::anyhow!("no WMBUS device with a spidev in config"))?;

    // Shared keystore: seeded from config + optional `--keys` file, then fed live
    // from the MQTT control topic exactly as the C++ metermon is.
    let keys = Arc::new(Mutex::new(KeyStore::new()));
    {
        let mut k = keys.lock().unwrap();
        k.seed_from_config(&cfg);
        if let Some(path) = keys_path {
            match KeyStore::load_file(path) {
                Ok(loaded) => {
                    for (id, hk) in loaded.iter() {
                        k.install(id, hk.to_string());
                    }
                }
                Err(e) => log::warn!("could not load keys from {path}: {e}"),
            }
        }
        log::info!("keystore seeded with {} key(s)", k.len());
    }

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        // Device manager (redb): persists device info + a significant-event log
        // (sample 1-in-20, flag a device silent after 10 min without an identified frame).
        let dm = Arc::new(devices::DeviceManager::open(db_path, 20, 600)?);
        log::info!("device store: {db_path}");

        // Device profiles: same durable-before-live discipline as keys. Load from redb
        // before broker contact so the gateway decodes with its device knowledge even
        // when the backend is unreachable.
        let profile_store = std::sync::Arc::new(std::sync::Mutex::new(
            crate::profiles::ProfileStore::new(),
        ));
        match dm.load_profiles() {
            Ok(rows) => {
                let mut ps = profile_store.lock().unwrap();
                for (id, model, firmware) in rows {
                    ps.install(
                        id,
                        mbus_rs::vendors::DeviceProfile { model, firmware },
                    );
                }
                log::info!("profile store holds {} device profile(s) from redb", ps.len());
            }
            Err(e) => anyhow::bail!("failed to load persisted device profiles from redb: {e}"),
        }

        // Load durably-persisted AES keys locally, before MQTT/radio init, so the gateway has
        // credentials immediately and never blocks on the control-topic pull for them. Fail
        // startup clearly if the persisted keys cannot be read or installed.
        match dm.load_keys() {
            Ok(pairs) => {
                let mut k = keys.lock().unwrap();
                for (id, hexkey) in pairs {
                    k.install(id, hexkey);
                }
                log::info!("keystore holds {} key(s) after loading from redb", k.len());
            }
            Err(e) => anyhow::bail!("failed to load persisted AES keys from redb: {e}"),
        }

        // Gateway self-instrumentation topics: retained online/offline status (offline is
        // delivered by the broker via Last-Will) + a periodic health heartbeat, so the
        // fleet is observable upstream without SSH.
        let status_topic = cfg.gateway_status_topic();
        let health_topic = cfg.gateway_health_topic();
        let offline_payload =
            serde_json::to_vec(&serde_json::json!({ "gwid": cfg.gwid, "status": "offline" }))
                .unwrap_or_default();

        // AES key pull + gateway status. Non-fatal if the broker is unreachable — the radio
        // monitor keeps running on whatever keys were seeded (store-and-forward via redb).
        let mut publisher = match publish::Publisher::connect(
            &cfg.mqtt,
            None,
            Some((status_topic.clone(), offline_payload)),
        ) {
            Ok(mut p) => {
                // Birth: retained "online" so a subscriber always sees the current state.
                let online =
                    serde_json::to_vec(&serde_json::json!({ "gwid": cfg.gwid, "status": "online" }))
                        .unwrap_or_default();
                if let Err(e) = p.publish_retained(&status_topic, &online) {
                    log::warn!("gateway birth publish failed: {e}");
                }
                match &cfg.mqtt.control_topic {
                    Some(control) => {
                        let key_handler = key_install_handler(keys.clone(), dm.clone());
                        let ps_cb = profile_store.clone();
                        let dm_profile = dm.clone();
                        match p.subscribe_control(control, move |msg| {
                            key_handler(msg);
                            if let Some((id, profile)) =
                                crate::profiles::ProfileStore::parse_profile_message(msg)
                            {
                                // Durable first, live second — a profile is never in
                                // use before it would survive a restart.
                                match dm_profile.store_profile(
                                    id,
                                    &profile.model,
                                    profile.firmware.as_deref(),
                                ) {
                                    Ok(()) => {
                                        log::info!(
                                            "installed + persisted profile for meter {id}: {}",
                                            profile.model
                                        );
                                        ps_cb.lock().unwrap().install(id, profile);
                                    }
                                    Err(e) => {
                                        log::warn!("rejected profile for meter {id}: {e}")
                                    }
                                }
                            }
                        }) {
                            Ok(()) => log::info!("pulling AES keys from control topic {control}"),
                            Err(e) => log::warn!("control-topic subscribe failed: {e}"),
                        }
                    }
                    None => log::warn!("no control-topic in config; live AES key pull disabled"),
                }
                // Protocol-compatible startup announcement (metermon.cc:207): published on
                // the data topic; the upstream backend pushes op:key messages to the
                // announced control topic in response. Keys are not retained, so this
                // announcement is what actually triggers key delivery.
                if let Err(e) = p.publish_json(&startup_announcement(
                    &cfg.gwid,
                    cfg.mqtt.control_topic.as_deref(),
                )) {
                    log::warn!("startup announce failed: {e}");
                }
                // Ask the Device Manager for profiles covering the fleet we actually
                // track (vendor-layers §7.2). Answers arrive as op:profile messages.
                match dm.device_ids() {
                    Ok(meters) => {
                        let req = serde_json::json!({
                            "op": "profile_request", "gw": cfg.gwid, "meters": meters,
                        });
                        match p.publish_json(&req) {
                            Ok(()) => log::info!(
                                "requested device profiles for {} meter(s)",
                                req["meters"].as_array().map_or(0, |m| m.len())
                            ),
                            Err(e) => log::warn!("profile request failed: {e}"),
                        }
                    }
                    Err(e) => log::warn!("could not enumerate devices for profile request: {e}"),
                }
                log::info!("gateway status → {status_topic}, health → {health_topic}");
                Some(p)
            }
            Err(e) => {
                log::warn!("MQTT connect failed ({e}); key pull + health off, using seeded keys");
                None
            }
        };

        // Dedicated key broker: when the primary upstream has no AES key functionality,
        // an optional `key-mqtt` broker carries the control topic instead. Subscription
        // only — data/status/health stay on the primary. The handle must stay alive for
        // the life of the monitor; rumqttc retries in the background if the broker is
        // down, and the persistent session re-delivers keys missed while offline.
        let _key_broker = cfg.key_mqtt.as_ref().and_then(|kb| {
            let clientid = kb
                .clientid
                .clone()
                .unwrap_or_else(|| format!("{}-keys", cfg.mqtt.clientid));
            let Some(control) = kb
                .control_topic
                .clone()
                .or_else(|| cfg.mqtt.control_topic.clone())
            else {
                log::warn!("key-mqtt configured but no control-topic to subscribe");
                return None;
            };
            match publish::Publisher::connect_subscriber(&kb.host, kb.port, &clientid) {
                Ok(mut p) => {
                    match p.subscribe_control(
                        &control,
                        key_install_handler(keys.clone(), dm.clone()),
                    ) {
                        Ok(()) => log::info!(
                            "pulling AES keys from key broker {}:{} topic {control}",
                            kb.host,
                            kb.port
                        ),
                        Err(e) => log::warn!("key-broker subscribe failed: {e}"),
                    }
                    // Announce under the (possibly legacy) gateway identity the keys were
                    // provisioned for, so the backend pushes them to our control topic.
                    let gw = kb.gwid.clone().unwrap_or_else(|| cfg.gwid.clone());
                    match p.publish_to(
                        &format!("meter/data/{gw}"),
                        &startup_announcement(&gw, Some(&control)),
                    ) {
                        Ok(()) => log::info!("announced startup as gateway {gw} on key broker"),
                        Err(e) => log::warn!("key-broker startup announce failed: {e}"),
                    }
                    Some(p)
                }
                Err(e) => {
                    log::warn!("key-broker connect failed ({e}); keys from primary/seeded only");
                    None
                }
            }
        });

        // SIGTERM (systemd stop) / SIGINT (^C) request a clean shutdown: the loop breaks
        // and parks the radio instead of the process dying mid-SPI-transaction, which can
        // wedge it in D-state holding the bus until reboot.
        let shutdown = Arc::new(AtomicBool::new(false));
        {
            let flag = shutdown.clone();
            tokio::spawn(async move {
                use tokio::signal::unix::{signal, SignalKind};
                let mut term = match signal(SignalKind::terminate()) {
                    Ok(s) => s,
                    Err(e) => {
                        log::warn!("cannot listen for SIGTERM: {e}");
                        return;
                    }
                };
                tokio::select! {
                    _ = term.recv() => {}
                    _ = tokio::signal::ctrl_c() => {}
                }
                flag.store(true, Ordering::SeqCst);
            });
        }

        let mut radio = source::Rfm69Source::open(&spidev).await?;
        radio.start().await?;
        log::info!("metermon-rs monitor on {spidev} — reporting every {report_secs}s");

        let mut stats: BTreeMap<u32, MeterStat> = BTreeMap::new();
        let start = tokio::time::Instant::now();
        let mut last_report = Instant::now();
        let interval = Duration::from_secs(report_secs);
        let sweep_interval = Duration::from_secs(sweep_hours.saturating_mul(3600));
        let mut last_sweep = Instant::now();
        if sweep_hours > 0 {
            log::info!("airwave discovery sweep enabled: every {sweep_hours}h");
        }

        // Gateway watchdog: recover the radio if no frame arrives within the stall window
        // (5 min vs the normal sub-minute cadence), escalating to a process restart after
        // 3 failed in-process recoveries (systemd relaunches).
        let mut watchdog = health::RadioWatchdog::new(300, 3);
        let mut last_frame_at: Option<Instant> = None;
        let mut radio_recoveries: u32 = 0;
        // CRC-failed frames with unknown (untrustworthy) ids, contained by the store.
        let mut contained: u64 = 0;
        // Record layouts learned from full frames, so the compact frames that make up
        // most of a Kamstrup meter's traffic decode to readings rather than blobs.
        let mut layouts = mbus_rs::wmbus::compact_frame::CompactLayoutCache::new();

        loop {
            if shutdown.load(Ordering::SeqCst) {
                break;
            }
            if let Some((frame, rssi, freq_offset)) = radio.poll().await? {
                last_frame_at = Some(Instant::now()); // any received frame proves RX is alive
                let (v, has_key) = {
                    let k = keys.lock().unwrap();
                    let v = {
                        let ps = profile_store.lock().unwrap();
                        decode::decode_frame_with_cache(&frame, &cfg, &k, &mut layouts, &ps)
                    };
                    let meter = v["meterid"].as_u64().unwrap_or(0) as u32;
                    (v, k.get(meter).is_some())
                };
                let meter = v["meterid"].as_u64().unwrap_or(0) as u32;
                let crc_ok = v["crc_ok"].as_bool().unwrap_or(false);
                let ft = v["frame_type"].as_str().unwrap_or("?").to_string();
                let ci = v["ci"].as_str().unwrap_or("-").to_string();
                // Human-readable summary of the decoded records (e.g. "25.555 m^3,
                // 18 C"). Only from a CRC-valid frame: a corrupted frame's values are
                // not a reading, however cleanly they happen to parse.
                let reading = crc_ok.then(|| summarize_records(&v)).flatten();

                // Persist to the device manager (info + significant/sampled events). The
                // store decides attribution: a CRC-failed frame with an unknown id is
                // contained (no device minted), and the in-memory stats follow suit so
                // ghost ids never appear in the report either.
                let obs = devices::Observation {
                    meter_id: meter,
                    manufacturer: v["manufacturer"].as_str().unwrap_or("?").to_string(),
                    version: v["version"].as_u64().unwrap_or(0) as u8,
                    device_type: v["type"].as_u64().unwrap_or(0) as u8,
                    frame_type: ft.clone(),
                    ci: ci
                        .strip_prefix("0x")
                        .and_then(|h| u8::from_str_radix(h, 16).ok()),
                    encrypted: v["encrypted"].as_bool().unwrap_or(false),
                    crc_ok,
                    rssi,
                    has_key,
                    reading: reading.clone(),
                    mode: radio.mode().to_string(),
                    freq_offset_hz: freq_offset,
                    applied_quirks: v["applied_quirks"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|q| q.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default(),
                };
                let attributed = match dm.record_frame(&obs) {
                    Ok(a) => a,
                    Err(err) => {
                        log::warn!("device store write failed: {err}");
                        true
                    }
                };
                if attributed {
                    let e = stats.entry(meter).or_default();
                    e.total += 1;
                    e.last_rssi = rssi;
                    e.sum_rssi += rssi as i64;
                    e.has_key = has_key;
                    if crc_ok {
                        e.ok += 1;
                    } else {
                        e.fail += 1;
                    }
                } else {
                    contained += 1;
                }

                match &reading {
                    Some(r) => log::info!(
                        "frame meter={meter} type={ft} CI={ci} crc_ok={crc_ok} key={has_key} rssi={rssi}dBm off={freq_offset}Hz reading=[{r}]"
                    ),
                    None => log::info!(
                        "frame meter={meter} type={ft} CI={ci} crc_ok={crc_ok} key={has_key} rssi={rssi}dBm off={freq_offset}Hz"
                    ),
                }
            } else {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }

            if last_report.elapsed() >= interval {
                if let Err(err) = dm.check_silence() {
                    log::warn!("silence check failed: {err}");
                }
                let nkeys = keys.lock().unwrap().len();
                print_stats(&stats, start.elapsed(), nkeys, contained);
                // redb is single-writer, so the standalone `devices` viewer can't open the
                // store while we hold it — print the persisted device report here too.
                if let Err(err) = dm.print_report(10) {
                    log::warn!("device report failed: {err}");
                }

                // --- gateway self-instrumentation: health heartbeat + radio watchdog ---
                let opmode = radio.opmode().await;
                let uptime = start.elapsed().as_secs();
                let total: u64 = stats.values().map(|s| s.total).sum();
                let ok: u64 = stats.values().map(|s| s.ok).sum();
                // "no frame yet" counts its age as the uptime, so a radio that is dead from
                // boot still trips the watchdog rather than waiting forever for a first frame.
                let frame_age = last_frame_at.map(|t| t.elapsed().as_secs());
                let stall_age = frame_age.unwrap_or(uptime);
                let gw = health::GatewayHealth {
                    gwid: cfg.gwid.clone(),
                    status: "online",
                    ts: devices::now_unix(),
                    uptime_secs: uptime,
                    radio_opmode: opmode,
                    radio_state: opmode
                        .map(|o| health::RadioState::from_opmode(o).as_str())
                        .unwrap_or("unknown"),
                    frames_per_min: if uptime > 0 {
                        total as f64 * 60.0 / uptime as f64
                    } else {
                        0.0
                    },
                    last_frame_age_secs: frame_age,
                    radio_recoveries,
                    meters_seen: stats.len(),
                    crc_pass_pct: (total > 0).then(|| (ok * 100 / total) as u8),
                    keys_held: nkeys,
                    cpu_temp_c: health::read_cpu_temp_c(),
                    load_avg_1m: health::read_load_avg_1m(),
                };
                if let Some(p) = publisher.as_mut() {
                    if let Err(e) = p.publish_to(&health_topic, &gw.to_json()) {
                        log::warn!("gateway health publish failed: {e}");
                    }
                }

                match watchdog.assess(stall_age) {
                    health::WatchdogAction::Healthy => {}
                    health::WatchdogAction::Recover { attempt } => {
                        radio_recoveries += 1;
                        log::warn!(
                            "radio stalled ({stall_age}s no frame, opmode={opmode:?}) — recovery attempt {attempt}"
                        );
                        let _ = dm.record_discovery(&format!(
                            "radio stall: {stall_age}s no frame, opmode={opmode:?}, recovery {attempt}"
                        ));
                        // Cheap in-process recovery (re-arm RX). If it works a frame arrives and
                        // resets the stall; if not, the age keeps growing to the next attempt.
                        if let Err(e) = radio.recover().await {
                            log::error!("radio recover failed: {e}");
                        }
                    }
                    health::WatchdogAction::Escalate => {
                        log::error!(
                            "radio unrecoverable after {} attempts — exiting for systemd restart",
                            watchdog.attempts()
                        );
                        let _ = dm.record_discovery(&format!(
                            "radio unrecoverable after {} attempts; restarting process",
                            watchdog.attempts()
                        ));
                        if let Some(p) = publisher.as_mut() {
                            let payload = serde_json::to_vec(&serde_json::json!({
                                "gwid": cfg.gwid, "status": "offline", "reason": "radio_unrecoverable"
                            }))
                            .unwrap_or_default();
                            let _ = p.publish_retained(&status_topic, &payload);
                        }
                        std::process::exit(1);
                    }
                }

                last_report = Instant::now();
            }

            // Periodic airwave discovery sweep: pause C reception, scan C/T + S, record.
            if sweep_hours > 0 && last_sweep.elapsed() >= sweep_interval {
                log::info!("airwave sweep: starting (pausing C reception ~2 min)");
                drop(radio); // release SPI for the raw sync-off capture
                match sweep::sweep_once(&spidev, 60) {
                    Ok((meters, t, s)) => {
                        let summary = format!("C:{meters:?} T-bursts:{t} S-bursts:{s}");
                        if let Err(e) = dm.record_discovery(&summary) {
                            log::warn!("discovery record failed: {e}");
                        }
                        log::info!("airwave sweep done: C meters {meters:?}, T-bursts {t}, S-bursts {s}");
                    }
                    Err(e) => log::warn!("airwave sweep failed: {e}"),
                }
                // Always restore C reception.
                radio = source::Rfm69Source::open(&spidev).await?;
                radio.start().await?;
                last_sweep = Instant::now();
            }
        }

        // Clean shutdown: retained offline status (the LWT only covers ungraceful
        // deaths), then park the radio before dropping the SPI handle.
        log::info!("shutdown requested — parking radio");
        if let Some(p) = publisher.as_mut() {
            let payload = serde_json::to_vec(&serde_json::json!({
                "gwid": cfg.gwid, "status": "offline", "reason": "shutdown"
            }))
            .unwrap_or_default();
            let _ = p.publish_retained(&status_topic, &payload);
        }
        if let Err(e) = radio.stop().await {
            log::warn!("radio shutdown failed: {e}");
        }
        log::info!("metermon-rs monitor stopped cleanly");
        Ok(())
    })
}

/// Condense a decoded frame's records into a short human-readable reading, e.g.
/// `"25.555 m^3, 18 C"`. Returns `None` when the frame carried no decoded records
/// (not plaintext, or encrypted without a usable key).
#[cfg(feature = "radio")]
fn summarize_records(v: &serde_json::Value) -> Option<String> {
    let records = v["records"].as_array()?;
    let parts: Vec<String> = records
        .iter()
        .filter_map(|r| {
            let value = r.get("value")?;
            let unit = r.get("unit").and_then(|u| u.as_str()).unwrap_or("");
            let text = match value {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => return None,
            };
            Some(if unit.is_empty() {
                text
            } else {
                format!("{text} {unit}")
            })
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

#[cfg(feature = "radio")]
fn print_stats(
    stats: &std::collections::BTreeMap<u32, MeterStat>,
    uptime: std::time::Duration,
    keys_held: usize,
    contained: u64,
) {
    let mins = uptime.as_secs_f64() / 60.0;
    println!("\n=== metermon-rs stats (uptime {mins:.1} min, {keys_held} AES key(s) held) ===");
    println!(
        "{:>10}  {:>5}  {:>5}  {:>5}  {:>6}  {:>8}  {:>8}  {:>3}",
        "meter", "total", "ok", "fail", "pass%", "avg_rssi", "last_rssi", "key"
    );
    let (mut t, mut o) = (0u64, 0u64);
    for (meter, s) in stats {
        let pass = if s.total > 0 {
            s.ok as f64 / s.total as f64 * 100.0
        } else {
            0.0
        };
        let avg = if s.total > 0 {
            s.sum_rssi / s.total as i64
        } else {
            0
        };
        println!(
            "{meter:>10}  {:>5}  {:>5}  {:>5}  {:>5.0}%  {:>6}dBm  {:>6}dBm  {:>3}",
            s.total,
            s.ok,
            s.fail,
            pass,
            avg,
            s.last_rssi,
            if s.has_key { "yes" } else { "-" }
        );
        t += s.total;
        o += s.ok;
    }
    let overall = if t > 0 {
        o as f64 / t as f64 * 100.0
    } else {
        0.0
    };
    println!(
        "total frames={t}  ok={o}  overall pass={overall:.0}%  unattributed-contained={contained}\n"
    );
}

#[cfg(not(feature = "radio"))]
fn run_monitor(
    _config_path: &str,
    _report_secs: u64,
    _keys_path: Option<&str>,
    _db_path: &str,
    _sweep_hours: u64,
) -> Result<()> {
    bail_no_radio("monitor")
}

/// Read-only RFM69 register dump over raw SPI — no reset, no reconfigure.
#[cfg(feature = "radio")]
fn run_dumpregs(config_path: &str) -> Result<()> {
    use rppal::spi::{BitOrder, Bus, Mode, SlaveSelect, Spi};

    let cfg = Config::load(config_path)?;
    let spidev = cfg
        .devices
        .values()
        .find(|d| d.dev_type.eq_ignore_ascii_case("WMBUS"))
        .and_then(|d| d.spidev.clone())
        .ok_or_else(|| anyhow::anyhow!("no WMBUS device with a spidev in config"))?;

    // Parse /dev/spidevB.C → (Bus, SlaveSelect).
    let tail = spidev
        .rsplit_once("spidev")
        .map(|(_, t)| t)
        .unwrap_or("0.0");
    let (b, c) = tail.split_once('.').unwrap_or(("0", "0"));
    let bus = match b.trim().parse::<u8>().unwrap_or(0) {
        1 => Bus::Spi1,
        2 => Bus::Spi2,
        _ => Bus::Spi0,
    };
    let ss = match c.trim().parse::<u8>().unwrap_or(0) {
        1 => SlaveSelect::Ss1,
        2 => SlaveSelect::Ss2,
        _ => SlaveSelect::Ss0,
    };

    let mut spi = Spi::new(bus, ss, 1_000_000, Mode::Mode0)?;
    spi.set_bit_order(BitOrder::MsbFirst)?;

    // Same register set metermon-rs dumps after its own config, for a direct diff.
    let regs: &[(&str, u8)] = &[
        ("OPMODE", 0x01),
        ("DATAMODUL", 0x02),
        ("BITRATEMSB", 0x03),
        ("BITRATELSB", 0x04),
        ("FDEVMSB", 0x05),
        ("FDEVLSB", 0x06),
        ("FRFMSB", 0x07),
        ("FRFMID", 0x08),
        ("FRFLSB", 0x09),
        ("LNA", 0x18),
        ("RXBW", 0x19),
        ("AFCBW", 0x1A),
        ("AFCFEI", 0x1E),
        ("RSSITHRESH", 0x29),
        ("RXTIMEOUT1", 0x2A),
        ("RXTIMEOUT2", 0x2B),
        ("PREAMBLEMSB", 0x2C),
        ("PREAMBLELSB", 0x2D),
        ("SYNCCONFIG", 0x2E),
        ("SYNCVALUE1", 0x2F),
        ("SYNCVALUE2", 0x30),
        ("SYNCVALUE3", 0x31),
        ("SYNCVALUE4", 0x32),
        ("SYNCVALUE5", 0x33),
        ("SYNCVALUE6", 0x34),
        ("SYNCVALUE7", 0x35),
        ("SYNCVALUE8", 0x36),
        ("PACKETCONFIG1", 0x37),
        ("PAYLOADLENGTH", 0x38),
        ("FIFOTHRESH", 0x3C),
        ("PACKETCONFIG2", 0x3D),
        ("DIOMAPPING1", 0x25),
        ("DIOMAPPING2", 0x26),
        ("TESTLNA", 0x58),
        ("TESTDAGC", 0x6F),
        ("TESTAFC", 0x71),
    ];
    println!("# RFM69 register dump from {spidev} (read-only, chip state retained)");
    for (name, reg) in regs {
        let mut rx = [0u8; 2];
        // Read transaction: address with MSB clear, then a dummy byte.
        spi.transfer(&mut rx, &[reg & 0x7F, 0x00])?;
        println!("{name} 0x{reg:02X} = 0x{:02X}", rx[1]);
    }
    Ok(())
}

#[cfg(not(feature = "radio"))]
fn run_dumpregs(_config_path: &str) -> Result<()> {
    bail_no_radio("dumpregs")
}

/// Sync-DISABLED raw bitstream capture: capture the demodulated stream (via
/// [`sweep::capture_raw`]) and write it to a binary file for offline C/T/S analysis.
/// Mode-agnostic. Run with the monitor stopped.
#[cfg(feature = "radio")]
fn run_probe_raw(
    config_path: &str,
    out: &str,
    seconds: u64,
    freq_hz: u64,
    bitrate_bps: u32,
) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let spidev = cfg
        .devices
        .values()
        .find(|d| d.dev_type.eq_ignore_ascii_case("WMBUS"))
        .and_then(|d| d.spidev.clone())
        .ok_or_else(|| anyhow::anyhow!("no WMBUS device with a spidev in config"))?;

    log::info!(
        "probe-raw: sync-OFF capture on {spidev} @ {:.3} MHz, {bitrate_bps} bps, {seconds}s -> {out}",
        freq_hz as f64 / 1e6
    );
    let raw = sweep::capture_raw(&spidev, freq_hz, bitrate_bps, seconds)?;
    std::fs::write(out, &raw)?;
    log::info!(
        "probe-raw: captured {} bytes ({:.1} KB/s) -> {out}",
        raw.len(),
        raw.len() as f64 / 1024.0 / seconds as f64
    );
    Ok(())
}

#[cfg(not(feature = "radio"))]
fn run_probe_raw(
    _config_path: &str,
    _out: &str,
    _seconds: u64,
    _freq_hz: u64,
    _bitrate_bps: u32,
) -> Result<()> {
    bail_no_radio("probe-raw")
}

/// A/B input path: identical bytes -> decode core -> JSON. Host-independent.
fn run_replay(capture: &str, config_path: &str, keys_path: Option<&str>) -> Result<()> {
    // Config is optional for replay: without keys we still decode headers/CRC.
    let cfg = Config::load(config_path).unwrap_or_else(|_| default_config());

    // Keys: production gets them over the control topic; for offline replay they
    // come from a captured key file (and/or the config's optional seed map).
    let mut keys = KeyStore::new();
    keys.seed_from_config(&cfg);
    if let Some(path) = keys_path {
        let loaded = KeyStore::load_file(path)?;
        for (id, k) in loaded.iter() {
            keys.install(id, k.to_string());
        }
    }
    if keys.is_empty() {
        log::warn!("no AES keys loaded; encrypted frames will decode to header only");
    }

    let mut src = FileReplaySource::from_path(capture)?;
    // A capture file is a time-ordered stream, so layouts learned from full frames
    // apply to the compact frames that follow — same as live.
    let mut layouts = mbus_rs::wmbus::compact_frame::CompactLayoutCache::new();
    while let Some(frame) = src.next_frame()? {
        let decoded = decode::decode_frame_with_cache(
            &frame,
            &cfg,
            &keys,
            &mut layouts,
            &crate::profiles::ProfileStore::new(),
        );
        println!("{}", serde_json::to_string(&decoded)?);
    }
    Ok(())
}

fn default_config() -> Config {
    serde_json::from_str(
        r#"{"gwid":"0","mqtt":{"host":"localhost","clientid":"metermon-rs","data-topic":"meter/data/0"}}"#,
    )
    .expect("valid default config")
}

#[cfg(feature = "radio")]
fn run_live(config_path: &str, shadow: bool) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let device = cfg
        .devices
        .values()
        .find(|d| d.dev_type.eq_ignore_ascii_case("WMBUS"))
        .ok_or_else(|| anyhow::anyhow!("no WMBUS device in config"))?;
    let spidev = device
        .spidev
        .clone()
        .ok_or_else(|| anyhow::anyhow!("WMBUS device has no spidev"))?;

    let shadow_topic = shadow.then(|| format!("{}-rust", cfg.mqtt.data_topic));
    // Shared keystore: fed live from the control topic exactly as metermon is.
    let keys = std::sync::Arc::new(std::sync::Mutex::new(KeyStore::new()));
    {
        let mut k = keys.lock().unwrap();
        k.seed_from_config(&cfg);
    }

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut radio = source::Rfm69Source::open(&spidev).await?;
        let mut pub_ = publish::Publisher::connect(&cfg.mqtt, shadow_topic.as_deref(), None)?;

        // Subscribe to meter/control/<gwid> and install keys from op:key messages.
        if let Some(control) = &cfg.mqtt.control_topic {
            let keys_cb = keys.clone();
            pub_.subscribe_control(control, move |msg| {
                if let Some(id) = keys_cb.lock().unwrap().handle_control(msg) {
                    log::info!("installed key for meter {id}");
                }
            })?;
            log::info!("listening for keys on {control}");
        }

        log::info!(
            "metermon-rs live on {spidev} -> mqtt topic {}",
            pub_.topic()
        );
        radio.start().await?;
        loop {
            if let Some((frame, _rssi, _off)) = radio.poll().await? {
                let decoded = {
                    let k = keys.lock().unwrap();
                    decode::decode_frame(&frame, &cfg, &k)
                };
                if let Err(e) = pub_.publish_json(&decoded) {
                    log::warn!("publish failed: {e}");
                }
            } else {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        }
    })
}

#[cfg(not(feature = "radio"))]
fn run_live(_config_path: &str, _shadow: bool) -> Result<()> {
    bail_no_radio("run")
}

/// Capture raw frames from the radio to a hex file (no decode, no MQTT).
#[cfg(feature = "radio")]
fn run_capture(config_path: &str, out: &str, seconds: u64, count: usize) -> Result<()> {
    use std::io::Write;

    let cfg = Config::load(config_path)?;
    let spidev = cfg
        .devices
        .values()
        .find(|d| d.dev_type.eq_ignore_ascii_case("WMBUS"))
        .and_then(|d| d.spidev.clone())
        .ok_or_else(|| anyhow::anyhow!("no WMBUS device with a spidev in config"))?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut radio = source::Rfm69Source::open(&spidev).await?;
        radio.start().await?;
        let mut file = std::fs::File::create(out)?;
        writeln!(
            file,
            "# metermon-rs capture from {spidev} (one raw wM-Bus frame per line, hex)"
        )?;

        log::info!("capturing up to {count} frames / {seconds}s from {spidev} -> {out}");
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(seconds);
        let mut n = 0usize;
        while n < count && tokio::time::Instant::now() < deadline {
            let remaining = deadline - tokio::time::Instant::now();
            match tokio::time::timeout(remaining, radio.poll()).await {
                Ok(Ok(Some((frame, rssi, off)))) => {
                    writeln!(file, "{}", hex::encode(&frame))?;
                    file.flush()?;
                    n += 1;
                    log::info!(
                        "frame {n}: {} bytes rssi={rssi}dBm off={off}Hz  {}",
                        frame.len(),
                        hex::encode(&frame)
                    );
                }
                Ok(Ok(None)) => {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
                Ok(Err(e)) => log::warn!("recv error: {e}"),
                Err(_) => break, // deadline
            }
        }
        log::info!("captured {n} frames to {out}");
        Ok(())
    })
}

#[cfg(not(feature = "radio"))]
fn run_capture(_config_path: &str, _out: &str, _seconds: u64, _count: usize) -> Result<()> {
    bail_no_radio("capture")
}

#[cfg(not(feature = "radio"))]
fn bail_no_radio(cmd: &str) -> Result<()> {
    anyhow::bail!(
        "the `{cmd}` subcommand needs the `radio` feature (Raspberry Pi + RFM69). \
         Build with: cargo build --features radio  (on the Pi). \
         Use `replay` for the host-independent A/B."
    )
}
