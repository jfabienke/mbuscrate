//! Frame sources: where raw wM-Bus frames come from.
//!
//! The decode core is identical regardless of source. That is the whole point of
//! the capture-replay A/B: `FileReplaySource` feeds the *same bytes* a real radio
//! saw through the same decoder, deterministically, on any host — while
//! `Rfm69Source` (Pi-only, behind the `radio` feature) is the live path.

use anyhow::Result;

#[cfg(feature = "radio")]
use mbus_rs::wmbus::radio::hal::Hal;

/// A source of raw wM-Bus frames (each item is one frame: L-field..CRC inclusive).
pub trait FrameSource {
    /// Return the next frame, or `Ok(None)` when the source is exhausted.
    fn next_frame(&mut self) -> Result<Option<Vec<u8>>>;
}

/// Replays frames from a capture file: one hex-encoded frame per line
/// (`#` comments and blank lines ignored). This is the A/B input.
pub struct FileReplaySource {
    frames: std::vec::IntoIter<Vec<u8>>,
}

impl FileReplaySource {
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_str(&text)
    }

    pub fn from_str(text: &str) -> Result<Self> {
        let mut frames = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Tolerate spaces within a hex line (e.g. "68 04 04 68 ...").
            let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            let bytes = hex::decode(&compact)
                .map_err(|e| anyhow::anyhow!("line {}: invalid hex: {e}", i + 1))?;
            frames.push(bytes);
        }
        Ok(Self {
            frames: frames.into_iter(),
        })
    }
}

impl FrameSource for FileReplaySource {
    fn next_frame(&mut self) -> Result<Option<Vec<u8>>> {
        Ok(self.frames.next())
    }
}

/// Live RFM69 radio source (Raspberry Pi only).
///
/// Reaches the radio through the `RadioDriver` trait rather than `WMBusHandle`,
/// because the handle is hardcoded to the SX126x driver (mbus-rs Phase 2.2 gap).
/// This is the one place the client depends on that unfinished wiring.
#[cfg(feature = "radio")]
pub struct Rfm69Source {
    driver: mbus_rs::wmbus::radio::rfm69::Rfm69Driver,
}

#[cfg(feature = "radio")]
impl Rfm69Source {
    /// Open and initialize the RFM69 on `spidev` (radio not yet receiving).
    pub async fn open(spidev: &str) -> Result<Self> {
        use mbus_rs::wmbus::radio::rfm69::{Rfm69Config, Rfm69Driver};
        let cfg = Rfm69Config {
            spidev: Some(spidev.to_string()),
            ..Default::default()
        };
        let mut driver = Rfm69Driver::new(cfg).await?;
        driver.initialize().await?;
        Ok(Self { driver })
    }

    /// Enter continuous receive mode. Call once before polling.
    pub async fn start(&mut self) -> Result<()> {
        use mbus_rs::wmbus::radio::radio_driver::RadioDriver;
        self.driver.start_receive().await?;
        Ok(())
    }

    /// Non-blocking poll for one received frame `(bytes, rssi_dbm, freq_offset_hz)`,
    /// if available. `freq_offset_hz` is the AFC-measured carrier offset from the
    /// 868.95 MHz center for that frame (0 if the driver did not report one).
    pub async fn poll(&mut self) -> Result<Option<(Vec<u8>, i16, i32)>> {
        use mbus_rs::wmbus::radio::radio_driver::RadioDriver;
        Ok(self
            .driver
            .get_received_packet()
            .await?
            .map(|p| (p.data, p.rssi_dbm, p.freq_error_hz.unwrap_or(0))))
    }

    /// wM-Bus radio mode this source receives on. The RFM69 is a single-channel
    /// mode-C receiver (868.95 MHz, 100 kbps NRZ), so this is always "C".
    pub fn mode(&self) -> &'static str {
        "C"
    }

    /// Current radio operating-mode byte (RegOpMode) for gateway health/watchdog, or
    /// `None` if the read fails. Mode field (bits 4:2): 0x10 = RX, 0x04 = STANDBY.
    pub async fn opmode(&self) -> Option<u8> {
        self.driver.read_opmode().await.ok()
    }

    /// Re-arm the receiver (drain FIFO, restart RX) as cheap in-process recovery for a
    /// radio that has fallen out of RX. The supervisor escalates to a full process
    /// restart if this does not restore reception.
    pub async fn recover(&mut self) -> Result<()> {
        use mbus_rs::wmbus::radio::radio_driver::RadioDriver;
        // Re-arming alone cannot fix a chip whose oscillator is uncalibrated — the
        // recurring wedge on this gateway — so try the analog rescue first. It is a
        // no-op when the chip is healthy.
        match self.driver.recover_analog().await {
            Ok(true) => {}
            Ok(false) => log::error!(
                "radio needs a full power removal: RC calibration failed, which a reboot \
                 will not clear"
            ),
            Err(e) => log::warn!("analog recovery attempt failed: {e}"),
        }
        self.driver.start_receive().await?;
        Ok(())
    }

    /// Park the radio for process shutdown: stop the interrupt task and put the chip
    /// to sleep so the SPI bus is quiescent before the handle is dropped. A process
    /// that dies mid-SPI-transaction can wedge in uninterruptible D-state holding the
    /// bus until reboot.
    pub async fn stop(&mut self) -> Result<()> {
        self.driver.shutdown().await?;
        Ok(())
    }
}

/// Live SX1262 radio source (Raspberry Pi only) — the Waveshare SX1262 XXXM HAT.
///
/// Pin assignments are the HAT's, verified empirically at bring-up: NSS is a plain
/// GPIO (21), not a hardware chip-select, and the antenna switch needs GPIO6 held
/// HIGH for the receive path in addition to DIO2 driving the other leg.
#[cfg(feature = "radio")]
pub struct Sx1262Source {
    driver: mbus_rs::wmbus::radio::driver::Sx126xDriver<
        mbus_rs::wmbus::radio::hal::RaspberryPiHal,
    >,
}

#[cfg(feature = "radio")]
impl Sx1262Source {
    const FREQ_HZ: u32 = 868_950_000;
    const BITRATE: u32 = 100_000;
    /// Antenna-switch hold pin on the Waveshare HAT (TXEN, inverted: HIGH = RX).
    const RF_SWITCH_GPIO: u8 = 6;

    /// Open and initialize the SX1262 on `spidev` (radio not yet receiving).
    pub async fn open(spidev: &str) -> Result<Self> {
        use mbus_rs::wmbus::radio::driver::Sx126xDriver;
        use mbus_rs::wmbus::radio::hal::raspberry_pi::GpioPins;
        use mbus_rs::wmbus::radio::hal::RaspberryPiHal;

        let pins = GpioPins {
            nss: Some(21),
            busy: 20,
            dio1: 16,
            dio2: None, // driven by the chip as the RF switch, not read by us
            reset: Some(18),
        };
        let mut hal = RaspberryPiHal::from_spidev(spidev, &pins)?;
        hal.reset()?;
        Ok(Self {
            driver: Sx126xDriver::new(hal, 32_000_000),
        })
    }

    /// Configure for wM-Bus mode C and enter continuous receive.
    pub async fn start(&mut self) -> Result<()> {
        // The switch must be held before RX so the LNA is actually connected;
        // a radio with the switch in TX is healthy-looking but ~9 dB deaf.
        std::process::Command::new("pinctrl")
            .args(["set", &Self::RF_SWITCH_GPIO.to_string(), "op", "dh"])
            .status()
            .ok();
        // Recalibrate now that band and modem are known; the power-on pass ran
        // before either was configured.
        self.driver.calibrate(0x7F)?;
        self.driver
            .configure_for_wmbus(Self::FREQ_HZ, Self::BITRATE)?;
        self.driver.set_dio2_as_rf_switch(true)?;
        self.driver.set_rx_boosted_gain(true)?;
        self.driver.set_rx_continuous()?;
        Ok(())
    }

    /// Non-blocking poll for one received frame `(bytes, rssi_dbm, freq_offset_hz)`.
    /// The SX126x does not report a per-frame AFC offset, so the third field is 0.
    pub async fn poll(&mut self) -> Result<Option<(Vec<u8>, i16, i32)>> {
        let hal = self.driver.hal_mut();

        let mut irq_buf = [0u8; 3];
        hal.read_command(0x12, &mut irq_buf)?; // GetIrqStatus (status + 2 bytes)
        let irq = u16::from_be_bytes([irq_buf[1], irq_buf[2]]);
        if irq == 0 {
            return Ok(None);
        }

        let mut frame = None;
        if irq & (1 << 1) != 0 {
            // RxDone: fetch length/offset, sync-latched RSSI, then the payload.
            let mut st = [0u8; 3];
            hal.read_command(0x13, &mut st)?; // GetRxBufferStatus
            let (len, offset) = (st[1], st[2]);

            // GFSK GetPacketStatus reply: status, RxStatus, RssiSync, RssiAvg.
            let mut pkt = [0u8; 4];
            hal.read_command(0x14, &mut pkt)?;
            let rssi_dbm = -(pkt[2] as i16) / 2;

            let mut buf = vec![0u8; len as usize];
            hal.read_register_buffer(offset, &mut buf)?;
            frame = Some((buf, rssi_dbm, 0));
        }
        // Clear everything seen (preamble/sync events included) so DIO1 releases
        // and the next frame produces a fresh edge.
        hal.write_command(0x02, &[0xFF, 0xFF])?; // ClearIrqStatus
        Ok(frame)
    }

    /// wM-Bus radio mode this source receives on (single-channel mode C).
    pub fn mode(&self) -> &'static str {
        "C"
    }

    /// Chip status byte for gateway health/watchdog, or `None` if the read fails.
    /// Chip mode is bits 6:4 — 0x5 = RX, 0x2 = STBY_RC.
    pub async fn opmode(&mut self) -> Option<u8> {
        let mut b = [0u8; 1];
        self.driver.hal_mut().read_command(0xC0, &mut b).ok()?;
        Some(b[0])
    }

    /// Re-arm the receiver. `set_rx_continuous` restages the radio from standby
    /// (fallback mode, buffer base, stale IRQs, packet params), which is the
    /// documented recovery for a receiver that has fallen out of RX.
    pub async fn recover(&mut self) -> Result<()> {
        self.driver.calibrate(0x7F)?;
        self.driver.set_rx_continuous()?;
        Ok(())
    }

    /// Park the radio for process shutdown. Standby rather than sleep: sleep stops
    /// the crystal, and a warm restart from that state cost this gateway hours of
    /// debugging on the previous radio.
    pub async fn stop(&mut self) -> Result<()> {
        use mbus_rs::wmbus::radio::driver::StandbyMode;
        self.driver.set_standby(StandbyMode::RC)?;
        Ok(())
    }
}

/// Live radio dispatch: the gateway's config names the driver, everything
/// downstream sees one type. Methods mirror the source structs exactly.
#[cfg(feature = "radio")]
pub enum RadioSource {
    Rfm69(Rfm69Source),
    Sx1262(Box<Sx1262Source>),
}

#[cfg(feature = "radio")]
impl RadioSource {
    /// Open the radio named by `driver` ("sx1262" when absent — the installed HAT).
    pub async fn open(driver: Option<&str>, spidev: &str) -> Result<Self> {
        match driver.unwrap_or("sx1262") {
            "sx1262" => Ok(Self::Sx1262(Box::new(Sx1262Source::open(spidev).await?))),
            "rfm69" => Ok(Self::Rfm69(Rfm69Source::open(spidev).await?)),
            other => anyhow::bail!("unknown radio driver {other:?} (want sx1262 or rfm69)"),
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        match self {
            Self::Rfm69(r) => r.start().await,
            Self::Sx1262(r) => r.start().await,
        }
    }

    pub async fn poll(&mut self) -> Result<Option<(Vec<u8>, i16, i32)>> {
        match self {
            Self::Rfm69(r) => r.poll().await,
            Self::Sx1262(r) => r.poll().await,
        }
    }

    pub fn mode(&self) -> &'static str {
        match self {
            Self::Rfm69(r) => r.mode(),
            Self::Sx1262(r) => r.mode(),
        }
    }

    pub async fn opmode(&mut self) -> Option<u8> {
        match self {
            Self::Rfm69(r) => r.opmode().await,
            Self::Sx1262(r) => r.opmode().await,
        }
    }

    /// Interpret a raw `opmode()` byte for this chip. The RFM69 reports RegOpMode
    /// (mode in bits 4:2); the SX126x reports its status byte (mode in bits 6:4).
    /// Same wire, different dialects — decoding must follow the speaker.
    pub fn decode_state(&self, raw: u8) -> crate::health::RadioState {
        match self {
            Self::Rfm69(_) => crate::health::RadioState::from_opmode(raw),
            Self::Sx1262(_) => crate::health::RadioState::from_sx126x_status(raw),
        }
    }

    pub async fn recover(&mut self) -> Result<()> {
        match self {
            Self::Rfm69(r) => r.recover().await,
            Self::Sx1262(r) => r.recover().await,
        }
    }

    pub async fn stop(&mut self) -> Result<()> {
        match self {
            Self::Rfm69(r) => r.stop().await,
            Self::Sx1262(r) => r.stop().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replays_hex_lines_ignoring_comments_and_spaces() {
        let text = "# a capture\n68 04 04 68\n\n1040015016\n";
        let mut src = FileReplaySource::from_str(text).unwrap();
        assert_eq!(
            src.next_frame().unwrap().unwrap(),
            vec![0x68, 0x04, 0x04, 0x68]
        );
        assert_eq!(
            src.next_frame().unwrap().unwrap(),
            vec![0x10, 0x40, 0x01, 0x50, 0x16]
        );
        assert!(src.next_frame().unwrap().is_none());
    }
}
