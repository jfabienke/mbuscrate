//! Frame sources: where raw wM-Bus frames come from.
//!
//! The decode core is identical regardless of source. That is the whole point of
//! the capture-replay A/B: `FileReplaySource` feeds the *same bytes* a real radio
//! saw through the same decoder, deterministically, on any host — while
//! `Rfm69Source` (Pi-only, behind the `radio` feature) is the live path.

use anyhow::Result;

#[cfg(feature = "radio")]
use mbus_rs::wmbus::radio::hal::Hal;

/// A frame from a live radio, tagged with the modem that captured it. wM-Bus
/// frames feed the decode pipeline; LoRa frames carry their demodulator metadata
/// and are routed to LoRa handling — feeding one to the other's decoder produces
/// garbage that looks like data.
#[derive(Debug, Clone)]
pub enum SourceFrame {
    Wmbus {
        bytes: Vec<u8>,
        rssi_dbm: i16,
        freq_off_hz: i32,
    },
    Lora {
        bytes: Vec<u8>,
        rssi_dbm: i16,
        snr_db: f32,
        freq_hz: u32,
        sf: u8,
    },
}

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
    pub async fn poll(&mut self) -> Result<Option<SourceFrame>> {
        use mbus_rs::wmbus::radio::radio_driver::RadioDriver;
        Ok(self
            .driver
            .get_received_packet()
            .await?
            .map(|p| SourceFrame::Wmbus {
                bytes: p.data,
                rssi_dbm: p.rssi_dbm,
                freq_off_hz: p.freq_error_hz.unwrap_or(0),
            }))
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
    driver: mbus_rs::wmbus::radio::driver::Sx126xDriver<mbus_rs::wmbus::radio::hal::RaspberryPiHal>,
    /// LoRa listen cadence; `None` = wM-Bus continuously.
    lora: Option<crate::config::LoraListenConfig>,
    /// Whether the radio currently runs the LoRa profile.
    in_lora_window: bool,
    /// When the current window closes / the next opens.
    window_boundary: std::time::Instant,
    /// Rotation position across the (frequency, SF) matrix.
    rotation: usize,
    /// The (frequency, SF) the current window listens on.
    current_point: (u32, u8),
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
            lora: None,
            in_lora_window: false,
            window_boundary: std::time::Instant::now(),
            rotation: 0,
            current_point: (0, 0),
        })
    }

    /// Enable periodic LoRa listen windows. Call before [`Sx1262Source::start`].
    pub fn set_lora_listen(&mut self, cfg: Option<crate::config::LoraListenConfig>) {
        self.lora = cfg;
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
        self.in_lora_window = false;
        if let Some(l) = &self.lora {
            self.window_boundary =
                std::time::Instant::now() + std::time::Duration::from_secs(l.period_secs);
        }
        Ok(())
    }

    /// The next (frequency, SF) in the rotation. Frequencies advance fastest so one
    /// SF pass covers all channels before the ladder moves.
    fn next_point(&mut self) -> (u32, u8) {
        let l = self
            .lora
            .as_ref()
            .expect("rotation only runs with LoRa enabled");
        let freqs = &l.freqs_hz;
        let sfs = &l.sfs;
        let f = freqs[self.rotation % freqs.len().max(1)];
        let sf = sfs[(self.rotation / freqs.len().max(1)) % sfs.len().max(1)];
        self.rotation += 1;
        (f, sf)
    }

    /// Drive the window schedule: switch profiles when a boundary passes. Kept
    /// separate from frame handling so a poll that returns a frame still observes
    /// boundaries on its next call.
    fn tick_windows(&mut self) -> Result<()> {
        let Some(l) = self.lora.clone() else {
            return Ok(());
        };
        use mbus_rs::wmbus::radio::driver::{LoRaProfile, RadioProfile, WmbusProfile};
        use mbus_rs::wmbus::radio::modulation::{CodingRate, LoRaBandwidth, SpreadingFactor};
        let now = std::time::Instant::now();
        if now < self.window_boundary {
            return Ok(());
        }
        if self.in_lora_window {
            // Window over: back to base wM-Bus.
            self.driver
                .switch_profile(&RadioProfile::Wmbus(WmbusProfile::mode_c(
                    Self::FREQ_HZ,
                    Self::BITRATE,
                )))?;
            self.driver.set_rx_boosted_gain(true)?;
            self.driver.set_rx_continuous()?;
            self.in_lora_window = false;
            // Anchor the next opening to this window's start, not its end, so the
            // cadence is the configured period rather than period + window.
            self.window_boundary =
                now + std::time::Duration::from_secs(l.period_secs.saturating_sub(l.window_secs));
            log::info!("lora window closed; wM-Bus RX resumed");
        } else {
            let (freq, sf_n) = self.next_point();
            let sf = match sf_n {
                5 => SpreadingFactor::SF5,
                6 => SpreadingFactor::SF6,
                7 => SpreadingFactor::SF7,
                8 => SpreadingFactor::SF8,
                9 => SpreadingFactor::SF9,
                10 => SpreadingFactor::SF10,
                11 => SpreadingFactor::SF11,
                _ => SpreadingFactor::SF12,
            };
            self.driver
                .switch_profile(&RadioProfile::LoRa(LoRaProfile {
                    frequency_hz: freq,
                    sf,
                    bw: LoRaBandwidth::BW125,
                    cr: CodingRate::CR4_5,
                    power_dbm: 14,
                    sync_word: Some(0x3444), // public LoRaWAN
                    implicit_header: false,
                    iq_inverted: false,
                }))?;
            self.driver.set_rx_boosted_gain(true)?;
            self.driver.set_rx_continuous()?;
            self.in_lora_window = true;
            self.current_point = (freq, sf_n);
            self.window_boundary = now + std::time::Duration::from_secs(l.window_secs);
            log::info!("lora window open: {:.3} MHz SF{}", freq as f64 / 1e6, sf_n);
        }
        Ok(())
    }

    /// Non-blocking poll for one received frame. With LoRa windows enabled this
    /// also advances the window schedule; the returned frame is tagged with the
    /// modem that captured it.
    pub async fn poll(&mut self) -> Result<Option<SourceFrame>> {
        self.tick_windows()?;
        if self.in_lora_window {
            match self.driver.process_irqs_with_mode() {
                Ok(Some(pkt)) => {
                    let lora = pkt.lora.as_ref();
                    return Ok(Some(SourceFrame::Lora {
                        bytes: pkt.payload,
                        rssi_dbm: pkt.rssi_dbm,
                        snr_db: lora.map(|l| l.snr_db).unwrap_or(f32::NAN),
                        freq_hz: self.current_point.0,
                        sf: self.current_point.1,
                    }));
                }
                Ok(None) => return Ok(None),
                Err(e) => {
                    log::debug!("lora irq processing: {e:?}");
                    return Ok(None);
                }
            }
        }
        self.poll_wmbus().await
    }

    /// The proven raw-read wM-Bus poll (RssiSync from the correct GetPacketStatus
    /// offset for GFSK, whose reply layout differs from LoRa's).
    async fn poll_wmbus(&mut self) -> Result<Option<SourceFrame>> {
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
            frame = Some(SourceFrame::Wmbus {
                bytes: buf,
                rssi_dbm,
                freq_off_hz: 0,
            });
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

    pub async fn poll(&mut self) -> Result<Option<SourceFrame>> {
        match self {
            Self::Rfm69(r) => r.poll().await,
            Self::Sx1262(r) => r.poll().await,
        }
    }

    /// Enable periodic LoRa listen windows (SX1262 only; ignored with a warning on
    /// radios that cannot switch modems).
    pub fn set_lora_listen(&mut self, cfg: Option<crate::config::LoraListenConfig>) {
        match self {
            Self::Sx1262(r) => r.set_lora_listen(cfg),
            Self::Rfm69(_) => {
                if cfg.is_some() {
                    log::warn!("lora-listen configured but the RFM69 has no LoRa modem; ignoring");
                }
            }
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
