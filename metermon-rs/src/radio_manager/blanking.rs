//! Transmit blanking: knowing which received frames were corrupted by *our own* transmitter.
//!
//! On a two-radio gateway the LoRa concentrator and the wM-Bus receiver share an enclosure
//! and often an antenna coupling path. While the concentrator transmits, the wM-Bus receiver
//! is desensitised or deaf. That is unavoidable — the point of this module is that it must
//! not be *invisible*.
//!
//! **The driver cannot do this.** An SX1262 driver has no way to know an SX1302 is
//! transmitting; the two are separate chips on separate buses. So the knowledge lives here,
//! in the manager, which is the only layer that sees both radios.
//!
//! Why it matters beyond flagging a few frames: a downlink burst makes meters look like they
//! went quiet. Our reception watchdog declares a stall after N seconds without a frame and
//! then *recovers the radio* — so without blanking, a healthy gateway responds to its own
//! transmission by resetting a perfectly good receiver, and does it again on every downlink.
//! [`Blanking::unblanked_gap`] exists precisely so the watchdog measures silence that the
//! gateway did not cause.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Which radio a transmission belongs to. Present so a future third radio, or a
/// per-radio policy, does not require reworking the type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioId {
    /// The wM-Bus receiver (SX1262 on the Waveshare; the coupled SX1262 on a WM1302).
    Wmbus,
    /// The LoRa radio — a single SX1262 today, an SX1302 concentrator after the HAT swap.
    Lora,
}

/// A period during which `radio` is transmitting and other receivers are suspect.
///
/// Registered **before** the transmission where the scheduler allows it: a window learned
/// only after the fact cannot mark the frames it corrupted, because they have already been
/// handed to the decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxWindow {
    pub radio: RadioId,
    pub start: Instant,
    pub duration: Duration,
}

impl TxWindow {
    pub fn new(radio: RadioId, start: Instant, duration: Duration) -> Self {
        Self {
            radio,
            start,
            duration,
        }
    }

    pub fn end(&self) -> Instant {
        self.start + self.duration
    }

    /// Whether this window overlaps the half-open interval `[from, to)`.
    fn overlaps_interval(&self, from: Instant, to: Instant) -> bool {
        self.start < to && from < self.end()
    }
}

/// A rolling registry of recent transmit windows.
///
/// Bounded in time rather than count: what matters is "could this frame have been hit",
/// and a window older than the retention period cannot have hit anything still in flight.
#[derive(Debug)]
pub struct Blanking {
    windows: VecDeque<TxWindow>,
    retain: Duration,
}

impl Blanking {
    /// `retain` should comfortably exceed the longest gap the watchdog evaluates, so a
    /// stall assessment never reaches back past the oldest window we still know about.
    pub fn new(retain: Duration) -> Self {
        Self {
            windows: VecDeque::new(),
            retain,
        }
    }

    /// Record a transmission. Windows are kept in insertion order; callers register in
    /// roughly chronological order, and correctness does not depend on it.
    pub fn register(&mut self, window: TxWindow) {
        self.windows.push_back(window);
    }

    /// Drop windows that ended more than `retain` ago. Cheap and idempotent; call on a
    /// timer or before a query.
    pub fn prune(&mut self, now: Instant) {
        while let Some(front) = self.windows.front() {
            if now.duration_since(front.end()) > self.retain {
                self.windows.pop_front();
            } else {
                break;
            }
        }
    }

    /// Whether a frame overlapped any transmission.
    ///
    /// Takes the frame's **end** and its airtime, because that is what the hardware can
    /// actually tell us: the SX126x has no preamble-detect timestamp, so the driver stamps
    /// `at` when it services RX_DONE — end of frame plus IRQ latency. The start is
    /// back-computed here. Using the delivery instant alone would miss frames whose
    /// *opening* bytes were stepped on, which at SF12 can be over a second earlier.
    pub fn overlaps(&self, frame_end: Instant, airtime: Duration) -> bool {
        let frame_start = frame_end - airtime;
        self.windows
            .iter()
            .any(|w| w.overlaps_interval(frame_start, frame_end))
    }

    /// Total time inside `[from, to)` covered by transmissions, merging overlaps so
    /// concurrent or adjacent windows are not double-counted.
    pub fn blanked_duration(&self, from: Instant, to: Instant) -> Duration {
        if to <= from {
            return Duration::ZERO;
        }
        let mut spans: Vec<(Instant, Instant)> = self
            .windows
            .iter()
            .filter(|w| w.overlaps_interval(from, to))
            .map(|w| (w.start.max(from), w.end().min(to)))
            .collect();
        if spans.is_empty() {
            return Duration::ZERO;
        }
        spans.sort_by_key(|(s, _)| *s);

        let mut total = Duration::ZERO;
        let (mut cur_start, mut cur_end) = spans[0];
        for (s, e) in spans.into_iter().skip(1) {
            if s <= cur_end {
                cur_end = cur_end.max(e); // overlapping or touching: extend
            } else {
                total += cur_end.duration_since(cur_start);
                cur_start = s;
                cur_end = e;
            }
        }
        total + cur_end.duration_since(cur_start)
    }

    /// The silence in `[since, now)` that the gateway did **not** cause — i.e. the gap the
    /// reception watchdog should judge.
    ///
    /// This is the whole reason the module exists. Feeding the raw gap to the watchdog makes
    /// every downlink burst look like a dying receiver, and the "fix" is to reset a radio
    /// that was working.
    pub fn unblanked_gap(&self, since: Instant, now: Instant) -> Duration {
        if now <= since {
            return Duration::ZERO;
        }
        now.duration_since(since) - self.blanked_duration(since, now)
    }

    /// Windows currently retained — for telemetry, not decisions.
    pub fn len(&self) -> usize {
        self.windows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed origin so tests read as arithmetic rather than wall-clock.
    fn t0() -> Instant {
        Instant::now()
    }
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_frame_wholly_inside_a_tx_window_is_blanked() {
        let base = t0();
        let mut b = Blanking::new(ms(60_000));
        b.register(TxWindow::new(RadioId::Lora, base + ms(100), ms(500)));
        assert!(b.overlaps(base + ms(250), ms(50))); // ends 250, started 200
    }

    #[test]
    fn a_frame_whose_START_is_clipped_is_blanked() {
        // The reason overlaps() takes the frame start, not the moment of delivery: this
        // frame is handed up after the window closes, but its opening bytes were stepped on.
        let base = t0();
        let mut b = Blanking::new(ms(60_000));
        b.register(TxWindow::new(RadioId::Lora, base + ms(100), ms(200)));
        // starts at 250 (inside the 100..300 window), ends at 450 (outside)
        assert!(b.overlaps(base + ms(450), ms(200)));
    }

    #[test]
    fn a_frame_whose_END_is_clipped_is_blanked() {
        let base = t0();
        let mut b = Blanking::new(ms(60_000));
        b.register(TxWindow::new(RadioId::Lora, base + ms(300), ms(200)));
        assert!(b.overlaps(base + ms(350), ms(150))); // 200..350 clips the 300 start
    }

    #[test]
    fn frames_strictly_outside_are_not_blanked() {
        let base = t0();
        let mut b = Blanking::new(ms(60_000));
        b.register(TxWindow::new(RadioId::Lora, base + ms(300), ms(200))); // 300..500
        assert!(!b.overlaps(base + ms(250), ms(150))); // 100..250, ends before
        assert!(!b.overlaps(base + ms(600), ms(100))); // 500..600, starts at the boundary
    }

    #[test]
    fn touching_at_the_boundary_does_not_count_as_overlap() {
        // Half-open intervals: a frame starting exactly when the window ends is clean.
        let base = t0();
        let mut b = Blanking::new(ms(60_000));
        b.register(TxWindow::new(RadioId::Lora, base, ms(100)));
        assert!(!b.overlaps(base + ms(110), ms(10))); // 100..110, starts at the boundary
        assert!(b.overlaps(base + ms(109), ms(10))); // 99..109, clips the last ms
    }

    #[test]
    fn blanked_duration_merges_overlapping_windows() {
        // Two radios transmitting concurrently must not be counted twice, or the watchdog
        // would credit more blanking than real time elapsed.
        let base = t0();
        let mut b = Blanking::new(ms(60_000));
        b.register(TxWindow::new(RadioId::Lora, base + ms(100), ms(300))); // 100..400
        b.register(TxWindow::new(RadioId::Wmbus, base + ms(200), ms(300))); // 200..500
        assert_eq!(b.blanked_duration(base, base + ms(1000)), ms(400)); // 100..500
    }

    #[test]
    fn blanked_duration_sums_disjoint_windows() {
        let base = t0();
        let mut b = Blanking::new(ms(60_000));
        b.register(TxWindow::new(RadioId::Lora, base + ms(100), ms(100))); // 100..200
        b.register(TxWindow::new(RadioId::Lora, base + ms(500), ms(200))); // 500..700
        assert_eq!(b.blanked_duration(base, base + ms(1000)), ms(300));
    }

    #[test]
    fn blanked_duration_clips_to_the_query_range() {
        let base = t0();
        let mut b = Blanking::new(ms(60_000));
        b.register(TxWindow::new(RadioId::Lora, base, ms(1000))); // 0..1000
                                                                  // Only the part inside 200..500 counts.
        assert_eq!(b.blanked_duration(base + ms(200), base + ms(500)), ms(300));
    }

    #[test]
    fn unblanked_gap_is_what_the_watchdog_must_use() {
        // The scenario that motivates the module: 10 s of apparent silence, 6 s of which
        // was our own downlink. A 8 s stall threshold must NOT trip on 4 s of real silence.
        let base = t0();
        let mut b = Blanking::new(ms(60_000));
        b.register(TxWindow::new(RadioId::Lora, base + ms(2_000), ms(6_000)));
        let raw = ms(10_000);
        let real = b.unblanked_gap(base, base + raw);
        assert_eq!(real, ms(4_000));
        assert!(raw > ms(8_000), "raw gap would have tripped an 8s watchdog");
        assert!(real < ms(8_000), "real gap correctly does not trip it");
    }

    #[test]
    fn unblanked_gap_with_no_transmissions_is_the_whole_gap() {
        let base = t0();
        let b = Blanking::new(ms(60_000));
        assert_eq!(b.unblanked_gap(base, base + ms(5_000)), ms(5_000));
    }

    #[test]
    fn prune_drops_only_windows_older_than_retention() {
        let base = t0();
        let mut b = Blanking::new(ms(1_000));
        b.register(TxWindow::new(RadioId::Lora, base, ms(100))); // ends at 100
        b.register(TxWindow::new(RadioId::Lora, base + ms(900), ms(100))); // ends at 1000
        assert_eq!(b.len(), 2);
        // now = 1500: first ended 1400 ago (> 1000, drop), second 500 ago (keep).
        b.prune(base + ms(1_500));
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn a_reversed_or_empty_range_is_zero_not_a_panic() {
        let base = t0();
        let mut b = Blanking::new(ms(60_000));
        b.register(TxWindow::new(RadioId::Lora, base, ms(100)));
        assert_eq!(b.blanked_duration(base + ms(500), base), Duration::ZERO);
        assert_eq!(b.unblanked_gap(base + ms(500), base), Duration::ZERO);
        assert_eq!(b.blanked_duration(base, base), Duration::ZERO);
    }
}
