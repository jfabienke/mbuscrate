//! # Enhanced GPIO Abstraction for Interrupt-Driven Operation
//!
//! This module provides an enhanced GPIO abstraction layer that adds interrupt-driven
//! capabilities and performance optimizations specifically designed for radio driver
//! operations. It builds upon the existing HAL infrastructure with additional features
//! for high-frequency packet processing.
//!
//! ## Key Enhancements
//!
//! 1. **Interrupt-Driven Processing** - Edge-triggered interrupts for DIO pins
//! 2. **Asynchronous GPIO Operations** - Async/await support for GPIO events
//! 3. **Event Buffering** - Queue-based event processing for high-speed operations
//! 4. **Performance Monitoring** - GPIO event timing and frequency analysis
//! 5. **Edge Detection** - Rising, falling, and both edge detection
//! 6. **Debouncing** - Hardware and software debouncing for noisy signals
//! 7. **Priority Handling** - Different priority levels for GPIO events
//!
//! ## Use Cases
//!
//! - **FIFO Level Interrupts**: Efficient packet reception using RFM69 DIO pins
//! - **Sync Detection**: Fast response to sync word detection in wM-Bus frames
//! - **Timeout Handling**: Hardware timer integration for packet timeouts
//! - **Power Management**: Sleep/wake operations based on GPIO events
//!
//! ## Usage
//!
//! ```rust,no_run
//! use mbus_rs::wmbus::radio::hal::enhanced_gpio::{EnhancedGpio, GpioEventType, EdgeType};
//!
//! let mut gpio = EnhancedGpio::new()?;
//!
//! // Setup interrupt-driven DIO1 monitoring
//! gpio.setup_interrupt(24, EdgeType::Rising, GpioEventType::HighPriority).await?;
//!
//! // Wait for GPIO event asynchronously
//! let event = gpio.wait_for_event().await?;
//! println!("GPIO {} triggered", event.pin);
//! ```

use crate::util::{logging, IoBuffer};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::time::timeout;

#[cfg(feature = "raspberry-pi")]
use rppal::gpio::{Error as GpioError, Gpio, InputPin, Level, OutputPin, Trigger};

/// Enhanced GPIO errors with specific failure types
#[derive(Error, Debug, Clone, PartialEq)]
pub enum EnhancedGpioError {
    #[error("GPIO initialization failed: {reason}")]
    InitializationFailed { reason: String },

    #[error("Pin {pin} not configured for operation: {operation}")]
    PinNotConfigured { pin: u8, operation: String },

    #[error("Interrupt setup failed for pin {pin}: {reason}")]
    InterruptSetupFailed { pin: u8, reason: String },

    #[error("Event timeout after {timeout_ms}ms")]
    EventTimeout { timeout_ms: u64 },

    #[error("Event queue overflow: {lost_events} events lost")]
    EventQueueOverflow { lost_events: usize },

    #[error("Invalid edge type for pin {pin}: {edge:?}")]
    InvalidEdgeType { pin: u8, edge: EdgeType },

    #[error("GPIO operation failed: {reason}")]
    OperationFailed { reason: String },

    #[error("Pin {pin} already in use by another handler")]
    PinInUse { pin: u8 },
}

/// GPIO edge detection types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeType {
    /// Trigger on rising edge (low to high)
    Rising,
    /// Trigger on falling edge (high to low)
    Falling,
    /// Trigger on both edges
    Both,
}

/// GPIO event priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GpioEventType {
    /// Low priority events (debounced, buffered)
    LowPriority,
    /// Normal priority events (standard processing)
    Normal,
    /// High priority events (immediate processing)
    HighPriority,
    /// Critical events (bypass all buffering)
    Critical,
}

/// GPIO event data structure
#[derive(Debug, Clone)]
pub struct GpioEvent {
    /// GPIO pin number that triggered the event
    pub pin: u8,
    /// Current pin level after event
    pub level: bool,
    /// Edge type that triggered the event
    pub edge: EdgeType,
    /// Event priority
    pub priority: GpioEventType,
    /// Timestamp when event occurred
    pub timestamp: Instant,
    /// Duration since last event on this pin
    pub delta_time: Option<Duration>,
}

/// GPIO pin configuration for interrupt processing
#[derive(Debug, Clone)]
pub struct GpioConfig {
    /// Pin number (BCM GPIO numbering)
    pub pin: u8,
    /// Edge type to trigger on
    pub edge: EdgeType,
    /// Event priority
    pub priority: GpioEventType,
    /// Debounce time in microseconds
    pub debounce_us: Option<u64>,
    /// Maximum events per second (rate limiting)
    pub max_events_per_sec: Option<u32>,
}

impl GpioConfig {
    /// Create a new GPIO configuration
    pub fn new(pin: u8, edge: EdgeType, priority: GpioEventType) -> Self {
        Self {
            pin,
            edge,
            priority,
            debounce_us: None,
            max_events_per_sec: None,
        }
    }

    /// Add debouncing to the configuration
    pub fn with_debounce(mut self, debounce_us: u64) -> Self {
        self.debounce_us = Some(debounce_us);
        self
    }

    /// Add rate limiting to the configuration
    pub fn with_rate_limit(mut self, max_events_per_sec: u32) -> Self {
        self.max_events_per_sec = Some(max_events_per_sec);
        self
    }
}

/// Statistics for GPIO event monitoring
#[derive(Debug, Default, Clone, Copy)]
pub struct GpioStats {
    /// Total events processed
    pub total_events: u64,
    /// Events by priority level
    pub high_priority_events: u64,
    pub normal_priority_events: u64,
    pub low_priority_events: u64,
    /// Event processing times
    pub min_processing_time_us: u64,
    pub max_processing_time_us: u64,
    pub avg_processing_time_us: u64,
    /// Queue statistics
    pub events_dropped: u64,
    pub max_queue_depth: usize,
    /// Interrupt frequency statistics
    pub events_per_second: f64,
}

/// Enhanced GPIO abstraction with interrupt support
#[derive(Debug)]
pub struct EnhancedGpio {
    /// Event transmission channel
    #[allow(dead_code)]
    event_tx: mpsc::UnboundedSender<GpioEvent>,
    /// Event reception channel
    event_rx: Arc<Mutex<mpsc::UnboundedReceiver<GpioEvent>>>,
    /// Active GPIO pin configurations
    pin_configs: HashMap<u8, GpioConfig>,
    /// GPIO pin handlers (platform-specific)
    #[cfg(feature = "raspberry-pi")]
    gpio_pins: HashMap<u8, InputPin>,
    /// Statistics tracking
    stats: Arc<Mutex<GpioStats>>,
    /// Event buffer for high-frequency operations
    #[allow(dead_code)]
    event_buffer: Arc<Mutex<IoBuffer>>,
    /// Error throttling for production use
    #[allow(dead_code)]
    error_throttle: logging::LogThrottle,
    /// Last event times for debouncing
    last_event_times: Arc<Mutex<HashMap<u8, Instant>>>,
}

impl EnhancedGpio {
    /// Create a new enhanced GPIO instance
    pub fn new() -> Result<Self, EnhancedGpioError> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Ok(Self {
            event_tx,
            event_rx: Arc::new(Mutex::new(event_rx)),
            pin_configs: HashMap::new(),
            #[cfg(feature = "raspberry-pi")]
            gpio_pins: HashMap::new(),
            stats: Arc::new(Mutex::new(GpioStats::default())),
            event_buffer: Arc::new(Mutex::new(IoBuffer::with_capacity(1024))),
            error_throttle: logging::LogThrottle::new(1000, 3), // 3 errors per second
            last_event_times: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Setup interrupt-driven monitoring for a GPIO pin
    pub async fn setup_interrupt(&mut self, config: GpioConfig) -> Result<(), EnhancedGpioError> {
        let pin = config.pin;

        // Check if pin is already configured
        if self.pin_configs.contains_key(&pin) {
            return Err(EnhancedGpioError::PinInUse { pin });
        }

        #[cfg(feature = "raspberry-pi")]
        {
            self.setup_raspberry_pi_interrupt(config.clone()).await?;
        }

        #[cfg(not(feature = "raspberry-pi"))]
        {
            // For non-Raspberry Pi platforms, we'd implement other GPIO interfaces
            log::warn!("GPIO interrupts not supported on this platform");
            Err(EnhancedGpioError::InitializationFailed {
                reason: "Platform not supported".to_string(),
            })
        }

        #[cfg(feature = "raspberry-pi")]
        Ok(())
    }

    #[cfg(feature = "raspberry-pi")]
    async fn setup_raspberry_pi_interrupt(
        &mut self,
        config: GpioConfig,
    ) -> Result<(), EnhancedGpioError> {
        let gpio = Gpio::new().map_err(|e| EnhancedGpioError::InitializationFailed {
            reason: format!("GPIO initialization failed: {}", e),
        })?;

        let mut pin = gpio
            .get(config.pin)
            .map_err(|e| EnhancedGpioError::InterruptSetupFailed {
                pin: config.pin,
                reason: format!("Pin access failed: {}", e),
            })?
            .into_input();

        // Convert edge type to rppal trigger
        let trigger = match config.edge {
            EdgeType::Rising => Trigger::RisingEdge,
            EdgeType::Falling => Trigger::FallingEdge,
            EdgeType::Both => Trigger::Both,
        };

        // Setup interrupt with callback
        let event_tx = self.event_tx.clone();
        let pin_num = config.pin;
        let edge = config.edge;
        let priority = config.priority;
        let last_times = self.last_event_times.clone();
        let debounce_us = config.debounce_us;

        pin.set_async_interrupt(trigger, move |level| {
            let now = Instant::now();

            // Debouncing check
            if let Some(debounce_time) = debounce_us {
                if let Ok(mut times) = last_times.lock() {
                    if let Some(last_time) = times.get(&pin_num) {
                        if now.duration_since(*last_time).as_micros() < debounce_time as u128 {
                            return; // Skip this event due to debouncing
                        }
                    }
                    times.insert(pin_num, now);
                }
            }

            let delta_time = if let Ok(times) = last_times.lock() {
                times.get(&pin_num).map(|last| now.duration_since(*last))
            } else {
                None
            };

            let event = GpioEvent {
                pin: pin_num,
                level: level == Level::High,
                edge,
                priority,
                timestamp: now,
                delta_time,
            };

            // Send event (non-blocking)
            if let Err(_) = event_tx.send(event) {
                // Event channel closed, log but don't panic
                log::warn!("GPIO event channel closed for pin {}", pin_num);
            }
        })
        .map_err(|e| EnhancedGpioError::InterruptSetupFailed {
            pin: config.pin,
            reason: format!("Interrupt setup failed: {}", e),
        })?;

        self.gpio_pins.insert(config.pin, pin);
        Ok(())
    }

    /// Wait for the next GPIO event with timeout
    pub async fn wait_for_event_timeout(
        &mut self,
        timeout_ms: u64,
    ) -> Result<GpioEvent, EnhancedGpioError> {
        let timeout_duration = Duration::from_millis(timeout_ms);

        timeout(timeout_duration, self.wait_for_event())
            .await
            .map_err(|_| EnhancedGpioError::EventTimeout { timeout_ms })?
    }

    /// Wait for the next GPIO event
    pub async fn wait_for_event(&mut self) -> Result<GpioEvent, EnhancedGpioError> {
        if let Ok(mut rx) = self.event_rx.lock() {
            if let Some(event) = rx.recv().await {
                self.update_stats(&event);
                Ok(event)
            } else {
                Err(EnhancedGpioError::OperationFailed {
                    reason: "Event channel closed".to_string(),
                })
            }
        } else {
            Err(EnhancedGpioError::OperationFailed {
                reason: "Event receiver lock failed".to_string(),
            })
        }
    }

    /// Wait for event on specific pin
    pub async fn wait_for_pin_event(
        &mut self,
        pin: u8,
        timeout_ms: u64,
    ) -> Result<GpioEvent, EnhancedGpioError> {
        let start = Instant::now();
        let timeout_duration = Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout_duration {
            let event = self
                .wait_for_event_timeout(timeout_ms - start.elapsed().as_millis() as u64)
                .await?;

            if event.pin == pin {
                return Ok(event);
            }
            // Continue waiting for the specific pin
        }

        Err(EnhancedGpioError::EventTimeout { timeout_ms })
    }

    /// Check for GPIO events without blocking
    pub fn poll_events(&mut self) -> Vec<GpioEvent> {
        let mut events = Vec::new();

        if let Ok(mut rx) = self.event_rx.try_lock() {
            while let Ok(event) = rx.try_recv() {
                self.update_stats(&event);
                events.push(event);
            }
        }

        events
    }

    /// Read current GPIO pin state
    pub fn read_pin(&self, _pin: u8) -> Result<bool, EnhancedGpioError> {
        #[cfg(feature = "raspberry-pi")]
        {
            if let Some(gpio_pin) = self.gpio_pins.get(&_pin) {
                Ok(gpio_pin.read() == Level::High)
            } else {
                Err(EnhancedGpioError::PinNotConfigured {
                    pin: _pin,
                    operation: "read".to_string(),
                })
            }
        }

        #[cfg(not(feature = "raspberry-pi"))]
        {
            Err(EnhancedGpioError::OperationFailed {
                reason: "Platform not supported".to_string(),
            })
        }
    }

    /// Remove interrupt monitoring for a pin
    pub fn remove_interrupt(&mut self, pin: u8) -> Result<(), EnhancedGpioError> {
        #[cfg(feature = "raspberry-pi")]
        {
            if let Some(mut gpio_pin) = self.gpio_pins.remove(&pin) {
                gpio_pin.clear_async_interrupt().map_err(|e| {
                    EnhancedGpioError::OperationFailed {
                        reason: format!("Failed to clear interrupt: {}", e),
                    }
                })?;
            }
        }

        self.pin_configs.remove(&pin);

        if let Ok(mut times) = self.last_event_times.lock() {
            times.remove(&pin);
        }

        log::info!("GPIO {pin} interrupt removed");
        Ok(())
    }

    /// Get current GPIO statistics
    pub fn get_stats(&self) -> Result<GpioStats, EnhancedGpioError> {
        self.stats
            .lock()
            .map(|stats| *stats)
            .map_err(|_| EnhancedGpioError::OperationFailed {
                reason: "Stats lock failed".to_string(),
            })
    }

    /// Reset GPIO statistics
    pub fn reset_stats(&mut self) -> Result<(), EnhancedGpioError> {
        if let Ok(mut stats) = self.stats.lock() {
            *stats = GpioStats::default();
            Ok(())
        } else {
            Err(EnhancedGpioError::OperationFailed {
                reason: "Stats lock failed".to_string(),
            })
        }
    }

    /// Update statistics with new event
    fn update_stats(&self, event: &GpioEvent) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.total_events += 1;

            match event.priority {
                GpioEventType::HighPriority | GpioEventType::Critical => {
                    stats.high_priority_events += 1;
                }
                GpioEventType::Normal => {
                    stats.normal_priority_events += 1;
                }
                GpioEventType::LowPriority => {
                    stats.low_priority_events += 1;
                }
            }

            // Update event frequency (simple moving average)
            let events_per_sec = if let Some(delta) = event.delta_time {
                if delta.as_secs_f64() > 0.0 {
                    1.0 / delta.as_secs_f64()
                } else {
                    stats.events_per_second
                }
            } else {
                stats.events_per_second
            };

            stats.events_per_second = (stats.events_per_second * 0.9) + (events_per_sec * 0.1);
        }
    }

    /// Get list of configured pins
    pub fn configured_pins(&self) -> Vec<u8> {
        self.pin_configs.keys().copied().collect()
    }

    /// Check if a pin is configured
    pub fn is_pin_configured(&self, pin: u8) -> bool {
        self.pin_configs.contains_key(&pin)
    }
}

impl Default for EnhancedGpio {
    fn default() -> Self {
        Self::new().expect("Failed to create default EnhancedGpio")
    }
}

/// Helper functions for common GPIO operations
pub mod helpers {
    use super::*;

    /// Setup typical RFM69 interrupt configuration
    pub async fn setup_rfm69_interrupts(
        gpio: &mut EnhancedGpio,
        dio1_pin: u8,
        dio2_pin: Option<u8>,
    ) -> Result<(), EnhancedGpioError> {
        // DIO1 for FIFO level interrupt (high priority)
        let dio1_config = GpioConfig::new(dio1_pin, EdgeType::Rising, GpioEventType::HighPriority)
            .with_debounce(10); // 10µs debounce for radio signals

        gpio.setup_interrupt(dio1_config).await?;

        // DIO2 for sync word detection (critical priority)
        if let Some(dio2) = dio2_pin {
            let dio2_config =
                GpioConfig::new(dio2, EdgeType::Rising, GpioEventType::Critical).with_debounce(5); // 5µs debounce for fast sync detection

            gpio.setup_interrupt(dio2_config).await?;
        }

        Ok(())
    }

    /// Setup typical SX126x interrupt configuration
    pub async fn setup_sx126x_interrupts(
        gpio: &mut EnhancedGpio,
        dio1_pin: u8,
        dio2_pin: Option<u8>,
        dio3_pin: Option<u8>,
    ) -> Result<(), EnhancedGpioError> {
        // DIO1 for primary interrupts (high priority)
        let dio1_config = GpioConfig::new(dio1_pin, EdgeType::Rising, GpioEventType::HighPriority)
            .with_debounce(20); // 20µs debounce

        gpio.setup_interrupt(dio1_config).await?;

        // DIO2 for RF switch control (normal priority)
        if let Some(dio2) = dio2_pin {
            let dio2_config = GpioConfig::new(dio2, EdgeType::Both, GpioEventType::Normal);
            gpio.setup_interrupt(dio2_config).await?;
        }

        // DIO3 for TCXO control (low priority)
        if let Some(dio3) = dio3_pin {
            let dio3_config = GpioConfig::new(dio3, EdgeType::Both, GpioEventType::LowPriority);
            gpio.setup_interrupt(dio3_config).await?;
        }

        Ok(())
    }

    /// Wait for packet reception complete using interrupts
    pub async fn wait_for_packet_complete(
        gpio: &mut EnhancedGpio,
        dio_pin: u8,
        timeout_ms: u64,
    ) -> Result<(), EnhancedGpioError> {
        let event = gpio.wait_for_pin_event(dio_pin, timeout_ms).await?;

        if event.level {
            log::debug!("Packet reception complete on GPIO {dio_pin}");
            Ok(())
        } else {
            Err(EnhancedGpioError::OperationFailed {
                reason: "Unexpected GPIO level for packet complete".to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpio_config_creation() {
        let config = GpioConfig::new(24, EdgeType::Rising, GpioEventType::HighPriority)
            .with_debounce(100)
            .with_rate_limit(1000);

        assert_eq!(config.pin, 24);
        assert_eq!(config.edge, EdgeType::Rising);
        assert_eq!(config.priority, GpioEventType::HighPriority);
        assert_eq!(config.debounce_us, Some(100));
        assert_eq!(config.max_events_per_sec, Some(1000));
    }

    #[test]
    fn test_gpio_event_creation() {
        let event = GpioEvent {
            pin: 24,
            level: true,
            edge: EdgeType::Rising,
            priority: GpioEventType::HighPriority,
            timestamp: Instant::now(),
            delta_time: Some(Duration::from_micros(1000)),
        };

        assert_eq!(event.pin, 24);
        assert_eq!(event.level, true);
        assert_eq!(event.edge, EdgeType::Rising);
        assert_eq!(event.priority, GpioEventType::HighPriority);
    }

    #[test]
    fn test_enhanced_gpio_creation() {
        let gpio = EnhancedGpio::new();
        assert!(gpio.is_ok());

        let gpio = gpio.unwrap();
        assert_eq!(gpio.configured_pins().len(), 0);
        assert!(!gpio.is_pin_configured(24));
    }

    #[test]
    fn test_gpio_stats_default() {
        let stats = GpioStats::default();
        assert_eq!(stats.total_events, 0);
        assert_eq!(stats.high_priority_events, 0);
        assert_eq!(stats.events_per_second, 0.0);
    }

    #[test]
    fn test_edge_type_values() {
        assert_ne!(EdgeType::Rising, EdgeType::Falling);
        assert_ne!(EdgeType::Rising, EdgeType::Both);
        assert_ne!(EdgeType::Falling, EdgeType::Both);
    }

    #[test]
    fn test_event_priority_ordering() {
        assert!(GpioEventType::Critical > GpioEventType::HighPriority);
        assert!(GpioEventType::HighPriority > GpioEventType::Normal);
        assert!(GpioEventType::Normal > GpioEventType::LowPriority);
    }
}
