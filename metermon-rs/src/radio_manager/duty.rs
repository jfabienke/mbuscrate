//! EU868 duty-cycle accounting, per sub-band, across every radio the gateway owns.
//!
//! **The manager owns the budget because only it sees all the radios.** A driver knows its
//! own transmissions; it cannot know that another chip in the same enclosure has already
//! spent the sub-band's allowance. The driver keeps a hard backstop of its own — that is
//! deliberate duplication, because a manager bug that over-transmits is a licence problem
//! rather than a bug report.
//!
//! The window is a rolling hour, which is the conservative reading of the ETSI limit: a
//! transmitter may occupy at most `limit` of any observation period, so we never let the
//! *trailing* hour exceed it rather than resetting on the hour and allowing a burst across
//! the boundary.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// The EU868 sub-bands this gateway transmits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubBand {
    /// 868.0–868.6 MHz — 1 %. LoRaWAN uplink channels and our wM-Bus band.
    G1,
    /// 869.4–869.65 MHz — 10 %. RX2 lives here, which is why RX2 may run at full power.
    G3,
}

impl SubBand {
    /// The sub-band a carrier falls in, or `None` if we have no rule for it (in which case
    /// the caller must refuse rather than assume).
    pub fn for_frequency(hz: u32) -> Option<Self> {
        match hz {
            868_000_000..=868_600_000 => Some(SubBand::G1),
            869_400_000..=869_650_000 => Some(SubBand::G3),
            _ => None,
        }
    }

    /// Permitted fraction of any observation window.
    pub fn limit(&self) -> f64 {
        match self {
            SubBand::G1 => 0.01,
            SubBand::G3 => 0.10,
        }
    }
}

/// Why a transmission was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DutyVerdict {
    /// Permitted; the caller must call [`DutyBudget::record`] when it actually transmits.
    Allowed,
    /// Would exceed the sub-band's allowance for the rolling window.
    WouldExceed,
    /// The frequency is not in a sub-band we hold a rule for. Refuse rather than guess.
    UnknownBand,
}

#[derive(Debug, Clone, Copy)]
struct Emission {
    band: SubBand,
    at: Instant,
    airtime: Duration,
}

/// Rolling duty-cycle ledger.
#[derive(Debug)]
pub struct DutyBudget {
    window: Duration,
    emissions: VecDeque<Emission>,
}

impl DutyBudget {
    /// `window` is the observation period — one hour for ETSI.
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            emissions: VecDeque::new(),
        }
    }

    pub fn hourly() -> Self {
        Self::new(Duration::from_secs(3600))
    }

    fn expire(&mut self, now: Instant) {
        while let Some(e) = self.emissions.front() {
            if now.duration_since(e.at) > self.window {
                self.emissions.pop_front();
            } else {
                break;
            }
        }
    }

    /// Airtime already spent in `band` within the trailing window.
    pub fn spent(&mut self, band: SubBand, now: Instant) -> Duration {
        self.expire(now);
        self.emissions
            .iter()
            .filter(|e| e.band == band)
            .map(|e| e.airtime)
            .sum()
    }

    /// Whether transmitting `airtime` on `freq_hz` would stay inside the allowance.
    ///
    /// Checked *before* transmitting; the caller records afterwards. Split deliberately so a
    /// failed transmission does not consume budget it never used.
    pub fn check(&mut self, freq_hz: u32, airtime: Duration, now: Instant) -> DutyVerdict {
        let Some(band) = SubBand::for_frequency(freq_hz) else {
            return DutyVerdict::UnknownBand;
        };
        let spent = self.spent(band, now);
        let allowed = self.window.mul_f64(band.limit());
        if spent + airtime <= allowed {
            DutyVerdict::Allowed
        } else {
            DutyVerdict::WouldExceed
        }
    }

    /// Record an emission that actually happened.
    pub fn record(&mut self, freq_hz: u32, airtime: Duration, now: Instant) {
        if let Some(band) = SubBand::for_frequency(freq_hz) {
            self.emissions.push_back(Emission {
                band,
                at: now,
                airtime,
            });
        }
    }

    /// Fraction of the allowance consumed, 0.0–1.0+, for telemetry.
    pub fn utilisation(&mut self, band: SubBand, now: Instant) -> f64 {
        let spent = self.spent(band, now).as_secs_f64();
        let allowed = self.window.mul_f64(band.limit()).as_secs_f64();
        if allowed == 0.0 {
            0.0
        } else {
            spent / allowed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Instant {
        Instant::now()
    }
    fn s(n: u64) -> Duration {
        Duration::from_secs(n)
    }
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn sub_bands_are_identified_from_the_carrier() {
        assert_eq!(SubBand::for_frequency(868_100_000), Some(SubBand::G1)); // LoRaWAN ch0
        assert_eq!(SubBand::for_frequency(868_500_000), Some(SubBand::G1)); // ch2
        assert_eq!(SubBand::for_frequency(868_950_000), None); // wM-Bus mode C: no rule held
        assert_eq!(SubBand::for_frequency(869_525_000), Some(SubBand::G3)); // RX2
        assert_eq!(SubBand::for_frequency(433_000_000), None);
    }

    #[test]
    fn an_unknown_band_is_refused_not_assumed() {
        // Refusing is the safe default: guessing a limit for a band we have no rule for is
        // how a licence gets breached quietly.
        let mut b = DutyBudget::hourly();
        assert_eq!(
            b.check(915_000_000, ms(50), now()),
            DutyVerdict::UnknownBand
        );
    }

    #[test]
    fn g1_allows_36s_per_hour_and_refuses_beyond() {
        // 1% of 3600 s = 36 s.
        let t = now();
        let mut b = DutyBudget::hourly();
        assert_eq!(b.check(868_100_000, s(36), t), DutyVerdict::Allowed);
        b.record(868_100_000, s(36), t);
        assert_eq!(b.spent(SubBand::G1, t), s(36));
        assert_eq!(b.check(868_100_000, ms(1), t), DutyVerdict::WouldExceed);
    }

    #[test]
    fn g3_allows_ten_times_more_which_is_why_rx2_can_run_hot() {
        let t = now();
        let mut b = DutyBudget::hourly();
        // 10% of 3600 s = 360 s.
        assert_eq!(b.check(869_525_000, s(360), t), DutyVerdict::Allowed);
        assert_eq!(b.check(869_525_000, s(361), t), DutyVerdict::WouldExceed);
    }

    #[test]
    fn the_budgets_are_independent_per_band() {
        let t = now();
        let mut b = DutyBudget::hourly();
        b.record(868_100_000, s(36), t); // G1 exhausted
        assert_eq!(b.check(868_100_000, ms(1), t), DutyVerdict::WouldExceed);
        // G3 is untouched — an exhausted uplink band must not block an RX2 downlink.
        assert_eq!(b.check(869_525_000, s(100), t), DutyVerdict::Allowed);
    }

    #[test]
    fn spend_expires_out_of_the_rolling_window() {
        let t = now();
        let mut b = DutyBudget::new(s(3600));
        b.record(868_100_000, s(36), t);
        assert_eq!(b.check(868_100_000, ms(1), t), DutyVerdict::WouldExceed);
        // An hour and a second later the emission has aged out.
        let later = t + s(3601);
        assert_eq!(b.spent(SubBand::G1, later), Duration::ZERO);
        assert_eq!(b.check(868_100_000, s(36), later), DutyVerdict::Allowed);
    }

    #[test]
    fn check_does_not_consume_budget() {
        // Separated so a transmission that fails (BUSY timeout, refused by the driver)
        // does not burn allowance it never used.
        let t = now();
        let mut b = DutyBudget::hourly();
        for _ in 0..100 {
            assert_eq!(b.check(868_100_000, s(30), t), DutyVerdict::Allowed);
        }
        assert_eq!(b.spent(SubBand::G1, t), Duration::ZERO);
    }

    #[test]
    fn our_actual_lorawan_join_beacon_is_well_inside_g1() {
        // The run we transmitted today: 60 packets, ~36 ms airtime, one per 10 s.
        let t = now();
        let mut b = DutyBudget::hourly();
        for i in 0..60 {
            let at = t + s(i * 10);
            assert_eq!(b.check(868_100_000, ms(36), at), DutyVerdict::Allowed);
            b.record(868_100_000, ms(36), at);
        }
        let end = t + s(600);
        // 60 x 36 ms = 2.16 s of a 36 s allowance.
        assert_eq!(b.spent(SubBand::G1, end), ms(2160));
        assert!(b.utilisation(SubBand::G1, end) < 0.07);
    }

    #[test]
    fn a_join_accept_at_sf12_is_the_expensive_case() {
        // ~1.3 s of airtime each. G1 permits ~27 of them per hour; G3 permits ~276,
        // which is the real reason RX2 sits in the high-power sub-band.
        let t = now();
        let mut b = DutyBudget::hourly();
        let mut n = 0;
        while b.check(868_100_000, ms(1300), t) == DutyVerdict::Allowed {
            b.record(868_100_000, ms(1300), t);
            n += 1;
            assert!(n < 1000, "loop guard");
        }
        assert_eq!(n, 27);
    }
}
