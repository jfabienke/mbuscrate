//! Multi-Channel Hopping for LoRa Resilience
//!
//! Implements dynamic channel scanning across EU868 sub-bands (or other regions)
//! to avoid interference from WiFi and other sources. Inspired by One Channel Hub's
//! fixed channel approach but extended for multi-channel resilience.

use std::collections::VecDeque;
use std::time::{Duration, Instant};
use log::{debug, info, warn};

/// Channel definition for frequency hopping
#[derive(Debug, Clone, Copy)]
pub struct Channel {
    /// Frequency in Hz
    pub frequency_hz: u32,

    /// Channel name/identifier
    pub name: &'static str,

    /// Duty cycle limit for this channel (percentage)
    pub duty_cycle_limit: f32,

    /// Last activity timestamp
    pub last_activity: Option<Instant>,

    /// Channel quality metric (0.0 = bad, 1.0 = excellent)
    pub quality: f32,
}

/// EU868 channel plan (8 standard channels + optional)
pub const EU868_CHANNELS: [Channel; 8] = [
    Channel { frequency_hz: 868_100_000, name: "EU868-1", duty_cycle_limit: 1.0, last_activity: None, quality: 1.0 },
    Channel { frequency_hz: 868_300_000, name: "EU868-2", duty_cycle_limit: 1.0, last_activity: None, quality: 1.0 },
    Channel { frequency_hz: 868_500_000, name: "EU868-3", duty_cycle_limit: 1.0, last_activity: None, quality: 1.0 },
    Channel { frequency_hz: 867_100_000, name: "EU868-4", duty_cycle_limit: 1.0, last_activity: None, quality: 1.0 },
    Channel { frequency_hz: 867_300_000, name: "EU868-5", duty_cycle_limit: 1.0, last_activity: None, quality: 1.0 },
    Channel { frequency_hz: 867_500_000, name: "EU868-6", duty_cycle_limit: 1.0, last_activity: None, quality: 1.0 },
    Channel { frequency_hz: 867_700_000, name: "EU868-7", duty_cycle_limit: 1.0, last_activity: None, quality: 1.0 },
    Channel { frequency_hz: 867_900_000, name: "EU868-8", duty_cycle_limit: 1.0, last_activity: None, quality: 1.0 },
];

/// US915 channel plan (first 8 uplink channels)
pub const US915_CHANNELS: [Channel; 8] = [
    Channel { frequency_hz: 902_300_000, name: "US915-0", duty_cycle_limit: 100.0, last_activity: None, quality: 1.0 },
    Channel { frequency_hz: 902_500_000, name: "US915-1", duty_cycle_limit: 100.0, last_activity: None, quality: 1.0 },
    Channel { frequency_hz: 902_700_000, name: "US915-2", duty_cycle_limit: 100.0, last_activity: None, quality: 1.0 },
    Channel { frequency_hz: 902_900_000, name: "US915-3", duty_cycle_limit: 100.0, last_activity: None, quality: 1.0 },
    Channel { frequency_hz: 903_100_000, name: "US915-4", duty_cycle_limit: 100.0, last_activity: None, quality: 1.0 },
    Channel { frequency_hz: 903_300_000, name: "US915-5", duty_cycle_limit: 100.0, last_activity: None, quality: 1.0 },
    Channel { frequency_hz: 903_500_000, name: "US915-6", duty_cycle_limit: 100.0, last_activity: None, quality: 1.0 },
    Channel { frequency_hz: 903_700_000, name: "US915-7", duty_cycle_limit: 100.0, last_activity: None, quality: 1.0 },
];

/// Channel hopping strategy
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HoppingStrategy {
    /// Round-robin through all channels
    RoundRobin,

    /// Random channel selection
    Random,

    /// Adaptive based on channel quality
    Adaptive,

    /// Fixed channel (no hopping)
    Fixed(usize),
}

/// Multi-channel hopping controller
pub struct ChannelHopper {
    /// Available channels
    channels: Vec<Channel>,

    /// Current channel index
    current_index: usize,

    /// Hopping strategy
    strategy: HoppingStrategy,

    /// Channel scan interval
    scan_interval: Duration,

    /// Last scan time
    last_scan: Instant,

    /// Channel quality history
    quality_history: Vec<VecDeque<(Instant, f32)>>,

    /// Blacklisted channels (avoid due to interference)
    blacklist: Vec<usize>,
}

impl ChannelHopper {
    /// Create a new channel hopper for EU868
    pub fn new_eu868(strategy: HoppingStrategy) -> Self {
        let channels = EU868_CHANNELS.to_vec();
        let quality_history = vec![VecDeque::with_capacity(100); channels.len()];

        Self {
            channels,
            current_index: 0,
            strategy,
            scan_interval: Duration::from_secs(60), // Scan every minute
            last_scan: Instant::now(),
            quality_history,
            blacklist: Vec::new(),
        }
    }

    /// Create a new channel hopper for US915
    pub fn new_us915(strategy: HoppingStrategy) -> Self {
        let channels = US915_CHANNELS.to_vec();
        let quality_history = vec![VecDeque::with_capacity(100); channels.len()];

        Self {
            channels,
            current_index: 0,
            strategy,
            scan_interval: Duration::from_secs(60),
            last_scan: Instant::now(),
            quality_history,
            blacklist: Vec::new(),
        }
    }

    /// Get the next channel based on strategy
    pub fn next_channel(&mut self) -> Channel {
        match self.strategy {
            HoppingStrategy::RoundRobin => self.next_round_robin(),
            HoppingStrategy::Random => self.next_random(),
            HoppingStrategy::Adaptive => self.next_adaptive(),
            HoppingStrategy::Fixed(idx) => self.channels[idx % self.channels.len()],
        }
    }

    /// Round-robin channel selection
    fn next_round_robin(&mut self) -> Channel {
        loop {
            self.current_index = (self.current_index + 1) % self.channels.len();

            // Skip blacklisted channels
            if !self.blacklist.contains(&self.current_index) {
                self.channels[self.current_index].last_activity = Some(Instant::now());
                return self.channels[self.current_index];
            }

            // If all channels are blacklisted, clear blacklist
            if self.blacklist.len() >= self.channels.len() {
                warn!("All channels blacklisted, clearing blacklist");
                self.blacklist.clear();
            }
        }
    }

    /// Random channel selection
    fn next_random(&mut self) -> Channel {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        loop {
            let idx = rng.gen_range(0..self.channels.len());

            if !self.blacklist.contains(&idx) {
                self.current_index = idx;
                self.channels[idx].last_activity = Some(Instant::now());
                return self.channels[idx];
            }
        }
    }

    /// Adaptive channel selection based on quality
    fn next_adaptive(&mut self) -> Channel {
        // Find channel with best quality that's not blacklisted
        let mut best_idx = self.current_index;
        let mut best_quality = 0.0;

        for (idx, channel) in self.channels.iter().enumerate() {
            if !self.blacklist.contains(&idx) && channel.quality > best_quality {
                best_quality = channel.quality;
                best_idx = idx;
            }
        }

        self.current_index = best_idx;
        self.channels[best_idx].last_activity = Some(Instant::now());

        debug!("Selected channel {} with quality {:.2}",
               self.channels[best_idx].name, best_quality);

        self.channels[best_idx]
    }

    /// Update channel quality based on packet reception
    pub fn update_quality(&mut self, channel_idx: usize, rssi: i16, success: bool) {
        if channel_idx >= self.channels.len() {
            return;
        }

        // Calculate quality metric (0.0 to 1.0)
        // Better RSSI and success rate = higher quality
        let rssi_quality = ((rssi + 120) as f32 / 70.0).max(0.0).min(1.0);
        let success_factor = if success { 1.0 } else { 0.5 };
        let new_quality = rssi_quality * success_factor;

        // Update with exponential moving average
        let alpha = 0.2; // Smoothing factor
        self.channels[channel_idx].quality =
            alpha * new_quality + (1.0 - alpha) * self.channels[channel_idx].quality;

        // Store in history
        self.quality_history[channel_idx].push_back((Instant::now(), new_quality));

        // Keep only recent history (last 100 samples)
        while self.quality_history[channel_idx].len() > 100 {
            self.quality_history[channel_idx].pop_front();
        }

        // Blacklist if quality drops too low
        if self.channels[channel_idx].quality < 0.2
            && !self.blacklist.contains(&channel_idx) {
                warn!("Blacklisting channel {} due to poor quality",
                      self.channels[channel_idx].name);
                self.blacklist.push(channel_idx);
            }
    }

    /// Perform channel scan to detect interference
    pub async fn scan_channels<F>(&mut self, mut scan_fn: F)
    where
        F: FnMut(u32) -> f32, // Returns noise floor in dBm
    {
        if self.last_scan.elapsed() < self.scan_interval {
            return;
        }

        info!("Starting channel scan for interference detection");
        self.last_scan = Instant::now();

        for (idx, channel) in self.channels.iter_mut().enumerate() {
            let noise_floor = scan_fn(channel.frequency_hz);

            // Update quality based on noise floor
            // Lower noise = better quality
            let noise_quality = ((-noise_floor - 70.0) / 50.0).max(0.0).min(1.0);
            channel.quality = channel.quality * 0.7 + noise_quality * 0.3;

            debug!("Channel {} noise floor: {:.1} dBm, quality: {:.2}",
                   channel.name, noise_floor, channel.quality);

            // Clear from blacklist if quality improves
            if channel.quality > 0.5 && self.blacklist.contains(&idx) {
                info!("Removing channel {} from blacklist (quality improved)", channel.name);
                self.blacklist.retain(|&x| x != idx);
            }
        }
    }

    /// Get current channel
    pub fn current_channel(&self) -> Channel {
        self.channels[self.current_index]
    }

    /// Get channel statistics
    pub fn get_stats(&self) -> ChannelStats {
        ChannelStats {
            total_channels: self.channels.len(),
            active_channels: self.channels.len() - self.blacklist.len(),
            blacklisted_channels: self.blacklist.len(),
            avg_quality: self.channels.iter().map(|c| c.quality).sum::<f32>()
                / self.channels.len() as f32,
            current_channel: self.channels[self.current_index].name,
        }
    }
}

/// Channel hopping statistics
#[derive(Debug, Clone)]
pub struct ChannelStats {
    pub total_channels: usize,
    pub active_channels: usize,
    pub blacklisted_channels: usize,
    pub avg_quality: f32,
    pub current_channel: &'static str,
}

// Add rand dependency for random selection
use rand;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_robin_hopping() {
        let mut hopper = ChannelHopper::new_eu868(HoppingStrategy::RoundRobin);

        let ch1 = hopper.next_channel();
        let ch2 = hopper.next_channel();

        assert_ne!(ch1.frequency_hz, ch2.frequency_hz);
    }

    #[test]
    fn test_adaptive_hopping() {
        let mut hopper = ChannelHopper::new_eu868(HoppingStrategy::Adaptive);

        // Update quality for channel 0
        hopper.update_quality(0, -70, true);  // Good quality
        hopper.update_quality(1, -100, false); // Poor quality

        // Should prefer channel 0
        let selected = hopper.next_channel();
        assert_eq!(selected.frequency_hz, EU868_CHANNELS[0].frequency_hz);
    }

    #[test]
    fn test_blacklisting() {
        let mut hopper = ChannelHopper::new_eu868(HoppingStrategy::Adaptive);

        // Simulate very poor quality to trigger blacklist
        for _ in 0..10 {
            hopper.update_quality(0, -120, false);
        }

        assert!(hopper.blacklist.contains(&0));
    }
}