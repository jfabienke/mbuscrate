//! Duplicate / stuck-receiver detection for the raw capture path.
//!
//! Two SX126x-continuous-RX pathologies stranded a gateway once: the radio
//! re-reading the same buffer region yields byte-identical frames, and the rapid
//! SPI-read storm that produces can drive a transaction into an unkillable `D`
//! state. Detecting a sustained run of them lets the capture loop bail *before*
//! the storm wedges the radio.
//!
//! # Why this compares only the IMMEDIATELY PRECEDING frame — do not widen it
//!
//! An earlier version of this note justified the filter by claiming a genuine
//! retransmission always differs because the meter increments its access number.
//! **That claim is false, and acting on it would be destructive.** Measured over a
//! 15-minute capture of 564 real frames: **408 of them are byte-identical to an
//! earlier, non-consecutive frame.** Many meters repeat a telegram verbatim across
//! transmissions; the access number does not increment on every send on every make.
//!
//! So widening this to a window, a set, or a hash table would delete roughly
//! **72 % of genuine traffic** while looking like a tidy generalisation. Only the
//! back-to-back case is safe, and it is safe for a different reason than the one
//! originally given: a stale buffer re-read arrives with no gap because nothing was
//! received in between, whereas a real repeat is separated by another meter's frame
//! or by the transmit interval.
//!
//! The independent measurement also showed **zero** byte-identical consecutive
//! frames on a receiver that returns to standby and restarts RX from the buffer base
//! after each frame — i.e. this guard compensates for a specific driver behaviour,
//! not for anything meters do.

/// What to do with a freshly received raw frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DupVerdict {
    /// A new, distinct frame — record it.
    New,
    /// Identical to the immediately preceding frame — skip it.
    Duplicate,
    /// Enough identical frames in a row that the receiver is judged stuck; the
    /// caller should stop and return the radio to standby.
    Stuck,
}

/// Tracks the last raw frame and how many identical ones have arrived in a row.
pub struct DupGuard {
    last: Option<Vec<u8>>,
    consecutive: usize,
    limit: usize,
}

impl DupGuard {
    /// `limit` consecutive identical frames trips [`DupVerdict::Stuck`].
    pub fn new(limit: usize) -> Self {
        Self {
            last: None,
            consecutive: 0,
            limit: limit.max(1),
        }
    }

    /// Classify the next raw frame. A non-identical frame resets the run, so
    /// distinct meters interleaving never trips the stuck detector.
    pub fn check(&mut self, frame: &[u8]) -> DupVerdict {
        if self.last.as_deref() == Some(frame) {
            self.consecutive += 1;
            if self.consecutive >= self.limit {
                DupVerdict::Stuck
            } else {
                DupVerdict::Duplicate
            }
        } else {
            self.consecutive = 0;
            self.last = Some(frame.to_vec());
            DupVerdict::New
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_frames_are_all_new() {
        let mut g = DupGuard::new(5);
        assert_eq!(g.check(&[1, 2, 3]), DupVerdict::New);
        assert_eq!(g.check(&[4, 5, 6]), DupVerdict::New);
        assert_eq!(g.check(&[7]), DupVerdict::New);
    }

    #[test]
    fn identical_repeats_are_duplicates_then_stuck() {
        let mut g = DupGuard::new(3);
        assert_eq!(g.check(&[0xAB, 0xCD]), DupVerdict::New);
        assert_eq!(g.check(&[0xAB, 0xCD]), DupVerdict::Duplicate); // run = 1
        assert_eq!(g.check(&[0xAB, 0xCD]), DupVerdict::Duplicate); // run = 2
        assert_eq!(g.check(&[0xAB, 0xCD]), DupVerdict::Stuck); // run = 3 == limit
    }

    #[test]
    fn a_distinct_frame_resets_the_run() {
        let mut g = DupGuard::new(3);
        g.check(&[1]); // New
        assert_eq!(g.check(&[1]), DupVerdict::Duplicate);
        assert_eq!(g.check(&[1]), DupVerdict::Duplicate);
        assert_eq!(g.check(&[2]), DupVerdict::New); // reset
                                                    // The run counter is back to zero, so it takes the full limit again.
        assert_eq!(g.check(&[2]), DupVerdict::Duplicate);
        assert_eq!(g.check(&[2]), DupVerdict::Duplicate);
        assert_eq!(g.check(&[2]), DupVerdict::Stuck);
    }

    #[test]
    fn interleaved_meters_never_stick() {
        let mut g = DupGuard::new(3);
        let a = [0x11, 0x11];
        let b = [0x22, 0x22];
        for _ in 0..10 {
            assert_eq!(g.check(&a), DupVerdict::New);
            assert_eq!(g.check(&b), DupVerdict::New);
        }
    }

    #[test]
    fn limit_of_zero_is_clamped_to_one() {
        let mut g = DupGuard::new(0);
        assert_eq!(g.check(&[9]), DupVerdict::New);
        // limit clamped to 1, so the first repeat is already stuck.
        assert_eq!(g.check(&[9]), DupVerdict::Stuck);
    }
}
