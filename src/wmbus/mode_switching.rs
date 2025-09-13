//! # wM-Bus Mode Switching and Negotiation
//!
//! This module implements mode switching protocol for wireless M-Bus devices
//! according to EN 13757-4. Devices can negotiate between different transmission
//! modes (T1, S1, C1) to find the best communication parameters.
//!
//! ## Mode Switching Sequence
//!
//! The standard switching sequence is:
//! 1. Try T1 mode (100 kbps, 3-out-of-6 encoding)
//! 2. Wait 10ms, then try S1 mode (32.768 kbps, Manchester)
//! 3. Wait 10ms, then try C1 mode (100 kbps, NRZ)
//! 4. Repeat cycle with backoff if no response
//!
//! ## Usage
//!
//! ```rust
//! use mbus_rs::wmbus::mode_switching::{ModeSwitcher, WMBusMode};
//!
//! let mut switcher = ModeSwitcher::new();
//!
//! // Try next mode in sequence
//! let next_mode = switcher.next_mode();
//!
//! // Configure radio for mode
//! radio.configure_mode(next_mode);
//!
//! // On successful communication
//! switcher.mode_established(next_mode);
//! ```

use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Wireless M-Bus communication modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WMBusMode {
    /// T-mode: 100 kbps with 3-out-of-6 encoding
    T1,
    /// T-mode frequent transmit variant
    T2,
    /// S-mode: 32.768 kbps with Manchester encoding
    S1,
    /// S-mode frequent transmit variant
    S2,
    /// C-mode: 100 kbps with NRZ encoding
    C1,
    /// C-mode frequent transmit variant
    C2,
}

impl WMBusMode {
    /// Get the chip rate in bits per second
    pub fn chip_rate(&self) -> u32 {
        match self {
            WMBusMode::T1 | WMBusMode::T2 => 100_000,
            WMBusMode::S1 | WMBusMode::S2 => 32_768,
            WMBusMode::C1 | WMBusMode::C2 => 100_000,
        }
    }

    /// Get the effective data rate after encoding
    pub fn data_rate(&self) -> u32 {
        match self {
            WMBusMode::T1 | WMBusMode::T2 => 66_667, // 100k / 1.5 (3-out-of-6)
            WMBusMode::S1 | WMBusMode::S2 => 16_384, // 32.768k / 2 (Manchester)
            WMBusMode::C1 | WMBusMode::C2 => 100_000, // No encoding overhead
        }
    }

    /// Get the preamble requirements for this mode
    pub fn preamble_chips(&self) -> u16 {
        match self {
            WMBusMode::T1 | WMBusMode::T2 => 19, // ≥19 chips
            WMBusMode::S1 => 279,                // ≥279 chips
            WMBusMode::S2 => 15,                 // ≥15 chips
            WMBusMode::C1 | WMBusMode::C2 => 64, // 8×0x55 bytes = 64 bits
        }
    }

    /// Get the sync word for this mode
    pub fn sync_word(&self) -> Vec<u8> {
        match self {
            WMBusMode::T1 | WMBusMode::T2 => vec![0x54, 0x3D], // T-mode sync
            WMBusMode::S1 | WMBusMode::S2 => vec![0x54, 0x3D], // S-mode sync
            WMBusMode::C1 | WMBusMode::C2 => vec![0x54, 0xCD], // C-mode sync
        }
    }

    /// Get the frequency for this mode (in MHz)
    pub fn frequency_mhz(&self) -> f32 {
        868.95 // All modes use same frequency, switching is time-based
    }
}

/// Mode switching state machine
#[derive(Debug)]
pub struct ModeSwitcher {
    /// Current mode being tried
    current_mode: WMBusMode,
    /// Established mode (if communication successful)
    established_mode: Option<WMBusMode>,
    /// Mode sequence for cycling
    mode_sequence: Vec<WMBusMode>,
    /// Current position in sequence
    sequence_index: usize,
    /// Last mode switch time
    last_switch: Instant,
    /// Number of complete cycles without success
    cycle_count: u32,
    /// Maximum cycles before giving up
    max_cycles: u32,
    /// Delay between mode switches (milliseconds)
    switch_delay_ms: u64,
    /// Statistics
    stats: SwitchingStats,
}

/// Statistics for mode switching
#[derive(Debug, Default)]
pub struct SwitchingStats {
    /// Total mode switches attempted
    pub switches_attempted: u64,
    /// Successful mode establishments
    pub switches_successful: u64,
    /// Failed cycles
    pub cycles_failed: u64,
    /// Time spent in each mode (milliseconds)
    pub time_per_mode: [u64; 6],
}

impl ModeSwitcher {
    /// Create a new mode switcher with standard T1→S1→C1 sequence
    pub fn new() -> Self {
        Self {
            current_mode: WMBusMode::T1,
            established_mode: None,
            mode_sequence: vec![WMBusMode::T1, WMBusMode::S1, WMBusMode::C1],
            sequence_index: 0,
            last_switch: Instant::now(),
            cycle_count: 0,
            max_cycles: 10,
            switch_delay_ms: 10,
            stats: SwitchingStats::default(),
        }
    }

    /// Create a mode switcher with custom sequence
    pub fn with_sequence(sequence: Vec<WMBusMode>) -> Self {
        if sequence.is_empty() {
            panic!("Mode sequence cannot be empty");
        }

        Self {
            current_mode: sequence[0],
            established_mode: None,
            mode_sequence: sequence,
            sequence_index: 0,
            last_switch: Instant::now(),
            cycle_count: 0,
            max_cycles: 10,
            switch_delay_ms: 10,
            stats: SwitchingStats::default(),
        }
    }

    /// Get the next mode in the switching sequence
    pub async fn next_mode(&mut self) -> Option<WMBusMode> {
        // If mode is established, keep using it
        if let Some(mode) = self.established_mode {
            return Some(mode);
        }

        // Check if we've exceeded max cycles
        if self.cycle_count >= self.max_cycles {
            return None;
        }

        // Wait for switch delay
        let elapsed = self.last_switch.elapsed();
        let delay_duration = Duration::from_millis(self.switch_delay_ms);
        if elapsed < delay_duration {
            sleep(delay_duration - elapsed).await;
        }

        // Move to next mode in sequence
        self.sequence_index = (self.sequence_index + 1) % self.mode_sequence.len();

        // Track complete cycles
        if self.sequence_index == 0 {
            self.cycle_count += 1;

            // Exponential backoff after failed cycles
            if self.cycle_count > 1 {
                self.switch_delay_ms = (self.switch_delay_ms * 2).min(1000);
            }
        }

        self.current_mode = self.mode_sequence[self.sequence_index];
        self.last_switch = Instant::now();
        self.stats.switches_attempted += 1;

        Some(self.current_mode)
    }

    /// Mark a mode as successfully established
    pub fn mode_established(&mut self, mode: WMBusMode) {
        self.established_mode = Some(mode);
        self.stats.switches_successful += 1;

        // Update time tracking
        let mode_index = mode_to_index(mode);
        let elapsed = self.last_switch.elapsed().as_millis() as u64;
        self.stats.time_per_mode[mode_index] += elapsed;
    }

    /// Reset the switcher to try again
    pub fn reset(&mut self) {
        self.established_mode = None;
        self.sequence_index = 0;
        self.cycle_count = 0;
        self.switch_delay_ms = 10;
        self.current_mode = self.mode_sequence[0];
        self.last_switch = Instant::now();
    }

    /// Get the current mode being tried
    pub fn current_mode(&self) -> WMBusMode {
        self.current_mode
    }

    /// Get the established mode (if any)
    pub fn established_mode(&self) -> Option<WMBusMode> {
        self.established_mode
    }

    /// Get switching statistics
    pub fn stats(&self) -> &SwitchingStats {
        &self.stats
    }

    /// Set custom switch delay (milliseconds)
    pub fn set_switch_delay(&mut self, delay_ms: u64) {
        self.switch_delay_ms = delay_ms;
    }

    /// Set maximum cycles before giving up
    pub fn set_max_cycles(&mut self, max_cycles: u32) {
        self.max_cycles = max_cycles;
    }
}

impl Default for ModeSwitcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Mode negotiation protocol handler
#[derive(Debug)]
pub struct ModeNegotiator {
    /// Supported modes by this device
    supported_modes: Vec<WMBusMode>,
    /// Preferred mode for communication
    #[allow(dead_code)]
    preferred_mode: WMBusMode,
    /// Mode switcher instance
    #[allow(dead_code)]
    switcher: ModeSwitcher,
}

impl ModeNegotiator {
    /// Create a new mode negotiator
    pub fn new(supported_modes: Vec<WMBusMode>) -> Self {
        if supported_modes.is_empty() {
            panic!("Must support at least one mode");
        }

        Self {
            preferred_mode: supported_modes[0],
            supported_modes: supported_modes.clone(),
            switcher: ModeSwitcher::with_sequence(supported_modes),
        }
    }

    /// Build mode capability frame for advertising supported modes
    ///
    /// Frame format:
    /// - CI: 0x7A (mode capabilities)
    /// - Data: Bitmask of supported modes
    pub fn build_capability_frame(&self) -> Vec<u8> {
        let mut frame = Vec::new();

        // CI field for mode capabilities
        frame.push(0x7A);

        // Build capability bitmask
        let mut capabilities = 0u8;
        for mode in &self.supported_modes {
            match mode {
                WMBusMode::T1 => capabilities |= 0x01,
                WMBusMode::T2 => capabilities |= 0x02,
                WMBusMode::S1 => capabilities |= 0x04,
                WMBusMode::S2 => capabilities |= 0x08,
                WMBusMode::C1 => capabilities |= 0x10,
                WMBusMode::C2 => capabilities |= 0x20,
            }
        }

        frame.push(capabilities);
        frame
    }

    /// Parse mode capability frame from remote device
    pub fn parse_capability_frame(&self, data: &[u8]) -> Option<Vec<WMBusMode>> {
        if data.len() < 2 || data[0] != 0x7A {
            return None;
        }

        let capabilities = data[1];
        let mut modes = Vec::new();

        if capabilities & 0x01 != 0 {
            modes.push(WMBusMode::T1);
        }
        if capabilities & 0x02 != 0 {
            modes.push(WMBusMode::T2);
        }
        if capabilities & 0x04 != 0 {
            modes.push(WMBusMode::S1);
        }
        if capabilities & 0x08 != 0 {
            modes.push(WMBusMode::S2);
        }
        if capabilities & 0x10 != 0 {
            modes.push(WMBusMode::C1);
        }
        if capabilities & 0x20 != 0 {
            modes.push(WMBusMode::C2);
        }

        Some(modes)
    }

    /// Select best mode from intersection of local and remote capabilities
    pub fn select_best_mode(&self, remote_modes: &[WMBusMode]) -> Option<WMBusMode> {
        // Prefer modes in this order: C1, T1, S1 (fastest to slowest data rate)
        let preference_order = [
            WMBusMode::C1,
            WMBusMode::C2,
            WMBusMode::T1,
            WMBusMode::T2,
            WMBusMode::S1,
            WMBusMode::S2,
        ];

        preference_order.into_iter().find(|&mode| self.supported_modes.contains(&mode) && remote_modes.contains(&mode))
    }
}

/// Convert mode to array index for statistics
fn mode_to_index(mode: WMBusMode) -> usize {
    match mode {
        WMBusMode::T1 => 0,
        WMBusMode::T2 => 1,
        WMBusMode::S1 => 2,
        WMBusMode::S2 => 3,
        WMBusMode::C1 => 4,
        WMBusMode::C2 => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mode_switching_sequence() {
        let mut switcher = ModeSwitcher::new();

        // Initial mode should be T1
        assert_eq!(switcher.current_mode(), WMBusMode::T1);

        // Next should be S1
        let next = switcher.next_mode().await;
        assert_eq!(next, Some(WMBusMode::S1));

        // Next should be C1
        let next = switcher.next_mode().await;
        assert_eq!(next, Some(WMBusMode::C1));

        // Should cycle back to T1
        let next = switcher.next_mode().await;
        assert_eq!(next, Some(WMBusMode::T1));
    }

    #[tokio::test]
    async fn test_mode_establishment() {
        let mut switcher = ModeSwitcher::new();

        // Try a few modes
        switcher.next_mode().await;
        switcher.next_mode().await;

        // Establish S1 mode
        switcher.mode_established(WMBusMode::S1);

        // Next mode should still be S1 (established)
        let next = switcher.next_mode().await;
        assert_eq!(next, Some(WMBusMode::S1));
        assert_eq!(switcher.established_mode(), Some(WMBusMode::S1));
    }

    #[test]
    fn test_mode_parameters() {
        // Test T1 mode
        assert_eq!(WMBusMode::T1.chip_rate(), 100_000);
        assert_eq!(WMBusMode::T1.data_rate(), 66_667);
        assert_eq!(WMBusMode::T1.preamble_chips(), 19);

        // Test S1 mode
        assert_eq!(WMBusMode::S1.chip_rate(), 32_768);
        assert_eq!(WMBusMode::S1.data_rate(), 16_384);
        assert_eq!(WMBusMode::S1.preamble_chips(), 279);

        // Test C1 mode
        assert_eq!(WMBusMode::C1.chip_rate(), 100_000);
        assert_eq!(WMBusMode::C1.data_rate(), 100_000);
        assert_eq!(WMBusMode::C1.preamble_chips(), 64);
    }

    #[test]
    fn test_capability_frame() {
        let negotiator = ModeNegotiator::new(vec![WMBusMode::T1, WMBusMode::S1, WMBusMode::C1]);

        let frame = negotiator.build_capability_frame();
        assert_eq!(frame[0], 0x7A); // CI field
        assert_eq!(frame[1] & 0x01, 0x01); // T1 bit
        assert_eq!(frame[1] & 0x04, 0x04); // S1 bit
        assert_eq!(frame[1] & 0x10, 0x10); // C1 bit
    }

    #[test]
    fn test_mode_selection() {
        let negotiator = ModeNegotiator::new(vec![WMBusMode::T1, WMBusMode::S1, WMBusMode::C1]);

        // Remote supports T1 and S1
        let remote_modes = vec![WMBusMode::T1, WMBusMode::S1];
        let best = negotiator.select_best_mode(&remote_modes);
        assert_eq!(best, Some(WMBusMode::T1)); // T1 preferred over S1

        // Remote only supports S1
        let remote_modes = vec![WMBusMode::S1];
        let best = negotiator.select_best_mode(&remote_modes);
        assert_eq!(best, Some(WMBusMode::S1));

        // No common modes
        let remote_modes = vec![WMBusMode::T2];
        let best = negotiator.select_best_mode(&remote_modes);
        assert_eq!(best, None);
    }
}
