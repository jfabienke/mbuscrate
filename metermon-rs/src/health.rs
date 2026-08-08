//! Gateway self-instrumentation: a health snapshot the gateway reports about *itself*
//! (upstream over MQTT), plus the watchdog decision logic that keeps a remote, unattended
//! gateway receiving.
//!
//! This module is deliberately platform-independent and side-effect-free so the decision
//! logic is unit-testable off-hardware; the radio reads and recovery actions are thin glue
//! in the monitor loop (Pi-only), and the MQTT publish is in `publish`.

use serde::Serialize;

/// RFM69 operating mode, decoded from the RegOpMode mode field (bits 4:2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioState {
    Sleep,
    Standby,
    FreqSynth,
    Tx,
    Rx,
    Unknown,
}

impl RadioState {
    /// Decode from a raw RegOpMode byte (mode = bits 4:2).
    pub fn from_opmode(opmode: u8) -> Self {
        match (opmode >> 2) & 0x07 {
            0 => RadioState::Sleep,
            1 => RadioState::Standby,
            2 => RadioState::FreqSynth,
            3 => RadioState::Tx,
            4 => RadioState::Rx,
            _ => RadioState::Unknown,
        }
    }

    /// Decode an SX126x GetStatus byte (chip mode in bits 6:4). The raw byte is
    /// still reported as `radio_opmode`; only the interpretation differs per chip.
    pub fn from_sx126x_status(status: u8) -> Self {
        match (status >> 4) & 0x07 {
            0x2 | 0x3 => RadioState::Standby,
            0x4 => RadioState::FreqSynth,
            0x5 => RadioState::Rx,
            0x6 => RadioState::Tx,
            _ => RadioState::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RadioState::Sleep => "sleep",
            RadioState::Standby => "standby",
            RadioState::FreqSynth => "fs",
            RadioState::Tx => "tx",
            RadioState::Rx => "rx",
            RadioState::Unknown => "unknown",
        }
    }
}

/// A point-in-time health snapshot of the gateway, published upstream so the fleet is
/// observable without SSH. Serializes to the JSON body of the `.../health` topic.
#[derive(Debug, Clone, Serialize)]
pub struct GatewayHealth {
    pub gwid: String,
    /// "online" while running; the retained "offline" is delivered by the broker via LWT.
    pub status: &'static str,
    /// Wall-clock seconds since the Unix epoch when this snapshot was taken.
    pub ts: i64,
    pub uptime_secs: u64,

    // --- radio ---
    pub radio_opmode: Option<u8>,
    pub radio_state: &'static str,
    pub frames_per_min: f64,
    pub last_frame_age_secs: Option<u64>,
    pub radio_recoveries: u32,

    // --- reception ---
    pub meters_seen: usize,
    pub crc_pass_pct: Option<u8>,
    pub keys_held: usize,

    // --- system (Linux/Pi; None where unavailable, e.g. off-device) ---
    pub cpu_temp_c: Option<f32>,
    pub load_avg_1m: Option<f32>,
}

impl GatewayHealth {
    pub fn to_json(&self) -> serde_json::Value {
        // Serializing a plain struct of primitives cannot fail.
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// Read the SoC temperature in °C from Linux thermal sysfs. `None` off-Linux or on any
/// read/parse failure (never panics).
pub fn read_cpu_temp_c() -> Option<f32> {
    let raw = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").ok()?;
    raw.trim().parse::<f32>().ok().map(|milli| milli / 1000.0)
}

/// Read the 1-minute load average from `/proc/loadavg`. `None` off-Linux or on failure.
pub fn read_load_avg_1m() -> Option<f32> {
    let raw = std::fs::read_to_string("/proc/loadavg").ok()?;
    raw.split_whitespace().next()?.parse::<f32>().ok()
}

/// What the watchdog wants the supervisor to do this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogAction {
    /// Radio is receiving — nothing to do.
    Healthy,
    /// Reception has stalled; attempt in-process recovery (re-init the radio).
    /// `attempt` is the 1-based consecutive-recovery count.
    Recover { attempt: u32 },
    /// In-process recovery has failed `max_attempts` times running; exit so the process
    /// supervisor (systemd `Restart=always`) relaunches, and raise a loud alert.
    Escalate,
}

/// Decides when a stalled radio needs recovery and when to escalate.
///
/// The trigger is deliberately **frame-recency**, not raw `opmode`: the driver briefly
/// drops to STANDBY between packets, so a single non-RX `opmode` read is normal and would
/// false-positive. "No decoded frame within `stall_after`" robustly catches every
/// reception-death cause (stuck STANDBY, frozen RX, wedged loop) without that noise. The
/// `opmode` is still reported in [`GatewayHealth`] for diagnosis.
#[derive(Debug, Clone)]
pub struct RadioWatchdog {
    stall_after_secs: u64,
    max_attempts: u32,
    attempts: u32,
}

impl RadioWatchdog {
    /// `stall_after_secs`: no-frame window before a stall is declared (set generously vs.
    /// the normal inter-frame gap). `max_attempts`: consecutive in-process recoveries
    /// before escalating to a process restart.
    pub fn new(stall_after_secs: u64, max_attempts: u32) -> Self {
        Self {
            stall_after_secs,
            max_attempts,
            attempts: 0,
        }
    }

    /// Consecutive recovery attempts since the radio was last healthy.
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Call once per supervision tick with the age of the most recent decoded frame.
    /// `None` means "no frame ever seen yet" (treated as its age being the uptime by the
    /// caller; pass a large value to trip the stall, or `None` to hold off until the first
    /// frame — see the monitor loop).
    pub fn assess(&mut self, since_last_frame_secs: u64) -> WatchdogAction {
        if since_last_frame_secs < self.stall_after_secs {
            self.attempts = 0;
            return WatchdogAction::Healthy;
        }
        self.attempts += 1;
        if self.attempts > self.max_attempts {
            WatchdogAction::Escalate
        } else {
            WatchdogAction::Recover {
                attempt: self.attempts,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opmode_decodes_rx_and_standby() {
        assert_eq!(RadioState::from_opmode(0x10), RadioState::Rx); // the working state
        assert_eq!(RadioState::from_opmode(0x04), RadioState::Standby); // the stall we hit
        assert_eq!(RadioState::from_opmode(0x00), RadioState::Sleep);
        assert_eq!(RadioState::from_opmode(0x0C), RadioState::Tx);
        assert_eq!(RadioState::from_opmode(0x10).as_str(), "rx");
        assert_eq!(RadioState::from_opmode(0x04).as_str(), "standby");
    }

    #[test]
    fn healthy_while_frames_are_recent() {
        let mut wd = RadioWatchdog::new(300, 3);
        assert_eq!(wd.assess(10), WatchdogAction::Healthy);
        assert_eq!(wd.assess(299), WatchdogAction::Healthy);
        assert_eq!(wd.attempts(), 0);
    }

    #[test]
    fn recovers_then_escalates_when_stalled() {
        let mut wd = RadioWatchdog::new(300, 3);
        // Stall (no frame for >= 300s) -> escalating recovery attempts.
        assert_eq!(wd.assess(301), WatchdogAction::Recover { attempt: 1 });
        assert_eq!(wd.assess(400), WatchdogAction::Recover { attempt: 2 });
        assert_eq!(wd.assess(500), WatchdogAction::Recover { attempt: 3 });
        // 4th consecutive stall exceeds max_attempts -> escalate to process restart.
        assert_eq!(wd.assess(600), WatchdogAction::Escalate);
    }

    #[test]
    fn recovery_counter_resets_after_frames_return() {
        let mut wd = RadioWatchdog::new(300, 3);
        assert_eq!(wd.assess(301), WatchdogAction::Recover { attempt: 1 });
        assert_eq!(wd.assess(400), WatchdogAction::Recover { attempt: 2 });
        // A fresh frame arrives -> healthy, counter resets.
        assert_eq!(wd.assess(5), WatchdogAction::Healthy);
        assert_eq!(wd.attempts(), 0);
        // Next stall starts the ladder over at attempt 1.
        assert_eq!(wd.assess(301), WatchdogAction::Recover { attempt: 1 });
    }
}
