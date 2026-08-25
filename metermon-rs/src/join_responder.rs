//! LoRaWAN OTAA join responder, on the seeed SX1262 driver.
//!
//! Answers JoinRequests from AppKeys the gateway already holds, so nothing on the
//! network is in the timing path: RX1 opens 5 s after the uplink ends, and the
//! crypto costs microseconds. That is the durable-before-live rule expressed as a
//! deadline — the Device Manager owns and distributes keys, the gateway answers.
//!
//! **This is a join responder, not a network server.** DevNonce anti-replay *is*
//! enforced durably (via the [`JoinStore`], with the version-aware
//! [`DevNoncePolicy`](mbus_rs::lorawan::DevNoncePolicy) — the fleet's 1.0.2 devices
//! draw DevNonce randomly, so the store remembers a window of used values rather than
//! a high-water counter), but there is no frame-counter policy, no MAC commands, no
//! duty-cycle accounting, and a single channel where a real gateway hears eight. It
//! exists to prove the provisioning chain end to end and to exercise the decode
//! path; a real LNS would replace it behind the same interface.
//!
//! ## Radio layer
//!
//! The SX1262 is driven through the seeed `radio-sx126x` driver (the same one the
//! wM-Bus RX path uses, see [`crate::source_seeed`]). Two consequences shape this file:
//!
//! * The driver is `embedded-hal-async`, so every radio call is `async`. This is a
//!   synchronous responder with precise `std::thread::sleep` window timing (converting
//!   the whole thing to async would cascade through `join_control`'s sync MQTT loop for
//!   no benefit), so the driver is driven through an owned current-thread-free
//!   [`tokio::runtime::Runtime`] with `block_on` around each radio operation. The
//!   window waits stay on `thread::sleep` — precise, and outside any `block_on`.
//! * The RF (TX/RX) antenna switch is owned by the driver's `PinSwitch` (from the board
//!   profile's TXEN wiring), driven inside `prepare_lora_tx`/`fire_lora_tx`. The old
//!   `pinctrl` subprocess is gone — it cost milliseconds per flip, far too much for a
//!   receive window, and the driver does it with one GPIO write.
#![cfg(feature = "seeed-radio")]

use anyhow::{Context, Result};
use mbus_rs::lorawan::{
    build_data_down, build_join_accept, derive_session_keys, link_adr_req, parse_link_adr_ans,
    DataFrame, DownlinkParams, FOpts, JoinAcceptParams, JoinAdmission, JoinRequest, Payload,
    SessionKeys,
};
use radio_core::error::Error;
use radio_core::lora::{LoraConfig, Profile};
use radio_core::traits::{DualMode, LoraReceiver};
use radio_core::units::{Dbm, Hertz};
use radio_linux::rpi::Sx1262RppalSwitched;
use radio_sx126x::Sx1262;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::source_seeed::board_wiring;

/// SPI clock for the join responder's SX1262 (clamped to the board max in `sx1262_parts`).
/// Matches the wM-Bus RX path's default.
const SPI_HZ: u32 = 8_000_000;

/// How early to issue SetTx so the preamble is rising as the window opens.
///
/// Small on purpose. The device listens for only a few symbols, and at SF9 an
/// 8-symbol preamble lasts ~33 ms — start far ahead of the window and the preamble
/// is finished before the device is listening, which looks exactly like never
/// transmitting. All the SPI setup happens before this lead, not inside it.
const TX_LEAD: Duration = Duration::from_millis(5);

/// JOIN_ACCEPT_DELAY1: RX1 opens 5 s after the *end* of the JoinRequest, on the same
/// channel and SF the device uplinked on (EU868 RX1DROffset 0), IQ-inverted. This is the
/// first window every device opens and — for a close device like a bench Pico — the most
/// reliable: same channel/SF the responder just received on (near-zero retune), a short
/// SF9 preamble, and RadioLib locks it easily. We answer RX1 *and* RX2 (below): whichever
/// the device catches first wins; if RX1 succeeds the device never opens RX2, so the
/// second fire is harmless.
const JOIN_ACCEPT_DELAY1: Duration = Duration::from_millis(5000);
/// RECEIVE_DELAY1 for *data* frames: RX1 opens 1 s after the uplink ends (EU868
/// default, and what we advertise as RxDelay in the JoinAccept). This is far tighter
/// than the 5 s join window, but staging is ~20 ms so it is comfortably met. We use it
/// to land a LinkADRReq in the device's first data RX1.
const RX1_DATA_DELAY: Duration = Duration::from_millis(1000);
/// The channel mask we pin joined devices to: bit 0 only = EU868 868.1 MHz (ch0), the
/// single default channel a one-channel gateway hears. See [`link_adr_req`].
const PIN_CHANNEL_MASK: u16 = 0x0001;
/// EU868 RX2 is fixed at 869.525 MHz, DR0 = **SF12** (not the uplink SF). The earlier
/// code sent the RX2 accept at the uplink SF, so a standards-compliant RX2 window (SF12)
/// never heard it; only a device with a non-standard RX2 DR did.
const RX2_SF: u8 = 12;
/// JOIN_ACCEPT_DELAY2: RX2 opens 1 s after RX1 (6 s after the JoinRequest). We answer it
/// as a fallback after RX1 (see JOIN_ACCEPT_DELAY1): the fixed 869.525 MHz / SF12 window
/// every device supports, at full +22 dBm for the downlink margin a distant device needs.
/// Because RX1 is answered with a short-airtime SF9 accept, it finishes in time to re-stage
/// and still make RX2.
///
/// **Measured caveat — at SF12 the RX2 fallback never lands.** When the uplink itself is
/// SF12 (a real Zenner HCA), the RX1 accept is ~1.3 s of airtime, so the re-stage finishes
/// after the RX2 deadline. Across a 15-cycle run against the meter this was dead
/// consistent: `RX2 window missed by 332.26 ms — staging took 6.327 s`. So at SF12 the
/// device gets exactly one shot, RX1, and RX2 is decorative; the accept must be good there
/// or not at all (it was — 9/9 joins that were heard verified both-sides). The caveat below
/// is therefore a measurement, not a worry.
///
/// A device that relies *only* on RX2 will miss it — the robust general
/// answer is a per-device/adaptive window choice, not yet implemented.
const JOIN_ACCEPT_DELAY2: Duration = Duration::from_millis(6000);
/// EU868 RX2 is fixed at 869.525 MHz, DR0 (SF12). It sits in the 869.4–869.65 MHz
/// sub-band, which permits +27 dBm ERP (10% duty), so the accept can go out at full
/// chip power for downlink margin — the difference between a close simulator and a
/// real device actually receiving it.
const RX2_FREQ_HZ: u32 = 869_525_000;
const RX2_POWER_DBM: i8 = 22;

/// A device this gateway is willing to join, as provisioned by the Device Manager.
#[derive(Debug, Clone)]
pub struct JoinCredential {
    /// DevEUI in wire order (little-endian), matching the JoinRequest.
    pub dev_eui_le: [u8; 8],
    pub app_key: [u8; 16],
}

/// A completed join, for reporting back up the provisioning chain.
#[derive(Debug, Clone)]
// Some fields are provisioning/telemetry context captured at join time but not yet
// consumed downstream (product-direction work, #32).
#[allow(dead_code)]
pub struct JoinedDevice {
    pub dev_eui: String,
    pub join_eui: String,
    pub dev_addr: u32,
    pub dev_nonce: u16,
    pub rssi_dbm: i16,
    pub snr_db: f32,
    pub keys: SessionKeys,
}

/// Per-session state beyond the derived keys: the network downlink counter and
/// whether the device has accepted the single-channel pin.
struct SessionState {
    keys: SessionKeys,
    /// NFCntDown for the MAC downlinks we send this session (starts at 0).
    nfcnt_down: u32,
    /// True once the device has ACKed our LinkADRReq channel pin.
    pinned: bool,
}

pub struct JoinResponder {
    /// Drives the async seeed radio from this synchronous responder. One `block_on` per
    /// radio operation; the window waits stay on `thread::sleep`, outside it.
    rt: tokio::runtime::Runtime,
    radio: Sx1262RppalSwitched,
    creds: Vec<JoinCredential>,
    freq_hz: u32,
    sf: u8,
    /// Sessions established this run, keyed by DevAddr.
    sessions: HashMap<u32, SessionState>,
    /// Durable 1.0.4 anti-replay state (DevNonce high-water, next JoinNonce). The
    /// JoinNonce that used to live in an in-memory field now comes from here, so it
    /// survives restarts instead of resetting.
    store: Box<dyn mbus_rs::lorawan::JoinStore>,
    capture: Option<std::io::BufWriter<std::fs::File>>,
}

impl JoinResponder {
    /// Build the responder and bring the SX1262 up on `board`'s wiring. `spidev` is kept
    /// for log continuity; the actual bus/pins come from the board profile (the seeed
    /// driver owns the wiring), so a mismatch is a debug note, not an override.
    pub fn new(
        spidev: &str,
        board: Option<&str>,
        freq_hz: u32,
        sf: u8,
        creds: Vec<JoinCredential>,
        store: Box<dyn mbus_rs::lorawan::JoinStore>,
    ) -> Result<Self> {
        let (b, wiring) = board_wiring(board)?;
        if !spidev.is_empty() && spidev != wiring.spidev {
            log::debug!(
                "join responder: config spidev {spidev} differs from the board wiring's {}; \
                 using the wiring",
                wiring.spidev
            );
        }
        let (spi, busy, irq, reset, delay, clock) =
            radio_linux::rpi::sx1262_parts(&wiring, &b, SPI_HZ).context("opening SX1262 bus")?;
        let rt = tokio::runtime::Runtime::new().context("tokio runtime")?;
        let radio = rt
            .block_on(Sx1262::new(
                spi,
                busy,
                irq,
                reset,
                delay,
                clock,
                &b,
                Hertz(freq_hz),
            ))
            .map_err(|e| anyhow::anyhow!("Sx1262::new: {e:?}"))?;
        // TXEN/RF-switch: Some on Waveshare (inverted polarity from the board profile),
        // None on WM-1302. `with_rf_switch` parks it in RX and drives it inside TX.
        let sw = radio_linux::rpi::sx1262_rf_switch(&wiring, &b).context("rf switch")?;
        let radio = radio.with_rf_switch(sw);
        Ok(Self {
            rt,
            radio,
            creds,
            freq_hz,
            sf,
            sessions: HashMap::new(),
            store,
            capture: None,
        })
    }

    /// Enter receive for uplinks: standard IQ (what devices transmit), public sync.
    fn arm_uplink_rx(&mut self) -> Result<()> {
        let cfg = LoraConfig::lorawan_uplink(Hertz(self.freq_hz), self.sf);
        let Self { rt, radio, .. } = self;
        rt.block_on(async {
            radio.switch_profile(&Profile::Lora(cfg)).await?;
            radio.start_lora_rx().await
        })
        .map_err(|e| anyhow::anyhow!("arm uplink rx: {e:?}"))
    }

    /// Stage a downlink with **inverted IQ**, as every LoRaWAN downlink must be.
    /// Everything expensive happens here; [`JoinResponder::fire_downlink`] only issues
    /// SetTx. `prepare_lora_tx` applies the full downlink `LoraConfig` from any current
    /// profile, so no separate `switch_profile` is needed — and it invalidates the RX
    /// arming, so callers re-arm (`arm_uplink_rx`) after the fire.
    fn stage_downlink_on(
        &mut self,
        freq_hz: u32,
        sf: u8,
        power_dbm: i8,
        frame: &[u8],
    ) -> Result<()> {
        let cfg = LoraConfig::lorawan_downlink(Hertz(freq_hz), sf);
        let Self { rt, radio, .. } = self;
        rt.block_on(radio.prepare_lora_tx(&cfg, frame, Dbm(power_dbm as f32)))
            .map_err(|e| anyhow::anyhow!("staging downlink: {e:?}"))
    }

    /// Fire the staged downlink (SetTx + TXEN, handled inside the driver). Waits TxDone.
    fn fire_downlink(&mut self) -> Result<()> {
        let Self { rt, radio, .. } = self;
        rt.block_on(radio.fire_lora_tx())
            .map_err(|e| anyhow::anyhow!("transmitting downlink: {e:?}"))
    }

    fn credential_for(&self, dev_eui_le: &[u8; 8]) -> Option<&JoinCredential> {
        self.creds.iter().find(|c| &c.dev_eui_le == dev_eui_le)
    }

    /// Capture every received frame as JSONL, for offline payload work.
    ///
    /// Records the ciphertext *and* the decrypted payload: the first lets a session
    /// be replayed at protocol level, the second is what a vendor payload decoder is
    /// developed against. Reverse-engineering a proprietary format from one-shot
    /// console output is not something anyone should have to do twice.
    pub fn set_capture(&mut self, path: &str) -> Result<()> {
        self.capture = Some(std::io::BufWriter::new(
            std::fs::File::create(path).with_context(|| format!("creating {path}"))?,
        ));
        Ok(())
    }

    fn record_capture(&mut self, kind: &str, raw: &[u8], plain: Option<&[u8]>, meta: &str) {
        let Some(f) = self.capture.as_mut() else {
            return;
        };
        use std::io::Write;
        // Millisecond resolution: whole seconds are too coarse to measure a device's
        // command-to-transmit defer, which is a ~10 s quantity whose *tail* is the
        // interesting part. `ts` stays whole seconds so existing readers are unaffected.
        let since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let ts = since_epoch.as_secs();
        let ts_ms = since_epoch.as_millis();
        // The channel and SF this frame arrived on. Constant for a single-channel
        // responder, but recording it makes each capture self-describing — so a file can
        // be interpreted without knowing how the run was armed, and a future
        // multi-channel receiver can log arrival channel in the same format.
        let (freq_hz, sf) = (self.freq_hz, self.sf);
        let _ = writeln!(
            f,
            "{{\"ts\":{ts},\"ts_ms\":{ts_ms},\"rx_freq_hz\":{freq_hz},\"rx_sf\":{sf},\
             \"kind\":\"{kind}\",\"raw_hex\":\"{}\",\"plain_hex\":{},{meta}}}",
            hex::encode(raw),
            match plain {
                Some(p) => format!("\"{}\"", hex::encode(p)),
                None => "null".to_string(),
            }
        );
        let _ = f.flush(); // a session that ends unexpectedly still leaves its frames
    }

    /// Run until `seconds` elapse, answering joins and reporting uplinks.
    pub fn run(
        &mut self,
        seconds: u64,
        mut on_join: impl FnMut(&JoinedDevice),
        mut on_uplink: impl FnMut(u32, u16, &[u8], i16, f32),
    ) -> Result<()> {
        println!(
            "join responder — {:.3} MHz SF{} · {} provisioned device(s)",
            self.freq_hz as f64 / 1e6,
            self.sf,
            self.creds.len()
        );
        self.arm_uplink_rx()?;

        let mut buf = [0u8; 256];
        let start = Instant::now();
        let mut last_beat = Instant::now();
        let mut seen = 0u32;
        while start.elapsed() < Duration::from_secs(seconds) {
            // A responder that only prints on success is indistinguishable from one
            // that is not listening. EU868 picks one of three join channels at
            // random and this gateway watches one, so "nothing yet" is the expected
            // state most of the time and needs to look different from "broken".
            if last_beat.elapsed() >= Duration::from_secs(15) {
                println!(
                    "  ── {:>3}s · listening · packets seen {seen}",
                    start.elapsed().as_secs()
                );
                last_beat = Instant::now();
            }
            // One receive. `IrqTimeout` is the empty-window signal (the call self-bounds
            // ~1 s), not an error; on a real error, log and keep listening.
            let rx = {
                let Self { rt, radio, .. } = self;
                rt.block_on(radio.receive_lora(&mut buf))
            };
            let (n, rssi, snr) = match rx {
                Ok(f) => (f.len, f.rssi.0 as i16, f.meta.snr_db),
                Err(Error::IrqTimeout) => continue,
                Err(e) => {
                    log::debug!("receive_lora: {e:?}");
                    continue;
                }
            };
            // Own the payload so the &mut self handlers below are free of the buf borrow.
            let payload = buf[..n].to_vec();
            // The clock starts the moment the frame lands: everything below has to
            // stage and fire the accept before the RX2 deadline (JOIN_ACCEPT_DELAY2).
            let rx_at = Instant::now();
            seen += 1;

            if let Ok(jr) = JoinRequest::parse(&payload) {
                let meta = format!(
                    "\"dev_eui\":\"{}\",\"dev_nonce\":{},\"rssi_dbm\":{rssi},\"snr_db\":{snr:.1}",
                    jr.dev_eui_display(),
                    jr.dev_nonce
                );
                self.record_capture("join_request", &payload, None, &meta);
                self.handle_join(jr, rx_at, rssi, snr, &mut on_join)?;
                self.arm_uplink_rx()?;
                continue;
            }
            match DataFrame::parse(&payload) {
                Ok(df) if df.is_uplink() => {
                    let plain = self.sessions.get(&df.dev_addr).map(|s| {
                        df.decrypt_payload(&s.keys.nwk_skey, &s.keys.app_skey, df.fcnt as u32)
                    });
                    let meta = format!(
                        "\"dev_addr\":\"{:08X}\",\"fcnt\":{},\"fport\":{},\"rssi_dbm\":{rssi},\"snr_db\":{snr:.1}",
                        df.dev_addr,
                        df.fcnt,
                        df.fport.map(|p| p.to_string()).unwrap_or("null".into())
                    );
                    self.record_capture("uplink", &payload, plain.as_deref(), &meta);
                    self.handle_uplink(&df, rx_at, rssi, snr, &mut on_uplink)?;
                }
                _ => {
                    // Capture it anyway: an unrecognised frame is exactly what a new
                    // vendor format looks like before it is understood.
                    let meta = format!("\"rssi_dbm\":{rssi},\"snr_db\":{snr:.1}");
                    self.record_capture("unparsed", &payload, None, &meta);
                    println!(
                        "  packet {}B rssi {rssi} dBm: neither a JoinRequest nor an uplink: {}",
                        payload.len(),
                        hex::encode(&payload)
                    )
                }
            }
        }
        Ok(())
    }

    fn handle_join(
        &mut self,
        jr: JoinRequest,
        rx_at: Instant,
        rssi: i16,
        snr: f32,
        on_join: &mut impl FnMut(&JoinedDevice),
    ) -> Result<()> {
        let Some(cred) = self.credential_for(&jr.dev_eui_le).cloned() else {
            println!(
                "join: {} — no AppKey provisioned, ignoring",
                jr.dev_eui_display()
            );
            return Ok(());
        };
        // The MIC is the only thing binding the claimed DevEUI to a key. Until it
        // verifies, every field in the request is attacker-controlled.
        if !jr.verify_mic(&cred.app_key) {
            println!("join: {} — MIC failed, ignoring", jr.dev_eui_display());
            return Ok(());
        }

        // DevNonce anti-replay, durable before we transmit: this records the DevNonce
        // (per the store's DevNoncePolicy — windowed for the 1.0.2 fleet) and reserves
        // a strictly-increasing JoinNonce in one committed write, so a replayed
        // JoinRequest is refused and a restart cannot regress the JoinNonce.
        let join_nonce = match self.store.admit_join(&jr.dev_eui_le, jr.dev_nonce) {
            Ok(JoinAdmission::Admitted { join_nonce }) => join_nonce,
            Ok(JoinAdmission::Replay { last, seen }) => {
                println!(
                    "join: {} — DevNonce replay (last {last}, seen {seen}), rejected",
                    jr.dev_eui_display()
                );
                return Ok(());
            }
            Err(e) => {
                // A join we cannot record durably is a join we must not grant, or the
                // next boot reopens the replay hole. Drop; the device retries.
                println!(
                    "join: {} — join store write failed: {e}",
                    jr.dev_eui_display()
                );
                return Ok(());
            }
        };

        // DevAddr comes from the durable store, not an in-memory counter: it is stable
        // for this DevEUI across rejoins and restarts, and two devices can never be
        // handed the same address. `previous` being Some is what makes this a re-join.
        let assignment = match self.store.assign_dev_addr(&jr.dev_eui_le) {
            Ok(a) => a,
            Err(e) => {
                // Same rule as the DevNonce record: no durable state, no transmit.
                println!(
                    "join: {} — DevAddr allocation failed: {e}",
                    jr.dev_eui_display()
                );
                return Ok(());
            }
        };

        let params = JoinAcceptParams {
            app_nonce: join_nonce,
            net_id: 0x0000_0013,
            dev_addr: assignment.dev_addr,
            dl_settings: 0,
            rx_delay: 1,
        };

        let accept = build_join_accept(&cred.app_key, &params);
        let keys = derive_session_keys(&cred.app_key, &params, jr.dev_nonce);

        // Answer in BOTH receive windows; whichever the device catches first wins.
        //
        // RX1 (JOIN_ACCEPT_DELAY1, +5 s): the uplink channel and SF, IQ-inverted, at
        // +14 dBm. The device opens this first, it needs almost no retune from the RX we
        // just did, and its short SF-lower preamble is trivial for RadioLib to lock — the
        // reliable path for a close bench device. Its short airtime finishes well before
        // RX2, so we can still make RX2.
        //
        // RX2 (JOIN_ACCEPT_DELAY2, +6 s): the fixed 869.525 MHz / **SF12** / +22 dBm
        // downlink — the standards-correct RX2 (the previous code sent it at the uplink
        // SF, which a compliant SF12 RX2 window could not hear). The high-power sub-band
        // gives a distant device the downlink margin RX1 at +14 dBm may lack.
        let rx1_sf = self.sf;
        let rx1_freq = self.freq_hz;

        // --- RX1 ---
        self.stage_downlink_on(rx1_freq, rx1_sf, 14, &accept)?;
        let rx1_staged = rx_at.elapsed();
        let fire_at = rx_at + JOIN_ACCEPT_DELAY1 - TX_LEAD;
        let now = Instant::now();
        if fire_at > now {
            std::thread::sleep(fire_at - now);
        } else {
            println!("join: RX1 window missed by {:?}", now - fire_at);
        }
        self.fire_downlink()?;
        println!(
            "join: JoinAccept fired in RX1 at +{:?} ({:.3} MHz @ 14 dBm, staging {:?})",
            rx_at.elapsed(),
            rx1_freq as f64 / 1e6,
            rx1_staged
        );

        // --- RX2 --- (re-stage in the ~0.8 s gap before the RX2 deadline)
        self.stage_downlink_on(RX2_FREQ_HZ, RX2_SF, RX2_POWER_DBM, &accept)?;
        let rx2_staged = rx_at.elapsed();
        let fire_at = rx_at + JOIN_ACCEPT_DELAY2 - TX_LEAD;
        let now = Instant::now();
        if fire_at > now {
            std::thread::sleep(fire_at - now);
        } else {
            println!(
                "join: RX2 window missed by {:?} — staging took {:?}",
                now - fire_at,
                rx2_staged
            );
        }
        self.fire_downlink()?;
        println!(
            "join: JoinAccept fired in RX2 at +{:?} (869.525 MHz SF12 @ {RX2_POWER_DBM} dBm, staging {:?})",
            rx_at.elapsed(),
            rx2_staged
        );

        let joined = JoinedDevice {
            dev_eui: jr.dev_eui_display().to_string(),
            join_eui: jr.join_eui_display().to_string(),
            dev_addr: params.dev_addr,
            dev_nonce: jr.dev_nonce,
            rssi_dbm: rssi,
            snr_db: snr,
            keys: keys.clone(),
        };
        println!(
            "join: {} accepted → DevAddr {:08X} (nonce {:04X}, rssi {rssi} dBm, snr {snr:.1} dB)",
            joined.dev_eui, joined.dev_addr, joined.dev_nonce
        );
        self.sessions.insert(
            params.dev_addr,
            SessionState {
                keys,
                nfcnt_down: 0,
                pinned: false,
            },
        );
        on_join(&joined);
        Ok(())
    }

    fn handle_uplink(
        &mut self,
        df: &DataFrame,
        rx_at: Instant,
        rssi: i16,
        snr: f32,
        on_uplink: &mut impl FnMut(u32, u16, &[u8], i16, f32),
    ) -> Result<()> {
        // Copy out the keys/pin-state so no borrow of self.sessions is held across the
        // downlink fire path below (which needs &mut self.radio).
        let (nwk, app, already_pinned) = match self.sessions.get(&df.dev_addr) {
            Some(s) => (s.keys.nwk_skey, s.keys.app_skey, s.pinned),
            None => {
                println!("uplink from unknown DevAddr {:08X}", df.dev_addr);
                return Ok(());
            }
        };
        // The frame carries only the low 16 bits of the counter; this responder has
        // no rollover tracking, so the upper half is assumed zero — fine for a bench
        // session, and one of the reasons this is not a network server.
        let fcnt = df.fcnt as u32;
        if !df.verify_mic(&nwk, fcnt) {
            println!("uplink {:08X} fcnt {fcnt}: MIC failed", df.dev_addr);
            return Ok(());
        }
        let plain = df.decrypt_payload(&nwk, &app, fcnt);
        on_uplink(df.dev_addr, df.fcnt, &plain, rssi, snr);

        // Channel pin (single-channel gateway): steer the device to ch0/868.1 so we
        // hear all its payloads, not the ~1/3 that a hopping device lands on us.
        if !already_pinned {
            match parse_link_adr_ans(&df.fopts) {
                Some(true) => {
                    if let Some(s) = self.sessions.get_mut(&df.dev_addr) {
                        s.pinned = true;
                    }
                    println!(
                        "pin: {:08X} accepted LinkADRReq — pinned to 868.1 (ch0)",
                        df.dev_addr
                    );
                    return Ok(());
                }
                Some(false) => {
                    println!(
                        "pin: {:08X} rejected LinkADRReq (status not all-ACK) — will retry",
                        df.dev_addr
                    );
                }
                None => {}
            }
            // Not yet pinned: answer this uplink's RX1 (RxDelay 1 s) with LinkADRReq.
            self.send_channel_pin(df.dev_addr, rx_at)?;
        }
        Ok(())
    }

    /// Land a `LinkADRReq` in the device's data RX1 window (RxDelay = 1 s), pinning it
    /// to the single default channel this gateway hears (ch0 = 868.1), leaving DR and
    /// power untouched. Advances the session's NFCntDown and re-arms uplink RX.
    fn send_channel_pin(&mut self, dev_addr: u32, rx_at: Instant) -> Result<()> {
        let (fcnt, nwk, app) = match self.sessions.get(&dev_addr) {
            Some(s) => (s.nfcnt_down, s.keys.nwk_skey, s.keys.app_skey),
            None => return Ok(()),
        };
        let params = DownlinkParams {
            dev_addr,
            fcnt,
            adr: true,
            ack: false,
            fpending: false,
            // A 5-byte LinkADRReq into a 15-byte FOpts: the length is a compile-time
            // fact, so this cannot fail. `unwrap_or_default` rather than `expect` keeps
            // the gateway panic-free, and an empty FOpts would produce a harmless
            // no-op downlink rather than a crash if that ever stopped being true.
            fopts: FOpts::from_slice(&link_adr_req(PIN_CHANNEL_MASK, 0x0F, 0x0F, 1))
                .unwrap_or_default(),
            fport: None,
            frm_payload: Payload::new(),
        };
        // `build_data_down` reports a frame it cannot represent as an error rather than
        // panicking. Over-long FOpts is now caught by the `FOpts` type itself; what
        // remains here is the PHY-maximum check, which ours cannot trip with a 5-byte
        // command and no payload — but the responder must not abort a session over a
        // downlink it failed to build.
        let frame = match build_data_down(&nwk, &app, &params) {
            Ok(f) => f,
            Err(e) => {
                println!("pin: {dev_addr:08X} — could not build LinkADRReq: {e}");
                return Ok(());
            }
        };

        // Data RX1 is the uplink channel and SF (RX1DROffset 0) — the same channel we
        // received on, since this is a single-channel responder.
        self.stage_downlink_on(self.freq_hz, self.sf, 14, &frame)?;
        let fire_at = rx_at + RX1_DATA_DELAY - TX_LEAD;
        let now = Instant::now();
        if fire_at > now {
            std::thread::sleep(fire_at - now);
        } else {
            println!("pin: RX1 (data) window missed by {:?}", now - fire_at);
        }
        self.fire_downlink()?;
        println!(
            "pin: sent LinkADRReq → {:08X} (ch0/868.1, fcnt {fcnt}) in RX1 at +{:?}",
            dev_addr,
            rx_at.elapsed()
        );
        if let Some(s) = self.sessions.get_mut(&dev_addr) {
            s.nfcnt_down = s.nfcnt_down.wrapping_add(1);
        }
        self.arm_uplink_rx()?;
        Ok(())
    }
}
