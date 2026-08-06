//! Frame sources: where raw wM-Bus frames come from.
//!
//! The decode core is identical regardless of source. That is the whole point of
//! the capture-replay A/B: `FileReplaySource` feeds the *same bytes* a real radio
//! saw through the same decoder, deterministically, on any host — while
//! `Rfm69Source` (Pi-only, behind the `radio` feature) is the live path.

use anyhow::Result;

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
