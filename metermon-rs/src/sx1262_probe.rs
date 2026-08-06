//! First-light probe for an SX1262 board — prove the chip answers before trusting
//! anything above it.
//!
//! Deliberately built at the HAL level rather than through `Sx126xDriver`: the point is
//! to find out whether power, SPI, the BUSY handshake, and the oscillator work at all,
//! and a driver that assumes those things cannot diagnose them.
//!
//! Every step reports its own verdict. That lesson is expensive: the RFM69 spent weeks
//! emitting "timeout waiting for mode ready", which turned out to cover a dead reset
//! line, an uncalibrated oscillator, and a chip that ignored mode writes — three faults
//! needing three different fixes, indistinguishable from the log. This probe separates
//! the layers so a failure names itself.

use anyhow::{Context, Result};
use mbus_rs::wmbus::radio::hal::raspberry_pi::GpioPins;
use mbus_rs::wmbus::radio::hal::{Hal, RaspberryPiHal};
use std::thread::sleep;
use std::time::{Duration, Instant};

// SX126x opcodes (Semtech DS.SX1261-2, §13). Named locally so the probe stays
// readable and independent of driver internals.
const OP_GET_STATUS: u8 = 0xC0;
const OP_GET_DEVICE_ERRORS: u8 = 0x17;
const OP_CLEAR_DEVICE_ERRORS: u8 = 0x07;
const OP_SET_STANDBY: u8 = 0x80;
const OP_SET_DIO3_AS_TCXO: u8 = 0x97;
const OP_CALIBRATE: u8 = 0x89;
/// Calibrate every block. Mandatory after switching the clock source to a TCXO:
/// the calibrations done at reset were against the RC oscillator and are invalid
/// once the reference changes. Skipping it leaves XOSC_START_ERR set forever.
const CALIBRATE_ALL: u8 = 0x7F;
const STANDBY_RC: u8 = 0x00;
const STANDBY_XOSC: u8 = 0x01;
/// LoRa sync-word register — a harmless scratch location for a write/read round trip.
const REG_LORA_SYNCWORD: u16 = 0x0740;

/// Chip mode from bits 6:4 of the status byte.
fn chip_mode(status: u8) -> &'static str {
    match (status >> 4) & 0x07 {
        0x0 => "unused",
        0x1 => "RFU",
        0x2 => "STBY_RC",
        0x3 => "STBY_XOSC",
        0x4 => "FS",
        0x5 => "RX",
        0x6 => "TX",
        _ => "?",
    }
}

/// Command status from bits 3:1 of the status byte.
fn cmd_status(status: u8) -> &'static str {
    match (status >> 1) & 0x07 {
        0x0 => "reserved",
        0x1 => "RFU",
        0x2 => "data available",
        0x3 => "command timeout",
        0x4 => "command processing error",
        0x5 => "failure to execute",
        0x6 => "command TX done",
        _ => "?",
    }
}

/// Decode `GetDeviceErrors`, which is the SX126x's own account of what failed to come
/// up. This is the single most valuable diagnostic on the chip: an oscillator that
/// never started or a PLL that never locked says so here by name.
fn device_errors(errs: u16) -> Vec<&'static str> {
    const FLAGS: &[(u16, &str)] = &[
        (1 << 0, "RC64K calibration"),
        (1 << 1, "RC13M calibration"),
        (1 << 2, "PLL calibration"),
        (1 << 3, "ADC calibration"),
        (1 << 4, "image calibration"),
        (1 << 5, "XOSC failed to start"),
        (1 << 6, "PLL failed to lock"),
        (1 << 8, "PA ramp"),
    ];
    FLAGS
        .iter()
        .filter(|(bit, _)| errs & bit != 0)
        .map(|(_, name)| *name)
        .collect()
}

fn get_status(hal: &mut RaspberryPiHal) -> Result<u8> {
    let mut buf = [0u8; 1];
    hal.read_command(OP_GET_STATUS, &mut buf)
        .context("GetStatus over SPI")?;
    Ok(buf[0])
}

fn get_device_errors(hal: &mut RaspberryPiHal) -> Result<u16> {
    let mut buf = [0u8; 3];
    hal.read_command(OP_GET_DEVICE_ERRORS, &mut buf)
        .context("GetDeviceErrors over SPI")?;
    // Response is [status, errors_hi, errors_lo].
    Ok(u16::from_be_bytes([buf[1], buf[2]]))
}

/// Run the probe. Read-mostly: it resets the chip, round-trips one scratch register
/// (restoring it), and tries to start the oscillator. It never transmits.
pub fn run(spidev: &str, pins: GpioPins, tcxo_mv: Option<u32>) -> Result<()> {
    println!("SX1262 first-light probe");
    println!("  spidev {spidev}");
    println!(
        "  nss {}  busy GPIO{}  dio1 GPIO{}  dio2 {}  reset {}",
        pins.nss
            .map_or("hardware CS".into(), |p| format!("GPIO{p}")),
        pins.busy,
        pins.dio1,
        pins.dio2.map_or("—".into(), |p| format!("GPIO{p}")),
        pins.reset.map_or("—".into(), |p| format!("GPIO{p}")),
    );
    println!();

    // --- 1. Claim the bus and pins -----------------------------------------
    let mut hal = RaspberryPiHal::from_spidev(spidev, &pins).with_context(|| {
        format!(
            "claiming {spidev} and GPIOs. If this fails with 'device or resource busy', \
             another process holds the bus (is the monitor running?)"
        )
    })?;
    println!("[1/6] SPI + GPIO claimed              OK");

    // --- 2. Reset ----------------------------------------------------------
    if pins.reset.is_some() {
        hal.reset().context("pulsing NRST")?;
        println!("[2/6] NRST pulsed                     OK");
    } else {
        println!("[2/6] NRST not configured             SKIPPED (chip keeps prior state)");
    }

    // --- 3. BUSY must fall ------------------------------------------------
    // BUSY high forever is the classic wrong-pin / unpowered signature: every SPI
    // command would silently wait on a line that never releases.
    let start = Instant::now();
    let mut busy_ok = false;
    while start.elapsed() < Duration::from_millis(500) {
        if !hal.busy() {
            busy_ok = true;
            break;
        }
        sleep(Duration::from_millis(5));
    }
    if busy_ok {
        println!(
            "[3/6] BUSY released after {:>3}ms         OK",
            start.elapsed().as_millis()
        );
    } else {
        println!("[3/6] BUSY stuck HIGH                 FAIL");
        println!(
            "\n  BUSY never went low. The chip is unpowered, or GPIO{} is not the BUSY \n  \
             line. Every SPI command waits on this pin, so nothing below can be trusted.",
            pins.busy
        );
        return Ok(());
    }

    // --- 4. Does the chip answer at all? -----------------------------------
    let status = get_status(&mut hal)?;
    println!(
        "[4/6] GetStatus 0x{status:02X}                  {}",
        if status == 0x00 || status == 0xFF {
            "FAIL"
        } else {
            "OK"
        }
    );
    println!(
        "        mode {} · command status {}",
        chip_mode(status),
        cmd_status(status)
    );
    if status == 0x00 || status == 0xFF {
        println!(
            "\n  An all-zero or all-ones status means MISO is not returning data: check \n  \
             the chip-select, MISO wiring, and that {spidev} is the right device."
        );
        return Ok(());
    }

    // --- 5. Register round trip proves the WRITE path ----------------------
    // Reading alone can pass on a half-working bus; writing and reading back cannot.
    // The raw response is printed because the SX126x returns status bytes ahead of
    // register data, and an off-by-one in that offset looks exactly like a chip that
    // ignores writes.
    let mut original = [0u8; 2];
    hal.read_register(REG_LORA_SYNCWORD, &mut original)
        .context("reading sync-word register")?;
    let sentinel = [original[0] ^ 0xFF, original[1] ^ 0xFF];
    hal.write_register(REG_LORA_SYNCWORD, &sentinel)?;
    let mut readback = [0u8; 2];
    hal.read_register(REG_LORA_SYNCWORD, &mut readback)?;
    hal.write_register(REG_LORA_SYNCWORD, &original)?; // restore
    let mut restored = [0u8; 2];
    hal.read_register(REG_LORA_SYNCWORD, &mut restored)?;

    let wrote_ok = readback == sentinel;
    println!(
        "[5/6] Register write/read round trip  {}",
        if wrote_ok { "OK" } else { "FAIL" }
    );
    println!(
        "        0x{:04X}: was {:02X?} · wrote {:02X?} · read {:02X?} · restored {:02X?}",
        REG_LORA_SYNCWORD, original, sentinel, readback, restored
    );
    if !wrote_ok {
        println!(
            "        (LoRa sync-word registers only latch once the packet type is set,
                      so a mismatch here is not conclusive on its own.)"
        );
    }

    // --- 6. Start the oscillator; determine TCXO empirically ---------------
    // A crystal board starts XOSC directly. A TCXO board cannot: DIO3 must power the
    // TCXO first, and without that the oscillator silently never starts. Rather than
    // assume which board this is, try plain XOSC and let GetDeviceErrors answer.
    hal.write_command(OP_SET_STANDBY, &[STANDBY_XOSC])?;
    sleep(Duration::from_millis(20));
    let mut st = get_status(&mut hal)?;
    let mut errs = get_device_errors(&mut hal)?;
    let mut tcxo_needed = false;

    if (st >> 4) & 0x07 != 0x3 || errs & (1 << 5) != 0 {
        // XOSC did not come up: configure DIO3 as TCXO supply and retry.
        let mv = tcxo_mv.unwrap_or(1700);
        let volt = match mv {
            1600 => 0x00,
            1700 => 0x01,
            1800 => 0x02,
            2200 => 0x03,
            2400 => 0x04,
            2700 => 0x05,
            3000 => 0x06,
            _ => 0x07, // 3.3 V
        };
        // Timeout is in 15.625 µs steps: 0x140 = 320 ticks = 5 ms, the value
        // Waveshare's own Core1262 demo uses for this module.
        let timeout: u32 = 0x140;
        hal.write_command(OP_CLEAR_DEVICE_ERRORS, &[0x00, 0x00])?;
        hal.write_command(
            OP_SET_DIO3_AS_TCXO,
            &[
                volt,
                ((timeout >> 16) & 0xFF) as u8,
                ((timeout >> 8) & 0xFF) as u8,
                (timeout & 0xFF) as u8,
            ],
        )?;
        // Recalibrate against the new reference before asking for XOSC.
        hal.write_command(OP_CALIBRATE, &[CALIBRATE_ALL])?;
        sleep(Duration::from_millis(20));
        hal.write_command(OP_SET_STANDBY, &[STANDBY_RC])?;
        sleep(Duration::from_millis(10));
        hal.write_command(OP_SET_STANDBY, &[STANDBY_XOSC])?;
        sleep(Duration::from_millis(50));
        st = get_status(&mut hal)?;
        errs = get_device_errors(&mut hal)?;
        tcxo_needed = (st >> 4) & 0x07 == 0x3 && errs & (1 << 5) == 0;
    }

    let xosc_up = (st >> 4) & 0x07 == 0x3 && errs & (1 << 5) == 0;
    println!(
        "[6/6] Oscillator (STBY_XOSC)          {}",
        if xosc_up { "OK" } else { "FAIL" }
    );
    let faults = device_errors(errs);
    if !faults.is_empty() {
        println!("        device errors: {}", faults.join(", "));
    }

    println!("\n── verdict ──");
    if xosc_up {
        println!("  The SX1262 is alive and its oscillator runs.");
        if tcxo_needed {
            println!(
                "  This board has a TCXO: XOSC only started after DIO3 was configured as\n  \
                 its supply. The driver MUST call configure_tcxo() before any RF use —\n  \
                 record this in the radio config."
            );
        } else {
            println!(
                "  Plain crystal: XOSC started without DIO3/TCXO configuration.\n  \
                 Do not enable TCXO in the driver config for this board."
            );
        }
        println!("  Next: configure a GFSK profile and attempt a wM-Bus receive.");
    } else {
        println!("  The chip answers SPI but its oscillator did not start.");
        println!("  If 'XOSC failed to start' is listed above and this board has a TCXO,");
        println!("  try --tcxo-mv with the board's actual TCXO voltage (1.6/1.7/1.8/2.2/");
        println!("  2.4/2.7/3.0/3.3 V). Otherwise suspect supply or the crystal itself.");
    }
    Ok(())
}
