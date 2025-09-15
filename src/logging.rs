use log::{debug, error, info, log_enabled, warn, Level};

// RTT + defmt logging modules
#[cfg(feature = "rtt-logging")]
pub mod rtt_init;
#[cfg(feature = "rtt-logging")]
pub mod defmt_writer;

// Re-export RTT functionality when available
#[cfg(feature = "rtt-logging")]
pub use rtt_init::{init_rtt_logging, is_rtt_available, get_rtt_stats, RttStats};
#[cfg(feature = "rtt-logging")]
pub use defmt_writer::{init_defmt_tracing, structured, encoders};

/// Initializes the logger with the `env_logger` crate.
///
/// For Pi deployments with RTT, use `init_enhanced_logging()` instead.
pub fn init_logger() {
    env_logger::init();
}

/// Enhanced logging initialization with RTT + defmt support
///
/// This function initializes the most appropriate logging backend:
/// - Pi 4/5 with RTT feature: Hardware-accelerated RTT + defmt logging
/// - Other platforms: Standard env_logger with enhanced formatting
///
/// # Examples
/// ```rust,no_run
/// use mbus_rs::logging::init_enhanced_logging;
///
/// // Initialize enhanced logging at startup
/// init_enhanced_logging().expect("Failed to initialize logging");
///
/// // Use standard tracing macros
/// tracing::info!("System initialized");
/// ```
pub fn init_enhanced_logging() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(feature = "rtt-logging")]
    {
        // Try RTT first, fallback to standard logging
        if let Err(e) = rtt_init::init_rtt_logging() {
            log::warn!("RTT initialization failed: {}, using standard logging", e);
            init_logger();
        } else {
            // Initialize defmt tracing subscriber
            defmt_writer::init_defmt_tracing()?;
        }
    }

    #[cfg(not(feature = "rtt-logging"))]
    {
        init_logger();
    }

    Ok(())
}

/// Logs an error message.
pub fn log_error(message: &str) {
    if log_enabled!(Level::Error) {
        error!("{message}");
    }
}

/// Logs a warning message.
pub fn log_warn(message: &str) {
    if log_enabled!(Level::Warn) {
        warn!("{message}");
    }
}

/// Logs an informational message.
pub fn log_info(message: &str) {
    if log_enabled!(Level::Info) {
        info!("{message}");
    }
}

/// Logs a debug message.
pub fn log_debug(message: &str) {
    if log_enabled!(Level::Debug) {
        debug!("{message}");
    }
}
