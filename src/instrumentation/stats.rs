//! # Per-Device Error Statistics and Monitoring
//!
//! This module provides comprehensive error tracking and statistics collection
//! on a per-device basis, enabling identification of problematic devices and
//! patterns in communication failures.
//!
//! ## Features
//!
//! - Per-device error counters (CRC, block, timeout, etc.)
//! - Time-windowed statistics for rate calculation
//! - Alert thresholds for error rates
//! - Integration with unified instrumentation model
//!
//! ## Usage
//!
//! ```rust
//! use instrumentation::stats::{DeviceStats, get_device_stats, update_device_error};
//!
//! // Track CRC error for a device
//! update_device_error("12345678", ErrorType::Crc);
//!
//! // Get statistics for monitoring
//! let stats = get_device_stats("12345678");
//! if stats.get_error_rate(ErrorType::Crc) > 5.0 {
//!     // Alert: High CRC error rate
//! }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json;

lazy_static! {
    /// Global registry of per-device statistics
    static ref DEVICE_STATS: Arc<Mutex<HashMap<String, Arc<Mutex<DeviceStats>>>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

/// Types of errors tracked per device
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorType {
    /// Frame-level CRC error
    Crc,
    /// Block-level CRC error in multi-block frames
    BlockCrc,
    /// Type A frame error
    TypeA,
    /// Type B frame error
    TypeB,
    /// Timeout waiting for response
    Timeout,
    /// Invalid header structure
    InvalidHeader,
    /// Decryption failure
    DecryptionFailed,
    /// FIFO overrun in radio
    FifoOverrun,
    /// Parsing error
    ParseError,
    /// Other error
    Other,
}

/// Time-windowed counter for rate calculation
#[derive(Debug, Clone)]
struct WindowedCounter {
    /// Counts per time window
    windows: Vec<(Instant, u64)>,
    /// Window duration
    window_duration: Duration,
    /// Maximum windows to keep
    max_windows: usize,
}

impl WindowedCounter {
    fn new(window_duration: Duration, max_windows: usize) -> Self {
        Self {
            windows: Vec::new(),
            window_duration,
            max_windows,
        }
    }

    fn increment(&mut self) {
        let now = Instant::now();
        self.cleanup_old_windows(now);

        if let Some(last) = self.windows.last_mut() {
            if now.duration_since(last.0) < self.window_duration {
                last.1 += 1;
                return;
            }
        }

        self.windows.push((now, 1));
    }

    fn get_rate(&mut self) -> f64 {
        let now = Instant::now();
        self.cleanup_old_windows(now);

        if self.windows.is_empty() {
            return 0.0;
        }

        let total: u64 = self.windows.iter().map(|(_, count)| count).sum();
        let duration = now.duration_since(self.windows[0].0).as_secs_f64();

        if duration > 0.0 {
            total as f64 / duration * 60.0 // Convert to per-minute rate
        } else {
            0.0
        }
    }

    fn cleanup_old_windows(&mut self, now: Instant) {
        // Remove windows older than max_windows * window_duration
        let cutoff = self.window_duration * self.max_windows as u32;
        self.windows.retain(|(time, _)| {
            now.duration_since(*time) < cutoff
        });

        // Keep only max_windows
        if self.windows.len() > self.max_windows {
            self.windows.drain(0..self.windows.len() - self.max_windows);
        }
    }

    fn get_total(&self) -> u64 {
        self.windows.iter().map(|(_, count)| count).sum()
    }
}

/// Per-device statistics and error tracking
#[derive(Debug)]
pub struct DeviceStats {
    /// Device identifier
    pub device_id: String,
    /// Manufacturer identifier if known
    pub manufacturer_id: Option<String>,
    /// Device type/medium
    pub device_type: Option<String>,
    /// Total frames received
    pub frames_received: u64,
    /// Total frames successfully processed
    pub frames_valid: u64,
    /// Error counters by type
    error_counters: HashMap<ErrorType, WindowedCounter>,
    /// Last seen timestamp
    pub last_seen: SystemTime,
    /// First seen timestamp
    pub first_seen: SystemTime,
    /// Alert thresholds (errors per minute)
    alert_thresholds: HashMap<ErrorType, f64>,
}

impl DeviceStats {
    /// Create new device statistics
    pub fn new(device_id: String) -> Self {
        let mut alert_thresholds = HashMap::new();
        // Default thresholds (errors per minute)
        alert_thresholds.insert(ErrorType::Crc, 5.0);
        alert_thresholds.insert(ErrorType::BlockCrc, 10.0);
        alert_thresholds.insert(ErrorType::Timeout, 2.0);
        alert_thresholds.insert(ErrorType::DecryptionFailed, 3.0);

        Self {
            device_id,
            manufacturer_id: None,
            device_type: None,
            frames_received: 0,
            frames_valid: 0,
            error_counters: HashMap::new(),
            last_seen: SystemTime::now(),
            first_seen: SystemTime::now(),
            alert_thresholds,
        }
    }

    /// Increment error counter for specific type
    pub fn increment_error(&mut self, error_type: ErrorType) {
        let counter = self.error_counters
            .entry(error_type)
            .or_insert_with(|| WindowedCounter::new(Duration::from_secs(60), 10));
        counter.increment();
        self.last_seen = SystemTime::now();

        // Log if threshold exceeded
        let rate = counter.get_rate();
        if let Some(&threshold) = self.alert_thresholds.get(&error_type) {
            if rate > threshold {
                log::warn!(
                    "Device {} exceeds {error_type:?} error threshold: {rate:.1}/min (threshold: {threshold:.1})",
                    self.device_id,
                );
            }
        }
    }

    /// Increment successful frame counter
    pub fn increment_success(&mut self) {
        self.frames_received += 1;
        self.frames_valid += 1;
        self.last_seen = SystemTime::now();
    }

    /// Increment received frame counter (regardless of success)
    pub fn increment_received(&mut self) {
        self.frames_received += 1;
        self.last_seen = SystemTime::now();
    }

    /// Get error rate for specific type (per minute)
    pub fn get_error_rate(&mut self, error_type: ErrorType) -> f64 {
        self.error_counters
            .get_mut(&error_type)
            .map(|c| c.get_rate())
            .unwrap_or(0.0)
    }

    /// Get total errors for specific type
    pub fn get_error_count(&self, error_type: ErrorType) -> u64 {
        self.error_counters
            .get(&error_type)
            .map(|c| c.get_total())
            .unwrap_or(0)
    }

    /// Get success rate percentage
    pub fn get_success_rate(&self) -> f64 {
        if self.frames_received == 0 {
            return 100.0;
        }
        (self.frames_valid as f64 / self.frames_received as f64) * 100.0
    }

    /// Check if any error rate exceeds threshold
    pub fn has_alerts(&mut self) -> Vec<(ErrorType, f64)> {
        let mut alerts = Vec::new();

        // Clone thresholds to avoid borrow conflict
        let thresholds = self.alert_thresholds.clone();

        for (error_type, threshold) in thresholds {
            let rate = self.get_error_rate(error_type);
            if rate > threshold {
                alerts.push((error_type, rate));
            }
        }

        alerts
    }

    /// Export statistics as JSON-serializable struct
    pub fn export(&self) -> DeviceStatsExport {
        DeviceStatsExport {
            device_id: self.device_id.clone(),
            manufacturer_id: self.manufacturer_id.clone(),
            device_type: self.device_type.clone(),
            frames_received: self.frames_received,
            frames_valid: self.frames_valid,
            success_rate: self.get_success_rate(),
            error_counts: self.error_counters
                .iter()
                .map(|(k, v)| (*k, v.get_total()))
                .collect(),
            last_seen: self.last_seen,
            first_seen: self.first_seen,
        }
    }
}

/// Exportable device statistics (for serialization)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatsExport {
    pub device_id: String,
    pub manufacturer_id: Option<String>,
    pub device_type: Option<String>,
    pub frames_received: u64,
    pub frames_valid: u64,
    pub success_rate: f64,
    pub error_counts: HashMap<ErrorType, u64>,
    pub last_seen: SystemTime,
    pub first_seen: SystemTime,
}

/// Get or create statistics for a device
pub fn get_device_stats(device_id: &str) -> Arc<Mutex<DeviceStats>> {
    let mut registry = DEVICE_STATS.lock().unwrap();
    registry
        .entry(device_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(DeviceStats::new(device_id.to_string()))))
        .clone()
}

/// Update device error counter
pub fn update_device_error(device_id: &str, error_type: ErrorType) {
    let stats = get_device_stats(device_id);
    let mut stats = stats.lock().unwrap();
    stats.increment_error(error_type);
}

/// Update device success counter
pub fn update_device_success(device_id: &str) {
    let stats = get_device_stats(device_id);
    let mut stats = stats.lock().unwrap();
    stats.increment_success();
}

/// Update device with manufacturer and type information
pub fn update_device_info(
    device_id: &str,
    manufacturer_id: Option<String>,
    device_type: Option<String>,
) {
    let stats = get_device_stats(device_id);
    let mut stats = stats.lock().unwrap();
    if manufacturer_id.is_some() {
        stats.manufacturer_id = manufacturer_id;
    }
    if device_type.is_some() {
        stats.device_type = device_type;
    }
}

/// Get all device statistics for monitoring
pub fn get_all_device_stats() -> Vec<DeviceStatsExport> {
    let registry = DEVICE_STATS.lock().unwrap();
    registry
        .values()
        .map(|stats| {
            let stats = stats.lock().unwrap();
            stats.export()
        })
        .collect()
}

/// Get devices with active alerts
pub fn get_devices_with_alerts() -> Vec<(String, Vec<(ErrorType, f64)>)> {
    let registry = DEVICE_STATS.lock().unwrap();
    let mut alerts = Vec::new();

    for (device_id, stats) in registry.iter() {
        let mut stats = stats.lock().unwrap();
        let device_alerts = stats.has_alerts();
        if !device_alerts.is_empty() {
            alerts.push((device_id.clone(), device_alerts));
        }
    }

    alerts
}

/// Clear statistics for a specific device
pub fn clear_device_stats(device_id: &str) {
    let mut registry = DEVICE_STATS.lock().unwrap();
    registry.remove(device_id);
}

/// Clear all device statistics
pub fn clear_all_stats() {
    let mut registry = DEVICE_STATS.lock().unwrap();
    registry.clear();
}

/// Export all device statistics
pub fn export_all_stats() -> std::collections::HashMap<String, DeviceStatsExport> {
    let registry = DEVICE_STATS.lock().unwrap();
    let mut all_stats = std::collections::HashMap::new();

    for (device_id, stats) in registry.iter() {
        let stats = stats.lock().unwrap();
        all_stats.insert(device_id.clone(), stats.export());
    }

    all_stats
}

/// Export all device statistics as JSON
pub fn export_all_stats_json() -> Result<String, serde_json::Error> {
    let stats = export_all_stats();
    serde_json::to_string_pretty(&stats)
}

/// Export statistics for a specific device as JSON
pub fn export_device_stats_json(device_id: &str) -> Result<String, serde_json::Error> {
    let stats = get_device_stats(device_id);
    let stats = stats.lock().unwrap();
    let export = stats.export();
    serde_json::to_string_pretty(&export)
}

/// LoRa-specific metrics tracker
pub struct LoRaMetricsTracker {
    rssi_sum: f64,
    rssi_count: u64,
    rssi_min: i16,
    rssi_max: i16,

    snr_sum: f64,
    snr_count: u64,
    snr_min: f32,
    snr_max: f32,

    uplink_count: u64,
    downlink_count: u64,

    toa_sum: f64,
    toa_count: u64,

    current_sf: Option<u8>,
}

impl Default for LoRaMetricsTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl LoRaMetricsTracker {
    /// Create a new LoRa metrics tracker
    pub fn new() -> Self {
        Self {
            rssi_sum: 0.0,
            rssi_count: 0,
            rssi_min: 0,
            rssi_max: -200,

            snr_sum: 0.0,
            snr_count: 0,
            snr_min: 100.0,
            snr_max: -100.0,

            uplink_count: 0,
            downlink_count: 0,

            toa_sum: 0.0,
            toa_count: 0,

            current_sf: None,
        }
    }

    /// Record an uplink packet with metrics
    pub fn record_uplink(&mut self, rssi: i16, snr: f32, toa_ms: f32) {
        self.uplink_count += 1;

        // Update RSSI stats
        self.rssi_sum += rssi as f64;
        self.rssi_count += 1;
        self.rssi_min = self.rssi_min.min(rssi);
        self.rssi_max = self.rssi_max.max(rssi);

        // Update SNR stats
        self.snr_sum += snr as f64;
        self.snr_count += 1;
        self.snr_min = self.snr_min.min(snr);
        self.snr_max = self.snr_max.max(snr);

        // Update ToA stats
        self.toa_sum += toa_ms as f64;
        self.toa_count += 1;
    }

    /// Record a downlink packet
    pub fn record_downlink(&mut self) {
        self.downlink_count += 1;
    }

    /// Set current spreading factor
    pub fn set_sf(&mut self, sf: u8) {
        self.current_sf = Some(sf);
    }

    /// Export as LoRaMetrics structure
    pub fn export(&self) -> Option<LoRaMetrics> {
        if self.uplink_count == 0 {
            return None;
        }

        Some(LoRaMetrics {
            avg_rssi: (self.rssi_sum / self.rssi_count as f64) as f32,
            min_rssi: self.rssi_min,
            max_rssi: self.rssi_max,
            avg_snr: (self.snr_sum / self.snr_count as f64) as f32,
            min_snr: self.snr_min,
            max_snr: self.snr_max,
            uplink_count: self.uplink_count,
            downlink_count: self.downlink_count,
            current_sf: self.current_sf,
            avg_toa_ms: if self.toa_count > 0 {
                (self.toa_sum / self.toa_count as f64) as f32
            } else {
                0.0
            },
        })
    }
}

/// LoRa-specific metrics structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRaMetrics {
    pub avg_rssi: f32,
    pub min_rssi: i16,
    pub max_rssi: i16,
    pub avg_snr: f32,
    pub min_snr: f32,
    pub max_snr: f32,
    pub uplink_count: u64,
    pub downlink_count: u64,
    pub current_sf: Option<u8>,
    pub avg_toa_ms: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_windowed_counter() {
        let mut counter = WindowedCounter::new(Duration::from_secs(1), 5);

        counter.increment();
        counter.increment();
        assert_eq!(counter.get_total(), 2);

        thread::sleep(Duration::from_millis(100));
        counter.increment();
        assert_eq!(counter.get_total(), 3);
    }

    #[test]
    fn test_device_stats() {
        let mut stats = DeviceStats::new("TEST123".to_string());

        stats.increment_error(ErrorType::Crc);
        stats.increment_error(ErrorType::Crc);
        stats.increment_success();

        assert_eq!(stats.get_error_count(ErrorType::Crc), 2);
        assert_eq!(stats.frames_valid, 1);
        assert_eq!(stats.frames_received, 1);
    }

    #[test]
    fn test_global_registry() {
        clear_all_stats(); // Clean state

        update_device_error("DEVICE1", ErrorType::Crc);
        update_device_success("DEVICE1");
        update_device_error("DEVICE2", ErrorType::Timeout);

        let all_stats = get_all_device_stats();
        assert_eq!(all_stats.len(), 2);

        let device1_stats = all_stats.iter().find(|s| s.device_id == "DEVICE1").unwrap();
        assert_eq!(device1_stats.frames_valid, 1);
        assert_eq!(device1_stats.error_counts.get(&ErrorType::Crc), Some(&1));
    }

    #[test]
    fn test_success_rate() {
        let mut stats = DeviceStats::new("TEST".to_string());

        assert_eq!(stats.get_success_rate(), 100.0); // No data yet

        stats.increment_received(); // Frame 1: received but failed
        stats.increment_received(); // Frame 2: received but failed
        stats.increment_success();  // Frame 3: received and successful

        assert_eq!(stats.frames_received, 3); // 2 failed + 1 successful
        assert_eq!(stats.frames_valid, 1);    // 1 successful

        let success_rate = stats.get_success_rate();
        assert!((success_rate - 33.333333).abs() < 0.001, "Success rate should be ~33.33%, got {}", success_rate);
    }

    #[test]
    fn test_alert_thresholds() {
        let mut stats = DeviceStats::new("TEST".to_string());

        // Set low threshold for testing
        stats.alert_thresholds.insert(ErrorType::Crc, 0.1);

        // Generate errors to exceed threshold
        for _ in 0..10 {
            stats.increment_error(ErrorType::Crc);
        }

        let alerts = stats.has_alerts();
        assert!(!alerts.is_empty());
        assert_eq!(alerts[0].0, ErrorType::Crc);
    }
}