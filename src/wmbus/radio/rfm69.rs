//! # RFM69 Radio Driver for wM-Bus
//!
//! This module provides a comprehensive async driver for the HopeRF RFM69HCW transceiver,
//! specifically optimized for wireless M-Bus (wM-Bus) applications. The implementation
//! includes critical enhancements for robust frame processing in real-world conditions.
//!
//! ## Features
//!
//! - Async-first design using Tokio for non-blocking I/O
//! - wM-Bus specific configuration (868.95 MHz, 100 kbps, 50 kHz deviation)
//! - Hardware AES encryption support
//! - Robust packet processing with frame recovery
//! - GPIO interrupt handling for efficient operation
//! - Comprehensive error handling and statistics
//!
//! ## Configuration
//!
//! The driver supports configuration via JSON/TOML:
//! ```json
//! {
//!   "spidev": "/dev/spidev0.0",
//!   "reset_pin": 5,
//!   "interrupt_pin": 23,
//!   "aes_key": "0123456789ABCDEF0123456789ABCDEF"
//! }
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use rfm69::Rfm69Driver;
//!
//! let mut driver = Rfm69Driver::new(config).await?;
//! driver.start_rx().await?;
//!
//! // Process packets in event loop
//! while let Some(packet) = driver.read_packet().await? {
//!     println!("Received: {:?}", packet);
//! }
//! ```

// The RFM69 driver is legacy diagnostic scaffolding pending retirement (#10): a few
// helper fns aren't wired up yet and some radio-config return types are intentionally
// complex. Silence those specific lints module-wide rather than churn code being removed.
#![allow(dead_code, clippy::type_complexity)]

use crate::wmbus::radio::rfm69_packet::*;
use crate::wmbus::radio::rfm69_registers::*;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[cfg(feature = "rfm69")]
use rppal::{
    gpio::{Gpio, InputPin, OutputPin, Trigger},
    spi::{Bus, Mode, SlaveSelect, Spi},
};

/// Parse a `/dev/spidevB.C` path into its rppal (Bus, SlaveSelect).
/// e.g. `/dev/spidev0.1` → (Spi0, Ss1). Falls back to (Spi0, Ss0) on any parse failure.
#[cfg(feature = "rfm69")]
fn parse_spidev(path: &str) -> (Bus, SlaveSelect) {
    let tail = path.rsplit_once("spidev").map(|(_, t)| t).unwrap_or("");
    let (b, c) = tail.split_once('.').unwrap_or(("0", "0"));
    let bus = match b.trim().parse::<u8>().unwrap_or(0) {
        1 => Bus::Spi1,
        2 => Bus::Spi2,
        3 => Bus::Spi3,
        4 => Bus::Spi4,
        5 => Bus::Spi5,
        6 => Bus::Spi6,
        _ => Bus::Spi0,
    };
    let ss = match c.trim().parse::<u8>().unwrap_or(0) {
        1 => SlaveSelect::Ss1,
        2 => SlaveSelect::Ss2,
        _ => SlaveSelect::Ss0,
    };
    (bus, ss)
}

/// Configuration for RFM69 driver
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rfm69Config {
    /// SPI device path (e.g., "/dev/spidev0.0")
    pub spidev: Option<String>,
    /// GPIO pin for radio reset (default: 5)
    pub reset_pin: Option<u8>,
    /// GPIO pin for interrupt (default: 23)
    pub interrupt_pin: Option<u8>,
    /// AES encryption key (32 hex chars, optional)
    pub aes_key: Option<String>,
    /// Node ID for addressing (optional)
    pub node_id: Option<u8>,
    /// Network ID (optional)
    pub network_id: Option<u8>,
    /// FIFO threshold for interrupt (default: 3)
    pub fifo_threshold: Option<u8>,
}

impl Default for Rfm69Config {
    fn default() -> Self {
        Self {
            spidev: Some("/dev/spidev0.0".to_string()),
            reset_pin: Some(DEFAULT_RESET_PIN),
            interrupt_pin: Some(DEFAULT_INTERRUPT_PIN),
            aes_key: None,
            node_id: None,
            network_id: None,
            fifo_threshold: Some(3),
        }
    }
}

/// Driver errors
#[derive(Debug, thiserror::Error)]
pub enum Rfm69Error {
    #[error("SPI communication error: {0}")]
    Spi(String),

    #[error("GPIO error: {0}")]
    Gpio(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Radio initialization failed: {0}")]
    InitFailed(String),

    #[error("Timeout waiting for: {0}")]
    Timeout(String),

    #[error("Invalid frame: {0}")]
    InvalidFrame(String),

    #[error("Packet processing error: {0}")]
    Packet(#[from] PacketError),

    #[error("Feature not enabled: {0}")]
    FeatureNotEnabled(String),
}

/// Operating modes for the RFM69
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rfm69Mode {
    Sleep = 0,
    Standby = 1,
    Tx = 2,
    Rx = 3,
}

/// A decoded register snapshot taken when a mode transition fails.
///
/// Exists because the bare `Timeout waiting for: Mode ready` covered several
/// unrelated faults that need different fixes — a dead SPI bus, a chip that ignored
/// the write, a PLL that never locked — and gave no way to tell them apart from the
/// logs of a gateway that had already been power-cycled.
#[derive(Debug, Clone)]
pub struct RadioDiagnostics {
    /// Mode we were entering and the OPMODE bits written for it, when known.
    pub attempted: Option<(Rfm69Mode, u8)>,
    pub version: Option<u8>,
    pub opmode: Option<u8>,
    pub irqflags1: Option<u8>,
    pub irqflags2: Option<u8>,
    pub osc1: Option<u8>,
    pub rssi: Option<u8>,
    pub temp: Option<u8>,
    pub palevel: Option<u8>,
}

impl RadioDiagnostics {
    /// Best available explanation of the fault, ordered from most to least
    /// fundamental so the first matching cause is the one worth acting on.
    pub fn verdict(&self) -> &'static str {
        match (self.version, self.opmode, self.irqflags1) {
            // Nothing answers: the bus or the chip's supply is gone.
            (None, _, _) => "SPI reads failing — bus or chip unpowered",
            // RFM69 family always reports 0x24; anything else is a bad/floating read.
            (Some(v), _, _) if v != 0x24 => {
                "implausible version register — SPI wiring, contention, or wrong chip"
            }
            (_, _, None) => "IRQ flags unreadable while OPMODE responds — partial SPI fault",
            (_, Some(_), Some(f)) if f & RF_IRQFLAGS1_PLLLOCK == 0 => {
                "PLL never locked — supply, crystal, or frequency configuration"
            }
            (_, Some(op), Some(_)) if self.attempted.is_some_and(|(_, w)| op & 0x1C != w) => {
                "chip did not adopt the commanded mode — state machine latched"
            }
            _ => "ModeReady never asserted with PLL locked — chip state machine stalled",
        }
    }

    /// RC oscillator calibration flag (`RegOsc1` bit 6): if calibration never
    /// completed, mode transitions legitimately cannot finish.
    fn rc_cal_done(&self) -> Option<bool> {
        self.osc1.map(|o| o & 0x40 != 0)
    }
}

impl std::fmt::Display for RadioDiagnostics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn hex(v: Option<u8>) -> String {
            v.map_or_else(|| "??".to_string(), |v| format!("0x{v:02X}"))
        }
        writeln!(f, "  verdict     : {}", self.verdict())?;
        if let Some((mode, bits)) = self.attempted {
            writeln!(f, "  attempted   : {mode:?} (OPMODE bits 0x{bits:02X})")?;
        }
        writeln!(f, "  version     : {} (expect 0x24)", hex(self.version))?;
        writeln!(
            f,
            "  opmode      : {} (mode bits 0x{:02X})",
            hex(self.opmode),
            self.opmode.unwrap_or(0) & 0x1C
        )?;
        if let Some(flags) = self.irqflags1 {
            writeln!(
                f,
                "  irqflags1   : 0x{flags:02X} [ModeReady {} PllLock {} RxReady {} TxReady {} Rssi {} Timeout {}]",
                flags & RF_IRQFLAGS1_MODEREADY != 0,
                flags & RF_IRQFLAGS1_PLLLOCK != 0,
                flags & RF_IRQFLAGS1_RXREADY != 0,
                flags & RF_IRQFLAGS1_TXREADY != 0,
                flags & RF_IRQFLAGS1_RSSI != 0,
                flags & RF_IRQFLAGS1_TIMEOUT != 0,
            )?;
        } else {
            writeln!(f, "  irqflags1   : ??")?;
        }
        writeln!(f, "  irqflags2   : {}", hex(self.irqflags2))?;
        writeln!(
            f,
            "  osc1        : {} (RcCalDone {})",
            hex(self.osc1),
            self.rc_cal_done()
                .map_or_else(|| "??".to_string(), |b| b.to_string())
        )?;
        writeln!(
            f,
            "  rssi        : {} (-{} dBm)",
            hex(self.rssi),
            self.rssi.unwrap_or(0) / 2
        )?;
        write!(
            f,
            "  palevel     : {}, temp raw : {}",
            hex(self.palevel),
            hex(self.temp)
        )
    }
}

/// Main RFM69 driver structure
pub struct Rfm69Driver {
    /// SPI interface for register access
    #[cfg(feature = "rfm69")]
    spi: Arc<Mutex<Spi>>,

    /// GPIO for radio reset
    #[cfg(feature = "rfm69")]
    reset_pin: Option<OutputPin>,

    /// GPIO for interrupt monitoring
    #[cfg(feature = "rfm69")]
    interrupt_pin: Option<InputPin>,

    /// Driver configuration
    config: Rfm69Config,

    /// Current operating mode
    current_mode: Rfm69Mode,

    /// Packet buffer for frame assembly
    packet_buffer: Arc<Mutex<PacketBuffer>>,

    /// Packet processing statistics
    stats: Arc<Mutex<PacketStats>>,

    /// Error logging throttle
    error_throttle: Arc<Mutex<LogThrottle>>,

    /// Completed frames handed from the interrupt task to `get_received_packet`.
    /// Without this the interrupt task assembled packets and dropped them.
    received: Arc<Mutex<std::collections::VecDeque<(Vec<u8>, i16)>>>,

    /// Interrupt processing task handle
    #[cfg(feature = "rfm69")]
    interrupt_task: Option<tokio::task::JoinHandle<()>>,

    /// Shutdown signal for graceful task termination
    #[cfg(feature = "rfm69")]
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Rfm69Driver {
    /// Create a new RFM69 driver instance
    pub async fn new(config: Rfm69Config) -> Result<Self, Rfm69Error> {
        #[cfg(not(feature = "rfm69"))]
        {
            return Err(Rfm69Error::FeatureNotEnabled(
                "rfm69 feature not enabled. Build with --features rfm69".to_string(),
            ));
        }

        #[cfg(feature = "rfm69")]
        {
            let spi = Self::init_spi(&config)?;
            let (reset_pin, interrupt_pin) = Self::init_gpio(&config)?;

            Ok(Self {
                spi: Arc::new(Mutex::new(spi)),
                reset_pin,
                interrupt_pin,
                config,
                current_mode: Rfm69Mode::Sleep,
                packet_buffer: Arc::new(Mutex::new(PacketBuffer::new())),
                stats: Arc::new(Mutex::new(PacketStats::default())),
                error_throttle: Arc::new(Mutex::new(LogThrottle::new(60_000, 5))), // 5 errors per minute
                received: Arc::new(Mutex::new(std::collections::VecDeque::new())),
                interrupt_task: None,
                shutdown_tx: None,
            })
        }
    }

    /// Initialize the RFM69 radio
    pub async fn initialize(&mut self) -> Result<(), Rfm69Error> {
        info!("Initializing RFM69 radio for wM-Bus operation");

        // Reset the radio chip
        self.reset().await?;

        // Verify chip communication
        self.verify_chip().await?;

        // The chip must be calibrated before any mode transition can complete. Doing
        // this at init turns the recurring wedge from a crash loop into either a
        // clean recovery or an explicit, actionable "power must be removed".
        if !self.ensure_rc_calibrated().await? {
            return Err(Rfm69Error::InitFailed(
                "RC oscillator calibration failed — analog section not running; \
                 remove power (a reboot will not clear this)"
                    .to_string(),
            ));
        }

        // Configure for wM-Bus operation
        self.configure_wmbus().await?;

        // Set up AES encryption if configured
        if let Some(ref aes_key) = self.config.aes_key {
            self.configure_aes(aes_key).await?;
        }

        // Configure addressing if specified
        self.configure_addressing().await?;

        // Start interrupt handling
        self.start_interrupt_handling().await?;

        // Enter receive mode
        self.set_mode(Rfm69Mode::Rx).await?;

        info!("RFM69 radio initialized successfully");
        Ok(())
    }

    /// Reset the radio chip
    async fn reset(&mut self) -> Result<(), Rfm69Error> {
        #[cfg(feature = "rfm69")]
        {
            let Some(pin) = self.config.reset_pin else {
                warn!("no reset pin configured — cannot hardware-reset the radio");
                return Ok(());
            };
            if self.reset_pin.is_none() {
                warn!("reset pin {pin} configured but not claimed — no hardware reset available");
                return Ok(());
            }

            // Prove the pulse actually reaches the chip. A reset line that is not
            // wired, is the wrong polarity, or is held by something else fails
            // silently: the driver "resets" forever while the chip never restarts,
            // which would make a persistent fault look like an unfixable chip.
            // Sentinel avoids needing the POR default: write a value, reset, and see
            // whether it survived. Survival means the reset did nothing.
            let scratch = self.read_register(REG_SYNCVALUE1).await?;
            let sentinel = scratch ^ 0xFF;
            self.write_register(REG_SYNCVALUE1, sentinel).await?;
            let armed = self.read_register(REG_SYNCVALUE1).await? == sentinel;

            info!("Resetting RFM69 chip (GPIO {pin})");
            if let Some(ref mut reset_pin) = self.reset_pin {
                // RFM69 reset is active high: assert, then release and let the chip
                // restart before any SPI access.
                reset_pin.set_high();
                sleep(Duration::from_millis(10)).await;
                reset_pin.set_low();
                sleep(Duration::from_millis(20)).await;
            }

            if armed {
                let after = self.read_register(REG_SYNCVALUE1).await?;
                if after == sentinel {
                    error!(
                        "RFM69 hardware reset had NO EFFECT: register survived the pulse on \
                         GPIO {pin} (wrote 0x{sentinel:02X}, still 0x{after:02X}). The reset \
                         line is not reaching the chip — check wiring and polarity. Every \
                         'reset' so far has been a no-op."
                    );
                } else {
                    info!(
                        "RFM69 hardware reset verified effective (register returned to \
                         0x{after:02X})"
                    );
                }
            } else {
                warn!("could not arm the reset-effectiveness check; SPI writes not sticking");
            }

            // Verify the chip talks after reset.
            let start = Instant::now();
            let timeout_duration = Duration::from_secs(5);
            let mut synced = false;
            while start.elapsed() < timeout_duration {
                self.write_register(REG_SYNCVALUE1, 0xAA).await?;
                if self.read_register(REG_SYNCVALUE1).await? == 0xAA {
                    synced = true;
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }
            if !synced {
                return Err(Rfm69Error::InitFailed(
                    "Failed to sync with radio chip".to_string(),
                ));
            }
            self.write_register(REG_SYNCVALUE1, scratch).await?;
            info!("RFM69 chip reset completed");
        }

        Ok(())
    }

    /// Verify chip communication
    async fn verify_chip(&self) -> Result<(), Rfm69Error> {
        // Read version register to verify communication
        let version = self.read_register(REG_VERSION).await?;
        info!("RFM69 chip version: 0x{:02X}", version);
        Ok(())
    }

    /// Configure radio for wM-Bus operation  
    async fn configure_wmbus(&self) -> Result<(), Rfm69Error> {
        info!("Configuring RFM69 for wM-Bus operation");

        // Set to standby mode for configuration
        self.write_register(REG_OPMODE, RF_OPMODE_STANDBY).await?;

        // Set frequency to 868.95 MHz
        self.set_frequency(WMBUS_FREQUENCY).await?;

        // Set bit rate to 100 kbps
        self.write_register(REG_BITRATEMSB, RF_BITRATEMSB_100KBPS)
            .await?;
        self.write_register(REG_BITRATELSB, RF_BITRATELSB_100KBPS)
            .await?;

        // Set frequency deviation to 50 kHz
        self.write_register(REG_FDEVMSB, RF_FDEVMSB_50000).await?;
        self.write_register(REG_FDEVLSB, RF_FDEVLSB_50000).await?;

        // Configure data modulation (Gaussian filter, BT = 1.0)
        self.write_register(REG_DATAMODUL, 1).await?;

        // Configure receiver bandwidth and LNA
        self.write_register(REG_LNA, 0x88).await?;
        self.write_register(REG_RXBW, 0xE0).await?;
        self.write_register(REG_AFCBW, 0xE0).await?;

        // Auto-AFC at each RX restart (AfcAutoOn). Meters have a frequency offset;
        // without this the receiver sits slightly off and misses the sync word.
        // The deployed metermon sets AFCFEI=0x10.
        self.write_register(REG_AFCFEI, 0x10).await?;

        // Configure test register for optimal performance
        self.write_register(REG_TESTDAGC, 0x30).await?;

        // Configure packet handling (no chip CRC, variable length)
        self.write_register(REG_PACKETCONFIG1, 0).await?;
        self.write_register(REG_PAYLOADLENGTH, 0).await?;

        // FIFO threshold = 10 (matches the deployed gateway). FifoLevel fires after
        // this many bytes accumulate post-sync.
        let threshold = self.config.fifo_threshold.unwrap_or(0x0A).max(0x0A);
        self.write_register(REG_FIFOTHRESH, threshold).await?;

        // Configure preamble (4 bytes)
        self.write_register(REG_PREAMBLEMSB, 0).await?;
        self.write_register(REG_PREAMBLELSB, 4).await?;

        // Enable hardware sync detection with the wM-Bus sync word 54 3D 54.
        // This is what actually triggers the FIFO to fill (fill-on-sync). The prior
        // "sync disabled" (0x00) never fired, so the FIFO stayed empty — the deployed
        // metermon uses exactly this config (SYNCCONFIG=0x90, SYNCVALUE1..3=54 3D 54).
        // SYNCCONFIG 0x90 = SyncOn(0x80) | SyncSize=3 bytes ((3-1)<<3 = 0x10).
        self.write_register(REG_SYNCCONFIG, 0x90).await?;
        self.write_register(REG_SYNCVALUE1, 0x54).await?;
        self.write_register(REG_SYNCVALUE2, 0x3D).await?;
        self.write_register(REG_SYNCVALUE3, 0x54).await?;

        // Configure DIO mapping for FIFO level interrupt on DIO1
        self.write_register(REG_DIOMAPPING1, 0).await?;
        // Match the deployed gateway's DIOMAPPING2 (ClkOut config).
        self.write_register(REG_DIOMAPPING2, 0x05).await?;

        // DIAGNOSTIC: dump the post-config register state to compare against the
        // known-working epulse config (which fills the FIFO on this same radio).
        for (name, reg) in [
            ("OPMODE", REG_OPMODE),
            ("DATAMODUL", REG_DATAMODUL),
            ("BITRATEMSB", REG_BITRATEMSB),
            ("BITRATELSB", REG_BITRATELSB),
            ("FDEVMSB", REG_FDEVMSB),
            ("FDEVLSB", REG_FDEVLSB),
            ("FRFMSB", REG_FRFMSB),
            ("FRFMID", REG_FRFMID),
            ("FRFLSB", REG_FRFLSB),
            ("LNA", REG_LNA),
            ("RXBW", REG_RXBW),
            ("AFCBW", REG_AFCBW),
            ("DIOMAPPING1", REG_DIOMAPPING1),
            ("RSSITHRESH", REG_RSSITHRESH),
            ("PREAMBLEMSB", REG_PREAMBLEMSB),
            ("PREAMBLELSB", REG_PREAMBLELSB),
            ("SYNCCONFIG", REG_SYNCCONFIG),
            ("PACKETCONFIG1", REG_PACKETCONFIG1),
            ("PAYLOADLENGTH", REG_PAYLOADLENGTH),
            ("FIFOTHRESH", REG_FIFOTHRESH),
            ("PACKETCONFIG2", REG_PACKETCONFIG2),
            ("TESTDAGC", REG_TESTDAGC),
        ] {
            let v = self.read_register(reg).await.unwrap_or(0xEE);
            debug!("REGDUMP {name}(0x{reg:02X}) = 0x{v:02X}");
        }

        info!("wM-Bus configuration completed");
        Ok(())
    }

    /// Configure AES encryption
    async fn configure_aes(&self, aes_key: &str) -> Result<(), Rfm69Error> {
        if aes_key.len() != 32 {
            return Err(Rfm69Error::Config(
                "AES key must be 32 hex characters".to_string(),
            ));
        }

        info!("Configuring AES encryption");

        // Parse hex key
        let mut key_bytes = [0u8; 16];
        for (i, chunk) in aes_key.as_bytes().chunks(2).enumerate() {
            if i >= 16 {
                break;
            }
            let hex_str = std::str::from_utf8(chunk)
                .map_err(|_| Rfm69Error::Config("Invalid hex in AES key".to_string()))?;
            key_bytes[i] = u8::from_str_radix(hex_str, 16)
                .map_err(|_| Rfm69Error::Config("Invalid hex in AES key".to_string()))?;
        }

        // Load key into chip registers
        for (i, &byte) in key_bytes.iter().enumerate() {
            self.write_register(REG_AESKEY1 + i as u8, byte).await?;
        }

        // Enable AES encryption
        self.write_register_bits(REG_PACKETCONFIG2, 0x01, RF_PACKET2_EAS_ON)
            .await?;

        info!("AES encryption enabled");
        Ok(())
    }

    /// Configure node and network addressing
    async fn configure_addressing(&self) -> Result<(), Rfm69Error> {
        // Set network ID if specified
        if let Some(network_id) = self.config.network_id {
            self.write_register(REG_SYNCVALUE2, network_id).await?;
            info!("Network ID set to: {}", network_id);
        }

        // Set node ID if specified
        if let Some(node_id) = self.config.node_id {
            self.write_register(REG_NODEADRS, node_id).await?;
            self.write_register_bits(REG_PACKETCONFIG1, 0x06, 0x04)
                .await?;
            info!("Node ID set to: {}", node_id);
        }

        Ok(())
    }

    /// Set radio operating mode
    /// Ensure the RC oscillator is calibrated, triggering calibration if it is not.
    ///
    /// `RegOsc1.RcCalDone` clear means the chip's internal RC calibration never
    /// completed, and no mode transition can finish until it does — the signature
    /// found on this gateway's recurring wedge. Calibration normally runs at
    /// power-on; triggering it explicitly is the one recovery available without
    /// physically removing power.
    ///
    /// Returns whether the oscillator ended up calibrated. Per the datasheet
    /// calibration is only valid in standby, so the caller must already be there.
    async fn ensure_rc_calibrated(&self) -> Result<bool, Rfm69Error> {
        let osc1 = self.read_register(REG_OSC1).await?;
        if osc1 & RF_OSC1_RCCAL_DONE != 0 {
            return Ok(true);
        }

        warn!("RFM69 RC oscillator not calibrated (RegOsc1 0x{osc1:02X}); triggering calibration");
        self.write_register(REG_OSC1, RF_OSC1_RCCAL_START).await?;

        // Datasheet puts calibration in the sub-millisecond range; allow far longer
        // before concluding the analog section is genuinely dead.
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(200) {
            if self.read_register(REG_OSC1).await? & RF_OSC1_RCCAL_DONE != 0 {
                info!(
                    "RFM69 RC calibration completed in {}ms",
                    start.elapsed().as_millis()
                );
                return Ok(true);
            }
            sleep(Duration::from_millis(5)).await;
        }

        error!(
            "RFM69 RC calibration did not complete in {}ms — the analog section is not \
             running (crystal/supply); this cannot be cleared from software and needs a \
             full power removal, not a reboot",
            start.elapsed().as_millis()
        );
        Ok(false)
    }

    /// Attempt to bring a wedged radio back without physical access.
    ///
    /// Forces standby, then re-runs RC calibration. Returns whether the chip looks
    /// usable afterwards. Reports honestly when it does not: a failure here is
    /// positive evidence that power must be removed, which is more actionable than
    /// an unexplained timeout.
    pub async fn recover_analog(&mut self) -> Result<bool, Rfm69Error> {
        info!("RFM69 analog recovery: forcing standby and recalibrating");
        // Write standby directly rather than through set_mode: set_mode verifies and
        // would fail on exactly the chip we are trying to rescue.
        self.write_register_bits(REG_OPMODE, 0x1C, RF_OPMODE_STANDBY)
            .await?;
        sleep(Duration::from_millis(10)).await;

        let calibrated = self.ensure_rc_calibrated().await?;
        if !calibrated {
            return Ok(false);
        }
        // Calibration alone is not success: the chip must now actually signal ready.
        let flags = self.read_register(REG_IRQFLAGS1).await?;
        let ready = flags & RF_IRQFLAGS1_MODEREADY != 0;
        if ready {
            self.current_mode = Rfm69Mode::Standby;
            info!("RFM69 analog recovery succeeded — chip is ready in standby");
        } else {
            let diag = self.diagnostics(None).await;
            error!("RFM69 recalibrated but still not ready\n{diag}");
        }
        Ok(ready)
    }

    /// A decoded snapshot of the registers that explain a failed mode transition.
    ///
    /// Captured at the moment of failure, because "Timeout waiting for: Mode ready"
    /// on its own cannot distinguish a dead SPI bus from a chip that accepted the
    /// write but whose PLL never locked — which need completely different fixes.
    async fn diagnostics(&self, attempted: Option<(Rfm69Mode, u8)>) -> RadioDiagnostics {
        // Each read is independent: a failing bus should still yield a usable
        // snapshot of whatever did respond, so a read error is recorded as None.
        let read = |reg: u8| async move { self.read_register(reg).await.ok() };
        RadioDiagnostics {
            attempted,
            version: read(REG_VERSION).await,
            opmode: read(REG_OPMODE).await,
            irqflags1: read(REG_IRQFLAGS1).await,
            irqflags2: read(REG_IRQFLAGS2).await,
            osc1: read(REG_OSC1).await,
            rssi: read(REG_RSSIVALUE).await,
            temp: read(REG_TEMP1).await,
            palevel: read(REG_PALEVEL).await,
        }
    }

    async fn set_mode(&mut self, mode: Rfm69Mode) -> Result<(), Rfm69Error> {
        if self.current_mode == mode {
            return Ok(()); // Already in requested mode
        }

        let opmode = match mode {
            Rfm69Mode::Sleep => RF_OPMODE_SLEEP,
            Rfm69Mode::Standby => RF_OPMODE_STANDBY,
            Rfm69Mode::Tx => RF_OPMODE_TRANSMITTER,
            Rfm69Mode::Rx => RF_OPMODE_RECEIVER,
        };

        let from = self.current_mode;
        self.write_register_bits(REG_OPMODE, 0x1C, opmode).await?;

        // Confirm the write actually landed. A write that does not read back is a
        // different failure (bus or chip not accepting writes) from one that lands
        // but never becomes ready (PLL/oscillator), and conflating them is why this
        // was previously undiagnosable.
        let mut readback = self.read_register(REG_OPMODE).await?;
        if readback & 0x1C != opmode {
            let diag = self.diagnostics(Some((mode, opmode))).await;
            error!("RFM69 mode write did not take effect: {from:?} -> {mode:?}\n{diag}");

            // A chip that ignores mode writes is usually one whose RC oscillator is
            // uncalibrated; try to rescue it once before giving up, so the fleet is
            // not stranded on a fault that software can clear.
            if self.recover_analog().await? {
                self.write_register_bits(REG_OPMODE, 0x1C, opmode).await?;
                readback = self.read_register(REG_OPMODE).await?;
            }
            if readback & 0x1C != opmode {
                return Err(Rfm69Error::InitFailed(format!(
                    "OPMODE write ignored: wrote 0x{opmode:02X}, read back 0x{:02X} ({})",
                    readback & 0x1C,
                    diag.verdict()
                )));
            }
            info!("RFM69 recovered; {from:?} -> {mode:?} accepted after recalibration");
        }

        // Wait for ModeReady on EVERY transition, not only when leaving sleep. The
        // datasheet requires it after any mode change; skipping it let a failed
        // Standby->RX pass silently and only surface later as "the radio is in
        // standby but we think it is receiving".
        self.wait_for_mode_ready(mode).await?;

        self.current_mode = mode;
        debug!("RFM69 mode set: {from:?} -> {mode:?}");
        Ok(())
    }

    /// Wait for the ModeReady flag after a mode change.
    ///
    /// On timeout this captures a decoded register snapshot and a verdict, because
    /// the bare timeout was unactionable: the same message covered a dead SPI bus, a
    /// chip that never left its previous mode, and a PLL that never locked.
    async fn wait_for_mode_ready(&self, target: Rfm69Mode) -> Result<(), Rfm69Error> {
        let start = Instant::now();
        let timeout_duration = Duration::from_millis(500);
        let mut polls = 0u32;
        // Highest-water flags seen while waiting: a PLL that locks and then drops
        // looks identical to one that never locked if only the final read is kept.
        let mut seen_flags = 0u8;

        while start.elapsed() < timeout_duration {
            let flags = self.read_register(REG_IRQFLAGS1).await?;
            polls += 1;
            seen_flags |= flags;
            if flags & RF_IRQFLAGS1_MODEREADY != 0 {
                if polls > 50 {
                    // Slow but successful: worth knowing before it becomes a failure.
                    warn!(
                        "RFM69 ModeReady for {target:?} took {}ms ({polls} polls)",
                        start.elapsed().as_millis()
                    );
                }
                return Ok(());
            }
            sleep(Duration::from_millis(1)).await;
        }

        let diag = self.diagnostics(None).await;
        error!(
            "RFM69 ModeReady timeout entering {target:?} after {}ms ({polls} polls); \
             flags seen while waiting: 0x{seen_flags:02X} \
             (PllLock {}, RxReady {})\n{diag}",
            start.elapsed().as_millis(),
            if seen_flags & RF_IRQFLAGS1_PLLLOCK != 0 {
                "yes"
            } else {
                "NEVER"
            },
            if seen_flags & RF_IRQFLAGS1_RXREADY != 0 {
                "yes"
            } else {
                "no"
            },
        );
        Err(Rfm69Error::Timeout(format!(
            "ModeReady entering {target:?} ({})",
            diag.verdict()
        )))
    }

    /// Set RF frequency
    async fn set_frequency(&self, frequency_hz: f64) -> Result<(), Rfm69Error> {
        let freq_reg = (frequency_hz / FSTEP) as u32;

        self.write_register(REG_FRFMSB, (freq_reg >> 16) as u8)
            .await?;
        self.write_register(REG_FRFMID, (freq_reg >> 8) as u8)
            .await?;
        self.write_register(REG_FRFLSB, freq_reg as u8).await?;

        debug!("Frequency set to: {:.3} MHz", frequency_hz / 1e6);
        Ok(())
    }

    /// Start interrupt handling task
    async fn start_interrupt_handling(&mut self) -> Result<(), Rfm69Error> {
        #[cfg(feature = "rfm69")]
        {
            if let Some(ref mut interrupt_pin) = self.interrupt_pin {
                info!(
                    "Starting interrupt handling on GPIO {}",
                    self.config.interrupt_pin.unwrap_or(DEFAULT_INTERRUPT_PIN)
                );

                // Configure interrupt pin for rising edge.
                // rppal 0.22 added a debounce parameter to set_interrupt.
                interrupt_pin
                    .set_interrupt(Trigger::RisingEdge, None)
                    .map_err(|e| Rfm69Error::Gpio(format!("Failed to set interrupt: {}", e)))?;

                // Clone references for the async task
                let spi = self.spi.clone();
                let packet_buffer = self.packet_buffer.clone();
                let stats = self.stats.clone();
                let error_throttle = self.error_throttle.clone();
                let received = self.received.clone();

                // Create shutdown channel
                let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

                // Spawn interrupt handling task
                let handle = tokio::spawn(async move {
                    Self::interrupt_handler_task(
                        spi,
                        packet_buffer,
                        stats,
                        error_throttle,
                        received,
                        shutdown_rx,
                    )
                    .await;
                });

                self.interrupt_task = Some(handle);
                self.shutdown_tx = Some(shutdown_tx);
            } else {
                warn!("No interrupt pin configured, using polling mode");
                // TODO: Start polling task as fallback
            }
        }

        Ok(())
    }

    /// Async interrupt handler task with proper GPIO interrupt handling
    #[cfg(feature = "rfm69")]
    async fn interrupt_handler_task(
        spi: Arc<Mutex<Spi>>,
        packet_buffer: Arc<Mutex<PacketBuffer>>,
        stats: Arc<Mutex<PacketStats>>,
        error_throttle: Arc<Mutex<LogThrottle>>,
        received: Arc<Mutex<std::collections::VecDeque<(Vec<u8>, i16)>>>,
        mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        info!("Interrupt handler task started");

        let mut last_status = Instant::now();
        // Last reported operating mode, so only transitions are logged at `info`.
        let mut last_opmode: Option<u8> = None;
        // Track the STRONGEST signal seen per window: RSSI reg is 2×(-dBm), so the
        // smallest register value = strongest signal. Sampling every loop (~1kHz)
        // catches the few-ms transmission bursts a 1Hz sample would miss.
        let mut peak_rssi_reg: u8 = 0xFF;
        loop {
            // Check for shutdown signal
            if shutdown_rx.try_recv().is_ok() {
                info!("Shutdown signal received");
                break;
            }

            // Sample RSSI every iteration for peak tracking.
            if let Ok(r) = Self::read_register_static(&spi, REG_RSSIVALUE).await {
                if r < peak_rssi_reg {
                    peak_rssi_reg = r;
                }
            }

            // Periodic radio-status diagnostic, rate-limited by wall time. A `tick % N`
            // cadence breaks when the idle loop spins faster than expected — it flooded the
            // log at ~47 lines/s; wall-time gating keeps it to ~1/s regardless of loop rate.
            //
            // This is a library on a gateway's hot path, so the per-second sample is `debug`
            // (86k lines/day at `info` buried the events that matter). An operating-mode
            // *transition* is a real event — losing RX is exactly what the watchdog hunts —
            // so that is reported at `info` regardless of level.
            if last_status.elapsed() >= Duration::from_secs(1) {
                last_status = Instant::now();
                let op = Self::read_register_static(&spi, REG_OPMODE)
                    .await
                    .unwrap_or(0xFF);
                let f1 = Self::read_register_static(&spi, REG_IRQFLAGS1)
                    .await
                    .unwrap_or(0);
                let f2 = Self::read_register_static(&spi, REG_IRQFLAGS2)
                    .await
                    .unwrap_or(0);
                let buflen = packet_buffer.lock().unwrap().len();
                if Some(op) != last_opmode {
                    match last_opmode {
                        Some(prev) => info!(
                            "RFM69 opmode changed 0x{prev:02X} -> 0x{op:02X} (irq1=0x{f1:02X} irq2=0x{f2:02X})"
                        ),
                        None => info!("RFM69 opmode 0x{op:02X} (irq1=0x{f1:02X} irq2=0x{f2:02X})"),
                    }
                    last_opmode = Some(op);
                }
                debug!(
                    "RFM69 status: opmode=0x{op:02X} irq1=0x{f1:02X} irq2=0x{f2:02X} peak_rssi=-{}dBm buf={buflen}",
                    peak_rssi_reg / 2
                );
                peak_rssi_reg = 0xFF; // reset window
            }

            // Check for FIFO level interrupt
            match Self::read_register_static(&spi, REG_IRQFLAGS2).await {
                Ok(flags2) => {
                    // Handle FIFO level interrupt
                    if flags2 & RF_IRQFLAGS2_FIFOLEVEL != 0 {
                        if let Err(e) =
                            Self::handle_fifo_interrupt(&spi, &packet_buffer, &stats, &received)
                                .await
                        {
                            // Throttled error logging
                            if error_throttle.lock().unwrap().allow() {
                                error!("FIFO interrupt handling failed: {}", e);
                            }
                        }
                    }

                    // Handle FIFO overrun
                    if flags2 & RF_IRQFLAGS2_FIFOOVERRUN != 0 {
                        warn!("FIFO overrun detected - clearing and resetting");
                        if let Err(e) =
                            Self::handle_fifo_overrun(&spi, &packet_buffer, &stats).await
                        {
                            error!("Failed to handle FIFO overrun: {}", e);
                        }
                    }

                    // Handle payload ready (complete packet received)
                    if flags2 & RF_IRQFLAGS2_PAYLOADREADY != 0 {
                        if let Err(e) =
                            Self::handle_payload_ready(&spi, &packet_buffer, &stats).await
                        {
                            if error_throttle.lock().unwrap().allow() {
                                error!("Payload ready handling failed: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    // Throttled error logging for SPI failures
                    if error_throttle.lock().unwrap().allow() {
                        error!("Failed to read interrupt flags: {}", e);
                    }
                    // Brief delay before retry
                    sleep(Duration::from_millis(10)).await;
                    continue;
                }
            }

            // Adaptive polling rate - faster when data is expected
            let polling_interval = if Self::fifo_not_empty(&spi).await.unwrap_or(false) {
                Duration::from_micros(500) // Fast polling when FIFO has data
            } else {
                Duration::from_millis(1) // Normal polling rate
            };

            sleep(polling_interval).await;
        }

        info!("Interrupt handler task shutting down");
    }

    /// Handle a FIFO-level interrupt: read one sync-triggered burst as a whole
    /// packet, deliver it, and re-arm the receiver — matching epulse's ReceivePacket +
    /// Interrupt model. HW sync (SYNCCONFIG=0x90) fires the FIFO fill per frame, so
    /// each burst is one frame; accumulating across bursts in a persistent buffer
    /// (the previous approach) misaligned and flooded "invalid header".
    #[cfg(feature = "rfm69")]
    async fn handle_fifo_interrupt(
        spi: &Arc<Mutex<Spi>>,
        _packet_buffer: &Arc<Mutex<PacketBuffer>>,
        stats: &Arc<Mutex<PacketStats>>,
        received: &Arc<Mutex<std::collections::VecDeque<(Vec<u8>, i16)>>>,
    ) -> Result<(), Rfm69Error> {
        // Signal is present now (sync just matched) — sample RSSI for this frame.
        let rssi_dbm: i16 = match Self::read_register_static(spi, REG_RSSIVALUE).await {
            Ok(r) => -((r as i16) / 2),
            Err(_) => -100,
        };

        // Fresh buffer for this burst. rppal's SPI (MsbFirst) already delivers the
        // FIFO bytes in normal wM-Bus order, so no rev8 is applied here (empirically
        // verified: captured frames normalize to the correct meter IDs as-is).
        let mut buf: Vec<u8> = Vec::with_capacity(64);
        let mut size: i32 = -1;
        let mut idle = 0u32;

        loop {
            // Determine the expected size once we have the 2-byte header.
            if size == -1 && buf.len() >= 2 {
                size = packet_size(&buf);
                if size <= 0 {
                    break; // 0 = not wM-Bus, -2 = invalid header → abandon this burst
                }
            }
            if size > 0 && buf.len() >= size as usize {
                break; // complete
            }

            if Self::fifo_not_empty(spi).await? {
                let b = Self::read_register_static(spi, REG_FIFO).await?;
                buf.push(b);
                idle = 0;
            } else {
                // FIFO briefly empty mid-burst: wait a moment for more bytes, then
                // give up (epulse's short usleep-and-retry to avoid truncation).
                idle += 1;
                if idle > 6 {
                    break;
                }
                sleep(Duration::from_micros(300)).await;
            }
        }

        let complete = size > 0 && buf.len() >= size as usize;

        // DIAGNOSTIC: read the AFC frequency correction for this frame (AfcAutoOn is
        // set, so it reflects this transmission's carrier offset). freq_err = AFC * Fstep.
        let afc = {
            let hi = Self::read_register_static(spi, REG_AFCMSB)
                .await
                .unwrap_or(0);
            let lo = Self::read_register_static(spi, REG_AFCLSB)
                .await
                .unwrap_or(0);
            i16::from_be_bytes([hi, lo])
        };
        let afc_hz = (afc as f64 * 61.03515625) as i32;

        // Re-arm the receiver for the next frame: STANDBY → RX (epulse per-packet
        // cycle). Read-modify-write only the mode bits (0x1C) of RegOpMode.
        let op = Self::read_register_static(spi, REG_OPMODE).await?;
        Self::write_register_static(spi, REG_OPMODE, (op & !0x1C) | RF_OPMODE_STANDBY).await?;

        if complete {
            let packet: Vec<u8> = buf[..size as usize].to_vec();
            // Raw capture line: `debug`, because the application above this layer logs the
            // decoded frame at `info`. Run the gateway with RUST_LOG=debug to recover the
            // on-air hex (that is how the test vectors in tests/wmbus_frames/ were captured).
            debug!(
                "FRAME afc={afc_hz}Hz len={} {}",
                packet.len(),
                hex::encode(&packet)
            );
            {
                let mut s = stats.lock().unwrap();
                s.packets_received += 1;
            }
            let mut q = received.lock().unwrap();
            q.push_back((packet, rssi_dbm));
            while q.len() > 256 {
                q.pop_front();
            }
        } else {
            // Noise-triggered sync or truncated burst: drain any remainder.
            while Self::fifo_not_empty(spi).await? {
                let _ = Self::read_register_static(spi, REG_FIFO).await?;
            }
        }

        let op = Self::read_register_static(spi, REG_OPMODE).await?;
        Self::write_register_static(spi, REG_OPMODE, (op & !0x1C) | RF_OPMODE_RECEIVER).await?;

        Ok(())
    }

    /// Handle FIFO overrun condition
    #[cfg(feature = "rfm69")]
    async fn handle_fifo_overrun(
        spi: &Arc<Mutex<Spi>>,
        packet_buffer: &Arc<Mutex<PacketBuffer>>,
        stats: &Arc<Mutex<PacketStats>>,
    ) -> Result<(), Rfm69Error> {
        // Update statistics
        {
            let mut stats = stats.lock().unwrap();
            stats.fifo_overruns += 1;
        }

        // Reset FIFO by switching to standby and back to RX
        Self::write_register_static(spi, REG_OPMODE, RF_OPMODE_STANDBY).await?;
        sleep(Duration::from_millis(1)).await;
        Self::write_register_static(spi, REG_OPMODE, RF_OPMODE_RECEIVER).await?;

        // Clear packet buffer
        {
            let mut buffer = packet_buffer.lock().unwrap();
            buffer.clear();
        }

        debug!("FIFO overrun handled, radio reset to RX mode");
        Ok(())
    }

    /// Handle payload ready interrupt (complete packet received)
    #[cfg(feature = "rfm69")]
    async fn handle_payload_ready(
        spi: &Arc<Mutex<Spi>>,
        packet_buffer: &Arc<Mutex<PacketBuffer>>,
        stats: &Arc<Mutex<PacketStats>>,
    ) -> Result<(), Rfm69Error> {
        // Read remaining data from FIFO
        while Self::fifo_not_empty(spi).await? {
            let byte = Self::read_register_static(spi, REG_FIFO).await?;

            {
                let mut buffer = packet_buffer.lock().unwrap();
                buffer.push_byte(byte);
            }
        }

        // Process the complete packet
        {
            let mut buffer = packet_buffer.lock().unwrap();
            if buffer.is_complete() {
                match buffer.extract_packet() {
                    Ok(packet) => {
                        let mut stats = stats.lock().unwrap();
                        stats.packets_received += 1;
                        debug!("Complete packet extracted: {} bytes", packet.len());
                    }
                    Err(e) => {
                        buffer.update_stats(PacketEvent::InvalidHeader);
                        warn!("Failed to extract complete packet: {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Static version of write_register for use in tasks
    #[cfg(feature = "rfm69")]
    async fn write_register_static(
        spi: &Arc<Mutex<Spi>>,
        reg: u8,
        value: u8,
    ) -> Result<(), Rfm69Error> {
        let tx = [reg | 0x80, value];

        {
            let mut spi = spi.lock().unwrap();
            spi.write(&tx)
                .map_err(|e| Rfm69Error::Spi(format!("Write register failed: {}", e)))?;
        }

        Ok(())
    }

    /// Check if FIFO is not empty
    #[cfg(feature = "rfm69")]
    async fn fifo_not_empty(spi: &Arc<Mutex<Spi>>) -> Result<bool, Rfm69Error> {
        let flags = Self::read_register_static(spi, REG_IRQFLAGS2).await?;
        Ok(flags & RF_IRQFLAGS2_FIFONOTEMPTY != 0)
    }

    /// Read burst of bytes from FIFO
    ///
    /// Reads up to expected_size bytes from FIFO in a single operation
    /// to prevent timing issues and partial frame corruption.
    ///
    /// # Arguments
    ///
    /// * `spi` - SPI interface
    /// * `expected_size` - Number of bytes to read
    ///
    /// # Returns
    ///
    /// * Vector of bytes read (may be less than expected if FIFO runs out)
    #[cfg(feature = "rfm69")]
    async fn read_burst(
        spi: &Arc<Mutex<Spi>>,
        expected_size: usize,
    ) -> Result<Vec<u8>, Rfm69Error> {
        let mut bytes = Vec::with_capacity(expected_size);
        let mut consecutive_empty = 0;

        // Read up to expected_size bytes, but stop if FIFO appears empty
        while bytes.len() < expected_size {
            // Check FIFO status
            if !Self::fifo_not_empty(spi).await? {
                consecutive_empty += 1;
                if consecutive_empty > 3 {
                    // FIFO seems to be empty, stop reading
                    break;
                }
                // Brief delay to allow FIFO to fill
                tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;
                continue;
            }
            consecutive_empty = 0;

            // Read byte from FIFO
            let byte = Self::read_register_static(spi, REG_FIFO).await?;
            bytes.push(byte);
        }

        if bytes.len() < expected_size {
            debug!(
                "Burst read incomplete: expected {}, got {} bytes",
                expected_size,
                bytes.len()
            );
        }

        Ok(bytes)
    }

    /// Enhanced FIFO interrupt handler with size-aware burst reading
    ///
    /// Uses packet size determination to read full frames atomically,
    /// preventing mid-frame corruption from timing issues.
    /// Inspired by One Channel Hub's sx126x_get_rx_buffer_status approach.
    #[cfg(feature = "rfm69")]
    async fn handle_fifo_interrupt_burst(
        spi: &Arc<Mutex<Spi>>,
        packet_buffer: &Arc<Mutex<PacketBuffer>>,
        stats: &Arc<Mutex<PacketStats>>,
    ) -> Result<(), Rfm69Error> {
        // First, get the payload size from FIFO status
        // This is critical for preventing partial frame reads
        let payload_size = Self::get_fifo_payload_size(spi).await?;

        if payload_size == 0 {
            debug!("FIFO interrupt with no payload");
            return Ok(());
        }

        // Validate payload size against maximum expected
        if payload_size > 255 {
            warn!("Invalid payload size detected: {}", payload_size);
            stats.lock().unwrap().fifo_overruns += 1;
            Self::clear_fifo(spi).await?;
            return Ok(());
        }

        // Now read the exact payload size in a single burst
        // This prevents partial frame corruption seen in logs
        let mut header_bytes = Vec::new();

        // Read first 2 bytes for packet type determination
        for _ in 0..2 {
            if Self::fifo_not_empty(spi).await? {
                let byte = Self::read_register_static(spi, REG_FIFO).await?;
                header_bytes.push(byte);
            }
        }

        if header_bytes.len() < 2 {
            return Ok(()); // Not enough data yet
        }

        // Determine expected packet size from header
        let expected_size = {
            let mut buffer = packet_buffer.lock().unwrap();
            // Add header bytes to buffer
            for byte in &header_bytes {
                buffer.push_byte(*byte);
            }

            // Try to determine packet size
            match buffer.determine_packet_size() {
                Some(size) => size,
                None => {
                    // Can't determine size yet, continue byte-by-byte
                    return Ok(());
                }
            }
        };

        // Read remaining bytes in burst
        let remaining = expected_size.saturating_sub(header_bytes.len());
        if remaining > 0 {
            match Self::read_burst(spi, remaining).await {
                Ok(data) => {
                    let mut buffer = packet_buffer.lock().unwrap();
                    for byte in data {
                        buffer.push_byte(byte);
                    }

                    // Check if packet is complete
                    if buffer.is_complete() {
                        debug!("Burst read complete: {} bytes total", expected_size);
                        let mut stats = stats.lock().unwrap();
                        stats.packets_received += 1;
                    }
                }
                Err(e) => {
                    warn!("Burst read failed: {}", e);
                    let mut stats = stats.lock().unwrap();
                    stats.fifo_overruns += 1;
                }
            }
        }

        Ok(())
    }

    /// Read a register value
    async fn read_register(&self, reg: u8) -> Result<u8, Rfm69Error> {
        #[cfg(feature = "rfm69")]
        {
            Self::read_register_static(&self.spi, reg).await
        }

        #[cfg(not(feature = "rfm69"))]
        {
            Err(Rfm69Error::FeatureNotEnabled(
                "rfm69 feature not enabled".to_string(),
            ))
        }
    }

    /// Current `RegOpMode` byte (radio operating-mode register). Used by the gateway
    /// watchdog / health report to distinguish RX (mode bits 0x10) from a stuck STANDBY
    /// (0x04) when frames stop arriving.
    pub async fn read_opmode(&self) -> Result<u8, Rfm69Error> {
        self.read_register(REG_OPMODE).await
    }

    /// Static version of read_register for use in tasks
    #[cfg(feature = "rfm69")]
    async fn read_register_static(spi: &Arc<Mutex<Spi>>, reg: u8) -> Result<u8, Rfm69Error> {
        let tx = [reg & 0x7F, 0];
        let mut rx = [0u8; 2];

        {
            let spi = spi.lock().unwrap();
            spi.transfer(&mut rx, &tx)
                .map_err(|e| Rfm69Error::Spi(format!("Read register failed: {}", e)))?;
        }

        Ok(rx[1])
    }

    /// Write a register value
    async fn write_register(&self, reg: u8, value: u8) -> Result<(), Rfm69Error> {
        #[cfg(feature = "rfm69")]
        {
            let tx = [reg | 0x80, value];

            {
                let mut spi = self.spi.lock().unwrap();
                spi.write(&tx)
                    .map_err(|e| Rfm69Error::Spi(format!("Write register failed: {}", e)))?;
            }

            Ok(())
        }

        #[cfg(not(feature = "rfm69"))]
        {
            Err(Rfm69Error::FeatureNotEnabled(
                "rfm69 feature not enabled".to_string(),
            ))
        }
    }

    /// Write specific bits in a register
    async fn write_register_bits(&self, reg: u8, mask: u8, bits: u8) -> Result<(), Rfm69Error> {
        let current = self.read_register(reg).await?;
        let new_value = (current & !mask) | bits;
        self.write_register(reg, new_value).await
    }

    /// Get packet statistics
    pub fn get_stats(&self) -> PacketStats {
        self.stats.lock().unwrap().clone()
    }

    /// Initialize SPI interface.
    ///
    /// The bus and chip-select are derived from the configured `spidev` path
    /// (e.g. `/dev/spidev0.1` → bus 0, CS1). Previously this hardcoded CS0, so a
    /// radio wired to CS1 never responded ("Failed to sync with radio chip").
    #[cfg(feature = "rfm69")]
    fn init_spi(config: &Rfm69Config) -> Result<Spi, Rfm69Error> {
        let path = config
            .spidev
            .as_deref()
            .unwrap_or("/dev/spidev0.0")
            .to_string();
        let (bus, ss) = parse_spidev(&path);
        let spi = Spi::new(bus, ss, SPI_SPEED, Mode::Mode0)
            .map_err(|e| Rfm69Error::Spi(format!("Failed to initialize SPI: {}", e)))?;

        info!("SPI interface initialized: {path} (bus={bus:?}, cs={ss:?})");
        Ok(spi)
    }

    /// Initialize GPIO pins
    #[cfg(feature = "rfm69")]
    fn init_gpio(
        config: &Rfm69Config,
    ) -> Result<(Option<OutputPin>, Option<InputPin>), Rfm69Error> {
        let gpio = Gpio::new()
            .map_err(|e| Rfm69Error::Gpio(format!("Failed to initialize GPIO: {}", e)))?;

        let reset_pin = if let Some(pin_num) = config.reset_pin {
            Some(
                gpio.get(pin_num)
                    .map_err(|e| {
                        Rfm69Error::Gpio(format!("Failed to get reset pin {}: {}", pin_num, e))
                    })?
                    .into_output(),
            )
        } else {
            None
        };

        let interrupt_pin = if let Some(pin_num) = config.interrupt_pin {
            Some(
                gpio.get(pin_num)
                    .map_err(|e| {
                        Rfm69Error::Gpio(format!("Failed to get interrupt pin {}: {}", pin_num, e))
                    })?
                    .into_input(),
            )
        } else {
            None
        };

        info!(
            "GPIO pins initialized - Reset: {:?}, Interrupt: {:?}",
            config.reset_pin, config.interrupt_pin
        );
        Ok((reset_pin, interrupt_pin))
    }
}

impl Drop for Rfm69Driver {
    fn drop(&mut self) {
        #[cfg(feature = "rfm69")]
        {
            // Send shutdown signal first
            if let Some(shutdown_tx) = self.shutdown_tx.take() {
                let _ = shutdown_tx.send(()); // Ignore if receiver is already dropped
            }

            // Then abort the task if it doesn't shutdown gracefully
            if let Some(handle) = self.interrupt_task.take() {
                handle.abort();
            }
        }
    }
}

impl Rfm69Driver {
    /// Get the current payload size in FIFO
    ///
    /// This is critical for atomic burst reading to prevent partial frames.
    /// Inspired by sx126x_get_rx_buffer_status from One Channel Hub.
    #[cfg(feature = "rfm69")]
    async fn get_fifo_payload_size(spi: &Arc<Mutex<Spi>>) -> Result<usize, Rfm69Error> {
        // For RFM69, we can determine size from the FIFO threshold and level
        // Read the number of bytes available in FIFO
        let fifo_status = Self::read_register_static(spi, 0x28).await?; // REG_IRQFLAGS2

        // Check if FIFO has data
        if (fifo_status & 0x40) == 0 {
            // FifoNotEmpty bit
            return Ok(0);
        }

        // For now, estimate based on typical wM-Bus frame sizes
        // In a full implementation, we'd peek at the length field
        // Most wM-Bus frames are 50-100 bytes
        Ok(100) // Conservative estimate to ensure we read enough
    }

    /// Clear the FIFO buffer
    ///
    /// Used when invalid data is detected to recover cleanly.
    #[cfg(feature = "rfm69")]
    async fn clear_fifo(spi: &Arc<Mutex<Spi>>) -> Result<(), Rfm69Error> {
        // Set and clear the FifoOverrun bit to flush FIFO
        let irq_flags = Self::read_register_static(spi, 0x28).await?; // REG_IRQFLAGS2
        Self::write_register_static(spi, 0x28, irq_flags | 0x10).await?; // Set FifoOverrun
        Ok(())
    }

    /// Gracefully shutdown the driver and its tasks
    pub async fn shutdown(&mut self) -> Result<(), Rfm69Error> {
        #[cfg(feature = "rfm69")]
        {
            info!("Shutting down RFM69 driver");

            // Send shutdown signal
            if let Some(shutdown_tx) = self.shutdown_tx.take() {
                if shutdown_tx.send(()).is_err() {
                    warn!("Failed to send shutdown signal - task may have already exited");
                }
            }

            // Wait for task to complete gracefully
            if let Some(handle) = self.interrupt_task.take() {
                if let Err(e) = tokio::time::timeout(Duration::from_secs(5), handle).await {
                    warn!("Interrupt task did not shutdown gracefully: {}", e);
                }
            }

            // Park in STANDBY, not SLEEP. Sleep powers down the crystal oscillator,
            // and this hardware repeatedly failed to restart it: every wedge observed
            // so far followed a process restart, with ModeReady and PllLock stuck low
            // afterwards. Standby quiesces the receiver (no RX, SPI idle — which is
            // what shutdown needs) while keeping the oscillator running, so the next
            // process never has to cold-start it.
            if let Err(e) = self.set_mode(Rfm69Mode::Standby).await {
                warn!("Failed to park radio in standby during shutdown: {e}");
            }

            info!("RFM69 driver shutdown completed");
        }

        Ok(())
    }
}

// Implementation of the RadioDriver trait for RFM69
#[async_trait::async_trait]
impl crate::wmbus::radio::radio_driver::RadioDriver for Rfm69Driver {
    async fn initialize(
        &mut self,
        config: crate::wmbus::radio::radio_driver::WMBusConfig,
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        // Update internal configuration from trait config
        if let Some(ref aes_key) = config.sync_word.get(0..32).map(hex::encode) {
            self.config.aes_key = Some(aes_key.clone());
        }

        // Initialize the RFM69 hardware
        self.initialize().await.map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "RFM69 init failed: {}",
                e
            ))
        })
    }

    async fn start_receive(
        &mut self,
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        use crate::wmbus::radio::radio_driver::RadioDriverError as RDE;
        // Replicate epulse's RX-entry sequence to avoid the RFM69 RX deadlock, where
        // a stale FIFO / stuck PayloadReady after reset leaves the receiver unable to
        // accept new data (FIFO never fills). Drain the FIFO, and if PayloadReady is
        // stuck, pulse RestartRx (RegPacketConfig2 bit2) before entering RX.
        let map_err = |e: Rfm69Error| RDE::DeviceError(format!("start RX: {e}"));

        // Drain any stale bytes left in the FIFO.
        for _ in 0..80 {
            let f2 = self.read_register(REG_IRQFLAGS2).await.map_err(map_err)?;
            if f2 & RF_IRQFLAGS2_FIFONOTEMPTY == 0 {
                break;
            }
            let _ = self.read_register(REG_FIFO).await.map_err(map_err)?;
        }

        // Deadlock avoidance: if PayloadReady is stuck, restart the receiver.
        let f2 = self.read_register(REG_IRQFLAGS2).await.map_err(map_err)?;
        if f2 & RF_IRQFLAGS2_PAYLOADREADY != 0 {
            self.write_register_bits(REG_PACKETCONFIG2, 0x04, 0x04)
                .await
                .map_err(map_err)?;
        }

        self.set_mode(Rfm69Mode::Rx).await.map_err(map_err)?;

        // Pulse RestartRx once after entering RX to arm the sync detector cleanly.
        self.write_register_bits(REG_PACKETCONFIG2, 0x04, 0x04)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn stop_receive(
        &mut self,
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        self.set_mode(Rfm69Mode::Standby).await.map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to stop RX: {}",
                e
            ))
        })
    }

    async fn transmit(
        &mut self,
        data: &[u8],
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        if data.len() > MAX_PACKET_SIZE {
            return Err(
                crate::wmbus::radio::radio_driver::RadioDriverError::InvalidParams(format!(
                    "Packet too large: {} > {}",
                    data.len(),
                    MAX_PACKET_SIZE
                )),
            );
        }

        // Switch to standby mode for TX preparation
        self.set_mode(Rfm69Mode::Standby).await.map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "TX standby failed: {}",
                e
            ))
        })?;

        // TODO: Load data into FIFO and transmit
        // This would involve:
        // 1. Clear FIFO
        // 2. Load packet data
        // 3. Switch to TX mode
        // 4. Wait for TX completion
        warn!("RFM69 transmit not yet implemented");

        Err(
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(
                "TX not implemented".to_string(),
            ),
        )
    }

    async fn get_received_packet(
        &mut self,
    ) -> Result<
        Option<crate::wmbus::radio::radio_driver::ReceivedPacket>,
        crate::wmbus::radio::radio_driver::RadioDriverError,
    > {
        // Pop a completed frame delivered by the interrupt task. (The interrupt
        // task is the sole owner of packet assembly; get_received_packet must not
        // re-extract from the buffer or the two would race and drop frames.)
        let entry = self.received.lock().unwrap().pop_front();
        match entry {
            Some((data, rssi_dbm)) => Ok(Some(crate::wmbus::radio::radio_driver::ReceivedPacket {
                data,
                rssi_dbm,
                freq_error_hz: None,
                lqi: None,
                crc_valid: true, // block CRCs validated downstream
            })),
            None => Ok(None),
        }
    }

    async fn get_stats(
        &mut self,
    ) -> Result<
        crate::wmbus::radio::radio_driver::RadioStats,
        crate::wmbus::radio::radio_driver::RadioDriverError,
    > {
        // Read the inherent PacketStats directly. Calling `self.get_stats()` here
        // resolves to *this* async trait method (name collision), not the inherent
        // one, yielding a Future instead of stats.
        let stats = self.stats.lock().unwrap().clone();
        Ok(crate::wmbus::radio::radio_driver::RadioStats {
            packets_received: stats.packets_received as u32,
            packets_crc_valid: stats.packets_valid as u32,
            packets_crc_error: stats.packets_crc_error as u32,
            packets_length_error: stats.packets_invalid_header as u32,
            last_rssi_dbm: -80, // TODO: Get real RSSI
        })
    }

    async fn reset_stats(
        &mut self,
    ) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        let mut stats = self.stats.lock().unwrap();
        *stats = PacketStats::default();
        Ok(())
    }

    async fn get_mode(
        &mut self,
    ) -> Result<
        crate::wmbus::radio::radio_driver::RadioMode,
        crate::wmbus::radio::radio_driver::RadioDriverError,
    > {
        let mode = match self.current_mode {
            Rfm69Mode::Sleep => crate::wmbus::radio::radio_driver::RadioMode::Sleep,
            Rfm69Mode::Standby => crate::wmbus::radio::radio_driver::RadioMode::Standby,
            Rfm69Mode::Tx => crate::wmbus::radio::radio_driver::RadioMode::Transmit,
            Rfm69Mode::Rx => crate::wmbus::radio::radio_driver::RadioMode::Receive,
        };
        Ok(mode)
    }

    async fn sleep(&mut self) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        self.set_mode(Rfm69Mode::Sleep).await.map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to sleep: {}",
                e
            ))
        })
    }

    async fn wake_up(&mut self) -> Result<(), crate::wmbus::radio::radio_driver::RadioDriverError> {
        self.set_mode(Rfm69Mode::Standby).await.map_err(|e| {
            crate::wmbus::radio::radio_driver::RadioDriverError::DeviceError(format!(
                "Failed to wake up: {}",
                e
            ))
        })
    }

    async fn get_rssi(
        &mut self,
    ) -> Result<i16, crate::wmbus::radio::radio_driver::RadioDriverError> {
        // TODO: Implement RSSI reading from RFM69
        // Read REG_RSSIVALUE and convert to dBm
        warn!("RFM69 RSSI reading not yet implemented");
        Ok(-80) // Placeholder value
    }

    async fn is_channel_clear(
        &mut self,
        threshold_dbm: i16,
        listen_duration: Duration,
    ) -> Result<bool, crate::wmbus::radio::radio_driver::RadioDriverError> {
        // Start receiving to measure RSSI
        self.start_receive().await?;

        // Wait for measurement to settle
        sleep(listen_duration).await;

        // Get RSSI measurement
        let rssi = self.get_rssi().await?;

        // Channel is clear if RSSI is below threshold
        Ok(rssi < threshold_dbm)
    }

    fn get_driver_info(&self) -> crate::wmbus::radio::radio_driver::DriverInfo {
        crate::wmbus::radio::radio_driver::DriverInfo {
            name: "RFM69HCW".to_string(),
            version: "1.0.0".to_string(),
            frequency_bands: vec![
                (863_000_000, 870_000_000), // EU wM-Bus bands
                (902_000_000, 928_000_000), // US ISM band
            ],
            max_packet_size: MAX_PACKET_SIZE,
            supported_bitrates: vec![100_000, 50_000, 32_768],
            power_range_dbm: (-18, 20), // RFM69HCW power range
            features: vec![
                "GFSK".to_string(),
                "AES128".to_string(),
                "wM-Bus".to_string(),
                "GPIO_Interrupt".to_string(),
                "Variable_Length".to_string(),
            ],
        }
    }
}
