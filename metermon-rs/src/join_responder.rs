//! LoRaWAN OTAA join responder.
//!
//! Answers JoinRequests from AppKeys the gateway already holds, so nothing on the
//! network is in the timing path: RX1 opens 5 s after the uplink ends, and the
//! crypto costs microseconds. That is the durable-before-live rule expressed as a
//! deadline — the Device Manager owns and distributes keys, the gateway answers.
//!
//! **This is a join responder, not a network server.** No DevNonce replay store, no
//! frame-counter policy, no MAC commands, no duty-cycle accounting, and a single
//! channel where a real gateway hears eight. It exists to prove the provisioning
//! chain end to end and to exercise the decode path; a real LNS would replace it
//! behind the same interface.

use anyhow::{Context, Result};
use mbus_rs::lorawan::{
    build_join_accept, derive_session_keys, DataFrame, JoinAcceptParams, JoinAdmission,
    JoinRequest, SessionKeys,
};
use mbus_rs::wmbus::radio::driver::{LoRaProfile, RadioProfile, Sx126xDriver};
use mbus_rs::wmbus::radio::hal::raspberry_pi::GpioPins;
use mbus_rs::wmbus::radio::hal::RaspberryPiHal;
use mbus_rs::wmbus::radio::modulation::{CodingRate, LoRaBandwidth, SpreadingFactor};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// JOIN_ACCEPT_DELAY1: RX1 opens this long after the *end* of the JoinRequest.
const JOIN_ACCEPT_DELAY1: Duration = Duration::from_millis(5000);
/// How early to issue SetTx so the preamble is rising as the window opens.
///
/// Small on purpose. The device listens for only a few symbols, and at SF9 an
/// 8-symbol preamble lasts ~33 ms — start far ahead of the window and the preamble
/// is finished before the device is listening, which looks exactly like never
/// transmitting. All the SPI setup happens before this lead, not inside it.
const TX_LEAD: Duration = Duration::from_millis(5);

/// A device this gateway is willing to join, as provisioned by the Device Manager.
#[derive(Debug, Clone)]
pub struct JoinCredential {
    /// DevEUI in wire order (little-endian), matching the JoinRequest.
    pub dev_eui_le: [u8; 8],
    pub app_key: [u8; 16],
}

/// A completed join, for reporting back up the provisioning chain.
#[derive(Debug, Clone)]
pub struct JoinedDevice {
    pub dev_eui: String,
    pub join_eui: String,
    pub dev_addr: u32,
    pub dev_nonce: u16,
    pub rssi_dbm: i16,
    pub snr_db: f32,
    pub keys: SessionKeys,
}

/// Hold the HAT's antenna switch for receive (`true`) or transmit (`false`).
fn set_rf_switch(receive: bool) {
    std::process::Command::new("pinctrl")
        .args(["set", "6", "op", if receive { "dh" } else { "dl" }])
        .status()
        .ok();
}

pub struct JoinResponder {
    driver: Sx126xDriver<RaspberryPiHal>,
    creds: Vec<JoinCredential>,
    freq_hz: u32,
    sf: u8,
    /// Sessions established this run, keyed by DevAddr.
    sessions: HashMap<u32, SessionKeys>,
    next_dev_addr: u32,
    /// Durable 1.0.4 anti-replay state (DevNonce high-water, next JoinNonce). The
    /// JoinNonce that used to live in an in-memory field now comes from here, so it
    /// survives restarts instead of resetting.
    store: Box<dyn mbus_rs::lorawan::JoinStore>,
    capture: Option<std::io::BufWriter<std::fs::File>>,
}

impl JoinResponder {
    pub fn new(
        spidev: &str,
        pins: GpioPins,
        freq_hz: u32,
        sf: u8,
        creds: Vec<JoinCredential>,
        store: Box<dyn mbus_rs::lorawan::JoinStore>,
    ) -> Result<Self> {
        let mut hal = RaspberryPiHal::from_spidev(spidev, &pins).context("opening SPI/GPIO")?;
        hal.reset().context("reset")?;
        let mut driver = Sx126xDriver::new(hal, 32_000_000);
        driver.calibrate(0x7F).context("calibrating")?;
        Ok(Self {
            driver,
            creds,
            freq_hz,
            sf,
            sessions: HashMap::new(),
            // 0x26xxxxxx is the conventional private-range DevAddr prefix.
            next_dev_addr: 0x2600_0001,
            store,
            capture: None,
        })
    }

    fn sf_enum(&self) -> SpreadingFactor {
        match self.sf {
            5 => SpreadingFactor::SF5,
            6 => SpreadingFactor::SF6,
            7 => SpreadingFactor::SF7,
            8 => SpreadingFactor::SF8,
            9 => SpreadingFactor::SF9,
            10 => SpreadingFactor::SF10,
            11 => SpreadingFactor::SF11,
            _ => SpreadingFactor::SF12,
        }
    }

    fn profile(&self, iq_inverted: bool) -> RadioProfile {
        RadioProfile::LoRa(LoRaProfile {
            frequency_hz: self.freq_hz,
            sf: self.sf_enum(),
            bw: LoRaBandwidth::BW125,
            cr: CodingRate::CR4_5,
            power_dbm: 14,
            // Public LoRaWAN sync word; a private-sync receiver is deaf to real
            // LoRaWAN devices and vice versa.
            sync_word: Some(0x3444),
            implicit_header: false,
            iq_inverted,
        })
    }

    /// Enter receive for uplinks: standard IQ, since that is what devices transmit.
    fn arm_uplink_rx(&mut self) -> Result<()> {
        set_rf_switch(true);
        self.driver
            .switch_profile(&self.profile(false))
            .context("uplink profile")?;
        self.driver.set_dio2_as_rf_switch(true)?;
        self.driver.set_rx_boosted_gain(true)?;
        self.driver.set_rx_continuous()?;
        Ok(())
    }

    /// Stage a downlink with **inverted IQ**, as every LoRaWAN downlink must be.
    /// Everything expensive happens here; [`JoinResponder::fire_downlink`] only
    /// issues SetTx.
    fn stage_downlink(&mut self, frame: &[u8]) -> Result<()> {
        self.driver
            .switch_profile(&self.profile(true))
            .context("downlink profile")?;
        self.driver.set_dio2_as_rf_switch(true)?;
        // pinctrl spawns a process, which costs milliseconds — far too much to do
        // inside a receive window, so the antenna switch moves during staging.
        set_rf_switch(false); // TXEN low = transmit
        self.driver
            .lora_prepare_tx(frame)
            .context("staging downlink")?;
        Ok(())
    }

    fn fire_downlink(&mut self) -> Result<()> {
        let res = self.driver.lora_fire_tx();
        set_rf_switch(true);
        res.context("transmitting downlink")?;
        Ok(())
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
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(
            f,
            "{{\"ts\":{ts},\"kind\":\"{kind}\",\"raw_hex\":\"{}\",\"plain_hex\":{},{meta}}}",
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
            let pkt = match self.driver.process_irqs_with_mode() {
                Ok(Some(p)) => p,
                Ok(None) => {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(e) => {
                    log::debug!("irq: {e:?}");
                    continue;
                }
            };
            // The clock starts the moment the frame lands: everything below has to
            // finish inside JOIN_ACCEPT_DELAY1.
            let rx_at = Instant::now();
            seen += 1;
            let rssi = pkt.rssi_dbm;
            let snr = pkt.lora.as_ref().map(|l| l.snr_db).unwrap_or(f32::NAN);

            if let Ok(jr) = JoinRequest::parse(&pkt.payload) {
                let meta = format!(
                    "\"dev_eui\":\"{}\",\"dev_nonce\":{},\"rssi_dbm\":{rssi},\"snr_db\":{snr:.1}",
                    jr.dev_eui_display(),
                    jr.dev_nonce
                );
                self.record_capture("join_request", &pkt.payload, None, &meta);
                self.handle_join(jr, rx_at, rssi, snr, &mut on_join)?;
                self.arm_uplink_rx()?;
                continue;
            }
            match DataFrame::parse(&pkt.payload) {
                Ok(df) if df.is_uplink() => {
                    let plain = self
                        .sessions
                        .get(&df.dev_addr)
                        .map(|k| df.decrypt_payload(&k.nwk_skey, &k.app_skey, df.fcnt as u32));
                    let meta = format!(
                        "\"dev_addr\":\"{:08X}\",\"fcnt\":{},\"fport\":{},\"rssi_dbm\":{rssi},\"snr_db\":{snr:.1}",
                        df.dev_addr,
                        df.fcnt,
                        df.fport.map(|p| p.to_string()).unwrap_or("null".into())
                    );
                    self.record_capture("uplink", &pkt.payload, plain.as_deref(), &meta);
                    self.handle_uplink(&df, rssi, snr, &mut on_uplink)
                }
                _ => {
                    // Capture it anyway: an unrecognised frame is exactly what a new
                    // vendor format looks like before it is understood.
                    let meta = format!("\"rssi_dbm\":{rssi},\"snr_db\":{snr:.1}");
                    self.record_capture("unparsed", &pkt.payload, None, &meta);
                    println!(
                        "  packet {}B rssi {rssi} dBm: neither a JoinRequest nor an uplink: {}",
                        pkt.payload.len(),
                        hex::encode(&pkt.payload)
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

        // 1.0.4 anti-replay, durable before we transmit: this records the DevNonce
        // and reserves a strictly-increasing JoinNonce in one committed write, so a
        // replayed JoinRequest is refused and a restart cannot regress the JoinNonce.
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

        let params = JoinAcceptParams {
            app_nonce: join_nonce,
            net_id: 0x0000_0013,
            dev_addr: self.next_dev_addr,
            dl_settings: 0,
            rx_delay: 1,
        };
        self.next_dev_addr = self.next_dev_addr.wrapping_add(1);

        let accept = build_join_accept(&cred.app_key, &params);
        let keys = derive_session_keys(&cred.app_key, &params, jr.dev_nonce);

        // Stage everything first, then sleep to the deadline and fire.
        self.stage_downlink(&accept)?;
        let staged_at = rx_at.elapsed();
        let fire_at = rx_at + JOIN_ACCEPT_DELAY1 - TX_LEAD;
        let now = Instant::now();
        if fire_at > now {
            std::thread::sleep(fire_at - now);
        } else {
            println!(
                "join: RX1 window missed by {:?} — staging took {:?}",
                now - fire_at,
                staged_at
            );
        }
        self.fire_downlink()?;
        println!(
            "join: JoinAccept fired at +{:?} (staging took {:?})",
            rx_at.elapsed(),
            staged_at
        );

        let joined = JoinedDevice {
            dev_eui: jr.dev_eui_display(),
            join_eui: jr.join_eui_display(),
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
        self.sessions.insert(params.dev_addr, keys);
        on_join(&joined);
        Ok(())
    }

    fn handle_uplink(
        &self,
        df: &DataFrame,
        rssi: i16,
        snr: f32,
        on_uplink: &mut impl FnMut(u32, u16, &[u8], i16, f32),
    ) {
        let Some(keys) = self.sessions.get(&df.dev_addr) else {
            println!("uplink from unknown DevAddr {:08X}", df.dev_addr);
            return;
        };
        // The frame carries only the low 16 bits of the counter; this responder has
        // no rollover tracking, so the upper half is assumed zero — fine for a bench
        // session, and one of the reasons this is not a network server.
        let fcnt = df.fcnt as u32;
        if !df.verify_mic(&keys.nwk_skey, fcnt) {
            println!("uplink {:08X} fcnt {fcnt}: MIC failed", df.dev_addr);
            return;
        }
        let plain = df.decrypt_payload(&keys.nwk_skey, &keys.app_skey, fcnt);
        on_uplink(df.dev_addr, df.fcnt, &plain, rssi, snr);
    }
}
