//! # Enhanced Logging Utilities
//!
//! This module provides enhanced logging patterns for M-Bus and wM-Bus implementations,
//! including rate limiting, structured logging with tracing spans, and debugging
//! utilities for protocol analysis.
//!
//! ## Features
//!
//! - Rate-limited logging to prevent log spam in production
//! - Structured logging with tracing spans for better observability
//! - Hex dump utilities for protocol debugging
//! - Performance-aware logging with minimal overhead
//! - Integration with the `log` and `tracing` crates
//!
//! ## Usage
//!
//! ```rust
//! use mbus_rs::util::logging::{LogThrottle, log_frame_hex, span_frame_processing};
//! use tracing::info_span;
//!
//! // Rate-limited logging
//! let mut throttle = LogThrottle::new(1000, 5); // 5 messages per second
//! if throttle.allow() {
//!     log::warn!("CRC error detected");
//! }
//!
//! // Structured frame processing
//! let _span = span_frame_processing("wM-Bus Type A");
//! log_frame_hex("Received frame", &frame_data);
//! ```

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Throttling structure for rate-limiting log messages
///
/// This prevents log spam in production environments where high-frequency
/// events can overwhelm the logging system, a common challenge in
/// continuous monitoring applications.
#[derive(Debug)]
pub struct LogThrottle {
    /// Time window for throttling (in milliseconds)
    window_ms: u64,
    /// Maximum messages allowed per window
    cap: u32,
    /// Current message count in window
    count: u32,
    /// Start time of current window
    t0: Instant,
}

impl LogThrottle {
    /// Create new throttle with time window and message cap
    ///
    /// # Arguments
    /// * `window_ms` - Time window in milliseconds
    /// * `cap` - Maximum messages allowed per window
    ///
    /// # Examples
    /// ```rust
    /// use mbus_rs::util::logging::LogThrottle;
    ///
    /// // Allow 5 messages per second
    /// let mut throttle = LogThrottle::new(1000, 5);
    /// ```
    pub fn new(window_ms: u64, cap: u32) -> Self {
        Self {
            window_ms,
            cap,
            count: 0,
            t0: Instant::now(),
        }
    }

    /// Check if logging is allowed (resets counter after window expires)
    ///
    /// Returns `true` if the message should be logged, `false` if it
    /// should be throttled.
    pub fn allow(&mut self) -> bool {
        let now = Instant::now();
        let elapsed_ms = now.duration_since(self.t0).as_millis() as u64;

        if elapsed_ms > self.window_ms {
            self.t0 = now;
            self.count = 0;
        }

        self.count += 1;
        self.count <= self.cap
    }

    /// Get current throttle statistics
    pub fn stats(&self) -> ThrottleStats {
        ThrottleStats {
            window_ms: self.window_ms,
            cap: self.cap,
            count: self.count,
            window_remaining_ms: self
                .window_ms
                .saturating_sub(self.t0.elapsed().as_millis() as u64),
        }
    }

    /// Reset the throttle (start new window immediately)
    pub fn reset(&mut self) {
        self.t0 = Instant::now();
        self.count = 0;
    }
}

/// Statistics about a log throttle instance
#[derive(Debug, Clone, Copy)]
pub struct ThrottleStats {
    pub window_ms: u64,
    pub cap: u32,
    pub count: u32,
    pub window_remaining_ms: u64,
}

/// Global throttle manager for different log categories
///
/// This allows different types of log messages to have their own
/// throttling rules without interfering with each other.
#[derive(Debug)]
pub struct ThrottleManager {
    throttles: HashMap<String, LogThrottle>,
}

impl ThrottleManager {
    /// Create a new throttle manager
    pub fn new() -> Self {
        Self {
            throttles: HashMap::new(),
        }
    }

    /// Check if logging is allowed for a specific category
    pub fn allow(&mut self, category: &str, window_ms: u64, cap: u32) -> bool {
        let throttle = self
            .throttles
            .entry(category.to_string())
            .or_insert_with(|| LogThrottle::new(window_ms, cap));

        throttle.allow()
    }

    /// Get statistics for all throttles
    pub fn all_stats(&self) -> HashMap<String, ThrottleStats> {
        self.throttles
            .iter()
            .map(|(k, v)| (k.clone(), v.stats()))
            .collect()
    }

    /// Reset all throttles
    pub fn reset_all(&mut self) {
        for throttle in self.throttles.values_mut() {
            throttle.reset();
        }
    }
}

impl Default for ThrottleManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Log frame data in hex format for debugging
///
/// Provides a consistent way to log frame data across the codebase
/// with optional length limits to prevent excessive log output.
pub fn log_frame_hex(prefix: &str, data: &[u8]) {
    const MAX_LOG_BYTES: usize = 64; // Limit hex output to prevent log spam

    let display_data = if data.len() > MAX_LOG_BYTES {
        &data[..MAX_LOG_BYTES]
    } else {
        data
    };

    let hex_str = crate::util::hex::format_hex_compact(display_data);
    let suffix = if data.len() > MAX_LOG_BYTES {
        format!(" ... ({} bytes total)", data.len())
    } else {
        String::new()
    };

    log::debug!("{prefix}: {hex_str}{suffix}");
}

/// Log frame data with structured information
///
/// Combines hex logging with structured data for better observability.
pub fn log_frame_structured(
    prefix: &str,
    data: &[u8],
    frame_type: Option<&str>,
    source: Option<&str>,
) {
    log::debug!(
        target: "mbus::frame",
        "{}: {} bytes, type={:?}, source={:?}, data={}",
        prefix,
        data.len(),
        frame_type,
        source,
        crate::util::hex::format_hex_compact(
            &data[..data.len().min(32)] // Limit to 32 bytes for structured logs
        )
    );
}

/// Create a tracing span for frame processing
///
/// Provides structured logging context for frame processing operations.
/// When tracing is enabled, this creates nested spans that help with
/// debugging complex protocol flows.
#[cfg(feature = "tracing")]
pub fn span_frame_processing(frame_type: &str) -> tracing::Span {
    tracing::info_span!("frame_processing", frame_type = frame_type)
}

/// Fallback span creation when tracing is not available
#[cfg(not(feature = "tracing"))]
pub fn span_frame_processing(_frame_type: &str) {
    // No-op when tracing is disabled
}

/// Create a tracing span for CRC operations
#[cfg(feature = "tracing")]
pub fn span_crc_validation(expected: u16, calculated: u16) -> tracing::Span {
    tracing::debug_span!(
        "crc_validation",
        expected = expected,
        calculated = calculated
    )
}

#[cfg(not(feature = "tracing"))]
pub fn span_crc_validation(_expected: u16, _calculated: u16) {
    // No-op when tracing is disabled
}

/// Macros for convenient throttled logging
///
/// These macros combine throttling with standard logging levels
/// for common use cases.
/// Log an error with throttling
#[macro_export]
macro_rules! log_error_throttled {
    ($throttle:expr, $($arg:tt)*) => {
        if $throttle.allow() {
            log::error!($($arg)*);
        }
    };
}

/// Log a warning with throttling
#[macro_export]
macro_rules! log_warn_throttled {
    ($throttle:expr, $($arg:tt)*) => {
        if $throttle.allow() {
            log::warn!($($arg)*);
        }
    };
}

/// Log an info message with throttling
#[macro_export]
macro_rules! log_info_throttled {
    ($throttle:expr, $($arg:tt)*) => {
        if $throttle.allow() {
            log::info!($($arg)*);
        }
    };
}

/// Debug logging utilities for protocol analysis
pub mod debug {

    /// Log packet statistics in a formatted way
    pub fn log_packet_stats(stats: &crate::wmbus::radio::rfm69_packet::PacketStats) {
        log::info!(
            "Packet stats: received={}, valid={}, crc_errors={}, invalid_headers={}, encrypted={}, fifo_overruns={}",
            stats.packets_received,
            stats.packets_valid,
            stats.packets_crc_error,
            stats.packets_invalid_header,
            stats.packets_encrypted,
            stats.fifo_overruns
        );
    }

    /// Log CRC validation results
    pub fn log_crc_result(expected: u16, calculated: u16, valid: bool) {
        if valid {
            log::debug!("CRC valid: {expected:04X}");
        } else {
            log::warn!(
                "CRC mismatch: expected {expected:04X}, calculated {calculated:04X}"
            );
        }
    }

    /// Log frame type detection
    pub fn log_frame_type_detection(sync_byte: u8, frame_type: &str) {
        log::debug!(
            "Frame type detected: sync={sync_byte:02X} -> {frame_type}"
        );
    }

    /// Log encryption detection
    pub fn log_encryption_detection(ci: u8, acc: u8, encrypted: bool) {
        log::debug!(
            "Encryption check: CI={ci:02X}, ACC={acc:02X} -> encrypted={encrypted}"
        );
    }
}

/// Performance-aware logging utilities
pub mod perf {
    use super::*;

    /// A simple performance timer for logging operation durations
    #[derive(Debug)]
    pub struct PerfTimer {
        start: Instant,
        operation: String,
    }

    impl PerfTimer {
        /// Start timing an operation
        pub fn start(operation: &str) -> Self {
            Self {
                start: Instant::now(),
                operation: operation.to_string(),
            }
        }

        /// Finish timing and log the result
        pub fn finish(self) {
            let duration = self.start.elapsed();
            log::debug!("Operation '{}' took {:?}", self.operation, duration);
        }

        /// Finish timing with a custom log level
        pub fn finish_with_level(self, level: log::Level) {
            let duration = self.start.elapsed();
            log::log!(level, "Operation '{}' took {:?}", self.operation, duration);
        }
    }

    /// Log performance metrics for frame processing
    pub fn log_frame_processing_time(frame_len: usize, duration: Duration) {
        let throughput = if duration.as_nanos() > 0 {
            (frame_len as f64 * 1_000_000_000.0) / duration.as_nanos() as f64
        } else {
            0.0
        };

        log::debug!(
            "Frame processing: {frame_len} bytes in {duration:?} ({throughput:.1} bytes/sec)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_log_throttle_basic() {
        let mut throttle = LogThrottle::new(1000, 3); // 3 messages per second

        // First 3 messages should be allowed
        assert!(throttle.allow());
        assert!(throttle.allow());
        assert!(throttle.allow());

        // 4th message should be throttled
        assert!(!throttle.allow());
        assert!(!throttle.allow());
    }

    #[test]
    fn test_log_throttle_reset() {
        let mut throttle = LogThrottle::new(1000, 2);

        // Use up the quota
        assert!(throttle.allow());
        assert!(throttle.allow());
        assert!(!throttle.allow());

        // Reset should allow new messages
        throttle.reset();
        assert!(throttle.allow());
        assert!(throttle.allow());
        assert!(!throttle.allow());
    }

    #[test]
    fn test_throttle_manager() {
        let mut manager = ThrottleManager::new();

        // Different categories should have independent throttles
        assert!(manager.allow("crc_errors", 1000, 2));
        assert!(manager.allow("frame_errors", 1000, 2));
        assert!(manager.allow("crc_errors", 1000, 2));
        assert!(!manager.allow("crc_errors", 1000, 2)); // Should be throttled
        assert!(manager.allow("frame_errors", 1000, 2)); // Different category, still allowed
    }

    #[test]
    fn test_throttle_stats() {
        let mut throttle = LogThrottle::new(1000, 5);
        throttle.allow();
        throttle.allow();

        let stats = throttle.stats();
        assert_eq!(stats.window_ms, 1000);
        assert_eq!(stats.cap, 5);
        assert_eq!(stats.count, 2);
    }

    #[test]
    fn test_perf_timer() {
        let timer = perf::PerfTimer::start("test_operation");
        std::thread::sleep(Duration::from_millis(1));
        timer.finish(); // Should not panic
    }
}
