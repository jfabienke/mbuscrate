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

fn read1(hal: &mut RaspberryPiHal, op: u8) -> Result<u8> {
    let mut b = [0u8; 1];
    hal.read_command(op, &mut b)?;
    Ok(b[0])
}

fn read_u16_after_status(hal: &mut RaspberryPiHal, op: u8) -> Result<u16> {
    let mut b = [0u8; 3];
    hal.read_command(op, &mut b)?;
    Ok(u16::from_be_bytes([b[1], b[2]]))
}

/// Running tally, printed on the heartbeat so a silent radio still reports its state.
#[derive(Default)]
struct Counters {
    preambles: u64,
    syncs: u64,
    rx_done: u64,
    crc_err: u64,
    decoded_ok: u64,
    decode_fail: u64,
}

pub fn run(spidev: &str, pins: GpioPins, rf_switch: Option<u8>, seconds: u64) -> Result<()> {
    println!("SX1262 wM-Bus mode C receive — 868.95 MHz, 100 kbps, sync 54 3D 54\n");

    let mut hal = RaspberryPiHal::from_spidev(spidev, &pins).context("opening SPI/GPIO")?;
    hal.reset().context("reset")?;

    // The antenna switch needs its non-DIO2 leg held for receive. On the Waveshare
    // HAT that is BCM6, and the sense is inverted from the usual convention: HIGH
    // selects RX. Get this wrong and the radio is healthy but deaf.
    if let Some(pin) = rf_switch {
        std::process::Command::new("pinctrl")
            .args(["set", &pin.to_string(), "op", "dh"])
            .status()
            .ok();
        println!("RF switch: GPIO{pin} driven HIGH (receive path)");
    }

    let mut driver = Sx126xDriver::new(hal, 32_000_000);
    driver
        .set_dio2_as_rf_switch(true)
        .context("DIO2 as RF switch")?;
    driver
        .configure_for_wmbus(WMBUS_FREQ_HZ, WMBUS_BITRATE)
        .context("applying the wM-Bus mode C profile")?;
    driver
        .set_rx_boosted_gain(true)
        .context("enabling RX boosted gain")?;

    // Verify rather than assume: read back what the chip actually holds. A profile
    // that failed to apply looks exactly like an empty band.
    let hal = driver.hal_mut();
    let mut sync = [0u8; 3];
    hal.read_register(0x06C0, &mut sync).ok();
    let status = read1(hal, OP_GET_STATUS)?;
    let errs = read_u16_after_status(hal, OP_GET_DEVICE_ERRORS)?;
    println!(
        "config readback: sync {:02X?} (want [54, 3D, 54]) · status 0x{status:02X} · errors {:?}",
        sync,
        device_error_names(errs)
    );
    if sync != [0x54, 0x3D, 0x54] {
        println!("  WARNING: sync word did not take — reception will not work.");
    }

    driver.set_rx_continuous().context("entering RX")?;
    println!("listening for {seconds}s (heartbeat every 15s)\n");

    let start = Instant::now();
    let mut last_beat = Instant::now();
    let mut c = Counters::default();

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

                let mut pkt = [0u8; 4];
                hal.read_command(OP_GET_PACKET_STATUS, &mut pkt)?;
                let rssi_dbm = -(pkt[1] as i16) / 2;

                let mut buf = vec![0u8; len as usize];
                hal.read_register_buffer(offset, &mut buf)
                    .context("reading the RX buffer")?;

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

        if last_beat.elapsed() >= Duration::from_secs(15) {
            let hal = driver.hal_mut();
            let status = read1(hal, OP_GET_STATUS)?;
            let errs = read_u16_after_status(hal, OP_GET_DEVICE_ERRORS)?;
            let rssi = read1(hal, OP_GET_RSSI_INST)?;
            println!(
                "  ── {:>3}s · mode {} · noise floor {} dBm · preamble {} sync {} rxdone {} crcerr {} decoded {}/{} · errors {:?}",
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
                c.preambles,
                c.syncs,
                c.rx_done,
                c.crc_err,
                c.decoded_ok,
                c.decoded_ok + c.decode_fail,
                device_error_names(errs),
            );
            last_beat = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(2));
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
