//! Duplicate / stuck-receiver detection for the raw capture path.
//!
//! Two SX126x-continuous-RX pathologies stranded a gateway once: the radio
//! re-reading the same buffer region yields byte-identical frames, and the rapid
//! SPI-read storm that produces can drive a transaction into an unkillable `D`
//! state. A *genuine* retransmission from the same meter increments its access
//! number, so its bytes differ — byte-identical frames back-to-back are always a
//! stale re-read, never a real second frame. Detecting a sustained run of them
//! lets the capture loop bail *before* the storm wedges the radio.

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
