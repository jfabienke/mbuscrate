//! Seeed-wm1302 radio backend for wM-Bus receive (Waveshare SX1262 or WM-1302 HAT).
//!
//! wM-Bus is received on the SX1262 on **both** HATs (the WM-1302's SX1302 is
//! LoRaWAN-only), so this one backend serves both boards — board choice changes only
//! the electrical profile + pin wiring passed at construction, not the receive loop.
//!
//! ## Why a task, not a poll-in-place source
//!
//! The seeed `GfskReceiver::receive()` future is **not cancel-safe mid-frame**: the
//! driver mirrors the chip's RX-buffer pointer, and dropping the future between the sync
//! IRQ and completion leaves that mirror stale. The gateway's monitor loop wraps
//! `RadioSource::poll()` in a cancelling `tokio::time::timeout` watchdog — so `receive()`
//! must never run inside `poll()`. Instead the radio lives in its own task, `receive()`
//! runs there uncancelled (it self-bounds at ~1 s and returns `IrqTimeout`), and validated
//! frames arrive over an mpsc channel that `poll()` drains non-blockingly. This is the
//! task-per-radio model of seeed design/17 §17.3, and it also sidesteps naming the
//! radio's deeply-generic concrete type.
#![cfg(feature = "seeed-radio")]

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

use radio_core::error::Error;
use radio_core::prelude::*;
use radio_sx126x::Sx1262;
use wmbus_link::assembler::ModeTC;

use crate::source::SourceFrame;

/// Health byte stashed by the radio task, read back by `opmode()` and mapped by
/// `RadioSource::decode_state`. Not a chip register (the chip lives in the task) — a
/// coarse liveness code sufficient for the gateway watchdog.
pub(crate) const HEALTH_UNKNOWN: u8 = 0;
pub(crate) const HEALTH_RX: u8 = 1; // armed and receiving (a frame or an IRQ-timeout window)
pub(crate) const HEALTH_ERROR: u8 = 2; // a non-timeout error occurred

/// Resolve the config `board` string to the seeed (board profile, pin wiring) pair.
/// wM-Bus RX is identical on both; only these electrical facts differ.
fn board_wiring(
    board: Option<&str>,
) -> Result<(
    radio_core::board::Sx1262Board,
    radio_linux::wiring::Sx1262Wiring,
)> {
    match board.unwrap_or("waveshare-sx1262-pi5") {
        "waveshare-sx1262-pi5" | "waveshare" => Ok((
            radio_boards::WAVESHARE_SX1262_PI5
                .sx1262
                .context("waveshare board profile has no sx1262")?,
            radio_linux::wiring::WAVESHARE_SX1262_PI5,
        )),
        "seeed-wm1302" | "wm1302" => Ok((
            radio_boards::SEEED_WM1302_SPI
                .sx1262
                .context("wm1302 board profile has no sx1262")?,
            radio_linux::wiring::WM1302_HAT_SX1262,
        )),
        other => anyhow::bail!(
            "unknown seeed board {other:?} (want waveshare-sx1262-pi5 or seeed-wm1302)"
        ),
    }
}

/// A detection the seeed assembler rejected (bad CRC / decode / truncation), surfaced for
/// A/B mechanism diagnosis. "never-detected" vs "detected-but-rejected" is only separable
/// when the rejects are captured alongside the accepted frames — the accepted-frame stream
/// alone cannot show it. Diagnostic only; not part of the normal decode path.
#[derive(Debug, Clone)]
pub struct WmbusReject {
    /// The leading bytes the driver kept for the rejected detection.
    pub head: Vec<u8>,
    pub rssi_dbm: i16,
    pub reason: String,
    pub len: usize,
}

/// Live SX1262 wM-Bus source over the seeed driver, board-selectable.
pub struct SeeedSx1262Source {
    board: Option<String>,
    spi_hz: u32,
    lora: Option<crate::config::LoraListenConfig>,
    frames: Option<mpsc::Receiver<SourceFrame>>,
    /// Rejected detections from the radio task, for the A/B mechanism split. Drained by
    /// the capture path when present; other consumers ignore it.
    rejects: Option<mpsc::Receiver<WmbusReject>>,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
    health: Arc<AtomicU8>,
}

impl SeeedSx1262Source {
    const FREQ_HZ: u32 = 868_950_000;
    const DEFAULT_SPI_HZ: u32 = 8_000_000;

    /// Prepare the source (validates board/wiring; does not open the bus — that happens
    /// in the task at `start`).
    pub async fn open(spidev: &str, board: Option<&str>, _region: Option<&str>) -> Result<Self> {
        let (_b, wiring) = board_wiring(board)?; // fail fast on a bad board name
        if !spidev.is_empty() && spidev != wiring.spidev {
            log::debug!(
                "seeed: config spidev {spidev} differs from the board wiring's {}; using the wiring",
                wiring.spidev
            );
        }
        Ok(Self {
            board: board.map(str::to_string),
            spi_hz: Self::DEFAULT_SPI_HZ,
            lora: None,
            frames: None,
            rejects: None,
            shutdown: None,
            task: None,
            health: Arc::new(AtomicU8::new(HEALTH_UNKNOWN)),
        })
    }

    /// Enable periodic LoRa listen windows. Not yet wired for the seeed backend — the
    /// wM-Bus path lands and is A/B-validated first, then LoRa windows (same branch).
    pub fn set_lora_listen(&mut self, cfg: Option<crate::config::LoraListenConfig>) {
        if cfg.is_some() {
            log::warn!(
                "lora-listen is configured but not yet wired for the seeed backend; \
                 receiving wM-Bus continuously (LoRa windows are the next step)"
            );
        }
        self.lora = cfg;
    }

    /// Spawn the radio task: open the bus, bring the SX1262 up, configure mode T/C, and
    /// stream validated frames over the channel until `stop`.
    pub async fn start(&mut self) -> Result<()> {
        let (board, wiring) = board_wiring(self.board.as_deref())?;
        let spi_hz = self.spi_hz;
        let freq_hz = Self::FREQ_HZ;
        let board_label = self
            .board
            .clone()
            .unwrap_or_else(|| "waveshare-sx1262-pi5".into());
        let health = self.health.clone();
        let (frame_tx, frame_rx) = mpsc::channel::<SourceFrame>(256);
        let (reject_tx, reject_rx) = mpsc::channel::<WmbusReject>(256);
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        let task = tokio::spawn(async move {
            let fail = |what: &str| {
                health.store(HEALTH_ERROR, Ordering::Relaxed);
                log::error!("seeed SX1262 {what} failed; radio task exiting");
            };

            let (spi, busy, irq, reset, delay, clock) =
                match radio_linux::rpi::sx1262_parts(&wiring, &board, spi_hz) {
                    Ok(p) => p,
                    Err(e) => {
                        log::error!("seeed sx1262_parts: {e}");
                        return fail("bus open");
                    }
                };
            let radio = match Sx1262::new(
                spi,
                busy,
                irq,
                reset,
                delay,
                clock,
                &board,
                Hertz(freq_hz),
            )
            .await
            {
                Ok(r) => r,
                Err(e) => {
                    log::error!("seeed Sx1262::new: {e:?}");
                    return fail("init");
                }
            };
            // TXEN/RF-switch: Some on Waveshare (inverted polarity from the board profile),
            // None on WM-1302. Missing it on the Waveshare leaves the LNA disconnected.
            let sw = match radio_linux::rpi::sx1262_rf_switch(&wiring, &board) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("seeed rf_switch: {e}");
                    return fail("rf switch");
                }
            };
            let mut radio = radio.with_rf_switch(sw);
            if let Err(e) = radio.configure(&wmbus_link::config::mode_tc()).await {
                log::error!("seeed configure mode T/C: {e:?}");
                return fail("configure");
            }
            if let Err(e) = radio.start_rx().await {
                log::error!("seeed start_rx: {e:?}");
                return fail("start_rx");
            }
            health.store(HEALTH_RX, Ordering::Relaxed);
            log::info!("seeed SX1262 wM-Bus RX armed (board={board_label})");

            let mut asm = ModeTC::new();
            let mut buf = [0u8; wmbus_link::MAX_FRAME];
            // A/B turnaround instrumentation: the non-receiving time per accepted frame
            // (build + mpsc send). A large value means the consumer/mpsc hop is stalling
            // and could drop frames in the busy band — ruling my adapter in or out as the
            // −100 deficit's cause versus the driver's readout path.
            let mut frames_ok = 0u64;
            let mut max_send_us = 0u128;
            let mut send_stalls = 0u64; // turnarounds over 5 ms
            loop {
                tokio::select! {
                    biased;
                    // Shutdown only drops receive() when we are tearing down anyway.
                    _ = &mut shutdown_rx => break,
                    r = radio.receive(&mut asm, &mut buf) => match r {
                        Ok(f) => {
                            health.store(HEALTH_RX, Ordering::Relaxed);
                            // Marker-first raw (seeed review): drop the leading 0x54 signalling
                            // byte, keep block CRCs — the 2026-08-21 A/B-validated form into
                            // mbus-core decode_frame. freq_off_hz is None: no GFSK FEI on SX126x.
                            let end = f.len;
                            let bytes = buf[1.min(end)..end].to_vec();
                            let sf = SourceFrame::Wmbus {
                                bytes,
                                rssi_dbm: f.rssi.0 as i16,
                                freq_off_hz: None,
                            };
                            let t0 = std::time::Instant::now();
                            if frame_tx.send(sf).await.is_err() {
                                break; // consumer gone
                            }
                            let us = t0.elapsed().as_micros();
                            frames_ok += 1;
                            max_send_us = max_send_us.max(us);
                            if us > 5_000 {
                                send_stalls += 1;
                            }
                            if frames_ok.is_multiple_of(200) {
                                log::info!(
                                    "seeed turnaround: {frames_ok} frames, max send {max_send_us}µs, {send_stalls} over 5ms"
                                );
                            }
                        }
                        Err(Error::IrqTimeout) => {
                            health.store(HEALTH_RX, Ordering::Relaxed); // alive, quiet window
                        }
                        Err(e) => {
                            log::warn!("seeed receive: {e:?}");
                            health.store(HEALTH_ERROR, Ordering::Relaxed);
                            // Re-arm from a clean base; also resets the driver rx_index mirror.
                            if let Err(e2) = radio.start_rx().await {
                                log::error!("seeed re-arm after error: {e2:?}");
                            }
                        }
                    }
                }
                // Surface rejected detections for the A/B mechanism split. `try_send` so a
                // full diagnostic channel drops rejects rather than back-pressuring RX.
                for rej in radio.drain_rejects() {
                    let head_len = (rej.head_len as usize).min(rej.head.len());
                    let _ = reject_tx.try_send(WmbusReject {
                        head: rej.head[..head_len].to_vec(),
                        rssi_dbm: rej.rssi.0 as i16,
                        reason: rej.reason.to_string(),
                        len: rej.len as usize,
                    });
                }
            }
            log::info!(
                    "seeed task stopping: {frames_ok} frames, max send-gap {max_send_us}µs, {send_stalls} over 5ms"
                );
            let _ = radio.standby().await; // quiesce the SPI bus before the task ends
            log::info!("seeed SX1262 task stopped");
        });

        self.frames = Some(frame_rx);
        self.rejects = Some(reject_rx);
        self.shutdown = Some(shutdown_tx);
        self.task = Some(task);
        Ok(())
    }

    /// Non-blocking: the next rejected detection the radio task saw, if any. Diagnostic
    /// only — used by the capture path to split "never-detected" from "detected-but-
    /// rejected" in the A/B. Returns `None` on empty or when the task has ended.
    pub fn try_recv_reject(&mut self) -> Option<WmbusReject> {
        self.rejects.as_mut().and_then(|rx| rx.try_recv().ok())
    }

    /// Non-blocking: hand back the next frame the radio task has delivered, if any.
    pub async fn poll(&mut self) -> Result<Option<SourceFrame>> {
        let Some(rx) = self.frames.as_mut() else {
            return Ok(None);
        };
        match rx.try_recv() {
            Ok(f) => Ok(Some(f)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => {
                anyhow::bail!("seeed radio task ended (see log for the cause)")
            }
        }
    }

    /// wM-Bus modulation this source receives on. `ModeTC` accepts both T and C; the
    /// gateway's fleet is mode C, reported here (per-frame T/C threading is a follow-on).
    pub fn mode(&self) -> &'static str {
        "C"
    }

    /// Coarse liveness code for the gateway watchdog (see the HEALTH_* constants).
    pub async fn opmode(&mut self) -> Option<u8> {
        Some(self.health.load(Ordering::Relaxed))
    }

    /// Restart the radio task from a clean state.
    pub async fn recover(&mut self) -> Result<()> {
        self.stop().await.ok();
        self.start().await
    }

    /// Signal the radio task to park the chip in standby and exit, then join it.
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.task.take() {
            let _ = t.await;
        }
        self.frames = None;
        self.rejects = None;
        self.health.store(HEALTH_UNKNOWN, Ordering::Relaxed);
        Ok(())
    }
}
