//! # SX126x Radio Calibration
//!
//! This module provides calibration functionality for the SX126x radio transceiver.
//! Calibration is essential for optimal radio performance and should be performed
//! after power-up and before critical operations.
//!
//! ## Calibration Types
//!
//! The SX126x supports several internal calibration routines:
//!
//! - **RC64K**: 64kHz RC oscillator calibration
//! - **RC13M**: 13MHz RC oscillator calibration  
//! - **PLL**: Phase-locked loop calibration
//! - **ADC_PULSE**: ADC pulse calibration
//! - **ADC_BULK_N**: ADC bulk N calibration
//! - **ADC_BULK_P**: ADC bulk P calibration
//! - **IMAGE**: Image frequency calibration
//!
//! ## When to Calibrate
//!
//! Calibration should be performed:
//! - After device power-up or reset
//! - When changing operating frequency significantly
//! - After temperature changes (optional, for high-precision applications)
//! - Before critical transmissions (optional)
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use crate::wmbus::radio::calib::{calibrate_radio, CalibParams};
//!
//! // Perform full calibration after power-up
//! calibrate_radio(&mut hal, CalibParams::ALL)?;
//!
//! // Perform only PLL calibration after frequency change
//! calibrate_radio(&mut hal, CalibParams::PLL)?;
//!
//! // Combine specific calibrations
//! let custom_calib = CalibParams::RC13M | CalibParams::PLL | CalibParams::IMAGE;
//! calibrate_radio(&mut hal, custom_calib)?;
//! ```
//!
//! ## Calibration Time
//!
//! Different calibrations have different completion times:
//! - RC oscillators: ~1ms
//! - PLL: ~3ms  
//! - ADC calibrations: ~5ms
//! - Image calibration: ~2ms
//! - Full calibration (ALL): ~15ms total

use bitflags::bitflags;
use thiserror::Error;

/// Errors that can occur during radio calibration
#[derive(Error, Debug)]
pub enum CalibError {
    /// Hardware abstraction layer communication error
    #[error("HAL error during calibration")]
    Hal,
}

bitflags! {
    /// SX126x calibration parameter flags
    ///
    /// These flags can be combined using bitwise OR operations to specify
    /// which calibration routines should be executed. Each calibration
    /// targets a specific internal circuit or function.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use crate::wmbus::radio::calib::CalibParams;
    ///
    /// // Calibrate only the PLL
    /// let pll_only = CalibParams::PLL;
    ///
    /// // Calibrate RC oscillators and PLL
    /// let osc_and_pll = CalibParams::RC64K | CalibParams::RC13M | CalibParams::PLL;
    ///
    /// // Full calibration (recommended after power-up)
    /// let full_calib = CalibParams::ALL;
    /// ```
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct CalibParams: u8 {
        /// 64kHz RC oscillator calibration
        /// 
        /// Calibrates the low-frequency RC oscillator used for timing functions.
        /// Duration: ~1ms
        const RC64K      = 0b0000_0001;
        
        /// 13MHz RC oscillator calibration
        /// 
        /// Calibrates the high-frequency RC oscillator used as system clock.
        /// Duration: ~1ms
        const RC13M      = 0b0000_0010;
        
        /// PLL (Phase-Locked Loop) calibration
        /// 
        /// Calibrates the PLL used for RF frequency synthesis.
        /// Critical for frequency accuracy. Should be run after frequency changes.
        /// Duration: ~3ms
        const PLL        = 0b0000_0100;
        
        /// ADC pulse calibration
        /// 
        /// Calibrates the analog-to-digital converter pulse response.
        /// Duration: ~5ms
        const ADC_PULSE  = 0b0000_1000;
        
        /// ADC bulk N-channel calibration
        /// 
        /// Calibrates the ADC N-channel bulk characteristics.
        /// Duration: ~5ms
        const ADC_BULK_N = 0b0001_0000;
        
        /// ADC bulk P-channel calibration
        /// 
        /// Calibrates the ADC P-channel bulk characteristics.
        /// Duration: ~5ms
        const ADC_BULK_P = 0b0010_0000;
        
        /// Image frequency calibration
        /// 
        /// Calibrates image frequency rejection in the receiver.
        /// Improves receiver selectivity and sensitivity.
        /// Duration: ~2ms
        const IMAGE      = 0b0100_0000;
        
        /// All calibration routines
        /// 
        /// Performs complete calibration of all internal circuits.
        /// Recommended after power-up or reset.
        /// Total duration: ~15ms
        const ALL        = 0b0111_1111;
    }
}

/// Perform radio calibration with specified parameters
///
/// This function initiates calibration of the specified internal circuits.
/// The calibration runs asynchronously in the radio hardware and will
/// complete automatically. The function returns immediately after starting
/// the calibration process.
///
/// # Arguments
///
/// * `hal` - Hardware abstraction layer for radio communication
/// * `params` - Calibration parameters specifying which circuits to calibrate
///
/// # Returns
///
/// * `Ok(())` - Calibration command sent successfully
/// * `Err(CalibError::Hal)` - Communication error with radio
///
/// # Important Notes
///
/// - This function starts calibration but does not wait for completion
/// - Calibration runs in the background and typically takes 1-15ms
/// - Do not send other commands until calibration completes
/// - The radio's BUSY pin can be monitored to detect completion
///
/// # Examples
///
/// ```rust,no_run
/// use crate::wmbus::radio::calib::{calibrate_radio, CalibParams};
///
/// // Full calibration (recommended after power-up)
/// calibrate_radio(&mut hal, CalibParams::ALL)?;
///
/// // Quick PLL calibration after frequency change  
/// calibrate_radio(&mut hal, CalibParams::PLL)?;
///
/// // Custom calibration set
/// let custom = CalibParams::RC13M | CalibParams::PLL | CalibParams::IMAGE;
/// calibrate_radio(&mut hal, custom)?;
/// ```
///
/// # Timing Considerations
///
/// After calling this function, allow sufficient time for calibration:
/// 
/// | Calibration Type | Typical Duration |
/// |-----------------|------------------|
/// | RC64K           | ~1ms            |
/// | RC13M           | ~1ms            |
/// | PLL             | ~3ms            |
/// | ADC_PULSE       | ~5ms            |
/// | ADC_BULK_N      | ~5ms            |
/// | ADC_BULK_P      | ~5ms            |
/// | IMAGE           | ~2ms            |
/// | ALL             | ~15ms           |
pub fn calibrate_radio(hal: &mut impl crate::wmbus::radio::hal::Hal, params: CalibParams) -> Result<(), CalibError> {
    let buf = [params.bits()];
    hal.write_command(0x89, &buf) // Calibrate command
        .map_err(|_| CalibError::Hal)?;
    Ok(())
}

/// Wait for calibration to complete by monitoring the BUSY pin
///
/// This function polls the radio's BUSY pin to determine when calibration
/// has finished. The BUSY pin goes low when calibration is complete.
///
/// # Arguments
///
/// * `hal` - Hardware abstraction layer for GPIO access
/// * `busy_pin` - GPIO pin number for the radio's BUSY signal
/// * `timeout_ms` - Maximum time to wait in milliseconds
///
/// # Returns
///
/// * `Ok(())` - Calibration completed successfully
/// * `Err(CalibError::Hal)` - GPIO read error or timeout
///
/// # Examples
///
/// ```rust,no_run
/// use crate::wmbus::radio::calib::{calibrate_radio, wait_for_calibration, CalibParams};
///
/// // Start calibration
/// calibrate_radio(&mut hal, CalibParams::ALL)?;
///
/// // Wait for completion (with 20ms timeout)
/// wait_for_calibration(&mut hal, 1, 20)?; // Pin 1 is BUSY
/// ```
pub fn wait_for_calibration(
    hal: &mut impl crate::wmbus::radio::hal::Hal,
    busy_pin: u8,
    timeout_ms: u32,
) -> Result<(), CalibError> {
    let start_time = std::time::Instant::now();
    let timeout_duration = std::time::Duration::from_millis(timeout_ms as u64);
    
    loop {
        // Check if BUSY pin is low (calibration complete)
        match hal.gpio_read(busy_pin) {
            Ok(false) => return Ok(()), // BUSY is low, calibration done
            Ok(true) => {               // BUSY is high, still calibrating
                if start_time.elapsed() > timeout_duration {
                    return Err(CalibError::Hal); // Timeout
                }
                // Small delay before next check
                std::thread::sleep(std::time::Duration::from_micros(100));
            }
            Err(_) => return Err(CalibError::Hal), // GPIO read error
        }
    }
}

/// Perform calibration and wait for completion
///
/// This convenience function combines calibration initiation with completion waiting.
/// It's the recommended approach for most applications.
///
/// # Arguments
///
/// * `hal` - Hardware abstraction layer
/// * `params` - Calibration parameters
/// * `busy_pin` - GPIO pin number for BUSY signal
///
/// # Returns
///
/// * `Ok(())` - Calibration completed successfully
/// * `Err(CalibError)` - Communication error or timeout
///
/// # Examples
///
/// ```rust,no_run
/// use crate::wmbus::radio::calib::{calibrate_and_wait, CalibParams};
///
/// // Perform full calibration and wait for completion
/// calibrate_and_wait(&mut hal, CalibParams::ALL, 1)?;
/// ```
pub fn calibrate_and_wait(
    hal: &mut impl crate::wmbus::radio::hal::Hal,
    params: CalibParams,
    busy_pin: u8,
) -> Result<(), CalibError> {
    calibrate_radio(hal, params)?;
    
    // Calculate timeout based on calibration type
    let timeout_ms = match params {
        params if params.contains(CalibParams::ALL) => 20,        // Full calibration
        params if params.contains(CalibParams::ADC_PULSE) || 
                 params.contains(CalibParams::ADC_BULK_N) ||
                 params.contains(CalibParams::ADC_BULK_P) => 10,     // ADC calibrations
        params if params.contains(CalibParams::PLL) => 5,         // PLL calibration
        _ => 3,                                                   // RC and IMAGE calibrations
    };
    
    wait_for_calibration(hal, busy_pin, timeout_ms)
}
