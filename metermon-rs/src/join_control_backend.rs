//! Mock Device Manager backend driver for the LoRaWAN join-control endpoint — the
//! opposite half of [`crate::join_control`], for exercising and fuzzing it end to end.
//!
//! In production the Config App drives a device optically and talks to the gateway
//! *through* the backend Device Manager; this module stands in for that backend on the
//! MQTT plane. It PUBLISHES the device-config-side messages ([`ArmRequest`],
//! [`FiredNotice`], [`VerifyRequest`]) and CONSUMES the gateway-side replies
//! ([`ArmReply`], [`JoinStatus`], [`VerifyReply`]), driving full arm→fire→verify cycles.
//!
//! It is **pure MQTT** — no radio, no [`crate::join_responder`] — so it runs on any host
//! (a Mac included) against a gateway endpoint that is either driving a real responder or
//! running under `--simulate-join`. The wire contract is the shared
//! [`lorawan_join_control`] crate; nothing here invents JSON, and no key material ever
//! crosses these topics.
//!
//! ## What it proves
//! One cycle is a full handshake: arm the responder, fire the optical join a few times
//! until the gateway reports an assigned DevAddr, then verify that both sides agree on a
//! [`JoinOutcome::VerifiedBothSides`]. Driving many cycles — optionally with random
//! wait-states, message reordering and duplication (`fuzz`) — checks that the gateway
//! endpoint stays correct under adverse control-plane timing: arm is idempotent, and
//! verify still reconciles regardless of how the messages are spaced or repeated.
//!
//! The PRNG is a tiny inline xorshift seeded by `seed`, so a fuzz run is fully
//! reproducible; no `rand` crate is pulled in for it.

use anyhow::{Context, Result};
use lorawan_join_control::{
    topics, ArmReply, ArmRequest, FiredNotice, JoinStatus, VerifyReply, VerifyRequest,
};
use rumqttc::{Client, Event, Incoming, MqttOptions, QoS};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::devices::now_unix;

/// NetID the simulated device claims in its `VerifyRequest` (`0x13` — a private/dev
/// NetID). The gateway reconciles on DevAddr, not NetID, so any value round-trips.
const DEVICE_NET_ID: u32 = 0x13;

/// Maximum number of `FiredNotice`s per cycle before giving up on an assignment.
const MAX_FIRES: u32 = 8;

/// How long to wait for a `JoinStatus` after each fire before firing again.
const FIRE_CADENCE: Duration = Duration::from_millis(1500);

/// Hard per-wait timeout: a reply that does not arrive within this fails the cycle
/// rather than blocking the whole run.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// Upper bound of the random wait-state injected before each publish when fuzzing.
const WAIT_MAX_MS: u64 = 4000;

/// Fuzzing probabilities (percent). Kept as named constants so the mix is easy to tune.
const FUZZ_REORDER_PCT: u64 = 20;
const FUZZ_DUPLICATE_PCT: u64 = 20;

/// One decoded gateway-side message, forwarded from the MQTT pump thread to the driver.
enum BackendMsg {
    ArmReply(ArmReply),
    Status(JoinStatus),
    VerifyReply(VerifyReply),
}

/// Tiny deterministic xorshift64 PRNG. Seeded for reproducibility; not for anything
/// cryptographic — it only spaces and shuffles fuzz messages.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // xorshift needs a non-zero state; the low-bit OR guarantees it.
        Rng((seed ^ 0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// Uniform in `[0, n)`; `below(0)` is defined as 0.
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next_u64() % n
        }
    }
    /// True with probability `pct`%.
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

/// Per-cycle measurements, collected for the end-of-run summary.
struct CycleResult {
    armed_latency: Option<Duration>,
    fires_to_assign: Option<u32>,
    time_to_verify: Option<Duration>,
    verified: bool,
}

/// Bus over the pump channel: absorbs gateway-side messages and holds the latest of each
/// so a status that lands while we are waiting on an arm-reply is not lost.
struct Bus {
    rx: Receiver<BackendMsg>,
    last_arm_reply: Option<ArmReply>,
    last_status: Option<JoinStatus>,
    last_verify_reply: Option<VerifyReply>,
}

impl Bus {
    fn new(rx: Receiver<BackendMsg>) -> Self {
        Bus {
            rx,
            last_arm_reply: None,
            last_status: None,
            last_verify_reply: None,
        }
    }

    fn absorb(&mut self, m: BackendMsg) {
        match m {
            BackendMsg::ArmReply(r) => self.last_arm_reply = Some(r),
            BackendMsg::Status(s) => self.last_status = Some(s),
            BackendMsg::VerifyReply(v) => self.last_verify_reply = Some(v),
        }
    }

    /// Discard any buffered messages so the next wait sees only fresh replies.
    fn clear(&mut self) {
        while self.rx.try_recv().is_ok() {}
        self.last_arm_reply = None;
        self.last_status = None;
        self.last_verify_reply = None;
    }

    /// Block until the next `ArmReply` (or timeout / pump gone).
    fn wait_arm_reply(&mut self, timeout: Duration) -> Result<Option<ArmReply>> {
        let deadline = Instant::now() + timeout;
        if let Some(r) = self.last_arm_reply.take() {
            return Ok(Some(r));
        }
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            match self.rx.recv_timeout(deadline - now) {
                Ok(m) => {
                    self.absorb(m);
                    if let Some(r) = self.last_arm_reply.take() {
                        return Ok(Some(r));
                    }
                }
                Err(RecvTimeoutError::Timeout) => return Ok(None),
                Err(RecvTimeoutError::Disconnected) => {
                    anyhow::bail!("mqtt pump thread gone")
                }
            }
        }
    }

    /// Block until a `JoinStatus` carrying an assigned DevAddr (or timeout / pump gone).
    fn wait_assigned(&mut self, timeout: Duration) -> Result<Option<u32>> {
        let deadline = Instant::now() + timeout;
        if let Some(addr) = self.last_status.as_ref().and_then(|s| s.assigned_dev_addr) {
            return Ok(Some(addr));
        }
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            match self.rx.recv_timeout(deadline - now) {
                Ok(m) => {
                    self.absorb(m);
                    if let Some(addr) = self.last_status.as_ref().and_then(|s| s.assigned_dev_addr)
                    {
                        return Ok(Some(addr));
                    }
                }
                Err(RecvTimeoutError::Timeout) => return Ok(None),
                Err(RecvTimeoutError::Disconnected) => {
                    anyhow::bail!("mqtt pump thread gone")
                }
            }
        }
    }

    /// Block until the next `VerifyReply` (or timeout / pump gone).
    fn wait_verify_reply(&mut self, timeout: Duration) -> Result<Option<VerifyReply>> {
        let deadline = Instant::now() + timeout;
        if let Some(v) = self.last_verify_reply.take() {
            return Ok(Some(v));
        }
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            match self.rx.recv_timeout(deadline - now) {
                Ok(m) => {
                    self.absorb(m);
                    if let Some(v) = self.last_verify_reply.take() {
                        return Ok(Some(v));
                    }
                }
                Err(RecvTimeoutError::Timeout) => return Ok(None),
                Err(RecvTimeoutError::Disconnected) => {
                    anyhow::bail!("mqtt pump thread gone")
                }
            }
        }
    }
}

/// Serialize `msg` as JSON and publish it at QoS 1 (not retained), after an optional
/// fuzz wait-state.
fn publish<T: serde::Serialize>(
    client: &Client,
    topic: &str,
    msg: &T,
    fuzz: bool,
    rng: &mut Rng,
) -> Result<()> {
    if fuzz {
        let wait = rng.below(WAIT_MAX_MS + 1);
        if wait > 0 {
            std::thread::sleep(Duration::from_millis(wait));
        }
    }
    let bytes = serde_json::to_vec(msg).context("serialize control message")?;
    client
        .publish(topic, QoS::AtLeastOnce, false, bytes)
        .with_context(|| format!("publish to {topic}"))?;
    Ok(())
}

/// Run `cycles` full arm→fire→verify handshakes against the gateway's join-control
/// endpoint. Blocks; prints a per-cycle line and an end-of-run summary. `fuzz` injects
/// random wait-states, reordering and duplication; `seed` makes that reproducible.
pub fn run(
    cfg: &Config,
    dev_eui: &str,
    channel_hz: u32,
    sf: u8,
    cycles: u32,
    fuzz: bool,
    seed: u64,
) -> Result<()> {
    let gwid = cfg.gwid.clone();

    // Topics: we publish on the device-config side, subscribe on the gateway side.
    let arm_topic = topics::arm(&gwid);
    let fired_topic = topics::fired(&gwid);
    let verify_topic = topics::verify(&gwid);
    let arm_reply_topic = topics::arm_reply(&gwid);
    let status_topic = topics::status(&gwid, dev_eui);
    let verify_reply_topic = topics::verify_reply(&gwid);

    let mut opts = MqttOptions::new(
        format!("{}-joinbackend", cfg.mqtt.clientid),
        cfg.mqtt.host.clone(),
        cfg.mqtt.port,
    );
    opts.set_keep_alive(Duration::from_secs(30));
    let (client, connection) = Client::new(opts, 16);

    client
        .subscribe(&arm_reply_topic, QoS::AtLeastOnce)
        .context("subscribe arm_reply topic")?;
    client
        .subscribe(&status_topic, QoS::AtLeastOnce)
        .context("subscribe status topic")?;
    client
        .subscribe(&verify_reply_topic, QoS::AtLeastOnce)
        .context("subscribe verify_reply topic")?;

    log::info!(
        "join-control backend on gateway {gwid}: driving {cycles} cycle(s) for {dev_eui} \
         @ {:.3} MHz SF{sf}{}",
        channel_hz as f64 / 1e6,
        if fuzz {
            format!(" [FUZZ seed={seed}]")
        } else {
            String::new()
        }
    );

    // MQTT pump on a background thread, forwarding decoded gateway-side replies to the
    // driver via an mpsc channel; the Client stays here for publishing. Same shape as
    // crate::join_control and crate::mock_backend.
    let (tx, rx) = mpsc::channel::<BackendMsg>();
    let arm_reply_t = arm_reply_topic.clone();
    let status_t = status_topic.clone();
    let verify_reply_t = verify_reply_topic.clone();
    let pump = std::thread::spawn(move || {
        let mut connection = connection;
        for event in connection.iter() {
            match event {
                Ok(Event::Incoming(Incoming::Publish(p))) => {
                    let msg = if p.topic == arm_reply_t {
                        match serde_json::from_slice::<ArmReply>(&p.payload) {
                            Ok(m) => Some(BackendMsg::ArmReply(m)),
                            Err(e) => {
                                log::warn!("bad ArmReply JSON on {}: {e}", p.topic);
                                None
                            }
                        }
                    } else if p.topic == status_t {
                        match serde_json::from_slice::<JoinStatus>(&p.payload) {
                            Ok(m) => Some(BackendMsg::Status(m)),
                            Err(e) => {
                                log::warn!("bad JoinStatus JSON on {}: {e}", p.topic);
                                None
                            }
                        }
                    } else if p.topic == verify_reply_t {
                        match serde_json::from_slice::<VerifyReply>(&p.payload) {
                            Ok(m) => Some(BackendMsg::VerifyReply(m)),
                            Err(e) => {
                                log::warn!("bad VerifyReply JSON on {}: {e}", p.topic);
                                None
                            }
                        }
                    } else {
                        None
                    };
                    if let Some(m) = msg {
                        if tx.send(m).is_err() {
                            break; // driver gone
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    log::warn!("join-control backend mqtt error: {e}; retrying");
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }
        log::warn!("join-control backend mqtt pump exited");
    });

    let mut bus = Bus::new(rx);
    let mut rng = Rng::new(seed);
    let mut results: Vec<CycleResult> = Vec::with_capacity(cycles as usize);

    for cycle in 1..=cycles {
        let result = run_cycle(
            &client,
            &mut bus,
            &mut rng,
            &arm_topic,
            &fired_topic,
            &verify_topic,
            dev_eui,
            channel_hz,
            sf,
            cycle,
            fuzz,
        )?;
        log_cycle(cycle, &result);
        results.push(result);
    }

    print_summary(&results);

    drop(client); // stop the pump so join() returns
    let _ = pump.join();
    Ok(())
}

/// Drive one full arm→fire→verify handshake.
#[allow(clippy::too_many_arguments)]
fn run_cycle(
    client: &Client,
    bus: &mut Bus,
    rng: &mut Rng,
    arm_topic: &str,
    fired_topic: &str,
    verify_topic: &str,
    dev_eui: &str,
    channel_hz: u32,
    sf: u8,
    cycle: u32,
    fuzz: bool,
) -> Result<CycleResult> {
    let mut result = CycleResult {
        armed_latency: None,
        fires_to_assign: None,
        time_to_verify: None,
        verified: false,
    };

    // Fresh window: drop anything buffered from a previous cycle.
    bus.clear();

    // 1) Arm. Optionally (fuzz) fire one bounded FiredNotice *before* waiting for the
    //    reply, to check the gateway's arm path is not order-sensitive.
    let arm = ArmRequest {
        dev_eui: dev_eui.to_string(),
        channel_hz,
        sf,
    };
    let armed_at = Instant::now();
    publish(client, arm_topic, &arm, fuzz, rng)?;

    if fuzz && rng.chance(FUZZ_REORDER_PCT) {
        let early = FiredNotice {
            dev_eui: dev_eui.to_string(),
            ts_unix: now_unix() as u64,
            fire_seq: 0,
        };
        log::info!("cycle {cycle}: (fuzz) firing seq 0 before arm_reply");
        publish(client, fired_topic, &early, fuzz, rng)?;
    }

    let arm_reply = match bus.wait_arm_reply(REPLY_TIMEOUT)? {
        Some(r) => r,
        None => {
            log::warn!("cycle {cycle}: no ArmReply within {REPLY_TIMEOUT:?} — cycle failed");
            return Ok(result);
        }
    };
    result.armed_latency = Some(armed_at.elapsed());
    if !arm_reply.armed || !arm_reply.creds_present {
        log::warn!(
            "cycle {cycle}: gateway did not arm (armed={}, creds_present={}) — cycle failed",
            arm_reply.armed,
            arm_reply.creds_present
        );
        return Ok(result);
    }

    // 2) Fire loop: up to MAX_FIRES notices on a cadence; stop once a DevAddr is assigned.
    let mut assigned: Option<u32> = None;
    for fire_seq in 1..=MAX_FIRES {
        let fired = FiredNotice {
            dev_eui: dev_eui.to_string(),
            ts_unix: now_unix() as u64,
            fire_seq,
        };
        publish(client, fired_topic, &fired, fuzz, rng)?;
        // Occasionally duplicate the fire — the gateway must not be knocked off course.
        if fuzz && rng.chance(FUZZ_DUPLICATE_PCT) {
            log::info!("cycle {cycle}: (fuzz) duplicating fire seq {fire_seq}");
            publish(client, fired_topic, &fired, fuzz, rng)?;
        }
        if let Some(addr) = bus.wait_assigned(FIRE_CADENCE)? {
            assigned = Some(addr);
            result.fires_to_assign = Some(fire_seq);
            break;
        }
    }
    // Last chance: give a slow gateway the rest of the reply timeout to assign.
    if assigned.is_none() {
        if let Some(addr) = bus.wait_assigned(REPLY_TIMEOUT)? {
            assigned = Some(addr);
            result.fires_to_assign = Some(MAX_FIRES);
        }
    }
    let Some(dev_addr) = assigned else {
        log::warn!("cycle {cycle}: no DevAddr assigned after {MAX_FIRES} fire(s) — cycle failed");
        return Ok(result);
    };

    // 3) Verify: claim the assigned DevAddr and expect VerifiedBothSides.
    let verify = VerifyRequest {
        dev_eui: dev_eui.to_string(),
        device_dev_addr: dev_addr,
        device_net_id: DEVICE_NET_ID,
    };
    let verify_at = Instant::now();
    publish(client, verify_topic, &verify, fuzz, rng)?;
    let verify_reply = match bus.wait_verify_reply(REPLY_TIMEOUT)? {
        Some(v) => v,
        None => {
            log::warn!("cycle {cycle}: no VerifyReply within {REPLY_TIMEOUT:?} — cycle failed");
            return Ok(result);
        }
    };
    result.time_to_verify = Some(verify_at.elapsed());
    result.verified = verify_reply.outcome.is_confirmed();
    if !result.verified {
        log::warn!(
            "cycle {cycle}: verify outcome {:?} is not VerifiedBothSides",
            verify_reply.outcome
        );
    }
    Ok(result)
}

fn log_cycle(cycle: u32, r: &CycleResult) {
    let armed = r
        .armed_latency
        .map(|d| format!("{} ms", d.as_millis()))
        .unwrap_or_else(|| "-".into());
    let fires = r
        .fires_to_assign
        .map(|n| n.to_string())
        .unwrap_or_else(|| "-".into());
    let ttv = r
        .time_to_verify
        .map(|d| format!("{} ms", d.as_millis()))
        .unwrap_or_else(|| "-".into());
    log::info!(
        "cycle {cycle}: {} | armed={armed} fires_to_assign={fires} time_to_verify={ttv}",
        if r.verified { "PASS" } else { "FAIL" }
    );
}

fn print_summary(results: &[CycleResult]) {
    let total = results.len();
    let ok = results.iter().filter(|r| r.verified).count();
    let rate = if total > 0 {
        ok as f64 * 100.0 / total as f64
    } else {
        0.0
    };

    let mut ttv: Vec<u128> = results
        .iter()
        .filter(|r| r.verified)
        .filter_map(|r| r.time_to_verify.map(|d| d.as_millis()))
        .collect();
    ttv.sort_unstable();

    println!("\n===== join-control backend summary =====");
    println!("cycles          : {total}");
    println!("verified        : {ok} ({rate:.1}%)");
    if let (Some(&min), Some(&max)) = (ttv.first(), ttv.last()) {
        let median = ttv[ttv.len() / 2];
        println!("time-to-verify  : min {min} ms / median {median} ms / max {max} ms");
    } else {
        println!("time-to-verify  : (no verified cycles)");
    }
    println!("========================================");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic_for_a_seed() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn rng_below_stays_in_range() {
        let mut r = Rng::new(7);
        for _ in 0..1000 {
            assert!(r.below(WAIT_MAX_MS + 1) <= WAIT_MAX_MS);
        }
        assert_eq!(r.below(0), 0);
    }

    #[test]
    fn chance_zero_and_hundred_are_never_and_always() {
        let mut r = Rng::new(99);
        for _ in 0..100 {
            assert!(!r.chance(0));
            assert!(r.chance(100));
        }
    }
}
