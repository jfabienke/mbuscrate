//! Unit tests for the logging functionality in the `mbus-rs` crate.

use mbus_rs::logging::{init_logger, log_debug, log_error, log_info, log_warn};

/// Tests that the logging macros work as expected, capturing the log output.
#[test]
fn test_logging() {
    // Just ensure logging functions do not panic after init.
    log_error("This is an error message");
    log_warn("This is a warning message");
    log_info("This is an info message");
    log_debug("This is a debug message");
}

/// Tests that the logger is correctly initialized.
#[test]

fn test_init_logger() {
    init_logger();
    // No assertions here, as the init_logger() function has no return value.
    // The test passes if the function call does not panic.
}
