//! SoC temperature, fan and clock instrumentation for the gateway host.
//!
//! A gateway is unattended, and on a Raspberry Pi 5 the two ways it silently gets slower
//! or dies are **thermal throttling** and a **stopped fan** — both invisible over MQTT
//! until someone notices missing meters. This module reads the Pi's thermal/fan/cpufreq
//! sysfs and turns it into something a fleet dashboard can alarm on.
//!
//! It matters more than usual when a HAT is stacked over the SoC: a concentrator or radio
//! HAT sits directly above the cooler, so airflow is restricted and the fan can be
//! physically blocked. [`ThermalStatus::Fan Stalled`](ThermalStatus::FanStalled) exists
//! precisely for that case — PWM commanded but no tacho pulses.
//!
//! Reads are pure sysfs (no process spawn, no `vcgencmd`), every field is `Option`, and
//! nothing panics: off-Linux or on a board without these interfaces the snapshot is simply
//! empty. The decision logic ([`assess`]) is pure and unit-tested off-hardware, matching
//! the split in [`crate::health`].

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Linux thermal/hwmon roots. Overridable in tests.
const THERMAL_ROOT: &str = "/sys/class/thermal";
const HWMON_ROOT: &str = "/sys/class/hwmon";
const CPUFREQ: &str = "/sys/devices/system/cpu/cpu0/cpufreq";

/// Above this the SoC is comfortable; the Pi 5's fan curve has not started ramping hard.
/// (Measured trip points on a Pi 5: fan steps at 50 / 60 / 67.5 / 75 °C.)
const WARM_C: f32 = 65.0;
/// Above this we are into the top of the fan curve and close to the clock-capping region.
const HOT_C: f32 = 75.0;
/// The Pi's soft temperature limit, where the firmware starts capping the ARM clock.
const THROTTLE_C: f32 = 80.0;

/// A point-in-time view of SoC thermals, the fan, and the resulting clock.
///
/// Every field is optional: absent means "this host does not expose it", never zero.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SocThermal {
    /// SoC temperature in °C.
    pub cpu_temp_c: Option<f32>,
    /// Measured fan speed (tachometer), RPM.
    pub fan_rpm: Option<u32>,
    /// Commanded fan PWM duty, 0–255 as the kernel reports it.
    pub fan_pwm: Option<u8>,
    /// Commanded duty as a percentage, for humans and dashboards.
    pub fan_pct: Option<u8>,
    /// Thermal-governor cooling step and its maximum (Pi 5: 0–4).
    pub cooling_state: Option<u32>,
    pub cooling_max_state: Option<u32>,
    /// Current and maximum ARM clock in MHz. A current well below max under load is the
    /// visible symptom of throttling (or of an `arm_freq` underclock in `config.txt`).
    pub cpu_freq_mhz: Option<u32>,
    pub cpu_freq_max_mhz: Option<u32>,
    /// True when the PMIC has latched an under-voltage condition — a frequent cause of
    /// mysterious instability once a HAT starts drawing real current.
    pub undervoltage: Option<bool>,
}

impl SocThermal {
    /// Read a snapshot from the running host. Never fails; unavailable fields are `None`.
    pub fn read() -> Self {
        Self::read_from(
            Path::new(THERMAL_ROOT),
            Path::new(HWMON_ROOT),
            Path::new(CPUFREQ),
        )
    }

    /// Root-relative form, so tests can point at a fixture tree.
    pub fn read_from(thermal_root: &Path, hwmon_root: &Path, cpufreq: &Path) -> Self {
        let cpu_temp_c = read_f32(&thermal_root.join("thermal_zone0/temp")).map(|m| m / 1000.0);

        // hwmon indices are not stable across boots, so locate the fan by its `name`
        // rather than assuming hwmon1.
        let fan = find_hwmon(hwmon_root, "pwmfan");
        let fan_rpm = fan.as_ref().and_then(|d| read_u32(&d.join("fan1_input")));
        let fan_pwm = fan
            .as_ref()
            .and_then(|d| read_u32(&d.join("pwm1")))
            .map(|v| v.min(255) as u8);
        let fan_pct = fan_pwm.map(|p| ((p as u32 * 100 + 127) / 255) as u8);

        // Same for the cooling device: match on type, not index.
        let cooling = find_cooling_device(thermal_root, "pwm-fan");
        let cooling_state = cooling
            .as_ref()
            .and_then(|d| read_u32(&d.join("cur_state")));
        let cooling_max_state = cooling
            .as_ref()
            .and_then(|d| read_u32(&d.join("max_state")));

        let cpu_freq_mhz = read_u32(&cpufreq.join("scaling_cur_freq")).map(|khz| khz / 1000);
        let cpu_freq_max_mhz = read_u32(&cpufreq.join("scaling_max_freq")).map(|khz| khz / 1000);

        // The PMIC exposes a latched low-critical alarm; 1 means under-voltage seen.
        let undervoltage = find_hwmon(hwmon_root, "rpi_volt")
            .and_then(|d| read_u32(&d.join("in0_lcrit_alarm")))
            .map(|v| v != 0);

        Self {
            cpu_temp_c,
            fan_rpm,
            fan_pwm,
            fan_pct,
            cooling_state,
            cooling_max_state,
            cpu_freq_mhz,
            cpu_freq_max_mhz,
            undervoltage,
        }
    }

    /// Classify this snapshot. See [`assess`] for the rules.
    pub fn status(&self) -> ThermalStatus {
        assess(self)
    }

    /// JSON form, for `--json` soak logs. Serializing plain primitives cannot fail.
    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    /// One-line human summary, for the CLI watch and log lines.
    pub fn summary(&self) -> String {
        let t = self
            .cpu_temp_c
            .map(|c| format!("{c:.1}°C"))
            .unwrap_or_else(|| "--".into());
        let fan = match (self.fan_rpm, self.fan_pct) {
            (Some(r), Some(p)) => format!("{r} rpm ({p}%)"),
            (Some(r), None) => format!("{r} rpm"),
            _ => "--".into(),
        };
        let step = match (self.cooling_state, self.cooling_max_state) {
            (Some(c), Some(m)) => format!("{c}/{m}"),
            _ => "--".into(),
        };
        let clk = match (self.cpu_freq_mhz, self.cpu_freq_max_mhz) {
            (Some(c), Some(m)) => format!("{c}/{m} MHz"),
            (Some(c), None) => format!("{c} MHz"),
            _ => "--".into(),
        };
        let uv = if self.undervoltage == Some(true) {
            "  UNDER-VOLTAGE"
        } else {
            ""
        };
        format!(
            "{t}  fan {fan}  step {step}  clk {clk}  [{}]{uv}",
            self.status().as_str()
        )
    }
}

/// What the thermal snapshot means operationally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalStatus {
    /// Comfortable.
    Ok,
    /// Fan is working and temperature is elevated but not near the cap.
    Warm,
    /// Close to the clock-capping threshold — sustained operation here is marginal.
    Hot,
    /// At or past the soft temperature limit: the firmware is capping the ARM clock.
    Throttling,
    /// The fan is commanded to spin but reports no rotation — blocked, unplugged or dead.
    /// Distinct from `Hot` because it is a *hardware fault*, actionable immediately, and
    /// the usual cause of it on a HAT-stacked board is the HAT itself fouling the fan.
    FanStalled,
    /// Not enough information (no thermal sysfs — e.g. running off-device).
    Unknown,
}

impl ThermalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ThermalStatus::Ok => "ok",
            ThermalStatus::Warm => "warm",
            ThermalStatus::Hot => "hot",
            ThermalStatus::Throttling => "throttling",
            ThermalStatus::FanStalled => "fan-stalled",
            ThermalStatus::Unknown => "unknown",
        }
    }

    /// Whether this state warrants an alert rather than a metric.
    pub fn is_alarm(self) -> bool {
        matches!(
            self,
            ThermalStatus::Throttling | ThermalStatus::FanStalled | ThermalStatus::Hot
        )
    }
}

/// Classify a snapshot.
///
/// A stalled fan outranks temperature: it is a hardware fault that will *become* a
/// thermal problem, and reporting only "warm" would hide the cause while the board slowly
/// cooks. Temperature bands otherwise follow the Pi 5's own fan trip points, with the
/// throttle band set at the firmware's soft temperature limit.
pub fn assess(t: &SocThermal) -> ThermalStatus {
    // Fan commanded but not turning. Require a meaningful duty: at very low PWM the fan
    // may legitimately be stopped, and some tachos read 0 until the fan spins up.
    if let (Some(pwm), Some(rpm)) = (t.fan_pwm, t.fan_rpm) {
        if pwm >= 32 && rpm == 0 {
            return ThermalStatus::FanStalled;
        }
    }
    match t.cpu_temp_c {
        None => ThermalStatus::Unknown,
        Some(c) if c >= THROTTLE_C => ThermalStatus::Throttling,
        Some(c) if c >= HOT_C => ThermalStatus::Hot,
        Some(c) if c >= WARM_C => ThermalStatus::Warm,
        Some(_) => ThermalStatus::Ok,
    }
}

// --- sysfs helpers: every one returns Option and never panics ---

fn read_u32(p: &Path) -> Option<u32> {
    std::fs::read_to_string(p).ok()?.trim().parse().ok()
}

fn read_f32(p: &Path) -> Option<f32> {
    std::fs::read_to_string(p).ok()?.trim().parse().ok()
}

/// Find the hwmon directory whose `name` matches, since hwmon numbering is not stable
/// across boots (the fan can be hwmon1 one boot and hwmon2 the next).
fn find_hwmon(root: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let dir = entry.path();
        if let Ok(n) = std::fs::read_to_string(dir.join("name")) {
            if n.trim() == name {
                return Some(dir);
            }
        }
    }
    None
}

/// Find the cooling device of a given `type` (e.g. "pwm-fan"), for the same reason.
fn find_cooling_device(thermal_root: &Path, kind: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(thermal_root).ok()?.flatten() {
        let dir = entry.path();
        if !dir
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("cooling_device"))
        {
            continue;
        }
        if let Ok(t) = std::fs::read_to_string(dir.join("type")) {
            if t.trim() == kind {
                return Some(dir);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(temp: Option<f32>, pwm: Option<u8>, rpm: Option<u32>) -> SocThermal {
        SocThermal {
            cpu_temp_c: temp,
            fan_pwm: pwm,
            fan_rpm: rpm,
            ..Default::default()
        }
    }

    #[test]
    fn temperature_bands_follow_the_pi5_fan_curve() {
        assert_eq!(assess(&snap(Some(45.0), None, None)), ThermalStatus::Ok);
        assert_eq!(assess(&snap(Some(64.9), None, None)), ThermalStatus::Ok);
        assert_eq!(assess(&snap(Some(65.0), None, None)), ThermalStatus::Warm);
        assert_eq!(assess(&snap(Some(74.9), None, None)), ThermalStatus::Warm);
        assert_eq!(assess(&snap(Some(75.0), None, None)), ThermalStatus::Hot);
        assert_eq!(assess(&snap(Some(79.9), None, None)), ThermalStatus::Hot);
        assert_eq!(
            assess(&snap(Some(80.0), None, None)),
            ThermalStatus::Throttling
        );
        assert_eq!(
            assess(&snap(Some(95.0), None, None)),
            ThermalStatus::Throttling
        );
    }

    #[test]
    fn a_commanded_fan_that_is_not_turning_outranks_temperature() {
        // The board can still be cool while the fan is blocked — report the fault, not
        // the (temporarily) fine temperature. This is the HAT-fouls-the-cooler case.
        assert_eq!(
            assess(&snap(Some(45.0), Some(120), Some(0))),
            ThermalStatus::FanStalled
        );
        // ...and it still outranks a genuinely hot reading.
        assert_eq!(
            assess(&snap(Some(85.0), Some(255), Some(0))),
            ThermalStatus::FanStalled
        );
    }

    #[test]
    fn a_stopped_fan_at_idle_duty_is_not_a_stall() {
        // Below the duty floor the fan is allowed to be stationary.
        assert_eq!(
            assess(&snap(Some(40.0), Some(0), Some(0))),
            ThermalStatus::Ok
        );
        assert_eq!(
            assess(&snap(Some(40.0), Some(31), Some(0))),
            ThermalStatus::Ok
        );
        // A spinning fan is never a stall, whatever the duty.
        assert_eq!(
            assess(&snap(Some(40.0), Some(255), Some(3200))),
            ThermalStatus::Ok
        );
    }

    #[test]
    fn missing_temperature_is_unknown_not_ok() {
        assert_eq!(assess(&snap(None, None, None)), ThermalStatus::Unknown);
        assert!(!ThermalStatus::Unknown.is_alarm());
    }

    #[test]
    fn alarm_states_are_the_actionable_ones() {
        assert!(ThermalStatus::Throttling.is_alarm());
        assert!(ThermalStatus::FanStalled.is_alarm());
        assert!(ThermalStatus::Hot.is_alarm());
        assert!(!ThermalStatus::Warm.is_alarm());
        assert!(!ThermalStatus::Ok.is_alarm());
    }

    #[test]
    fn pwm_percentage_rounds_across_the_range() {
        // 0/255 = 0%, 75/255 ~ 29% (the observed idle duty), 255/255 = 100%.
        let pct = |p: u32| ((p * 100 + 127) / 255) as u8;
        assert_eq!(pct(0), 0);
        assert_eq!(pct(75), 29);
        assert_eq!(pct(128), 50);
        assert_eq!(pct(255), 100);
    }

    #[test]
    fn reading_a_fixture_tree_finds_devices_by_name_not_index() {
        // hwmon indices shuffle across boots, so the fan must be found by `name`.
        let tmp = std::env::temp_dir().join(format!("thermal-fixture-{}", std::process::id()));
        let hwmon = tmp.join("hwmon");
        let thermal = tmp.join("thermal");
        let cpufreq = tmp.join("cpufreq");
        // Deliberately put the fan at hwmon7, not hwmon1.
        let fan = hwmon.join("hwmon7");
        std::fs::create_dir_all(&fan).unwrap();
        std::fs::write(fan.join("name"), "pwmfan\n").unwrap();
        std::fs::write(fan.join("fan1_input"), "1825\n").unwrap();
        std::fs::write(fan.join("pwm1"), "75\n").unwrap();
        // A decoy hwmon that must not match.
        let decoy = hwmon.join("hwmon0");
        std::fs::create_dir_all(&decoy).unwrap();
        std::fs::write(decoy.join("name"), "cpu_thermal\n").unwrap();

        let zone = thermal.join("thermal_zone0");
        std::fs::create_dir_all(&zone).unwrap();
        std::fs::write(zone.join("temp"), "55650\n").unwrap();
        let cool = thermal.join("cooling_device0");
        std::fs::create_dir_all(&cool).unwrap();
        std::fs::write(cool.join("type"), "pwm-fan\n").unwrap();
        std::fs::write(cool.join("cur_state"), "1\n").unwrap();
        std::fs::write(cool.join("max_state"), "4\n").unwrap();

        std::fs::create_dir_all(&cpufreq).unwrap();
        std::fs::write(cpufreq.join("scaling_cur_freq"), "1600000\n").unwrap();
        std::fs::write(cpufreq.join("scaling_max_freq"), "2400000\n").unwrap();

        let t = SocThermal::read_from(&thermal, &hwmon, &cpufreq);
        assert_eq!(t.cpu_temp_c, Some(55.65));
        assert_eq!(t.fan_rpm, Some(1825));
        assert_eq!(t.fan_pwm, Some(75));
        assert_eq!(t.fan_pct, Some(29));
        assert_eq!(t.cooling_state, Some(1));
        assert_eq!(t.cooling_max_state, Some(4));
        assert_eq!(t.cpu_freq_mhz, Some(1600));
        assert_eq!(t.cpu_freq_max_mhz, Some(2400));
        assert_eq!(t.status(), ThermalStatus::Ok);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn an_absent_sysfs_tree_yields_an_empty_snapshot_not_a_panic() {
        let nowhere = Path::new("/nonexistent/thermal/path");
        let t = SocThermal::read_from(nowhere, nowhere, nowhere);
        assert_eq!(t, SocThermal::default());
        assert_eq!(t.status(), ThermalStatus::Unknown);
    }
}
