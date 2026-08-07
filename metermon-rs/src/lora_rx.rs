//! Instrumented LoRa receive on an SX1262 — bring-up twin of `sx1262_rx`.
//!
//! A single SX126x demodulates one (frequency, SF, bandwidth) combination at a
//! time, unlike a real LoRaWAN gateway chip that hears eight in parallel — so this
//! probe either parks on one setting or sweeps a schedule of them, dwelling on
//! each long enough to catch periodic uplinks. Received frames get their LoRaWAN
//! header decoded (message type, DevAddr or Join EUIs) — enough to recognise a
//! device without implementing MAC-layer crypto.

use anyhow::{Context, Result};
use mbus_rs::wmbus::radio::driver::{LoRaProfile, RadioProfile, Sx126xDriver};
use mbus_rs::wmbus::radio::modulation::{CodingRate, LoRaBandwidth, SpreadingFactor};
use mbus_rs::wmbus::radio::hal::raspberry_pi::GpioPins;
use mbus_rs::wmbus::radio::hal::RaspberryPiHal;
use std::time::{Duration, Instant};

/// Public LoRaWAN sync word. Private LoRa networks use the chip default 0x1424.
const LORAWAN_PUBLIC_SYNC: u16 = 0x3444;

fn sf_from(n: u8) -> Result<SpreadingFactor> {
    Ok(match n {
        5 => SpreadingFactor::SF5,
        6 => SpreadingFactor::SF6,
        7 => SpreadingFactor::SF7,
        8 => SpreadingFactor::SF8,
        9 => SpreadingFactor::SF9,
        10 => SpreadingFactor::SF10,
        11 => SpreadingFactor::SF11,
        12 => SpreadingFactor::SF12,
        _ => anyhow::bail!("SF must be 5..=12, got {n}"),
    })
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
        0b010 | 0b100 | 0b011 | 0b101 if payload.len() >= 12 => {
            // Data up/down: MHDR | DevAddr(4) | FCtrl | FCnt(2) | ...
            let dir = if mtype == 0b010 || mtype == 0b100 {
                "Up"
            } else {
                "Down"
            };
            let conf = if mtype >= 0b100 { "Confirmed" } else { "Unconfirmed" };
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

pub struct SweepPoint {
    pub freq_hz: u32,
    pub sf: u8,
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    spidev: &str,
    pins: GpioPins,
    freq_hz: u32,
    sf: u8,
    sweep: bool,
    dwell_secs: u64,
    private_sync: bool,
    capture: Option<String>,
    seconds: u64,
) -> Result<()> {
    // The EU868 join channels every LoRaWAN device must use, crossed with the SF
    // ladder devices climb as joins go unanswered. Slow SFs get proportionally
    // more dwell because their airtime is longer and their duty cycle sparser.
    let schedule: Vec<SweepPoint> = if sweep {
        let mut v = Vec::new();
        for sf_n in [12u8, 9, 7, 10, 8, 11] {
            for f in [868_100_000u32, 868_300_000, 868_500_000] {
                v.push(SweepPoint { freq_hz: f, sf: sf_n });
            }
        }
        v
    } else {
        vec![SweepPoint { freq_hz, sf }]
    };

    let sync = if private_sync { 0x1424 } else { LORAWAN_PUBLIC_SYNC };
    println!(
        "SX1262 LoRa receive — {} · sync 0x{sync:04X} ({})",
        if sweep {
            format!("sweep {} points, {dwell_secs}s dwell", schedule.len())
        } else {
            format!("{:.3} MHz SF{sf}", freq_hz as f64 / 1e6)
        },
        if private_sync { "private" } else { "public LoRaWAN" },
    );

    let mut hal = RaspberryPiHal::from_spidev(spidev, &pins).context("opening SPI/GPIO")?;
    hal.reset().context("reset")?;

    // Antenna switch to receive — same hardware reality as wM-Bus: a switch left
    // in TX is a healthy-looking radio that hears nothing.
    std::process::Command::new("pinctrl")
        .args(["set", "6", "op", "dh"])
        .status()
        .ok();

    let mut driver = Sx126xDriver::new(hal, 32_000_000);
    driver.calibrate(0x7F).context("calibrating")?;
    driver.set_dio2_as_rf_switch(true).context("DIO2 RF switch")?;
    driver.set_rx_boosted_gain(true).context("RX boost")?;

    let mut capture_file = match capture.as_deref() {
        Some(path) => Some(std::io::BufWriter::new(
            std::fs::File::create(path).with_context(|| format!("creating {path}"))?,
        )),
        None => None,
    };

    let start = Instant::now();
    let mut frames = 0u64;
    let mut crc_errs = 0u64;
    let mut point_idx = 0usize;

    'outer: while start.elapsed() < Duration::from_secs(seconds) {
        let point = &schedule[point_idx % schedule.len()];
        point_idx += 1;

        let profile = RadioProfile::LoRa(LoRaProfile {
            frequency_hz: point.freq_hz,
            sf: sf_from(point.sf)?,
            bw: LoRaBandwidth::BW125,
            cr: CodingRate::CR4_5,
            power_dbm: 14,
            sync_word: Some(sync),
        });
        driver.switch_profile(&profile).context("profile switch")?;
        driver.set_rx_continuous().context("entering RX")?;
        if sweep {
            println!(
                "── {:>4}s · listening {:.3} MHz SF{}",
                start.elapsed().as_secs(),
                point.freq_hz as f64 / 1e6,
                point.sf
            );
        }

        let dwell_end = Instant::now() + Duration::from_secs(dwell_secs);
        while Instant::now() < dwell_end {
            if start.elapsed() >= Duration::from_secs(seconds) {
                break 'outer;
            }
            match driver.process_irqs_with_mode() {
                Ok(Some(pkt)) => {
                    frames += 1;
                    let lora = pkt.lora.as_ref();
                    println!(
                        "RX {:>3}B rssi {:>4} dBm snr {:>5.1} dB ferr {:>6} Hz  {:.3} MHz SF{}  {}",
                        pkt.payload.len(),
                        pkt.rssi_dbm,
                        lora.map(|l| l.snr_db).unwrap_or(f32::NAN),
                        lora.and_then(|l| l.freq_error_hz).unwrap_or(0),
                        point.freq_hz as f64 / 1e6,
                        point.sf,
                        describe_lorawan(&pkt.payload)
                    );
                    if let Some(f) = capture_file.as_mut() {
                        use std::io::Write;
                        let _ = writeln!(f, "{}", hex::encode(&pkt.payload));
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    crc_errs += 1;
                    log::debug!("irq processing: {e:?}");
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    println!(
        "\n── {}s summary ──\n  frames {frames} · errors {crc_errs}",
        start.elapsed().as_secs()
    );
    if frames == 0 {
        println!(
            "  Nothing decoded. LoRaWAN uplinks are sparse — a device transmitting \
             every few minutes on one of 3 channels and 6 SFs is easy to miss in a \
             short sweep. Longer runs or a known (channel, SF) pin the odds down."
        );
    }
    Ok(())
}
