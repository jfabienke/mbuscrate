//! Dual-mode profile scheduler for a single SX126x radio.
//!
//! A single SX126x can run only one [`RadioProfile`] at a time. This scheduler holds the
//! radio in a base wM-Bus (GFSK) profile in continuous receive, and opens scheduled windows
//! during which it switches to another profile (typically LoRa) — e.g. around Class-A
//! downlink windows — then returns to base.
//!
//! It owns only the profile + RX/TX *lifecycle*; parsing and decoding live elsewhere
//! (see the mode-tagged routing in [`WMBusHandle`](crate::wmbus::handle)). Every profile
//! change leaves RX first, switches, then explicitly re-arms RX (honouring
//! [`switch_profile`](crate::wmbus::radio::driver::Sx126xDriver::switch_profile)'s
//! standby-ending contract). On error, timeout, or cancellation the base GFSK profile is
//! always restored, and each run ends in the documented state: base GFSK RX.
//!
//! Timing uses the monotonic [`tokio::time`] clock (`sleep_until`), never wall-clock, so
//! it is deterministic under a paused test runtime.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, Notify};
use tokio::time::{sleep_until, Instant};

use crate::wmbus::radio::driver::{RadioProfile, Sx126xExt};
use crate::wmbus::radio::radio_driver::{RadioDriver, RadioDriverError};

/// A race-safe cancellation signal for [`ProfileScheduler::run`].
///
/// It pairs an atomic `cancelled` flag — checked before and after every profile operation
/// and around every timer wait — with a [`Notify`] used *only* to wake a pending wait. A
/// bare `Notify::notify_waiters()` is lost if no task is waiting at that instant; here the
/// flag records the request regardless, so cancellation can never be missed.
#[derive(Clone, Default)]
pub struct CancelToken {
    inner: Arc<CancelInner>,
}

#[derive(Default)]
struct CancelInner {
    cancelled: AtomicBool,
    notify: Notify,
}

impl CancelToken {
    /// A fresh, un-cancelled token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Request cancellation. Idempotent, and safe to call with no waiter present.
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        self.inner.notify.notify_waiters();
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    /// Wait for the next `cancel()` wake. Used only to interrupt a timer wait; correctness
    /// against a lost wake is provided by [`CancelToken::is_cancelled`].
    async fn notified(&self) {
        self.inner.notify.notified().await;
    }
}

/// A scheduled window during which the radio leaves the base profile for `profile`.
///
/// `offset` is measured from the start of [`ProfileScheduler::run`]; `duration` is how long
/// the alternate profile stays active before returning to base.
#[derive(Debug, Clone)]
pub struct ScheduledWindow {
    /// Offset from the scheduler start at which the window opens.
    pub offset: Duration,
    /// How long the window stays open.
    pub duration: Duration,
    /// The profile to run during the window (typically a LoRa profile).
    pub profile: RadioProfile,
}

/// Errors from the profile scheduler.
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    /// A radio operation failed.
    #[error("radio error: {0}")]
    Radio(#[from] RadioDriverError),
    /// Two scheduled windows overlap (or are out of order); the window opening at this
    /// offset starts before the previous one has closed.
    #[error("overlapping windows: a window opens at {0:?} before the previous one closes")]
    OverlappingWindows(Duration),
    /// Restoring the base GFSK profile failed after an earlier error or a cancellation, so
    /// the radio is in an undefined profile/state. Both the triggering error (`original`,
    /// `None` if recovery followed a cancellation) and the recovery failure are retained.
    #[error("base GFSK restore failed (radio state undefined) after {original:?}")]
    RecoveryFailed {
        /// The error that triggered recovery, if any (`None` after a cancellation).
        original: Option<Box<SchedulerError>>,
        /// The failure encountered while restoring base GFSK RX.
        #[source]
        recovery: RadioDriverError,
    },
}

/// Validate that windows are ordered by offset and non-overlapping.
fn validate_windows(windows: &[ScheduledWindow]) -> Result<(), SchedulerError> {
    let mut cursor = Duration::ZERO;
    for w in windows {
        if w.offset < cursor {
            return Err(SchedulerError::OverlappingWindows(w.offset));
        }
        cursor = w.offset + w.duration;
    }
    Ok(())
}

/// Outcome of the timeline loop, so `run` knows whether a base restore is still needed.
#[derive(PartialEq, Eq)]
enum Completed {
    /// Ran to the end of the schedule; already back in base GFSK RX.
    Finished,
    /// Interrupted by the cancel signal; caller must restore base.
    Cancelled,
}

#[derive(PartialEq, Eq)]
enum Wait {
    Elapsed,
    Cancelled,
}

/// Sleep until `deadline`, or return early if `cancel` fires.
///
/// The flag is checked before waiting (catches a cancellation requested before the wait
/// started) and after the timer elapses (catches a `cancel()` whose wake raced the timer),
/// with `notify` only used to interrupt an in-progress wait — so no cancellation is lost.
async fn wait_until(deadline: Instant, cancel: &CancelToken) -> Wait {
    if cancel.is_cancelled() {
        return Wait::Cancelled;
    }
    tokio::select! {
        _ = sleep_until(deadline) => {
            if cancel.is_cancelled() {
                Wait::Cancelled
            } else {
                Wait::Elapsed
            }
        }
        _ = cancel.notified() => Wait::Cancelled,
    }
}

/// Drives base-profile RX plus scheduled alternate-profile windows on a **shared** radio.
///
/// The driver is held as `Arc<Mutex<D>>` — the *same* instance the [`WMBusHandle`] receiver
/// polls (build one via [`WMBusHandle::shared_driver`](crate::wmbus::handle::WMBusHandle::shared_driver)),
/// never a second driver. Each transition holds the lock for its whole `stop → switch →
/// start` sequence, so the receiver cannot interleave RX commands mid-transition.
pub struct ProfileScheduler<D> {
    driver: Arc<Mutex<D>>,
    base: RadioProfile,
}

impl<D: RadioDriver + Sx126xExt + Send> ProfileScheduler<D> {
    /// Create a scheduler over the shared `driver`, resting in `base` (the wM-Bus/GFSK
    /// profile). Pass the handle's driver Arc so both share one radio instance.
    pub fn new(driver: Arc<Mutex<D>>, base: RadioProfile) -> Self {
        Self { driver, base }
    }

    /// The shared driver this scheduler drives (a clone of the `Arc`).
    pub fn driver(&self) -> Arc<Mutex<D>> {
        self.driver.clone()
    }

    /// Immediately switch to `profile` (leave RX → switch → arm RX), serialized under the
    /// driver lock. On failure the radio may be in an undefined state, so the error is
    /// returned (never swallowed) for the caller to treat as a fault.
    pub async fn switch_to(&self, profile: &RadioProfile) -> Result<(), SchedulerError> {
        self.enter_profile_rx(profile)
            .await
            .map_err(SchedulerError::Radio)
    }

    /// Leave RX, switch to `profile`, then arm continuous RX — the whole sequence under one
    /// lock hold on the shared driver.
    ///
    /// `stop_receive` first satisfies "stop/leave RX before every profile change" from any
    /// state; `switch_profile` ends in standby by contract, so RX is armed explicitly after.
    /// Holding the lock across all three prevents the receiver loop from issuing a competing
    /// RX command between them.
    async fn enter_profile_rx(&self, profile: &RadioProfile) -> Result<(), RadioDriverError> {
        let mut driver = self.driver.lock().await;
        driver.stop_receive().await?;
        driver.switch_profile(profile).await?;
        driver.start_receive().await?;
        Ok(())
    }

    /// Return the radio to base GFSK RX. Used on the error/cancel path; a failure here is
    /// surfaced (as [`SchedulerError::RecoveryFailed`] by the caller), never swallowed —
    /// otherwise the caller would believe the radio is in GFSK RX when it may still be in
    /// LoRa or standby.
    async fn restore_base(&self) -> Result<(), RadioDriverError> {
        let base = self.base.clone();
        self.enter_profile_rx(&base).await
    }

    /// Run the schedule: enter base GFSK RX, then open each window in turn, always ending in
    /// base GFSK RX.
    ///
    /// `cancel` interrupts cooperatively (via [`Notify::notify_one`] /
    /// [`Notify::notify_waiters`]). On cancellation, a radio error, or a timeout, the base
    /// GFSK profile is restored before returning.
    pub async fn run(
        &self,
        windows: &[ScheduledWindow],
        cancel: &CancelToken,
    ) -> Result<(), SchedulerError> {
        validate_windows(windows)?;

        match self.run_inner(windows, cancel).await {
            // Finished cleanly — the last step already returned to base GFSK RX.
            Ok(Completed::Finished) => Ok(()),
            // Cancelled mid-schedule — restore the documented base state; if that fails, the
            // radio state is undefined and the caller must know.
            Ok(Completed::Cancelled) => {
                self.restore_base()
                    .await
                    .map_err(|recovery| SchedulerError::RecoveryFailed {
                        original: None,
                        recovery,
                    })
            }
            // Radio error — restore base, then surface either the original error (restore
            // ok) or a RecoveryFailed carrying both errors (restore also failed).
            Err(original) => match self.restore_base().await {
                Ok(()) => Err(original),
                Err(recovery) => Err(SchedulerError::RecoveryFailed {
                    original: Some(Box::new(original)),
                    recovery,
                }),
            },
        }
    }

    async fn run_inner(
        &self,
        windows: &[ScheduledWindow],
        cancel: &CancelToken,
    ) -> Result<Completed, SchedulerError> {
        let base = self.base.clone();
        self.enter_profile_rx(&base).await?;

        let start = Instant::now();
        for w in windows {
            // Hold base RX until the window opens.
            if wait_until(start + w.offset, cancel).await == Wait::Cancelled {
                return Ok(Completed::Cancelled);
            }
            // Re-check right before the profile change (closes the wait→switch race).
            if cancel.is_cancelled() {
                return Ok(Completed::Cancelled);
            }
            self.enter_profile_rx(&w.profile).await?;

            // Hold the window open for its duration.
            if wait_until(start + w.offset + w.duration, cancel).await == Wait::Cancelled {
                return Ok(Completed::Cancelled);
            }
            if cancel.is_cancelled() {
                return Ok(Completed::Cancelled);
            }
            // Each window ends by returning to base GFSK RX.
            self.enter_profile_rx(&base).await?;
        }
        Ok(Completed::Finished)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wmbus::radio::driver::{LoRaProfile, Sx126xDriver, WmbusProfile};
    use crate::wmbus::radio::hal::RecordingHal;
    use crate::wmbus::radio::modulation::{CodingRate, LoRaBandwidth, SpreadingFactor};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// A scheduler over a fresh shared driver backed by `hal`, resting in base GFSK.
    fn scheduler_with(hal: RecordingHal) -> ProfileScheduler<Sx126xDriver<RecordingHal>> {
        let driver = Sx126xDriver::new(hal, 32_000_000);
        ProfileScheduler::new(Arc::new(Mutex::new(driver)), wmbus_base())
    }

    // SX126x command opcodes referenced by the assertions.
    const SET_STANDBY: u8 = 0x80;
    const SET_RX: u8 = 0x82;
    const SET_PACKET_TYPE: u8 = 0x8A;
    const GFSK: u8 = 0x00;
    const LORA: u8 = 0x01;

    fn wmbus_base() -> RadioProfile {
        RadioProfile::Wmbus(WmbusProfile::mode_c(868_950_000, 100_000))
    }

    fn lora_window() -> RadioProfile {
        RadioProfile::LoRa(LoRaProfile {
            frequency_hz: 868_100_000,
            sf: SpreadingFactor::SF7,
            bw: LoRaBandwidth::BW125,
            cr: CodingRate::CR4_5,
            power_dbm: 14,
            sync_word: None,
        })
    }

    fn one_lora_window() -> Vec<ScheduledWindow> {
        vec![ScheduledWindow {
            offset: Duration::from_secs(1),
            duration: Duration::from_secs(1),
            profile: lora_window(),
        }]
    }

    /// Sequence of packet-type selections (the data byte of each SetPacketType), in order.
    fn packet_type_sequence(cmds: &[(u8, Vec<u8>)]) -> Vec<u8> {
        cmds.iter()
            .filter(|(op, _)| *op == SET_PACKET_TYPE)
            .map(|(_, d)| d[0])
            .collect()
    }

    #[test]
    fn rejects_overlapping_windows() {
        let windows = vec![
            ScheduledWindow {
                offset: Duration::from_secs(0),
                duration: Duration::from_secs(2),
                profile: lora_window(),
            },
            ScheduledWindow {
                offset: Duration::from_secs(1), // opens before the first closes at t=2s
                duration: Duration::from_secs(1),
                profile: lora_window(),
            },
        ];
        assert!(matches!(
            validate_windows(&windows),
            Err(SchedulerError::OverlappingWindows(_))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn runs_window_and_returns_to_base() {
        let hal = RecordingHal::new();
        let probe = hal.clone();
        let sched = scheduler_with(hal);
        let cancel = CancelToken::new();

        sched.run(&one_lora_window(), &cancel).await.unwrap();

        let cmds = probe.commands();
        // GFSK (base) -> LoRa (window) -> GFSK (window end).
        assert_eq!(packet_type_sequence(&cmds), vec![GFSK, LORA, GFSK]);
        // Ends armed in RX.
        assert_eq!(cmds.last().map(|(op, _)| *op), Some(SET_RX));
        // Every profile change is preceded by leaving RX (SetStandby).
        let first_pkt = cmds
            .iter()
            .position(|(op, _)| *op == SET_PACKET_TYPE)
            .unwrap();
        let first_standby = cmds.iter().position(|(op, _)| *op == SET_STANDBY).unwrap();
        assert!(first_standby < first_pkt);
    }

    #[tokio::test(start_paused = true)]
    async fn restores_base_on_error() {
        // Fault the LoRa packet-type selection during the window switch.
        let hal = RecordingHal::fail_on(SET_PACKET_TYPE, &[LORA]);
        let probe = hal.clone();
        let sched = scheduler_with(hal);
        let cancel = CancelToken::new();

        let result = sched.run(&one_lora_window(), &cancel).await;
        assert!(
            matches!(result, Err(SchedulerError::Radio(_))),
            "the LoRa switch fault surfaces as a radio error (base restore succeeds)"
        );

        // After the fault, the scheduler must have restored base GFSK RX: the *last*
        // packet-type selected is GFSK, and a SetRx follows it.
        let cmds = probe.commands();
        let last_pkt = cmds
            .iter()
            .rposition(|(op, _)| *op == SET_PACKET_TYPE)
            .expect("a packet type was selected");
        assert_eq!(cmds[last_pkt].1, vec![GFSK], "restored to GFSK modem");
        let last_rx = cmds
            .iter()
            .rposition(|(op, _)| *op == SET_RX)
            .expect("RX was armed");
        assert!(last_rx > last_pkt, "RX re-armed after returning to GFSK");
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_restores_base() {
        let hal = RecordingHal::new();
        let probe = hal.clone();
        let sched = scheduler_with(hal);
        let cancel = CancelToken::new();

        // Pre-cancel: the flag is set before `run`, so the scheduler's first wait sees it
        // immediately (no lost-wake race) and never enters the LoRa window.
        cancel.cancel();
        sched.run(&one_lora_window(), &cancel).await.unwrap();

        // Never entered LoRa; ended in base GFSK RX.
        let cmds = probe.commands();
        assert!(
            !cmds
                .iter()
                .any(|(op, d)| *op == SET_PACKET_TYPE && d == &[LORA]),
            "cancellation before the window means LoRa is never selected"
        );
        assert_eq!(packet_type_sequence(&cmds).last(), Some(&GFSK));
        assert_eq!(cmds.last().map(|(op, _)| *op), Some(SET_RX));
    }

    #[tokio::test]
    async fn recovery_failure_is_surfaced() {
        // Fail every SetPacketType so the base restore itself cannot complete: the scheduler
        // must report RecoveryFailed and retain the triggering error rather than claim success.
        let hal = RecordingHal::fail_every(SET_PACKET_TYPE);
        let sched = scheduler_with(hal);
        let cancel = CancelToken::new();

        let result = sched.run(&one_lora_window(), &cancel).await;
        match result {
            Err(SchedulerError::RecoveryFailed { original, .. }) => {
                assert!(original.is_some(), "the triggering error must be retained");
            }
            other => panic!("expected RecoveryFailed, got {other:?}"),
        }
    }
}
