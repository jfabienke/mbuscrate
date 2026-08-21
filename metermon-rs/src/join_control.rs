//! MQTT-driven LoRaWAN join-control endpoint — the gateway half of the two-app OTAA
//! join dance.
//!
//! A separate device-provisioning app drives the meter optically (SET OTAA, Join
//! Request, read `CheckJoinAccept`); this endpoint parks the gateway's single-channel
//! [`JoinResponder`](crate::join_responder::JoinResponder) on the requested channel/SF,
//! answers the JoinRequest it hears, and reports back so the two sides can agree on one
//! [`JoinOutcome`]. The wire contract — every message type and topic — is the shared
//! [`lorawan_join_control`] crate; nothing here invents JSON.
//!
//! ## Arm-once, fire-many
//! One device is handled at a time. An [`ArmRequest`] parks the responder for
//! `arm_window_secs`; within that window the provisioning app may trigger the optical
//! join several times ([`FiredNotice`] per attempt) and the responder answers every
//! JoinRequest it hears. The window ends when it elapses or when a [`VerifyRequest`]
//! reconciles to a fully-confirmed [`JoinOutcome::VerifiedBothSides`]. The window is run
//! as a sequence of short sub-windows so an incoming verify is serviced promptly rather
//! than only after the full arm window drains.
//!
//! ## Key custody
//! AppKeys are **never** carried on these topics. Credentials are provisioned
//! out-of-band (the `--creds` file, same format the `lorawan-join` subcommand loads) and
//! held only by the gateway; every message here references a device by DevEUI alone. An
//! arm for a DevEUI we hold no credential for is answered with `creds_present: false`
//! and otherwise ignored — the responder cannot MIC-check or answer it.

use anyhow::{Context, Result};
use lorawan_join_control::{
    topics, ArmReply, ArmRequest, FiredNotice, JoinOutcome, JoinStatus, VerifyReply, VerifyRequest,
};
use rumqttc::{Client, Event, Incoming, MqttOptions, QoS};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::devices::now_unix;
use crate::join_responder::{JoinResponder, JoinedDevice};
use mbus_rs::wmbus::radio::hal::raspberry_pi::GpioPins;

/// Sub-window length: the responder runs in slices this long so a `verify` (or a new
/// `arm`) queued on MQTT is drained between slices rather than after the whole window.
const SUB_WINDOW_SECS: u64 = 4;

/// One decoded incoming control message, forwarded from the MQTT pump thread to the
/// single-threaded main loop.
enum Cmd {
    Arm(ArmRequest),
    Verify(VerifyRequest),
    Fired(FiredNotice),
}

/// First synthetic DevAddr handed out in `--simulate-join` mode; each simulated join
/// takes the next value up. Mirrors the live gateway's assignment base (see the
/// `lorawan_join_control` reconcile tests, which use `0x2600_0001`).
const SIM_DEV_ADDR_BASE: u32 = 0x2600_0001;

/// Run the join-control endpoint. Blocks, servicing one armed device at a time, until
/// the MQTT connection is lost (the pump thread exits and closes the channel).
///
/// When `simulate_join` is true no radio is touched: an arm is answered as usual, then a
/// short latency later a synthetic [`JoinStatus`] is published and kept as the current
/// status, so a following [`VerifyRequest`] reconciles to
/// [`JoinOutcome::VerifiedBothSides`]. The arm-once/window/verify logic is identical to
/// the live path; only *how* a join happens differs. This exists to exercise and fuzz the
/// MQTT control-plane timing without RF or a meter.
pub fn run(
    cfg: &Config,
    spidev: &str,
    pins: GpioPins,
    creds_path: &str,
    join_db: &str,
    arm_window_secs: u64,
    simulate_join: bool,
) -> Result<()> {
    let gwid = cfg.gwid.clone();

    let mut opts = MqttOptions::new(
        format!("{}-joinctl", cfg.mqtt.clientid),
        cfg.mqtt.host.clone(),
        cfg.mqtt.port,
    );
    opts.set_keep_alive(Duration::from_secs(30));
    let (client, connection) = Client::new(opts, 16);

    // Inbound topics we act on.
    let arm_topic = topics::arm(&gwid);
    let verify_topic = topics::verify(&gwid);
    let fired_topic = topics::fired(&gwid);
    client
        .subscribe(&arm_topic, QoS::AtLeastOnce)
        .context("subscribe arm topic")?;
    client
        .subscribe(&verify_topic, QoS::AtLeastOnce)
        .context("subscribe verify topic")?;
    client
        .subscribe(&fired_topic, QoS::AtLeastOnce)
        .context("subscribe fired topic")?;

    log::info!(
        "join-control on gateway {gwid}: listening for arm/verify/fired, {}s arm window, creds {creds_path}, store {join_db}{}",
        arm_window_secs,
        if simulate_join { " [SIMULATE-JOIN: no radio]" } else { "" }
    );

    // The MQTT event loop is the pump; it lives on a background thread and forwards
    // decoded commands to the main thread. The Client stays here for publishing.
    let (tx, rx) = mpsc::channel::<Cmd>();
    let pump = std::thread::spawn(move || {
        let mut connection = connection;
        for event in connection.iter() {
            match event {
                Ok(Event::Incoming(Incoming::Publish(p))) => {
                    let cmd = if p.topic == arm_topic {
                        match serde_json::from_slice::<ArmRequest>(&p.payload) {
                            Ok(m) => Some(Cmd::Arm(m)),
                            Err(e) => {
                                log::warn!("bad ArmRequest JSON on {}: {e}", p.topic);
                                None
                            }
                        }
                    } else if p.topic == verify_topic {
                        match serde_json::from_slice::<VerifyRequest>(&p.payload) {
                            Ok(m) => Some(Cmd::Verify(m)),
                            Err(e) => {
                                log::warn!("bad VerifyRequest JSON on {}: {e}", p.topic);
                                None
                            }
                        }
                    } else if p.topic == fired_topic {
                        match serde_json::from_slice::<FiredNotice>(&p.payload) {
                            Ok(m) => Some(Cmd::Fired(m)),
                            Err(e) => {
                                log::warn!("bad FiredNotice JSON on {}: {e}", p.topic);
                                None
                            }
                        }
                    } else {
                        None
                    };
                    if let Some(c) = cmd {
                        if tx.send(c).is_err() {
                            break; // main loop gone
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    log::warn!("join-control mqtt error: {e}; retrying");
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }
        log::warn!("join-control mqtt pump exited");
    });

    // Main loop: arm-once / run-the-window, one device at a time. A new arm that
    // arrives mid-window is carried over here rather than dropped.
    let mut next: Option<ArmRequest> = None;
    // Monotonic DevAddr counter for `--simulate-join`; unused in the live path.
    let mut sim_dev_addr: u32 = SIM_DEV_ADDR_BASE;
    loop {
        let arm = match next.take() {
            Some(a) => a,
            None => match rx.recv() {
                Ok(Cmd::Arm(req)) => req,
                // A verify/fired with no device armed has nothing to reconcile against.
                Ok(Cmd::Verify(vr)) => {
                    log::warn!("verify for {} with nothing armed — ignoring", vr.dev_eui);
                    continue;
                }
                Ok(Cmd::Fired(f)) => {
                    log::warn!("fired for {} with nothing armed — ignoring", f.dev_eui);
                    continue;
                }
                Err(_) => break, // pump thread gone
            },
        };

        next = handle_arm(
            &client,
            &rx,
            &gwid,
            spidev,
            &pins,
            creds_path,
            join_db,
            arm_window_secs,
            arm,
            simulate_join,
            &mut sim_dev_addr,
        )?;
    }

    let _ = pump.join();
    Ok(())
}

/// Handle one armed device for the length of the arm window. Returns any `ArmRequest`
/// that arrived mid-window so the caller can service it next.
#[allow(clippy::too_many_arguments)]
fn handle_arm(
    client: &Client,
    rx: &Receiver<Cmd>,
    gwid: &str,
    spidev: &str,
    pins: &GpioPins,
    creds_path: &str,
    join_db: &str,
    arm_window_secs: u64,
    arm: ArmRequest,
    simulate_join: bool,
    sim_dev_addr: &mut u32,
) -> Result<Option<ArmRequest>> {
    let dev_eui = arm.dev_eui.clone();

    // Credentials are loaded fresh per arm (the provisioning app may have just added
    // this device). `creds_present` decides whether we can actually answer.
    let creds = crate::load_join_creds(creds_path)
        .with_context(|| format!("loading join creds {creds_path}"))?;
    let creds_present = match dev_eui_hex_to_le(&dev_eui) {
        Some(le) => creds.iter().any(|c| c.dev_eui_le == le),
        None => {
            log::warn!("arm dev_eui {dev_eui:?} is not 8-byte hex");
            false
        }
    };

    // Answer the arm on the reply topic regardless: the app needs to know what we are
    // listening on and whether we hold a credential.
    let reply = ArmReply {
        dev_eui: dev_eui.clone(),
        armed: creds_present,
        listening_hz: arm.channel_hz,
        listening_sf: arm.sf,
        creds_present,
    };
    publish(client, &topics::arm_reply(gwid), &reply)?;

    if !creds_present {
        log::warn!(
            "arm {dev_eui}: no provisioned credential — cannot MIC-check or answer; not arming"
        );
        return Ok(None);
    }

    log::info!(
        "arm {dev_eui}: {} on {:.3} MHz SF{} for {}s",
        if simulate_join {
            "SIMULATING join (no radio)"
        } else {
            "parking responder"
        },
        arm.channel_hz as f64 / 1e6,
        arm.sf,
        arm_window_secs
    );

    // Live path only: durable join state (DevNonce window + next JoinNonce), same store
    // the `lorawan-join` subcommand uses, feeding the RF responder. In `--simulate-join`
    // mode neither the store nor the responder (nor any radio) is touched.
    let mut responder = if simulate_join {
        None
    } else {
        let store = crate::join_store::RedbJoinStore::open(join_db)
            .map_err(|e| anyhow::anyhow!("opening join store {join_db}: {e}"))?;
        Some(
            JoinResponder::new(
                spidev,
                pins.clone(),
                arm.channel_hz,
                arm.sf,
                creds,
                Box::new(store),
            )
            .context("building join responder")?,
        )
    };

    // Simulated join: reserve this arm's DevAddr and pick a short latency to model the
    // over-the-air join delay before the synthetic status is published.
    let sim_assigned = *sim_dev_addr;
    if simulate_join {
        *sim_dev_addr = sim_dev_addr.wrapping_add(1);
    }
    let sim_latency = Duration::from_millis(1000 + u64::from(sim_assigned % 3) * 1000);
    let mut sim_fired = false;

    // Gateway-side view of this device, updated by the on-join callback and read by
    // verify reconciliation.
    let status = Arc::new(Mutex::new(JoinStatus {
        dev_eui: dev_eui.clone(),
        heard: false,
        mic_ok: None,
        assigned_dev_addr: None,
        accept_ts_unix: None,
        rssi_dbm: None,
        snr_db: None,
    }));

    let start = Instant::now();
    let window = Duration::from_secs(arm_window_secs);
    let mut confirmed = false;
    let mut pending_arm: Option<ArmRequest> = None;

    while start.elapsed() < window && !confirmed && pending_arm.is_none() {
        // Run one short receive slice. Live: the on-join callback records the accept and
        // publishes the updated JoinStatus; on-uplink just logs. Simulated: no radio, so
        // just wait out the latency and publish one synthetic JoinStatus.
        if let Some(r) = responder.as_mut() {
            let status_cb = status.clone();
            let client_cb = client.clone();
            let status_topic = topics::status(gwid, &dev_eui);
            let on_join = move |j: &JoinedDevice| {
                let snapshot = {
                    let mut s = status_cb.lock().unwrap();
                    s.heard = true;
                    s.mic_ok = Some(true);
                    s.assigned_dev_addr = Some(j.dev_addr);
                    s.accept_ts_unix = Some(now_unix() as u64);
                    s.rssi_dbm = Some(j.rssi_dbm);
                    s.snr_db = Some(j.snr_db);
                    s.clone()
                };
                match serde_json::to_vec(&snapshot) {
                    Ok(bytes) => {
                        if let Err(e) =
                            client_cb.publish(&status_topic, QoS::AtLeastOnce, false, bytes)
                        {
                            log::warn!("status publish failed: {e}");
                        }
                    }
                    Err(e) => log::warn!("status serialize failed: {e}"),
                }
            };
            let on_uplink = |dev_addr: u32, fcnt: u16, payload: &[u8], rssi: i16, snr: f32| {
                log::info!(
                    "join-control uplink {dev_addr:08X} fcnt {fcnt} rssi {rssi} dBm snr {snr:.1} dB: {}",
                    hex::encode(payload)
                );
            };
            r.run(SUB_WINDOW_SECS, on_join, on_uplink)
                .context("responder sub-window")?;
        } else {
            // Simulated slice. On the first slice sleep the modelled join latency, then
            // publish exactly one synthetic status; later slices just sleep a sub-window
            // so an incoming verify is still drained promptly between them.
            if !sim_fired {
                std::thread::sleep(sim_latency.min(Duration::from_secs(SUB_WINDOW_SECS)));
                let snapshot = {
                    let mut s = status.lock().unwrap();
                    s.heard = true;
                    s.mic_ok = Some(true);
                    s.assigned_dev_addr = Some(sim_assigned);
                    s.accept_ts_unix = Some(now_unix() as u64);
                    s.rssi_dbm = Some(-70);
                    s.snr_db = Some(6.0);
                    s.clone()
                };
                let status_topic = topics::status(gwid, &dev_eui);
                match serde_json::to_vec(&snapshot) {
                    Ok(bytes) => {
                        if let Err(e) =
                            client.publish(&status_topic, QoS::AtLeastOnce, false, bytes)
                        {
                            log::warn!("simulated status publish failed: {e}");
                        } else {
                            log::info!(
                                "simulated join {dev_eui}: assigned {sim_assigned:08X}, status published"
                            );
                        }
                    }
                    Err(e) => log::warn!("simulated status serialize failed: {e}"),
                }
                sim_fired = true;
            } else {
                std::thread::sleep(Duration::from_secs(SUB_WINDOW_SECS));
            }
        }

        // Drain any control messages that arrived during the slice.
        loop {
            match rx.try_recv() {
                Ok(Cmd::Verify(vr)) => {
                    let snapshot = status.lock().unwrap().clone();
                    let outcome = JoinOutcome::reconcile(&snapshot, vr.device_dev_addr);
                    log::info!(
                        "verify {}: device_dev_addr {:08X} → {:?}",
                        vr.dev_eui,
                        vr.device_dev_addr,
                        outcome
                    );
                    let vreply = VerifyReply {
                        dev_eui: vr.dev_eui.clone(),
                        outcome: outcome.clone(),
                    };
                    publish(client, &topics::verify_reply(gwid), &vreply)?;
                    if outcome.is_confirmed() {
                        confirmed = true;
                    }
                }
                Ok(Cmd::Fired(f)) => {
                    log::info!(
                        "fired {}: attempt {} at ts {}",
                        f.dev_eui,
                        f.fire_seq,
                        f.ts_unix
                    );
                }
                Ok(Cmd::Arm(new_arm)) => {
                    // Finish the current device, then hand this one back to the caller.
                    log::info!(
                        "arm for {} received mid-window; queued until {} finishes",
                        new_arm.dev_eui,
                        dev_eui
                    );
                    pending_arm = Some(new_arm);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    let final_status = status.lock().unwrap().clone();
    log::info!(
        "arm {dev_eui} done: heard={} mic_ok={:?} assigned={:?} confirmed={confirmed}",
        final_status.heard,
        final_status.mic_ok,
        final_status.assigned_dev_addr,
    );

    Ok(pending_arm)
}

/// Serialize `msg` as JSON and publish it at QoS 1 (not retained).
fn publish<T: serde::Serialize>(client: &Client, topic: &str, msg: &T) -> Result<()> {
    let bytes = serde_json::to_vec(msg).context("serialize control message")?;
    client
        .publish(topic, QoS::AtLeastOnce, false, bytes)
        .with_context(|| format!("publish to {topic}"))?;
    Ok(())
}

/// Convert a big-endian DevEUI hex string (as written by people and stored in the creds
/// file, separators tolerated) to the little-endian wire order the responder matches on.
fn dev_eui_hex_to_le(dev_eui: &str) -> Option<[u8; 8]> {
    let bytes = hex::decode(dev_eui.replace([':', '-'], "")).ok()?;
    if bytes.len() != 8 {
        return None;
    }
    let mut le = [0u8; 8];
    for (i, b) in bytes.iter().rev().enumerate() {
        le[i] = *b;
    }
    Some(le)
}
