//! defmt timestamp implementation for Pi RTT logging
//!
//! This module provides a high-resolution timestamp for defmt logs
//! using the ARM architectural timer on Raspberry Pi.

#[cfg(feature = "rtt-logging")]
use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "rtt-logging")]
static BOOT_TIME_US: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "rtt-logging")]
use cortex_a::registers::{CNTFRQ_EL0, CNTPCT_EL0};

#[cfg(feature = "rtt-logging")]
use tock_registers::interfaces::Readable;

/// Initialize defmt timestamp using Pi's system timer
#[cfg(feature = "rtt-logging")]
pub fn init_timestamp() {
    let boot_time = get_system_time_us();
    BOOT_TIME_US.store(boot_time, Ordering::Relaxed);
}

/// Get high-resolution timestamp in microseconds since boot
#[cfg(feature = "rtt-logging")]
fn get_system_time_us() -> u64 {
    #[cfg(target_arch = "aarch64")]
    {
        // Use ARM architectural timer for high-precision timestamps
        // This is available on Pi 4/5 ARM Cortex-A72/A76 cores
        use cortex_a::asm::barrier;

        // Frequency of the timer (should be 54 MHz on Pi)
        let freq_hz = CNTFRQ_EL0.get();

        // Current counter value
        barrier::isb(barrier::SY);
        let counter = CNTPCT_EL0.get();

        // Convert to microseconds
        (counter * 1_000_000) / freq_hz
    }

    #[cfg(not(target_arch = "aarch64"))]
    {
        // Fallback for non-ARM platforms using system time
        use std::time::{SystemTime, UNIX_EPOCH};

        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }
}

/// defmt timestamp implementation (called by defmt runtime)
#[cfg(feature = "rtt-logging")]
#[no_mangle]
pub extern "C" fn __defmt_timestamp() -> u64 {
    let current_time = get_system_time_us();
    let boot_time = BOOT_TIME_US.load(Ordering::Relaxed);

    // Return microseconds since boot initialization
    current_time.saturating_sub(boot_time)
}

/// Critical section implementation for RTT (required by defmt-rtt)
#[cfg(feature = "rtt-logging")]
mod critical_section_impl {
    struct StdCriticalSection;
    critical_section::set_impl!(StdCriticalSection);

    unsafe impl critical_section::Impl for StdCriticalSection {
        unsafe fn acquire() -> critical_section::RawRestoreState {
            // For std environment, we use a simple approach
            // In a real embedded system, this would disable interrupts
            std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
            false
        }

        unsafe fn release(_restore_state: critical_section::RawRestoreState) {
            std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
        }
    }
}