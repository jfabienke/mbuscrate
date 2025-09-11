//! # Raspberry Pi Quick Start Example
//!
//! A minimal example showing how to get started with SX126x radio on Raspberry Pi.
//! This example demonstrates the simplest possible setup for wM-Bus reception.

use mbus_rs::wmbus::radio::{
    driver::Sx126xDriver,
    hal::{RaspberryPiHalBuilder, GpioPins},
};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Enable logging
    env_logger::init();

    println!("🚀 Raspberry Pi SX126x Quick Start");

    // Method 1: Use default pin configuration
    let hal = RaspberryPiHalBuilder::new().build()?;
    
    /* Alternative configurations:
    
    // Method 2: Custom pin configuration
    let hal = RaspberryPiHalBuilder::new()
        .spi_bus(0)           // Use primary SPI bus
        .spi_speed(8_000_000) // 8 MHz SPI clock
        .busy_pin(25)         // GPIO 25 for BUSY
        .dio1_pin(24)         // GPIO 24 for DIO1
        .dio2_pin(23)         // GPIO 23 for DIO2
        .reset_pin(22)        // GPIO 22 for RESET
        .build()?;
    
    // Method 3: Minimal configuration (no optional pins)
    let hal = RaspberryPiHalBuilder::new()
        .busy_pin(25)
        .dio1_pin(24)
        .no_dio2()            // Don't use DIO2
        .no_reset()           // Don't use RESET pin
        .build()?;
    */

    println!("✅ HAL initialized: {}", hal.get_info());

    // Create radio driver with 32MHz crystal
    let mut driver = Sx126xDriver::new(hal, 32_000_000);

    // One-line wM-Bus configuration
    driver.configure_for_wmbus(868_950_000, 100_000)?; // EU 868.95 MHz, 100 kbps
    println!("✅ Radio configured for wM-Bus");

    // Start receiving
    driver.set_rx_continuous()?;
    println!("📡 Listening for wM-Bus frames... (Press Ctrl+C to stop)");

    let mut frame_count = 0;

    // Simple receive loop
    loop {
        if let Some(payload) = driver.process_irqs()? {
            frame_count += 1;
            println!("📦 Frame #{}: {} bytes - {:02X?}", 
                     frame_count, 
                     payload.len(),
                     &payload[..payload.len().min(16)]); // Show first 16 bytes
        }
        
        std::thread::sleep(Duration::from_millis(100));
    }
}