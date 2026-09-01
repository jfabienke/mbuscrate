//! Instrumented LoRa receive/transmit on an SX1262, over the seeed driver.
//!
//! A single SX126x demodulates one (frequency, SF, bandwidth) combination at a
//! time, unlike a real LoRaWAN gateway chip that hears eight in parallel — so this
//! probe either parks on one setting or sweeps a schedule of them, dwelling on
//! each long enough to catch periodic uplinks. Received frames get their LoRaWAN
//! header decoded (message type, DevAddr or Join EUIs) — enough to recognise a
//! device without implementing MAC-layer crypto.
//!
//! Radio layer: the SX1262 is driven through the seeed `radio-sx126x` driver (async),
//! so each entry point wraps its work in one `block_on`; the radio is a local of
//! inferred type (never named), and the antenna switch is owned by the driver's
//! `PinSwitch` — no `pinctrl` subprocess. RX-gain boost is a board-profile fact under
//! this driver, not a per-call toggle.
#![cfg(feature = "seeed-radio")]

use anyhow::{Context, Result};
use radio_core::error::Error;
use radio_core::lora::{Bandwidth, Ldro, LoraConfig, LoraSync, Profile};
use radio_core::traits::{DualMode, LoraReceiver};
use radio_core::units::{Dbm, Hertz};
use radio_sx126x::Sx1262;
use std::time::{Duration, Instant};

use crate::source_seeed::board_wiring;

/// SPI clock for the probe's SX1262 (clamped to the board max in `sx1262_parts`).
const SPI_HZ: u32 = 8_000_000;

/// Public LoRaWAN sync word. Private LoRa networks use the chip default 0x1424.
const LORAWAN_PUBLIC_SYNC: u16 = 0x3444;

/// Placeholder payload length for implicit-header (fixed-length) sweep points. Implicit
/// mode has no length field on air, so the receiver must be told the exact size — for a
/// discovery *hunt* that size is unknown by definition, so this is a best-effort guess.
/// Explicit-header points (the common case) ignore it.
const IMPLICIT_HUNT_LEN: u8 = 32;

fn require_sf(n: u8) -> Result<u8> {
    if (5..=12).contains(&n) {
        Ok(n)
    } else {
        anyhow::bail!("SF must be 5..=12, got {n}")
    }
}

fn bw_from(khz: u32) -> Result<Bandwidth> {
    Ok(match khz {
        125 => Bandwidth::Bw125,
        250 => Bandwidth::Bw250,
        500 => Bandwidth::Bw500,
        _ => anyhow::bail!("bandwidth must be 125, 250 or 500 kHz, got {khz}"),
    })
}

fn sync_from(sync: u16) -> LoraSync {
    match sync {
        0x3444 => LoraSync::Public,
        0x1424 => LoraSync::Private,
        other => LoraSync::Custom(other),
    }
}

/// Render a LoRaWAN EUI (transmitted little-endian) in its conventional
/// big-endian, colon-separated display form.
fn eui_display(le_bytes: &[u8]) -> String {
    le_bytes
        .iter()
        .rev()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// One line describing a LoRaWAN PHYPayload, from the MHDR alone plus the fields
/// that are cleartext for that message type. No crypto — identification only.
fn describe_lorawan(payload: &[u8]) -> String {
    if payload.is_empty() {
        return "empty".into();
    }
    // Printable ASCII is a bench beacon, not LoRaWAN. Worth checking first: a
    // LoRaWAN reading of "PICO-BEACON-00001" yields a confident and entirely
    // fictitious UnconfirmedDataUp from DevAddr 2D4F4349 — which is just "ICO-"
    // little-endian. A decoder that always finds structure hides its own errors.
    if payload.len() >= 4
        && payload
            .iter()
            .all(|b| (0x20..0x7F).contains(b) || *b == b'\n' || *b == b'\r')
    {
        return format!("text {:?}", String::from_utf8_lossy(payload).trim_end());
    }
    let mhdr = payload[0];
    let mtype = mhdr >> 5;
    match mtype {
        0b000 if payload.len() >= 23 => {
            // JoinRequest: MHDR | JoinEUI(8) | DevEUI(8) | DevNonce(2) | MIC(4)
            format!(
                "JoinRequest join_eui={} dev_eui={} nonce={:04X}",
                eui_display(&payload[1..9]),
                eui_display(&payload[9..17]),
                u16::from_le_bytes([payload[17], payload[18]])
            )
        }
        0b010..=0b101 if payload.len() >= 12 => {
            // Data up/down: MHDR | DevAddr(4) | FCtrl | FCnt(2) | ...
            let dir = if mtype == 0b010 || mtype == 0b100 {
                "Up"
            } else {
                "Down"
            };
            let conf = if mtype >= 0b100 {
                "Confirmed"
            } else {
                "Unconfirmed"
            };
            format!(
                "{conf}Data{dir} dev_addr={:08X} fcnt={}",
                u32::from_le_bytes([payload[1], payload[2], payload[3], payload[4]]),
                u16::from_le_bytes([payload[6], payload[7]])
            )
        }
        0b001 => "JoinAccept (encrypted)".into(),
        0b111 => "Proprietary".into(),
        _ => format!("mtype={mtype} (short/unknown)"),
    }
}

/// Transmit `count` frames on the LoRa profile, for proving the gateway's TX path.
///
/// The antenna switch is handled inside the driver's `transmit_lora` (TXEN via the
/// board `PinSwitch`), so unlike the old path there is no `pinctrl` bracketing here.
#[allow(clippy::too_many_arguments)]
pub fn transmit(
    spidev: &str,
    board: Option<&str>,
    freq_hz: u32,
    sf: u8,
    bw_khz: u32,
    private_sync: bool,
    power_dbm: i8,
    count: u32,
    interval_ms: u64,
) -> Result<()> {
    let sf = require_sf(sf)?;
    let bw = bw_from(bw_khz)?;
    let sync = if private_sync {
        0x1424
    } else {
        LORAWAN_PUBLIC_SYNC
    };
    println!(
        "SX1262 LoRa transmit — {:.3} MHz SF{sf} BW{bw_khz} sync 0x{sync:04X} {power_dbm} dBm",
        freq_hz as f64 / 1e6
    );

    let (b, wiring) = board_wiring(board)?;
    if !spidev.is_empty() && spidev != wiring.spidev {
        log::debug!(
            "lora tx: config spidev {spidev} differs from the board wiring's {}; using the wiring",
            wiring.spidev
        );
    }
    let (spi, busy, irq, reset, delay, clock) =
        radio_linux::rpi::sx1262_parts(&wiring, &b, SPI_HZ).context("opening SX1262 bus")?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let radio = Sx1262::new(spi, busy, irq, reset, delay, clock, &b, Hertz(freq_hz))
            .await
            .map_err(|e| anyhow::anyhow!("Sx1262::new: {e:?}"))?;
        let sw = radio_linux::rpi::sx1262_rf_switch(&wiring, &b).context("rf switch")?;
        let mut radio = radio.with_rf_switch(sw);

        // The LoRaWAN uplink preset (explicit header, CRC on, normal IQ), with the sweep's
        // bandwidth and sync overlaid — matching the old LoRaProfile this replaces.
        let mut cfg = LoraConfig::lorawan_uplink(Hertz(freq_hz), sf);
        cfg.bw = bw;
        cfg.sync = sync_from(sync);

        for n in 1..=count {
            let msg = format!("GATEWAY-TX-{n:05}");
            match radio
                .transmit_lora(&cfg, msg.as_bytes(), Dbm(power_dbm as f32))
                .await
            {
                Ok(()) => println!("tx {n}: {msg:?}"),
                Err(e) => println!("tx {n}: FAILED {e:?}"),
            }
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
        }
        Ok::<(), anyhow::Error>(())
    })
}

pub struct SweepPoint {
    pub freq_hz: u32,
    pub sf: u8,
    pub bw_khz: u32,
    pub sync: u16,
    pub implicit: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    spidev: &str,
    board: Option<&str>,
    freq_hz: u32,
    sf: u8,
    bw_khz: u32,
    sweep: bool,
    hunt_e22: bool,
    dwell_secs: u64,
    private_sync: bool,
    rx_boost: bool,
    capture: Option<String>,
    seconds: u64,
) -> Result<()> {
    // The EU868 join channels every LoRaWAN device must use, crossed with the SF
    // ladder devices climb as joins go unanswered. Slow SFs get proportionally
    // more dwell because their airtime is longer and their duty cycle sparser.
    let sync = if private_sync {
        0x1424
    } else {
        LORAWAN_PUBLIC_SYNC
    };
    let schedule: Vec<SweepPoint> = if hunt_e22 {
        // E22 discovery: the module's air-rate presets map to undocumented
        // (SF, BW) pairs and its sync word is unverified, so cover the whole
        // space at its (documented) channel frequency. A transmitter looping
        // every ~2s guarantees at least two frames land in each 5s dwell of
        // the matching point — one full pass is bounded and decisive.
        // Ordered most-likely-first: the module reports a 2.4k air rate, which on
        // an SX1262 is SF10/BW125, and Ebyte's default sync is the private 0x1424.
        // Header mode is included because it must match exactly — an explicit-header
        // receiver is deaf to an implicit-header sender with no error to say so.
        let mut v = Vec::new();
        for sy in [0x1424u16, LORAWAN_PUBLIC_SYNC] {
            for implicit in [false, true] {
                for bw in [125u32, 250, 500] {
                    for sf_n in [10u8, 9, 11, 12, 8, 7, 6, 5] {
                        v.push(SweepPoint {
                            freq_hz,
                            sf: sf_n,
                            bw_khz: bw,
                            sync: sy,
                            implicit,
                        });
                    }
                }
            }
        }
        v
    } else if sweep {
        let mut v = Vec::new();
        for sf_n in [12u8, 9, 7, 10, 8, 11] {
            for f in [868_100_000u32, 868_300_000, 868_500_000] {
                v.push(SweepPoint {
                    freq_hz: f,
                    sf: sf_n,
                    bw_khz,
                    sync,
                    implicit: false,
                });
            }
        }
        v
    } else {
        vec![SweepPoint {
            freq_hz,
            sf: require_sf(sf)?,
            bw_khz,
            sync,
            implicit: false,
        }]
    };
    println!(
        "SX1262 LoRa receive — {}",
        if hunt_e22 {
            format!(
                "E22 hunt at {:.3} MHz: {} (SF x BW x sync) points, {dwell_secs}s dwell",
                freq_hz as f64 / 1e6,
                schedule.len()
            )
        } else if sweep {
            format!(
                "sweep {} points, {dwell_secs}s dwell · sync 0x{sync:04X}",
                schedule.len()
            )
        } else {
            format!(
                "{:.3} MHz SF{sf} BW{bw_khz} · sync 0x{sync:04X}",
                freq_hz as f64 / 1e6
            )
        },
    );
    if !rx_boost {
        // Under the seeed driver, RX-gain is set from the board profile at configure time
        // (Boosted on the Waveshare HAT), not switched per call. The old close-range
        // "boost off" knob has no seeed equivalent here.
        println!(
            "note: --no-rx-boost has no effect on the seeed driver (gain is board-profile-set)"
        );
    }

    let (b, wiring) = board_wiring(board)?;
    if !spidev.is_empty() && spidev != wiring.spidev {
        log::debug!(
            "lora rx: config spidev {spidev} differs from the board wiring's {}; using the wiring",
            wiring.spidev
        );
    }
    let (spi, busy, irq, reset, delay, clock) =
        radio_linux::rpi::sx1262_parts(&wiring, &b, SPI_HZ).context("opening SX1262 bus")?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let radio = Sx1262::new(spi, busy, irq, reset, delay, clock, &b, Hertz(freq_hz))
            .await
            .map_err(|e| anyhow::anyhow!("Sx1262::new: {e:?}"))?;
        let sw = radio_linux::rpi::sx1262_rf_switch(&wiring, &b).context("rf switch")?;
        let mut radio = radio.with_rf_switch(sw);

        let mut capture_file = match capture.as_deref() {
            Some(path) => Some(std::io::BufWriter::new(
                std::fs::File::create(path).with_context(|| format!("creating {path}"))?,
            )),
            None => None,
        };

        let mut buf = [0u8; 256];
        let start = Instant::now();
        let mut frames = 0u64;
        let mut point_idx = 0usize;

        'outer: while start.elapsed() < Duration::from_secs(seconds) {
            let point = &schedule[point_idx % schedule.len()];
            point_idx += 1;

            let cfg = LoraConfig {
                freq: Hertz(point.freq_hz),
                sf: point.sf,
                bw: bw_from(point.bw_khz)?,
                cr: 5,
                ldro: Ldro::Auto,
                preamble: 8,
                implicit_len: point.implicit.then_some(IMPLICIT_HUNT_LEN),
                crc: true,
                invert_iq: false,
                sync: sync_from(point.sync),
            };
            radio
                .switch_profile(&Profile::Lora(cfg))
                .await
                .map_err(|e| anyhow::anyhow!("profile switch: {e:?}"))?;
            if sweep || hunt_e22 {
                println!(
                    "── {:>4}s · listening {:.3} MHz SF{} BW{} sync 0x{:04X} {}",
                    start.elapsed().as_secs(),
                    point.freq_hz as f64 / 1e6,
                    point.sf,
                    point.bw_khz,
                    point.sync,
                    if point.implicit {
                        "implicit"
                    } else {
                        "explicit"
                    }
                );
            }

            let dwell_end = Instant::now() + Duration::from_secs(dwell_secs);
            while Instant::now() < dwell_end {
                if start.elapsed() >= Duration::from_secs(seconds) {
                    break 'outer;
                }
                // `receive_lora` self-bounds (~1 s) and returns `IrqTimeout` on an empty
                // window; CRC/header-failed packets are counted and skipped inside the
                // driver, so there is no separate error branch to poll here.
                match radio.receive_lora(&mut buf).await {
                    Ok(f) => {
                        frames += 1;
                        let payload = &buf[..f.len];
                        println!(
                            "RX {:>3}B rssi {:>4} dBm snr {:>5.1} dB ferr {:>6} Hz  {:.3} MHz SF{} BW{} sync 0x{:04X}  {}",
                            f.len,
                            f.rssi.0 as i16,
                            f.meta.snr_db,
                            f.meta.freq_error_hz.unwrap_or(0),
                            point.freq_hz as f64 / 1e6,
                            point.sf,
                            point.bw_khz,
                            point.sync,
                            if hunt_e22 {
                                format!("payload {:?}", String::from_utf8_lossy(payload))
                            } else {
                                describe_lorawan(payload)
                            }
                        );
                        if let Some(fh) = capture_file.as_mut() {
                            use std::io::Write;
                            let _ = writeln!(fh, "{}", hex::encode(payload));
                        }
                    }
                    Err(Error::IrqTimeout) => {}
                    Err(e) => log::debug!("receive_lora: {e:?}"),
                }
            }
        }

        println!(
            "\n── {}s summary ──\n  frames {frames} (CRC/header failures counted inside the driver)",
            start.elapsed().as_secs()
        );
        if frames == 0 {
            println!(
                "  Nothing decoded. LoRaWAN uplinks are sparse — a device transmitting \
                 every few minutes on one of 3 channels and 6 SFs is easy to miss in a \
                 short sweep. Longer runs or a known (channel, SF) pin the odds down."
            );
        }
        Ok::<(), anyhow::Error>(())
    })
}
