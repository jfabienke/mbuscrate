//! Async IRQ Event Queue for LoRa Radio
//!
//! Implements non-blocking IRQ handling with event queuing for high-throughput
//! packet processing. Inspired by One Channel Hub's interrupt handling and
//! SWL2001's event-driven patterns.

use crate::wmbus::radio::irq::IrqStatus;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};
use log::{debug, warn, error};

/// IRQ event types that can be queued
#[derive(Debug, Clone)]
pub enum IrqEvent {
    /// Packet received successfully
    RxDone {
        timestamp: Instant,
        rssi: Option<i16>,
        snr: Option<f32>,
    },

    /// Transmission completed
    TxDone {
        timestamp: Instant,
    },

    /// CRC error in received packet
    CrcError {
        timestamp: Instant,
        partial_data: Option<Vec<u8>>,
    },

    /// Reception timeout
    RxTimeout {
        timestamp: Instant,
    },

    /// Preamble detected (start of reception)
    PreambleDetected {
        timestamp: Instant,
    },

    /// Header validation failed
    HeaderError {
        timestamp: Instant,
    },

    /// Channel activity detected
    CadDetected {
        timestamp: Instant,
        channel_rssi: Option<i16>,
    },

    /// Generic error event
    Error {
        timestamp: Instant,
        description: String,
    },
}

/// Priority levels for IRQ events
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventPriority {
    /// Critical events (errors, overruns)
    Critical = 0,

    /// High priority (RX done, TX done)
    High = 1,

    /// Medium priority (CAD, preamble)
    Medium = 2,

    /// Low priority (status updates)
    Low = 3,
}

impl IrqEvent {
    /// Get the priority of this event
    pub fn priority(&self) -> EventPriority {
        match self {
            IrqEvent::CrcError { .. } | IrqEvent::HeaderError { .. } | IrqEvent::Error { .. } => {
                EventPriority::Critical
            }
            IrqEvent::RxDone { .. } | IrqEvent::TxDone { .. } => EventPriority::High,
            IrqEvent::PreambleDetected { .. } | IrqEvent::CadDetected { .. } => {
                EventPriority::Medium
            }
            IrqEvent::RxTimeout { .. } => EventPriority::Low,
        }
    }

    /// Get the timestamp of this event
    pub fn timestamp(&self) -> Instant {
        match self {
            IrqEvent::RxDone { timestamp, .. }
            | IrqEvent::TxDone { timestamp }
            | IrqEvent::CrcError { timestamp, .. }
            | IrqEvent::RxTimeout { timestamp }
            | IrqEvent::PreambleDetected { timestamp }
            | IrqEvent::HeaderError { timestamp }
            | IrqEvent::CadDetected { timestamp, .. }
            | IrqEvent::Error { timestamp, .. } => *timestamp,
        }
    }
}

/// Statistics for IRQ event processing
#[derive(Debug, Default, Clone)]
pub struct IrqStats {
    /// Total events received
    pub total_events: u64,

    /// Events by type
    pub rx_done_count: u64,
    pub tx_done_count: u64,
    pub crc_error_count: u64,
    pub timeout_count: u64,

    /// Queue statistics
    pub queue_overflows: u64,
    pub events_dropped: u64,

    /// Processing statistics
    pub avg_processing_time_us: u64,
    pub max_processing_time_us: u64,
}

/// Async IRQ event queue for non-blocking interrupt processing
pub struct IrqEventQueue {
    /// Channel sender for queuing events
    tx: Sender<IrqEvent>,

    /// Channel receiver for processing events
    rx: Arc<Mutex<Receiver<IrqEvent>>>,

    /// Queue capacity
    capacity: usize,

    /// Statistics
    stats: Arc<Mutex<IrqStats>>,

    /// Maximum time to wait for event processing
    processing_timeout: Duration,
}

impl IrqEventQueue {
    /// Create a new IRQ event queue
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of events to queue
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = channel(capacity);

        Self {
            tx,
            rx: Arc::new(Mutex::new(rx)),
            capacity,
            stats: Arc::new(Mutex::new(IrqStats::default())),
            processing_timeout: Duration::from_millis(100),
        }
    }

    /// Create a new IRQ event queue with default capacity
    pub fn default() -> Self {
        Self::new(64)  // Default to 64 events queue depth
    }

    /// Queue an IRQ event for processing
    ///
    /// This is non-blocking and returns immediately.
    /// If the queue is full, the event is dropped and statistics are updated.
    pub async fn queue_event(&self, event: IrqEvent) -> Result<(), String> {
        let mut stats = self.stats.lock().await;
        stats.total_events += 1;

        // Update type-specific counters
        match &event {
            IrqEvent::RxDone { .. } => stats.rx_done_count += 1,
            IrqEvent::TxDone { .. } => stats.tx_done_count += 1,
            IrqEvent::CrcError { .. } => stats.crc_error_count += 1,
            IrqEvent::RxTimeout { .. } => stats.timeout_count += 1,
            _ => {}
        }

        // Try to send event to queue
        match self.tx.try_send(event) {
            Ok(()) => {
                debug!("IRQ event queued successfully");
                Ok(())
            }
            Err(e) => {
                stats.queue_overflows += 1;
                stats.events_dropped += 1;
                warn!("Failed to queue IRQ event: {e}");
                Err(format!("Queue full: {e}"))
            }
        }
    }

    /// Process IRQ status and queue appropriate events
    ///
    /// This converts raw IRQ status bits into typed events.
    pub async fn process_irq_status(
        &self,
        status: IrqStatus,
        rssi: Option<i16>,
        snr: Option<f32>,
    ) -> Result<(), String> {
        let timestamp = Instant::now();

        // Process each active interrupt bit
        if status.rx_done() {
            self.queue_event(IrqEvent::RxDone {
                timestamp,
                rssi,
                snr,
            })
            .await?;
        }

        if status.tx_done() {
            self.queue_event(IrqEvent::TxDone { timestamp }).await?;
        }

        if status.crc_err() {
            self.queue_event(IrqEvent::CrcError {
                timestamp,
                partial_data: None,  // Could be enhanced to include partial frame
            })
            .await?;
        }

        if status.timeout() {
            self.queue_event(IrqEvent::RxTimeout { timestamp }).await?;
        }

        if status.preamble_detected() {
            self.queue_event(IrqEvent::PreambleDetected { timestamp })
                .await?;
        }

        if status.header_error() {
            self.queue_event(IrqEvent::HeaderError { timestamp }).await?;
        }

        if status.cad_detected() {
            self.queue_event(IrqEvent::CadDetected {
                timestamp,
                channel_rssi: rssi,
            })
            .await?;
        }

        Ok(())
    }

    /// Get the next event from the queue
    ///
    /// This is async and will wait for an event if the queue is empty.
    pub async fn get_next_event(&self) -> Option<IrqEvent> {
        let mut rx = self.rx.lock().await;
        rx.recv().await
    }

    /// Try to get the next event without blocking
    pub async fn try_get_next_event(&self) -> Option<IrqEvent> {
        let mut rx = self.rx.lock().await;
        rx.try_recv().ok()
    }

    /// Process all pending events with a callback
    ///
    /// This drains the queue and processes each event with the provided callback.
    pub async fn process_all_events<F>(&self, mut callback: F) -> Result<usize, String>
    where
        F: FnMut(IrqEvent) -> Result<(), String>,
    {
        let mut processed = 0;
        let start_time = Instant::now();

        while let Some(event) = self.try_get_next_event().await {
            // Check for timeout
            if start_time.elapsed() > self.processing_timeout {
                warn!(
                    "Event processing timeout after {processed} events"
                );
                break;
            }

            // Process event
            let event_start = Instant::now();
            callback(event)?;
            let processing_time = event_start.elapsed();

            // Update statistics
            let mut stats = self.stats.lock().await;
            let processing_us = processing_time.as_micros() as u64;
            stats.avg_processing_time_us =
                (stats.avg_processing_time_us * processed as u64 + processing_us)
                / (processed as u64 + 1);
            stats.max_processing_time_us = stats.max_processing_time_us.max(processing_us);

            processed += 1;
        }

        Ok(processed)
    }

    /// Get current queue statistics
    pub async fn get_stats(&self) -> IrqStats {
        self.stats.lock().await.clone()
    }

    /// Reset queue statistics
    pub async fn reset_stats(&self) {
        *self.stats.lock().await = IrqStats::default();
    }

    /// Get the number of events currently in the queue
    pub fn queue_depth(&self) -> usize {
        // This is an approximation as the actual depth can change
        self.capacity - self.tx.capacity()
    }
}

/// IRQ processor task that handles events from the queue
///
/// This runs as a separate async task to process IRQ events without blocking
/// the main interrupt handler.
pub async fn irq_processor_task<F>(
    queue: Arc<IrqEventQueue>,
    mut handler: F,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) where
    F: FnMut(IrqEvent) -> Result<(), String> + Send + 'static,
{
    debug!("IRQ processor task started");

    loop {
        tokio::select! {
            // Check for shutdown signal
            _ = &mut shutdown_rx => {
                debug!("IRQ processor task received shutdown signal");
                break;
            }

            // Process next event
            Some(event) = queue.get_next_event() => {
                let priority = event.priority();
                let event_type = match &event {
                    IrqEvent::RxDone { .. } => "RxDone",
                    IrqEvent::TxDone { .. } => "TxDone",
                    IrqEvent::CrcError { .. } => "CrcError",
                    IrqEvent::RxTimeout { .. } => "RxTimeout",
                    IrqEvent::PreambleDetected { .. } => "PreambleDetected",
                    IrqEvent::HeaderError { .. } => "HeaderError",
                    IrqEvent::CadDetected { .. } => "CadDetected",
                    IrqEvent::Error { .. } => "Error",
                };

                debug!("Processing {event_type} event with priority {priority:?}");

                if let Err(e) = handler(event) {
                    error!("Error processing IRQ event: {e}");
                }
            }

            // Small delay to prevent busy waiting
            _ = tokio::time::sleep(Duration::from_millis(1)) => {
                // Continue processing
            }
        }
    }

    debug!("IRQ processor task stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_queue_basic() {
        let queue = IrqEventQueue::new(10);

        // Queue an event
        let event = IrqEvent::RxDone {
            timestamp: Instant::now(),
            rssi: Some(-80),
            snr: Some(10.0),
        };

        queue.queue_event(event.clone()).await.unwrap();

        // Get the event back
        let retrieved = queue.get_next_event().await.unwrap();
        match retrieved {
            IrqEvent::RxDone { rssi, snr, .. } => {
                assert_eq!(rssi, Some(-80));
                assert_eq!(snr, Some(10.0));
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_event_priority() {
        let event = IrqEvent::CrcError {
            timestamp: Instant::now(),
            partial_data: None,
        };
        assert_eq!(event.priority(), EventPriority::Critical);

        let event = IrqEvent::RxDone {
            timestamp: Instant::now(),
            rssi: None,
            snr: None,
        };
        assert_eq!(event.priority(), EventPriority::High);
    }

    #[tokio::test]
    async fn test_process_irq_status() {
        let queue = IrqEventQueue::new(10);

        // Create a status with multiple bits set
        let status = IrqStatus::from(
            IrqMaskBit::RxDone as u16 | IrqMaskBit::CrcErr as u16
        );

        queue.process_irq_status(status, Some(-85), Some(12.0)).await.unwrap();

        // Should have queued 2 events
        let stats = queue.get_stats().await;
        assert_eq!(stats.rx_done_count, 1);
        assert_eq!(stats.crc_error_count, 1);
        assert_eq!(stats.total_events, 2);
    }
}