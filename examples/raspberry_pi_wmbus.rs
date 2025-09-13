//! # Raspberry Pi wM-Bus Example
//!
//! This example demonstrates how to use the SX126x radio driver on Raspberry Pi 4/5
//! for wireless M-Bus communication. It shows both basic receiver and transmitter
//! configurations.
//!
//! ## Hardware Setup
//!
//! Connect your SX126x module to the Raspberry Pi as follows:
//!
//! ```text
//! Raspberry Pi 4/5    SX126x Module
//! ================    =============
//! Pin 19 (GPIO 10)    MOSI
//! Pin 21 (GPIO 9)     MISO
//! Pin 23 (GPIO 11)    SCLK
//! Pin 24 (GPIO 8)     NSS
//! Pin 22 (GPIO 25)    BUSY
//! Pin 18 (GPIO 24)    DIO1
//! Pin 16 (GPIO 23)    DIO2 (optional)
//! Pin 15 (GPIO 22)    NRESET (optional)
//! Pin 1  (3.3V)       VCC
//! Pin 6  (GND)        GND
//! ```
//!
//! ## Prerequisites
//!
//! 1. Enable SPI in `/boot/config.txt`:
//!    ```
//!    dtparam=spi=on
//!    ```
//!
//! 2. Add dependencies to `Cargo.toml`:
//!    ```toml
//!    [dependencies]
//!    mbus-rs = { path = "." }
//!    rppal = "0.14"
//!    tokio = { version = "1.0", features = ["full"] }
//!    env_logger = "0.10"
//!    log = "0.4"
//!    ```
//!
//! 3. Run with appropriate permissions:
//!    ```bash
//!    sudo ./target/release/raspberry_pi_wmbus
//!    ```
//!
//! 4. Run with: `cargo run --example raspberry_pi_wmbus --features raspberry-pi`

#[cfg(not(feature = "raspberry-pi"))]
fn main() {
    eprintln!("This example requires the 'raspberry-pi' feature.");
    eprintln!("Run with: cargo run --example raspberry_pi_wmbus --features raspberry-pi");
}

#[cfg(feature = "raspberry-pi")]
use log::{error, info, warn};
#[cfg(feature = "raspberry-pi")]
use mbus_rs::wmbus::radio::{
    driver::Sx126xDriver,
    hal::{GpioPins, RaspberryPiHal},
};
#[cfg(feature = "raspberry-pi")]
use std::time::Duration;

/// Default GPIO pin configuration for SX126x on Raspberry Pi
#[cfg(feature = "raspberry-pi")]
const DEFAULT_GPIO_PINS: GpioPins = GpioPins {
    busy: 25,        // Pin 22
    dio1: 24,        // Pin 18
    dio2: Some(23),  // Pin 16 (optional)
    reset: Some(22), // Pin 15 (optional)
};

/// EU wM-Bus S-mode frequency (868.95 MHz)
#[cfg(feature = "raspberry-pi")]
const WMBUS_EU_FREQ: u32 = 868_950_000;

/// wM-Bus data rate (100 kbps)
#[cfg(feature = "raspberry-pi")]
const WMBUS_BITRATE: u32 = 100_000;

/// Crystal frequency for SX126x (32 MHz typical)
#[cfg(feature = "raspberry-pi")]
const CRYSTAL_FREQ: u32 = 32_000_000;

#[cfg(feature = "raspberry-pi")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting Raspberry Pi wM-Bus example");

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("receive");

    match mode {
        "receive" | "rx" => run_receiver().await?,
        "transmit" | "tx" => run_transmitter().await?,
        "test" => run_hardware_test().await?,
        _ => {
            eprintln!("Usage: {} [receive|transmit|test]", args[0]);
            eprintln!("  receive  - Listen for wM-Bus frames (default)");
            eprintln!("  transmit - Send test wM-Bus frames");
            eprintln!("  test     - Test hardware connectivity");
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Run as wM-Bus receiver
#[cfg(feature = "raspberry-pi")]
async fn run_receiver() -> Result<(), Box<dyn std::error::Error>> {
    info!("Initializing wM-Bus receiver");

    // Create HAL instance
    let hal = RaspberryPiHal::new(0, &DEFAULT_GPIO_PINS)?;
    info!("HAL initialized: {}", hal.get_info());

    // Create radio driver
    let mut driver = Sx126xDriver::new(hal, CRYSTAL_FREQ);

    // Configure for wM-Bus operation
    driver.configure_for_wmbus(WMBUS_EU_FREQ, WMBUS_BITRATE)?;
    info!("Radio configured for wM-Bus EU S-mode (868.95 MHz, 100 kbps)");

    // Start continuous reception
    driver.set_rx_continuous()?;
    info!("Started continuous reception - listening for wM-Bus frames...");

    let mut packet_count = 0;

    // Main reception loop
    loop {
        // Process any pending interrupts
        match driver.process_irqs() {
            Ok(Some(payload)) => {
                packet_count += 1;
                info!(
                    "📡 Received wM-Bus frame #{}: {} bytes",
                    packet_count,
                    payload.len()
                );

                // Display frame data
                print_frame_data(&payload);

                // Optionally parse as M-Bus frame
                if let Err(e) = parse_wmbus_frame(&payload) {
                    warn!("Frame parsing failed: {}", e);
                }
            }
            Ok(None) => {
                // No data received, continue polling
            }
            Err(e) => {
                error!("Radio error: {}", e);
                // Attempt to recover by restarting reception
                if let Err(restart_err) = driver.set_rx_continuous() {
                    error!("Failed to restart reception: {}", restart_err);
                    break;
                }
            }
        }

        // Small delay to prevent excessive CPU usage
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    Ok(())
}

/// Run as wM-Bus transmitter
#[cfg(feature = "raspberry-pi")]
async fn run_transmitter() -> Result<(), Box<dyn std::error::Error>> {
    info!("Initializing wM-Bus transmitter");

    // Create HAL instance
    let hal = RaspberryPiHal::new(0, &DEFAULT_GPIO_PINS)?;
    info!("HAL initialized: {}", hal.get_info());

    // Create radio driver
    let mut driver = Sx126xDriver::new(hal, CRYSTAL_FREQ);

    // Configure for wM-Bus operation
    driver.configure_for_wmbus(WMBUS_EU_FREQ, WMBUS_BITRATE)?;
    info!("Radio configured for wM-Bus EU S-mode (868.95 MHz, 100 kbps)");

    let mut sequence = 0u16;

    // Transmission loop
    loop {
        // Generate test wM-Bus frame
        let test_frame = generate_test_frame(sequence);

        // Load frame into radio buffer
        driver.write_buffer(0, &test_frame)?;

        // Start transmission
        info!(
            "📤 Transmitting test frame #{} ({} bytes)",
            sequence,
            test_frame.len()
        );
        driver.set_tx(5000)?; // 5 second timeout

        // Wait for transmission to complete
        let start = std::time::Instant::now();
        loop {
            match driver.process_irqs() {
                Ok(Some(_)) => {
                    // Unexpected received data during TX
                    warn!("Unexpected RX data during transmission");
                }
                Ok(None) => {
                    // Check for TX completion by reading IRQ status
                    let irq_status = driver.get_irq_status()?;
                    if irq_status.tx_done() {
                        info!("✅ Transmission completed in {:?}", start.elapsed());
                        driver.clear_irq_status(0xFFFF)?; // Clear all IRQs
                        break;
                    }
                    if irq_status.timeout() {
                        error!("❌ Transmission timeout");
                        driver.clear_irq_status(0xFFFF)?;
                        break;
                    }
                }
                Err(e) => {
                    error!("Radio error during transmission: {}", e);
                    break;
                }
            }

            // Check for overall timeout
            if start.elapsed() > Duration::from_secs(10) {
                error!("❌ Transmission timeout (10s)");
                break;
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        sequence = sequence.wrapping_add(1);

        // Wait before next transmission (regulatory compliance)
        info!("Waiting 10 seconds before next transmission...");
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

/// Test hardware connectivity
#[cfg(feature = "raspberry-pi")]
async fn run_hardware_test() -> Result<(), Box<dyn std::error::Error>> {
    info!("Running hardware connectivity test");

    // Create HAL instance
    let mut hal = RaspberryPiHal::new(0, &DEFAULT_GPIO_PINS)?;
    info!("HAL initialized: {}", hal.get_info());

    // Test hardware reset (if available)
    if let Ok(()) = hal.reset_radio() {
        info!("✅ Hardware reset successful");
    } else {
        warn!("⚠️  Hardware reset not available (no reset pin configured)");
    }

    // Create radio driver
    let mut driver = Sx126xDriver::new(hal, CRYSTAL_FREQ);

    // Test basic communication
    info!("Testing radio communication...");

    // Try to read radio version/status
    match driver.get_irq_status() {
        Ok(status) => {
            info!(
                "✅ SPI communication working - IRQ status: 0x{:04X}",
                status.raw()
            );
        }
        Err(e) => {
            error!("❌ SPI communication failed: {}", e);
            return Err(e.into());
        }
    }

    // Test GPIO pins
    info!("Testing GPIO pins...");

    // Check DIO1 pin
    match driver.gpio_read(1) {
        Ok(state) => info!("✅ DIO1 pin read: {}", if state { "HIGH" } else { "LOW" }),
        Err(e) => error!("❌ DIO1 pin read failed: {}", e),
    }

    // Check DIO2 pin (if configured)
    match driver.gpio_read(2) {
        Ok(state) => info!("✅ DIO2 pin read: {}", if state { "HIGH" } else { "LOW" }),
        Err(_) => info!("ℹ️  DIO2 pin not configured or read failed"),
    }

    // Test configuration
    info!("Testing radio configuration...");
    match driver.configure_for_wmbus(WMBUS_EU_FREQ, WMBUS_BITRATE) {
        Ok(()) => info!("✅ wM-Bus configuration successful"),
        Err(e) => {
            error!("❌ wM-Bus configuration failed: {}", e);
            return Err(e.into());
        }
    }

    // Test calibration
    info!("Testing radio calibration...");
    // Note: calibration is included in configure_for_wmbus()
    info!("✅ Radio calibration completed");

    // Brief receive test
    info!("Testing receive mode...");
    match driver.set_rx(1000) {
        // 1 second receive test
        Ok(()) => {
            info!("✅ Receive mode activated");
            tokio::time::sleep(Duration::from_millis(1100)).await; // Wait for timeout

            // Check for any received data or timeout
            match driver.process_irqs() {
                Ok(Some(data)) => info!("📡 Received {} bytes during test", data.len()),
                Ok(None) => info!("✅ Receive test completed (no data received)"),
                Err(e) => warn!("⚠️  Receive test error: {}", e),
            }
        }
        Err(e) => {
            error!("❌ Receive mode failed: {}", e);
            return Err(e.into());
        }
    }

    info!("🎉 Hardware test completed successfully!");
    info!("Your Raspberry Pi is ready for wM-Bus communication.");

    Ok(())
}

/// Print received frame data in hex format
#[cfg(feature = "raspberry-pi")]
fn print_frame_data(data: &[u8]) {
    const BYTES_PER_LINE: usize = 16;

    println!("Frame data ({} bytes):", data.len());
    for (i, chunk) in data.chunks(BYTES_PER_LINE).enumerate() {
        print!("  {:04X}: ", i * BYTES_PER_LINE);

        // Print hex bytes
        for (j, byte) in chunk.iter().enumerate() {
            print!("{:02X} ", byte);
            if j == 7 {
                print!(" ");
            } // Add space in middle
        }

        // Pad if last line is incomplete
        if chunk.len() < BYTES_PER_LINE {
            for j in chunk.len()..BYTES_PER_LINE {
                print!("   ");
                if j == 7 {
                    print!(" ");
                }
            }
        }

        // Print ASCII representation
        print!(" |");
        for byte in chunk {
            if byte.is_ascii_graphic() || *byte == b' ' {
                print!("{}", *byte as char);
            } else {
                print!(".");
            }
        }
        println!("|");
    }
    println!();
}

/// Parse wM-Bus frame (basic parsing example)
#[cfg(feature = "raspberry-pi")]
fn parse_wmbus_frame(data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    if data.len() < 10 {
        return Err("Frame too short for wM-Bus".into());
    }

    // Basic wM-Bus frame structure check
    let length = data[0];
    if length as usize != data.len() - 1 {
        return Err(format!(
            "Length mismatch: declared {}, actual {}",
            length,
            data.len() - 1
        )
        .into());
    }

    // Extract basic fields (simplified)
    let c_field = data[1];
    let m_field = u16::from_le_bytes([data[2], data[3]]);
    let a_field = [data[4], data[5], data[6], data[7], data[8], data[9]];

    println!("wM-Bus Frame Analysis:");
    println!("  Length: {} bytes", length);
    println!("  C-Field: 0x{:02X}", c_field);
    println!("  M-Field: 0x{:04X}", m_field);
    println!(
        "  A-Field: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        a_field[0], a_field[1], a_field[2], a_field[3], a_field[4], a_field[5]
    );

    // Decode manufacturer
    if m_field != 0 {
        let man_id = decode_manufacturer_id(m_field);
        println!("  Manufacturer: {}", man_id);
    }

    println!();
    Ok(())
}

/// Decode 2-byte manufacturer ID to 3-letter string
#[cfg(feature = "raspberry-pi")]
fn decode_manufacturer_id(m_field: u16) -> String {
    if m_field == 0 {
        return "Unknown".to_string();
    }

    // M-Bus manufacturer ID encoding: 5 bits per character, A=1, B=2, ..., Z=26
    let char1 = ((m_field >> 10) & 0x1F) as u8;
    let char2 = ((m_field >> 5) & 0x1F) as u8;
    let char3 = (m_field & 0x1F) as u8;

    let mut result = String::new();
    for &char_val in &[char1, char2, char3] {
        if char_val >= 1 && char_val <= 26 {
            result.push((b'A' + char_val - 1) as char);
        } else {
            result.push('?');
        }
    }

    result
}

/// Generate a test wM-Bus frame
#[cfg(feature = "raspberry-pi")]
fn generate_test_frame(sequence: u16) -> Vec<u8> {
    let mut frame = Vec::new();

    // Length (will be set at the end)
    frame.push(0x00);

    // C-Field (SND_NR = Send No Reply)
    frame.push(0x44);

    // M-Field (Manufacturer ID) - "TST" = Test
    let m_field = encode_manufacturer_id("TST");
    frame.extend_from_slice(&m_field.to_le_bytes());

    // A-Field (Address) - use sequence number
    frame.extend_from_slice(&[
        sequence as u8,
        (sequence >> 8) as u8,
        0x00,
        0x00,
        0x00,
        0x00,
    ]);

    // CI-Field (Control Information) - Simple test data
    frame.push(0x72); // Variable data structure

    // Test payload
    let payload = format!("Test frame #{:04}", sequence);
    frame.extend_from_slice(payload.as_bytes());

    // Update length field (total frame length - 1)
    frame[0] = (frame.len() - 1) as u8;

    frame
}

/// Encode 3-letter manufacturer ID to 2-byte M-Field
#[cfg(feature = "raspberry-pi")]
fn encode_manufacturer_id(id: &str) -> u16 {
    if id.len() != 3 {
        return 0;
    }

    let mut result = 0u16;
    for (i, ch) in id.chars().take(3).enumerate() {
        let upper = ch.to_ascii_uppercase();
        if upper >= 'A' && upper <= 'Z' {
            let char_value = ((upper as u16) - ('A' as u16) + 1); // A=1, B=2, ..., Z=26
            result |= char_value << (10 - i * 5);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manufacturer_encoding() {
        assert_eq!(encode_manufacturer_id("TST"), 20 << 10 | 19 << 5 | 20); // T=20, S=19, T=20
        assert_eq!(decode_manufacturer_id(encode_manufacturer_id("TST")), "TST");
        assert_eq!(decode_manufacturer_id(encode_manufacturer_id("ABC")), "ABC");
    }

    #[test]
    fn test_frame_generation() {
        let frame = generate_test_frame(123);
        assert!(frame.len() > 10);
        assert_eq!(frame[0] as usize, frame.len() - 1); // Length field check
        assert_eq!(frame[1], 0x44); // C-Field check
    }
}
