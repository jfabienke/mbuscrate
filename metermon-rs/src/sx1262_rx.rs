//! Instrumented wM-Bus receive on an SX1262.
//!
//! Two jobs. Get frames off the air, and make it obvious *why* when it doesn't:
//! configuration is verified by reading registers back rather than assumed, the
//! chip's own error and IRQ registers are decoded by name, and a periodic heartbeat
//! reports what the radio thinks it is doing even when nothing arrives — silence with
//! no explanation is the failure mode that cost the most time on the RFM69.

use anyhow::{Context, Result};
use mbus_rs::wmbus::mode_c::decode_mode_c;
use mbus_rs::wmbus::radio::hal::raspberry_pi::GpioPins;
use mbus_rs::wmbus::radio::hal::{Hal, RaspberryPiHal};
use mbus_rs::wmbus::radio::driver::Sx126xDriver;
use std::time::{Duration, Instant};

const OP_GET_STATUS: u8 = 0xC0;
const OP_GET_DEVICE_ERRORS: u8 = 0x17;
const OP_GET_IRQ_STATUS: u8 = 0x12;
const OP_CLEAR_IRQ_STATUS: u8 = 0x02;
const OP_GET_RX_BUFFER_STATUS: u8 = 0x13;
const OP_GET_PACKET_STATUS: u8 = 0x14;
const OP_GET_RSSI_INST: u8 = 0x15;
const OP_READ_BUFFER: u8 = 0x1E;
const OP_GET_PACKET_TYPE: u8 = 0x11;
/// Holds an ASCII chip identifier ("SX1261" / "SX1262"). Reading it proves the SPI
/// link carries real data rather than plausible-looking zeros.
const REG_VERSION_STRING: u16 = 0x0320;

/// wM-Bus mode C: 868.95 MHz, 100 kbps.
const WMBUS_FREQ_HZ: u32 = 868_950_000;
const WMBUS_BITRATE: u32 = 100_000;

fn irq_names(irq: u16) -> Vec<&'static str> {
    const F: &[(u16, &str)] = &[
        (1 << 0, "TxDone"),
        (1 << 1, "RxDone"),
        (1 << 2, "PreambleDetected"),
        (1 << 3, "SyncWordValid"),
        (1 << 4, "HeaderValid"),
        (1 << 5, "HeaderErr"),
        (1 << 6, "CrcErr"),
        (1 << 7, "CadDone"),
        (1 << 8, "CadDetected"),
        (1 << 9, "Timeout"),
    ];
    F.iter()
        .filter(|(b, _)| irq & b != 0)
        .map(|(_, n)| *n)
        .collect()
}

fn device_error_names(errs: u16) -> Vec<&'static str> {
    const F: &[(u16, &str)] = &[
        (1 << 0, "RC64K calib"),
        (1 << 1, "RC13M calib"),
        (1 << 2, "PLL calib"),
        (1 << 3, "ADC calib"),
        (1 << 4, "IMG calib"),
        (1 << 5, "XOSC start"),
        (1 << 6, "PLL lock"),
        (1 << 8, "PA ramp"),
    ];
    F.iter()
        .filter(|(b, _)| errs & b != 0)
        .map(|(_, n)| *n)
        .collect()
}

/// Read a command whose reply *is* the status byte (GetStatus alone).
fn read_status(hal: &mut RaspberryPiHal, op: u8) -> Result<u8> {
    let mut b = [0u8; 1];
    hal.read_command(op, &mut b)?;
    Ok(b[0])
}

/// Read a one-byte result that follows the status byte.
///
/// Every SX126x Get* reply begins with status; asking for a one-byte buffer
/// returns that status and nothing else. Reading it as the result is silent —
/// the status byte in RX mode is 0xD2, which as an RSSI decodes to a perfectly
/// believable -105 dBm that never changes.
fn read1_after_status(hal: &mut RaspberryPiHal, op: u8) -> Result<u8> {
    let mut b = [0u8; 2];
    hal.read_command(op, &mut b)?;
    Ok(b[1])
}

fn read_u16_after_status(hal: &mut RaspberryPiHal, op: u8) -> Result<u16> {
    let mut b = [0u8; 3];
    hal.read_command(op, &mut b)?;
    Ok(u16::from_be_bytes([b[1], b[2]]))
}

/// Running tally, printed on the heartbeat so a silent radio still reports its state.
struct Counters {
    /// Noise-floor spread over the heartbeat window. A receiver with no antenna
    /// reads a dead-flat floor; a connected one wanders by several dB as ambient
    /// energy comes and goes. That difference distinguishes "quiet band" from
    /// "nothing plugged in", which otherwise look identical.
    rssi_min: i16,
    rssi_max: i16,
    rssi_samples: u64,
    /// Histogram over the raw RSSI byte. Preamble detection depends on the whole
    /// modem being configured correctly, but a transmission is visible as raw energy
    /// regardless: a wM-Bus frame is ~10 ms of carrier, so if meters are audible at
    /// all the distribution must show a tail well above the floor. No tail means the
    /// problem is in front of the modem, not inside it.
    rssi_hist: [u32; 256],
    preambles: u64,
    syncs: u64,
    rx_done: u64,
    crc_err: u64,
    decoded_ok: u64,
    decode_fail: u64,
}

impl Default for Counters {
    fn default() -> Self {
        Self {
            rssi_min: i16::MAX,
            rssi_max: i16::MIN,
            rssi_samples: 0,
            rssi_hist: [0u32; 256],
            preambles: 0,
            syncs: 0,
            rx_done: 0,
            crc_err: 0,
            decoded_ok: 0,
            decode_fail: 0,
        }
    }
}

impl Counters {
    /// Percentile over the raw RSSI byte. The raw value ascends as the signal
    /// weakens, so the histogram is walked from the top down to go from weak to
    /// strong.
    fn percentile(&self, p: f64) -> i16 {
        let total: u32 = self.rssi_hist.iter().sum();
        let target = ((total as f64) * p) as u32;
        let mut acc = 0u32;
        for (raw, n) in self.rssi_hist.iter().enumerate().rev() {
            acc += n;
            if acc >= target && *n > 0 {
                return -(raw as i16) / 2;
            }
        }
        0
    }

    /// Samples more than 12 dB above the median, i.e. plausible transmissions.
    fn bursts_above(&self, threshold_dbm: i16) -> u32 {
        self.rssi_hist
            .iter()
            .enumerate()
            .filter(|(raw, _)| -(*raw as i16) / 2 >= threshold_dbm)
            .map(|(_, n)| *n)
            .sum()
    }
}

pub fn run(
    spidev: &str,
    pins: GpioPins,
    rf_switch: Option<u8>,
    rf_switch_high: bool,
    dio2_rf_switch: bool,
    sync_bytes: u8,
    preamble_detect_bits: u8,
    sync_hex: Option<String>,
    freq_hz: u32,
    capture: Option<String>,
    seconds: u64,
) -> Result<()> {
    println!("SX1262 wM-Bus mode C receive — 868.95 MHz, 100 kbps, sync 54 3D 54\n");

    let mut hal = RaspberryPiHal::from_spidev(spidev, &pins).context("opening SPI/GPIO")?;
    hal.reset().context("reset")?;

    // The antenna switch needs its non-DIO2 leg held for receive. On the Waveshare
    // HAT that is BCM6, and the sense is inverted from the usual convention: HIGH
    // selects RX. Get this wrong and the radio is healthy but deaf.
    if let Some(pin) = rf_switch {
        let level = if rf_switch_high { "dh" } else { "dl" };
        std::process::Command::new("pinctrl")
            .args(["set", &pin.to_string(), "op", level])
            .status()
            .ok();
        println!(
            "RF switch: GPIO{pin} driven {} · DIO2-as-RF-switch {}",
            if rf_switch_high { "HIGH" } else { "LOW" },
            if dio2_rf_switch { "on" } else { "off" }
        );
    }

    let mut driver = Sx126xDriver::new(hal, 32_000_000);
    let mut profile = mbus_rs::wmbus::radio::driver::WmbusProfile::mode_c(freq_hz, WMBUS_BITRATE);
    profile.sync_word_len = sync_bytes;
    profile.preamble_detect_bits = preamble_detect_bits;
    if let Some(h) = sync_hex.as_deref() {
        let raw = hex::decode(h).context("--sync-hex must be hex bytes")?;
        for (slot, b) in profile.sync_word.iter_mut().zip(raw.iter()) {
            *slot = *b;
        }
    }
    // Re-run the chip's self-calibration before configuring. The power-on pass
    // happens before the chip knows its band or modem, and this is a bring-up tool
    // where paying a few milliseconds for a known-good starting point is worth it.
    driver.calibrate(0x7F).context("calibrating")?;
    // configure_for_wmbus selects the GFSK modem and applies the stock profile;
    // re-applying with the overrides keeps that setup and only changes detection.
    driver
        .configure_for_wmbus(freq_hz, WMBUS_BITRATE)
        .context("selecting the GFSK modem")?;
    driver
        .apply_wmbus_profile(&profile)
        .context("applying the wM-Bus mode C profile")?;
    println!(
        "{:.3} MHz · sync {:02X?} · match {sync_bytes} byte(s) · preamble detector {preamble_detect_bits} bits",
        freq_hz as f64 / 1e6,
        &profile.sync_word[..sync_bytes as usize]
    );
    // After the profile: applying it rewrites the DIO configuration, so claiming DIO2
    // for the antenna switch beforehand would be undone.
    driver
        .set_dio2_as_rf_switch(dio2_rf_switch)
        .context("DIO2 as RF switch")?;
    driver
        .set_rx_boosted_gain(true)
        .context("enabling RX boosted gain")?;

    // Verify rather than assume: read back what the chip actually holds. A profile
    // that failed to apply looks exactly like an empty band.
    let hal = driver.hal_mut();
    let mut sync = [0u8; 3];
    hal.read_register(0x06C0, &mut sync).ok();
    let _ = &sync;
    let status = read_status(hal, OP_GET_STATUS)?;
    let errs = read_u16_after_status(hal, OP_GET_DEVICE_ERRORS)?;

    // Identity and modem selection, read back from the chip. GFSK is packet type 0x00;
    // if this reports LoRa then the demodulator is running the wrong modem and no
    // amount of GFSK packet configuration can make it hear anything.
    let mut version = [0u8; 16];
    hal.read_register(REG_VERSION_STRING, &mut version).ok();
    let chip: String = version
        .iter()
        .take_while(|b| b.is_ascii_graphic())
        .map(|b| *b as char)
        .collect();
    let packet_type = read1_after_status(hal, OP_GET_PACKET_TYPE)?;
    println!(
        "chip {:?} · packet type 0x{packet_type:02X} ({})",
        chip,
        match packet_type {
            0x00 => "GFSK",
            0x01 => "LoRa",
            0x02 => "BPSK",
            0x03 => "LR-FHSS",
            _ => "unknown",
        }
    );
    println!(
        "config readback: sync {:02X?} · status 0x{status:02X} · errors {:?}",
        sync,
        device_error_names(errs)
    );
    if sync[..sync_bytes.min(3) as usize] != profile.sync_word[..sync_bytes.min(3) as usize] {
        println!("  WARNING: sync word did not take — reception will not work.");
    }

    driver.set_rx_continuous().context("entering RX")?;
    println!("listening for {seconds}s (heartbeat every 15s)\n");

    // Raw capture: one hex frame per line, the exact bytes handed to the decoder,
    // so `metermon-rs replay` reproduces this session and A/B rigs feed the same
    // decode path.
    let mut capture_file = match capture.as_deref() {
        Some(path) => Some(std::io::BufWriter::new(
            std::fs::File::create(path).with_context(|| format!("creating {path}"))?,
        )),
        None => None,
    };

    let start = Instant::now();
    let mut last_beat = Instant::now();
    let mut c = Counters::default();
    let mut last_rssi_sample = Instant::now();

    while start.elapsed() < Duration::from_secs(seconds) {
        let hal = driver.hal_mut();
        let irq = read_u16_after_status(hal, OP_GET_IRQ_STATUS)?;

        if irq != 0 {
            // Count the whole receive chain, not just completions: preamble without
            // sync means the band is live but the sync word is wrong, and sync
            // without RxDone means the packet engine is mis-parameterised. Those are
            // different faults and each has a different fix.
            if irq & (1 << 2) != 0 {
                c.preambles += 1;
            }
            if irq & (1 << 3) != 0 {
                c.syncs += 1;
            }
            if irq & (1 << 6) != 0 {
                c.crc_err += 1;
            }

            if irq & (1 << 1) != 0 {
                c.rx_done += 1;
                let mut st = [0u8; 3];
                hal.read_command(OP_GET_RX_BUFFER_STATUS, &mut st)?;
                let (len, offset) = (st[1], st[2]);

                // GFSK reply: status, RxStatus, RssiSync, RssiAvg. RssiSync is
                // latched when the sync word matched, i.e. during the actual frame.
                let mut pkt = [0u8; 4];
                hal.read_command(OP_GET_PACKET_STATUS, &mut pkt)?;
                let rssi_dbm = -(pkt[2] as i16) / 2;

                let mut buf = vec![0u8; len as usize];
                hal.read_register_buffer(offset, &mut buf)
                    .context("reading the RX buffer")?;

                if let Some(f) = capture_file.as_mut() {
                    use std::io::Write;
                    let _ = writeln!(f, "{}", hex::encode(&buf));
                }
                match decode_mode_c(&buf) {
                    Ok(f) => {
                        c.decoded_ok += 1;
                        println!(
                            "RX {len:3}B rssi {rssi_dbm:>4} dBm  meter {:>8}  mfr {}  crc_ok {}",
                            f.device_address,
                            mbus_rs::id_to_manufacturer(f.manufacturer_id),
                            f.crc_ok
                        );
                    }
                    Err(e) => {
                        c.decode_fail += 1;
                        println!(
                            "RX {len:3}B rssi {rssi_dbm:>4} dBm  undecodable: {e}\n     {}",
                            hex::encode(&buf)
                        );
                    }
                }
            }
            let hal = driver.hal_mut();
            hal.write_command(OP_CLEAR_IRQ_STATUS, &[0xFF, 0xFF])?;
        }

        if last_rssi_sample.elapsed() >= Duration::from_millis(2) {
            let hal = driver.hal_mut();
            let raw = read1_after_status(hal, OP_GET_RSSI_INST)?;
            c.rssi_hist[raw as usize] += 1;
            let r = -(raw as i16) / 2;
            c.rssi_min = c.rssi_min.min(r);
            c.rssi_max = c.rssi_max.max(r);
            c.rssi_samples += 1;
            last_rssi_sample = Instant::now();
        }

        if last_beat.elapsed() >= Duration::from_secs(15) {
            let hal = driver.hal_mut();
            let status = read_status(hal, OP_GET_STATUS)?;
            let errs = read_u16_after_status(hal, OP_GET_DEVICE_ERRORS)?;
            let rssi = read1_after_status(hal, OP_GET_RSSI_INST)?;
            println!(
                "  ── {:>3}s · mode {} · floor {} dBm (min {} max {} spread {} over {} samples) · preamble {} sync {} rxdone {} crcerr {} decoded {}/{} · errors {:?}",
                start.elapsed().as_secs(),
                match (status >> 4) & 0x07 {
                    0x2 => "STBY_RC",
                    0x3 => "STBY_XOSC",
                    0x4 => "FS",
                    0x5 => "RX",
                    0x6 => "TX",
                    _ => "?",
                },
                -(rssi as i16) / 2,
                c.rssi_min,
                c.rssi_max,
                c.rssi_max - c.rssi_min,
                c.rssi_samples,
                c.preambles,
                c.syncs,
                c.rx_done,
                c.crc_err,
                c.decoded_ok,
                c.decoded_ok + c.decode_fail,
                device_error_names(errs),
            );
            // Machine-readable twin of the line above, so the radio's state can be
            // scraped, trended or shipped upstream without parsing prose.
            let floor = c.percentile(0.5);
            println!(
                "TELEMETRY {{\"t\":{},\"mode\":\"{}\",\"chip_status\":{},\"errors\":{},\
                 \"rssi_floor_dbm\":{},\"rssi_p90_dbm\":{},\"rssi_p99_dbm\":{},\"rssi_max_dbm\":{},\
                 \"rssi_samples\":{},\"bursts\":{},\"preamble\":{},\"sync\":{},\"rxdone\":{},\
                 \"crcerr\":{},\"decoded\":{},\"decode_fail\":{}}}",
                start.elapsed().as_secs(),
                match (status >> 4) & 0x07 {
                    0x2 => "STBY_RC",
                    0x3 => "STBY_XOSC",
                    0x4 => "FS",
                    0x5 => "RX",
                    0x6 => "TX",
                    _ => "?",
                },
                status,
                errs,
                floor,
                c.percentile(0.10),
                c.percentile(0.01),
                c.rssi_max,
                c.rssi_samples,
                c.bursts_above(floor + 12),
                c.preambles,
                c.syncs,
                c.rx_done,
                c.crc_err,
                c.decoded_ok,
                c.decode_fail,
            );
            last_beat = Instant::now();
        }
        std::thread::sleep(Duration::from_micros(200));
    }

    println!(
        "\n── {}s summary ──\n  preamble {} · sync {} · RxDone {} · CRC err {} · decoded {} of {}",
        seconds,
        c.preambles,
        c.syncs,
        c.rx_done,
        c.crc_err,
        c.decoded_ok,
        c.decoded_ok + c.decode_fail
    );
    let total: u32 = c.rssi_hist.iter().sum();
    let floor = c.percentile(0.5);
    let burst_threshold = floor + 12;
    let bursts = c.bursts_above(burst_threshold);
    println!(
        "  RSSI {} samples · p50 {} · p90 {} · p99 {} · max {} dBm\n  \
         samples >{} dBm (floor+12): {} ({:.3}%)",
        total,
        floor,
        c.percentile(0.10),
        c.percentile(0.01),
        c.rssi_max,
        burst_threshold,
        bursts,
        100.0 * bursts as f64 / total.max(1) as f64
    );

    if c.rx_done == 0 {
        println!(
            "\n  No packets completed. Read the counters above:\n  \
             · preamble 0, sync 0  -> nothing on air, or the RF switch/antenna path is wrong\n  \
             · preamble > 0, sync 0 -> band is live but the sync word does not match\n  \
             · sync > 0, RxDone 0   -> sync matches but the packet parameters mis-size the frame"
        );
    }
    Ok(())
}
