//! Low-Power Duty Cycle Management for LoRa
//!
//! Implements sleep/wake cycles for battery-powered devices with
//! duty cycle compliance and IRQ-based wake-up. Inspired by
//! SWL2001's Zephyr-based timer management.

use crate::wmbus::radio::driver::{RadioState, Sx126xDriver};
use crate::wmbus::radio::hal::Hal;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::{interval, sleep};
use log::{debug, info, warn};

/// Power modes for the radio
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PowerMode {
    /// Full power - radio always on
    Active,

    /// Low power - periodic sleep/wake cycles
    LowPower {
        /// Active time per cycle
        active_duration: Duration,
        /// Sleep time per cycle
        sleep_duration: Duration,
    },

    /// Ultra-low power - mostly sleeping, wake on IRQ
    UltraLowPower {
        /// Minimum sleep duration
        min_sleep: Duration,
        /// Maximum sleep duration (for periodic check-in)
        max_sleep: Duration,
    },

    /// Deep sleep - radio off, manual wake required
    DeepSleep,
}

/// Duty cycle manager for low-power operation
pub struct DutyCycleManager<H: Hal> {
    /// Radio driver reference
    driver: Arc<Mutex<Sx126xDriver<H>>>,

    /// Current power mode
    power_mode: Arc<Mutex<PowerMode>>,

    /// Duty cycle limit (percentage)
    duty_cycle_percent: f32,

    /// Transmission time tracking
    tx_time_window: Arc<Mutex<TransmissionWindow>>,

    /// Wake-up source tracking
    wake_source: Arc<Mutex<WakeSource>>,

    /// Power consumption estimator
    power_estimator: PowerEstimator,
}

/// Transmission time window for duty cycle calculation
struct TransmissionWindow {
    /// Window duration (typically 1 hour)
    window_duration: Duration,

    /// Transmission events in current window
    transmissions: Vec<(Instant, Duration)>,

    /// Window start time
    window_start: Instant,
}

impl TransmissionWindow {
    fn new(window_duration: Duration) -> Self {
        Self {
            window_duration,
            transmissions: Vec::new(),
            window_start: Instant::now(),
        }
    }

    /// Add a transmission event
    fn add_transmission(&mut self, duration: Duration) {
        let now = Instant::now();

        // Check if we need to start a new window
        if now.duration_since(self.window_start) > self.window_duration {
            self.transmissions.clear();
            self.window_start = now;
        }

        self.transmissions.push((now, duration));
    }

    /// Calculate current duty cycle percentage
    fn calculate_duty_cycle(&mut self) -> f32 {
        let now = Instant::now();

        // Remove old transmissions outside window
        let cutoff = now - self.window_duration;
        self.transmissions.retain(|(time, _)| *time > cutoff);

        // Sum transmission times
        let total_tx_time: Duration = self.transmissions
            .iter()
            .map(|(_, duration)| *duration)
            .sum();

        // Calculate percentage
        let window_ms = self.window_duration.as_millis() as f32;
        let tx_ms = total_tx_time.as_millis() as f32;

        (tx_ms / window_ms) * 100.0
    }

    /// Check if transmission is allowed
    fn can_transmit(&mut self, duration: Duration, limit: f32) -> bool {
        let current_duty = self.calculate_duty_cycle();
        let projected_duty = current_duty +
            (duration.as_millis() as f32 / self.window_duration.as_millis() as f32) * 100.0;

        projected_duty <= limit
    }
}

/// Wake-up source tracking
#[derive(Debug, Clone, Copy)]
enum WakeSource {
    /// Timer-based wake-up
    Timer,

    /// IRQ-based wake-up (packet received)
    Irq,

    /// Manual wake-up
    Manual,

    /// Unknown/initial state
    Unknown,
}

/// Power consumption estimator
struct PowerEstimator {
    /// Sleep mode current in mA
    sleep_current_ma: f32,

    /// Standby current in mA
    standby_current_ma: f32,

    /// RX current in mA
    rx_current_ma: f32,

    /// TX current in mA (at max power)
    tx_current_ma: f32,

    /// Time spent in each mode
    sleep_time: Duration,
    standby_time: Duration,
    rx_time: Duration,
    tx_time: Duration,

    /// Tracking start time
    start_time: Instant,
}

impl PowerEstimator {
    fn new() -> Self {
        Self {
            // Typical SX126x currents
            sleep_current_ma: 0.0002,    // 200 nA
            standby_current_ma: 0.6,     // 600 µA
            rx_current_ma: 11.0,          // 11 mA
            tx_current_ma: 45.0,          // 45 mA at +14 dBm

            sleep_time: Duration::ZERO,
            standby_time: Duration::ZERO,
            rx_time: Duration::ZERO,
            tx_time: Duration::ZERO,

            start_time: Instant::now(),
        }
    }

    /// Record time in a power state
    fn record_state(&mut self, state: RadioState, duration: Duration) {
        match state {
            RadioState::Sleep => self.sleep_time += duration,
            RadioState::StandbyRc | RadioState::StandbyXosc => self.standby_time += duration,
            RadioState::Rx => self.rx_time += duration,
            RadioState::Tx => self.tx_time += duration,
            _ => {}
        }
    }

    /// Calculate average current consumption in mA
    fn calculate_average_current(&self) -> f32 {
        let total_time = Instant::now().duration_since(self.start_time);
        if total_time.is_zero() {
            return 0.0;
        }

        let total_ms = total_time.as_millis() as f32;

        let sleep_contribution = (self.sleep_time.as_millis() as f32 / total_ms) * self.sleep_current_ma;
        let standby_contribution = (self.standby_time.as_millis() as f32 / total_ms) * self.standby_current_ma;
        let rx_contribution = (self.rx_time.as_millis() as f32 / total_ms) * self.rx_current_ma;
        let tx_contribution = (self.tx_time.as_millis() as f32 / total_ms) * self.tx_current_ma;

        sleep_contribution + standby_contribution + rx_contribution + tx_contribution
    }

    /// Estimate battery life in hours
    fn estimate_battery_life(&self, battery_capacity_mah: f32) -> f32 {
        let avg_current = self.calculate_average_current();
        if avg_current > 0.0 {
            battery_capacity_mah / avg_current
        } else {
            f32::INFINITY
        }
    }
}

impl<H: Hal> DutyCycleManager<H> {
    /// Create a new duty cycle manager
    pub fn new(driver: Arc<Mutex<Sx126xDriver<H>>>, duty_cycle_percent: f32) -> Self {
        Self {
            driver,
            power_mode: Arc::new(Mutex::new(PowerMode::Active)),
            duty_cycle_percent,
            tx_time_window: Arc::new(Mutex::new(TransmissionWindow::new(Duration::from_secs(3600)))),
            wake_source: Arc::new(Mutex::new(WakeSource::Unknown)),
            power_estimator: PowerEstimator::new(),
        }
    }

    /// Set the power mode
    pub async fn set_power_mode(&mut self, mode: PowerMode) {
        *self.power_mode.lock().await = mode;

        info!("Power mode changed to: {mode:?}");

        // Apply immediate changes based on mode
        match mode {
            PowerMode::DeepSleep => {
                // Put radio to sleep immediately
                let mut driver = self.driver.lock().await;
                if let Err(e) = driver.set_sleep(crate::wmbus::radio::driver::SleepConfig {
                    warm_start: false,
                    rtc_wake: false,
                }) {
                    warn!("Failed to enter deep sleep: {e:?}");
                }
            }
            PowerMode::Active => {
                // Wake radio if sleeping
                let mut driver = self.driver.lock().await;
                if let Err(e) = driver.set_standby(crate::wmbus::radio::driver::StandbyMode::RC) {
                    warn!("Failed to wake from sleep: {e:?}");
                }
            }
            _ => {}
        }
    }

    /// Run the duty cycle management loop
    pub async fn run(&mut self, mut shutdown_rx: tokio::sync::oneshot::Receiver<()>) {
        info!("Duty cycle manager started");

        let mut tick_interval = interval(Duration::from_millis(100));

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    info!("Duty cycle manager shutting down");
                    break;
                }

                _ = tick_interval.tick() => {
                    if let Err(e) = self.process_tick().await {
                        warn!("Duty cycle tick error: {e:?}");
                    }
                }
            }
        }
    }

    /// Process a single tick of the duty cycle manager
    async fn process_tick(&mut self) -> Result<(), String> {
        let mode = *self.power_mode.lock().await;

        match mode {
            PowerMode::Active => {
                // Always active, just track power consumption
                self.power_estimator.record_state(RadioState::Rx, Duration::from_millis(100));
            }

            PowerMode::LowPower { active_duration, sleep_duration } => {
                // Implement sleep/wake cycling
                let mut driver = self.driver.lock().await;

                // Active period
                driver.set_rx_continuous()
                    .map_err(|e| format!("Failed to enter RX: {e:?}"))?;
                self.power_estimator.record_state(RadioState::Rx, active_duration);
                sleep(active_duration).await;

                // Check duty cycle before sleeping
                let mut tx_window = self.tx_time_window.lock().await;
                if tx_window.calculate_duty_cycle() < self.duty_cycle_percent {
                    // Sleep period
                    driver.set_sleep(crate::wmbus::radio::driver::SleepConfig {
                        warm_start: true,
                        rtc_wake: false,
                    })
                        .map_err(|e| format!("Failed to enter sleep: {e:?}"))?;
                    self.power_estimator.record_state(RadioState::Sleep, sleep_duration);
                    sleep(sleep_duration).await;
                }
            }

            PowerMode::UltraLowPower { min_sleep, max_sleep } => {
                // Sleep until IRQ or timeout
                let mut driver = self.driver.lock().await;

                // Configure wake on IRQ
                driver.set_sleep(crate::wmbus::radio::driver::SleepConfig {
                    warm_start: true,
                    rtc_wake: true,  // Enable RTC wake for periodic check
                })
                    .map_err(|e| format!("Failed to enter ultra-low power: {e:?}"))?;

                // Sleep for calculated duration
                let sleep_time = self.calculate_adaptive_sleep(min_sleep, max_sleep).await;
                self.power_estimator.record_state(RadioState::Sleep, sleep_time);
                sleep(sleep_time).await;

                // Wake and check for packets
                *self.wake_source.lock().await = WakeSource::Timer;
                driver.set_rx(10) // 10ms timeout
                    .map_err(|e| format!("Failed to wake for RX: {e:?}"))?;
            }

            PowerMode::DeepSleep => {
                // Stay in deep sleep
                self.power_estimator.record_state(RadioState::Sleep, Duration::from_millis(100));
            }
        }

        Ok(())
    }

    /// Calculate adaptive sleep duration based on traffic patterns
    async fn calculate_adaptive_sleep(&self, min: Duration, max: Duration) -> Duration {
        // In a real implementation, this would analyze traffic patterns
        // For now, use a simple approach
        let duty_cycle = self.tx_time_window.lock().await.calculate_duty_cycle();

        if duty_cycle > self.duty_cycle_percent * 0.8 {
            // High duty cycle, sleep longer
            max
        } else if duty_cycle < self.duty_cycle_percent * 0.2 {
            // Low duty cycle, can wake more frequently
            min
        } else {
            // Middle ground
            Duration::from_millis(((min.as_millis() + max.as_millis()) / 2) as u64)
        }
    }

    /// Handle transmission with duty cycle compliance
    pub async fn transmit(&mut self, data: &[u8], duration: Duration) -> Result<(), String> {
        let mut tx_window = self.tx_time_window.lock().await;

        // Check duty cycle
        if !tx_window.can_transmit(duration, self.duty_cycle_percent) {
            return Err(format!(
                "Duty cycle limit exceeded: current {:.2}%",
                tx_window.calculate_duty_cycle()
            ));
        }

        // Perform transmission
        let mut driver = self.driver.lock().await;
        // Write data to buffer and start transmission
        driver.write_buffer(0, data)
            .map_err(|e| format!("Failed to write buffer: {e:?}"))?;
        driver.set_tx(1000) // 1 second timeout
            .map_err(|e| format!("Transmission failed: {e:?}"))?;

        // Record transmission
        tx_window.add_transmission(duration);
        self.power_estimator.record_state(RadioState::Tx, duration);

        Ok(())
    }

    /// Get current power statistics
    pub fn get_power_stats(&self) -> PowerStats {
        PowerStats {
            average_current_ma: self.power_estimator.calculate_average_current(),
            estimated_battery_life_hours: self.power_estimator.estimate_battery_life(2000.0), // 2000mAh battery
            current_duty_cycle: 0.0, // Would need async access to calculate
            power_mode: PowerMode::Active, // Would need async access
        }
    }

    /// Wake the radio from sleep (manual wake)
    pub async fn wake(&mut self) -> Result<(), String> {
        *self.wake_source.lock().await = WakeSource::Manual;

        let mut driver = self.driver.lock().await;
        driver.set_standby(crate::wmbus::radio::driver::StandbyMode::RC)
            .map_err(|e| format!("Failed to wake: {e:?}"))?;

        info!("Radio woken manually");
        Ok(())
    }

    /// Handle IRQ wake event
    pub async fn handle_irq_wake(&mut self) {
        *self.wake_source.lock().await = WakeSource::Irq;
        debug!("Radio woken by IRQ");
    }
}

/// Power consumption statistics
#[derive(Debug, Clone)]
pub struct PowerStats {
    /// Average current consumption in mA
    pub average_current_ma: f32,

    /// Estimated battery life in hours
    pub estimated_battery_life_hours: f32,

    /// Current duty cycle percentage
    pub current_duty_cycle: f32,

    /// Current power mode
    pub power_mode: PowerMode,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transmission_window() {
        let mut window = TransmissionWindow::new(Duration::from_secs(60));

        // Add some transmissions
        window.add_transmission(Duration::from_millis(100));
        window.add_transmission(Duration::from_millis(200));

        // Should be 0.5% duty cycle (300ms / 60000ms)
        let duty = window.calculate_duty_cycle();
        assert!(duty > 0.4 && duty < 0.6);

        // Check if we can transmit
        assert!(window.can_transmit(Duration::from_millis(100), 1.0));
        assert!(!window.can_transmit(Duration::from_secs(1), 0.5));
    }

    #[test]
    fn test_power_estimator() {
        let mut estimator = PowerEstimator::new();

        // Record some activity
        estimator.record_state(RadioState::Sleep, Duration::from_secs(50));
        estimator.record_state(RadioState::Rx, Duration::from_secs(10));

        // Average should be mostly sleep current
        let avg = estimator.calculate_average_current();
        assert!(avg < 2.0); // Should be low due to mostly sleeping

        // Battery life should be high
        let battery_life = estimator.estimate_battery_life(2000.0);
        assert!(battery_life > 1000.0); // Should last > 1000 hours
    }
}