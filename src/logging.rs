use log::{debug, error, info, log_enabled, warn, Level};

/// Initializes the logger with the `env_logger` crate.
pub fn init_logger() {
    env_logger::init();
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
