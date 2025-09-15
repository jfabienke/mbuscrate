//! RTT + defmt Logging Demo for Raspberry Pi
//!
//! This example demonstrates the RTT + defmt logging implementation for
//! high-performance structured logging on Raspberry Pi 4/5.
//!
//! Usage:
//!   cargo run --example rtt_logging_demo --features rtt-logging
//!
//! Monitor with probe-rs:
//!   probe-rs rtt --chip BCM2711 --defmt

use mbus_rs::logging::{init_enhanced_logging, is_rtt_available, get_rtt_stats};
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== RTT + defmt Logging Demo ===");

    // Initialize enhanced logging with RTT support
    init_enhanced_logging()?;

    // Check RTT availability
    let rtt_available = is_rtt_available();
    println!("RTT Available: {}", rtt_available);

    #[cfg(feature = "rtt-logging")]
    {
        if rtt_available {
            let stats = get_rtt_stats();
            println!("RTT Stats: Platform: {}, Channels: {}, SWO: {} Hz",
                     stats.platform, stats.channels_active, stats.swo_baud);
        }

        println!("Starting structured logging demo...");
        println!("Monitor with: probe-rs rtt --chip BCM2711 --defmt");
        println!("Press Ctrl+C to stop");

        // Demo different types of structured logging
        demo_irq_logging().await;
        demo_lora_logging().await;
        demo_crypto_logging().await;
        demo_performance_test().await;
    }

    #[cfg(not(feature = "rtt-logging"))]
    {
        println!("RTT logging feature not enabled. Enable with --features rtt-logging");
    }

    Ok(())
}

#[cfg(feature = "rtt-logging")]
async fn demo_irq_logging() {
    use mbus_rs::logging::structured;

    println!("\n--- IRQ Event Logging Demo ---");

    // Simulate SX1262 IRQ events
    let irq_events = vec![
        (0x01, 2500, 26),   // TX_DONE on GPIO 26
        (0x02, 3200, 26),   // RX_DONE on GPIO 26
        (0x04, 50000, 26),  // RX_TIMEOUT on GPIO 26
        (0x08, 15000, 26),  // CAD_DONE on GPIO 26
    ];

    for (mask, latency_ns, pin) in irq_events {
        println!("IRQ: mask=0x{:02X}, latency={}ns, pin={}", mask, latency_ns, pin);
        structured::log_irq_event(mask, latency_ns, pin);
        sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(feature = "rtt-logging")]
async fn demo_lora_logging() {
    use mbus_rs::logging::{structured, encoders::LoRaEventType};

    println!("\n--- LoRa Event Logging Demo ---");

    // Simulate LoRa packet lifecycle
    println!("LoRa TX Start: SF7, 868.95 MHz");
    structured::log_lora_event(LoRaEventType::TxStart, 0, 0.0, 868950000, 7, 64);
    sleep(Duration::from_millis(50)).await;

    println!("LoRa RX Complete: RSSI=-85dBm, SNR=12.5dB");
    structured::log_lora_event(LoRaEventType::RxComplete, -85, 12.5, 868950000, 7, 32);
    sleep(Duration::from_millis(50)).await;

    println!("LoRa Channel Hop: 868.95 -> 868.30 MHz");
    structured::log_lora_event(LoRaEventType::ChannelHop, 0, 0.0, 868300000, 8, 0);
    sleep(Duration::from_millis(50)).await;
}

#[cfg(feature = "rtt-logging")]
async fn demo_crypto_logging() {
    use mbus_rs::logging::{structured, encoders::{CryptoOp, CryptoBackend}};

    println!("\n--- Crypto Operation Logging Demo ---");

    // Simulate crypto operations
    println!("AES-128 Hardware Encryption: 256 bytes");
    structured::log_crypto_event(CryptoOp::Encrypt, CryptoBackend::Hardware, 256, 15000);
    sleep(Duration::from_millis(50)).await;

    println!("AES-128 Software Decryption: 128 bytes");
    structured::log_crypto_event(CryptoOp::Decrypt, CryptoBackend::Software, 128, 8000);
    sleep(Duration::from_millis(50)).await;

    println!("HMAC-SHA1 Hardware: 512 bytes");
    structured::log_crypto_event(CryptoOp::Hmac, CryptoBackend::Hardware, 512, 20000);
    sleep(Duration::from_millis(50)).await;
}

#[cfg(feature = "rtt-logging")]
async fn demo_performance_test() {
    use mbus_rs::logging::{structured, encoders::LoRaEventType};

    println!("\n--- Performance Test: 1000 Events ---");

    let start = Instant::now();

    for i in 0..1000 {
        structured::log_lora_event(
            LoRaEventType::RxComplete,
            -80 - (i % 20) as i16,
            10.0 + (i % 10) as f32,
            868950000 + (i % 3) * 200000,
            7 + (i % 3) as u8,
            32 + (i % 32) as u16,
        );
    }

    let duration = start.elapsed();
    println!("1000 events logged in {:?} ({:.2} events/ms)",
             duration, 1000.0 / duration.as_millis() as f64);

    if is_rtt_available() {
        println!("RTT provides extremely high throughput with minimal overhead!");
    }
}