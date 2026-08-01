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
mod keystore;
mod publish;
mod source;

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
    /// Run continuously, decoding frames and accumulating per-meter statistics
    /// (count, CRC pass/fail, RSSI). Prints a stats table periodically. (`radio` feature.)
    Monitor {
        #[arg(long, default_value = "metermon.conf")]
        config: String,
        /// Seconds between stats reports.
        #[arg(long, default_value_t = 60)]
        report: u64,
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
        Cmd::Monitor { config, report } => run_monitor(&config, report),
    }
}

/// Per-meter running statistics for the monitor.
#[derive(Default)]
struct MeterStat {
    total: u64,
    ok: u64,
    fail: u64,
    last_rssi: i16,
    sum_rssi: i64,
}

#[cfg(feature = "radio")]
fn run_monitor(config_path: &str, report_secs: u64) -> Result<()> {
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};

    let cfg = Config::load(config_path)?;
    let spidev = cfg
        .devices
        .values()
        .find(|d| d.dev_type.eq_ignore_ascii_case("WMBUS"))
        .and_then(|d| d.spidev.clone())
        .ok_or_else(|| anyhow::anyhow!("no WMBUS device with a spidev in config"))?;

    let mut keys = KeyStore::new();
    keys.seed_from_config(&cfg);

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut radio = source::Rfm69Source::open(&spidev).await?;
        radio.start().await?;
        log::info!("metermon-rs monitor on {spidev} — reporting every {report_secs}s");

        let mut stats: BTreeMap<u32, MeterStat> = BTreeMap::new();
        let start = tokio::time::Instant::now();
        let mut last_report = Instant::now();
        let interval = Duration::from_secs(report_secs);

        loop {
            if let Some((frame, rssi)) = radio.poll().await? {
                let v = decode::decode_frame(&frame, &cfg, &keys);
                let meter = v["meterid"].as_u64().unwrap_or(0) as u32;
                let crc_ok = v["crc_ok"].as_bool().unwrap_or(false);
                let ft = v["frame_type"].as_str().unwrap_or("?").to_string();
                let ci = v["ci"].as_str().unwrap_or("-").to_string();
                let e = stats.entry(meter).or_default();
                e.total += 1;
                e.last_rssi = rssi;
                e.sum_rssi += rssi as i64;
                if crc_ok {
                    e.ok += 1;
                } else {
                    e.fail += 1;
                }
                log::info!("frame meter={meter} type={ft} CI={ci} crc_ok={crc_ok} rssi={rssi}dBm");
            } else {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }

            if last_report.elapsed() >= interval {
                print_stats(&stats, start.elapsed());
                last_report = Instant::now();
            }
        }
    })
}

#[cfg(feature = "radio")]
fn print_stats(stats: &std::collections::BTreeMap<u32, MeterStat>, uptime: std::time::Duration) {
    let mins = uptime.as_secs_f64() / 60.0;
    println!("\n=== metermon-rs stats (uptime {:.1} min) ===", mins);
    println!(
        "{:>10}  {:>5}  {:>5}  {:>5}  {:>6}  {:>8}  {:>8}",
        "meter", "total", "ok", "fail", "pass%", "avg_rssi", "last_rssi"
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
            "{meter:>10}  {:>5}  {:>5}  {:>5}  {:>5.0}%  {:>6}dBm  {:>6}dBm",
            s.total, s.ok, s.fail, pass, avg, s.last_rssi
        );
        t += s.total;
        o += s.ok;
    }
    let overall = if t > 0 {
        o as f64 / t as f64 * 100.0
    } else {
        0.0
    };
    println!("total frames={t}  ok={o}  overall pass={overall:.0}%\n");
}

#[cfg(not(feature = "radio"))]
fn run_monitor(_config_path: &str, _report_secs: u64) -> Result<()> {
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
    while let Some(frame) = src.next_frame()? {
        let decoded = decode::decode_frame(&frame, &cfg, &keys);
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
        let mut pub_ = publish::Publisher::connect(&cfg.mqtt, shadow_topic.as_deref())?;

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
            if let Some((frame, _rssi)) = radio.poll().await? {
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
                Ok(Ok(Some((frame, rssi)))) => {
                    writeln!(file, "{}", hex::encode(&frame))?;
                    file.flush()?;
                    n += 1;
                    log::info!(
                        "frame {n}: {} bytes rssi={rssi}dBm  {}",
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
