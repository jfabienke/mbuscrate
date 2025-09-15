//! Comprehensive tests for PIO IRQ debouncing
//!
//! This test suite validates the PIO-based IRQ debouncing implementation
//! for Raspberry Pi 5, including hardware functionality, software fallbacks,
//! and integration with the SX1262 driver.

#[cfg(feature = "pio-irq")]
use mbus_rs::wmbus::radio::pio_irq::{
    get_pio_irq_backend, PioIrqBackend, SoftwareBackend,
    DIO0_TX_DONE, DIO1_RX_DONE, DIO2_MASK, DIO3_MASK,
    DIO_PINS, MAX_DEBOUNCE_US,
};

#[cfg(feature = "pio-irq")]
use mbus_rs::wmbus::radio::lora::sx1262::{Sx1262Driver, LoRaConfig};

use std::time::{Duration, Instant};
use std::thread;

/// Test basic PIO IRQ backend functionality
#[cfg(feature = "pio-irq")]
#[test]
fn test_pio_backend_initialization() {
    let backend = get_pio_irq_backend();

    // Backend should be initialized successfully
    assert!(!backend.name().is_empty());
    println!("Initialized backend: {}", backend.name());

    // Should not be pending initially
    assert!(!backend.is_irq_pending());
}

/// Test DIO pin constant definitions
#[cfg(feature = "pio-irq")]
#[test]
fn test_dio_pin_constants() {
    // Verify DIO pin mapping matches SX1262 HAT
    assert_eq!(DIO_PINS, [25, 26, 27, 28]);

    // Verify bitmask constants
    assert_eq!(DIO0_TX_DONE, 0x01);
    assert_eq!(DIO1_RX_DONE, 0x02);
    assert_eq!(DIO2_MASK, 0x04);
    assert_eq!(DIO3_MASK, 0x08);

    // Verify all masks are unique
    let all_masks = [DIO0_TX_DONE, DIO1_RX_DONE, DIO2_MASK, DIO3_MASK];
    for i in 0..all_masks.len() {
        for j in i + 1..all_masks.len() {
            assert_ne!(all_masks[i], all_masks[j], "DIO masks should be unique");
        }
    }
}

/// Test software backend fallback
#[cfg(feature = "pio-irq")]
#[test]
fn test_software_backend() {
    let mut backend = SoftwareBackend::new();

    assert_eq!(backend.name(), "Software Polling");
    assert!(!backend.is_irq_pending());

    // Test debouncing with various parameters
    let test_cases = [
        (DIO1_RX_DONE, 1),   // Minimum debounce
        (DIO0_TX_DONE, 10),  // Typical debounce
        (0x0F, 50),          // All pins, longer debounce
        (0x00, 100),         // No pins
    ];

    for (mask, debounce_us) in test_cases {
        let result = backend.debounce_irq(mask, debounce_us);
        assert_eq!(result & 0xF0, 0, "Upper bits should be clear");
        assert_eq!(result & !mask, 0, "Only requested pins should be set");
    }

    // Test FIFO operations (no-ops for software backend)
    backend.clear_irq_fifo(); // Should not panic
}

/// Test debounce parameter validation
#[cfg(feature = "pio-irq")]
#[test]
fn test_debounce_parameter_validation() {
    let mut backend = get_pio_irq_backend();

    // Test normal range
    for debounce_us in [1, 5, 10, 25, 50, 100] {
        let result = backend.debounce_irq(DIO1_RX_DONE, debounce_us);
        assert_eq!(result & 0xF0, 0, "Upper bits should be clear for {}μs", debounce_us);
    }

    // Test boundary conditions
    let result = backend.debounce_irq(DIO1_RX_DONE, 0);
    assert_eq!(result & 0xF0, 0, "Zero debounce should work");

    let result = backend.debounce_irq(DIO1_RX_DONE, MAX_DEBOUNCE_US);
    assert_eq!(result & 0xF0, 0, "Maximum debounce should work");

    // Test excessive debounce (should be clamped)
    let result = backend.debounce_irq(DIO1_RX_DONE, MAX_DEBOUNCE_US * 2);
    assert_eq!(result & 0xF0, 0, "Excessive debounce should be clamped");
}

/// Test concurrent IRQ handling
#[cfg(feature = "pio-irq")]
#[test]
fn test_concurrent_irq_handling() {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let backend = Arc::new(Mutex::new(get_pio_irq_backend()));
    let results = Arc::new(Mutex::new(Vec::new()));

    let mut handles = Vec::new();

    // Spawn multiple threads to test concurrent access
    for i in 0..4 {
        let backend_clone = Arc::clone(&backend);
        let results_clone = Arc::clone(&results);

        let handle = thread::spawn(move || {
            let mut backend = backend_clone.lock().unwrap();
            let dio_mask = 1 << i; // Each thread tests different DIO
            let debounce_us = 10 + i * 5; // Different debounce times

            for _ in 0..10 {
                let result = backend.debounce_irq(dio_mask, debounce_us as u32);
                results_clone.lock().unwrap().push((i, result));
                thread::sleep(Duration::from_millis(1));
            }
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread should complete successfully");
    }

    let results = results.lock().unwrap();
    assert_eq!(results.len(), 40, "All thread operations should complete");

    // Verify no corruption occurred
    for (thread_id, result) in results.iter() {
        assert_eq!(result & 0xF0, 0, "Thread {} result should have clean upper bits", thread_id);
    }
}

/// Test FIFO operations
#[cfg(feature = "pio-irq")]
#[test]
fn test_fifo_operations() {
    let mut backend = get_pio_irq_backend();

    // Initially should not be pending
    assert!(!backend.is_irq_pending());

    // Clear FIFO (should not panic)
    backend.clear_irq_fifo();
    assert!(!backend.is_irq_pending());

    // Test multiple clears
    for _ in 0..5 {
        backend.clear_irq_fifo();
        assert!(!backend.is_irq_pending());
    }
}

/// Test performance characteristics
#[cfg(feature = "pio-irq")]
#[test]
fn test_irq_performance() {
    let mut backend = get_pio_irq_backend();

    // Measure debounce operation latency
    let iterations = 1000;
    let start = Instant::now();

    for i in 0..iterations {
        let mask = ((i % 4) + 1) as u8; // Rotate through DIO0-3
        let _ = backend.debounce_irq(mask, 10);
    }

    let elapsed = start.elapsed();
    let avg_latency = elapsed.as_nanos() / iterations;

    println!("Average debounce latency: {} ns ({} operations)", avg_latency, iterations);

    // Performance targets (adjusted for software vs hardware)
    let max_latency_ns = if backend.name().contains("Hardware") {
        50_000  // 50μs for hardware (including mmap overhead)
    } else {
        200_000 // 200μs for software fallback
    };

    assert!(avg_latency < max_latency_ns,
            "Average latency {} ns exceeds target {} ns for {}",
            avg_latency, max_latency_ns, backend.name());
}

/// Test IRQ storm handling
#[cfg(feature = "pio-irq")]
#[test]
fn test_irq_storm_handling() {
    let mut backend = get_pio_irq_backend();

    // Simulate rapid IRQ events
    let storm_iterations = 10000;
    let start = Instant::now();

    for i in 0..storm_iterations {
        let mask = DIO1_RX_DONE;
        let debounce_us = 1; // Minimal debounce for maximum throughput

        let result = backend.debounce_irq(mask, debounce_us);

        // Verify correct behavior during storm
        assert_eq!(result & 0xF0, 0, "Iteration {}: upper bits corrupted", i);
        assert_eq!(result & !mask, 0, "Iteration {}: unexpected pins set", i);
    }

    let elapsed = start.elapsed();
    let throughput = storm_iterations as f64 / elapsed.as_secs_f64();

    println!("IRQ storm throughput: {:.0} ops/sec ({} backend)",
             throughput, backend.name());

    // Throughput targets
    let min_throughput = if backend.name().contains("Hardware") {
        1000.0  // 1k ops/sec for hardware PIO
    } else {
        100.0   // 100 ops/sec for software (limited by GPIO access)
    };

    assert!(throughput > min_throughput,
            "Throughput {:.0} ops/sec below target {:.0} for {}",
            throughput, min_throughput, backend.name());
}

/// Test SX1262 driver integration
#[cfg(feature = "pio-irq")]
#[test]
fn test_sx1262_integration() {
    // This test may fail on non-Pi hardware, which is expected
    match Sx1262Driver::new() {
        Ok(mut driver) => {
            println!("SX1262 driver created successfully");

            // Test configuration
            let result = driver.configure_for_wmbus(868_950_000, 125_000);
            assert!(result.is_ok(), "wM-Bus configuration should succeed");

            // Test RX mode setup
            let result = driver.set_rx_continuous();
            assert!(result.is_ok(), "RX continuous mode should succeed");

            // Test packet ready check (should be false initially)
            let result = driver.is_packet_ready();
            assert!(result.is_ok(), "Packet ready check should succeed");
            assert!(!result.unwrap(), "No packet should be ready initially");

            // Test signal quality getters
            let rssi = driver.get_rssi();
            let snr = driver.get_snr();
            println!("Initial RSSI: {} dBm, SNR: {} dB", rssi, snr);

            // Values should be in reasonable ranges
            assert!(rssi >= -150 && rssi <= 0, "RSSI should be in valid range");
            assert!(snr >= -20 && snr <= 20, "SNR should be in valid range");
        }
        Err(e) => {
            println!("SX1262 driver creation failed (expected on non-Pi): {}", e);
            // This is expected on development machines without SX1262 hardware
        }
    }
}

/// Test LoRa configuration validation
#[cfg(feature = "pio-irq")]
#[test]
fn test_lora_configuration() {
    let config = LoRaConfig::default();

    // Verify default configuration
    assert_eq!(config.frequency_hz, 868_950_000);
    assert_eq!(config.bandwidth_hz, 125_000);
    assert_eq!(config.spreading_factor, 7);
    assert_eq!(config.coding_rate, 1);
    assert_eq!(config.sync_word, 0x12);
    assert_eq!(config.preamble_length, 8);
    assert!(config.header_type);
    assert_eq!(config.payload_length, 255);
    assert!(config.crc_on);
    assert!(!config.invert_iq);

    // Test configuration cloning
    let cloned = config.clone();
    assert_eq!(config.frequency_hz, cloned.frequency_hz);
    assert_eq!(config.bandwidth_hz, cloned.bandwidth_hz);
}

/// Test error handling
#[cfg(feature = "pio-irq")]
#[test]
fn test_error_handling() {
    // Test software backend error conditions
    let mut backend = SoftwareBackend::new();

    // These should not panic even with invalid inputs
    let result = backend.debounce_irq(0xFF, u32::MAX);
    assert_eq!(result & 0xF0, 0, "Invalid input should not corrupt output");

    backend.clear_irq_fifo(); // Should not panic

    // Test concurrent access to backend selection
    let mut handles = Vec::new();

    for _ in 0..10 {
        let handle = thread::spawn(|| {
            let backend = get_pio_irq_backend();
            assert!(!backend.name().is_empty());
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("Backend selection should be thread-safe");
    }
}

/// Benchmark different debounce windows
#[cfg(feature = "pio-irq")]
#[test]
fn test_debounce_window_performance() {
    let mut backend = get_pio_irq_backend();

    let debounce_windows = [1, 5, 10, 25, 50, 100];

    for &debounce_us in &debounce_windows {
        let iterations = 100;
        let start = Instant::now();

        for _ in 0..iterations {
            let _ = backend.debounce_irq(DIO1_RX_DONE, debounce_us);
        }

        let elapsed = start.elapsed();
        let avg_time = elapsed.as_nanos() / iterations;

        println!("Debounce {}μs: {} ns/op", debounce_us, avg_time);

        // Longer debounce windows may take slightly longer, but not proportionally
        assert!(avg_time < 1_000_000, // 1ms max per operation
                "Debounce {}μs took too long: {} ns", debounce_us, avg_time);
    }
}

/// Integration test simulating real SX1262 usage pattern
#[cfg(feature = "pio-irq")]
#[test]
fn test_realistic_usage_pattern() {
    match Sx1262Driver::new() {
        Ok(mut driver) => {
            // Simulate typical wM-Bus receiver workflow

            // 1. Configure for EU band
            driver.configure_for_wmbus(868_950_000, 125_000)
                .expect("Configuration should succeed");

            // 2. Start receiving
            driver.set_rx_continuous()
                .expect("RX start should succeed");

            // 3. Poll for packets (simulating main loop)
            for i in 0..10 {
                let packet_ready = driver.is_packet_ready()
                    .expect("Packet check should succeed");

                if packet_ready {
                    println!("Iteration {}: Packet detected", i);
                    // In real usage, would call read_packet() here
                } else {
                    println!("Iteration {}: No packet", i);
                }

                thread::sleep(Duration::from_millis(10));
            }

            // 4. Test transmission
            let test_payload = b"Hello wM-Bus";
            let tx_result = driver.transmit_packet(test_payload);
            assert!(tx_result.is_ok(), "Transmission should succeed");

            // 5. Wait for TX completion
            let tx_done = driver.wait_tx_done(1000)
                .expect("TX wait should succeed");
            println!("TX completed: {}", tx_done);

        }
        Err(_) => {
            println!("Skipping realistic test - no SX1262 hardware available");
        }
    }
}

/// Test platform compatibility
#[test]
fn test_platform_compatibility() {
    // This test should pass on all platforms

    #[cfg(feature = "pio-irq")]
    {
        let backend = get_pio_irq_backend();
        println!("Platform: {}", std::env::consts::OS);
        println!("Architecture: {}", std::env::consts::ARCH);
        println!("Selected backend: {}", backend.name());

        // Backend should be available on all platforms (with fallback)
        assert!(!backend.name().is_empty());
    }

    #[cfg(not(feature = "pio-irq"))]
    {
        println!("PIO IRQ feature not enabled - skipping backend tests");
    }
}

/// Test state machine reset functionality
#[test]
fn test_state_machine_reset() {
    #[cfg(feature = "pio-irq")]
    {
        let backend = get_pio_irq_backend();

        // Test reset operation
        let result = backend.reset();
        assert!(result.is_ok(), "Reset should succeed on all backends");

        // After reset, backend should still be functional
        assert!(!backend.name().is_empty());
        assert_eq!(backend.is_irq_pending(), false);

        // Debounce should work after reset
        let events = backend.debounce_irq(0x02, 10);
        assert_eq!(events, 0); // No events expected in test environment

        println!("✅ Backend reset test passed for: {}", backend.name());
    }

    #[cfg(not(feature = "pio-irq"))]
    {
        println!("PIO IRQ feature not enabled - skipping reset tests");
    }
}

/// Test reset with different configurations
#[test]
fn test_reset_with_reconfiguration() {
    #[cfg(feature = "pio-irq")]
    {
        let backend = get_pio_irq_backend();

        // Test multiple reset cycles
        for i in 1..=3 {
            let result = backend.reset();
            assert!(result.is_ok(), "Reset cycle {} should succeed", i);

            // Test functionality after each reset
            backend.clear_irq_fifo();
            let events = backend.debounce_irq(0x0F, 5 * i); // Different debounce values
            assert_eq!(events, 0);
        }

        println!("✅ Multiple reset cycles passed for: {}", backend.name());
    }

    #[cfg(not(feature = "pio-irq"))]
    {
        println!("PIO IRQ feature not enabled - skipping reconfiguration tests");
    }
}

/// Test reset performance and timing
#[test]
fn test_reset_performance() {
    #[cfg(feature = "pio-irq")]
    {
        let backend = get_pio_irq_backend();
        let start = std::time::Instant::now();

        // Perform multiple resets to measure performance
        const RESET_COUNT: usize = 10;
        for _ in 0..RESET_COUNT {
            let result = backend.reset();
            assert!(result.is_ok(), "Reset should always succeed");
        }

        let duration = start.elapsed();
        let avg_reset_time = duration / RESET_COUNT as u32;

        println!("Reset performance for {}: {} resets in {:?} (avg: {:?})",
                backend.name(), RESET_COUNT, duration, avg_reset_time);

        // Reset should be fast (< 1ms each on average)
        assert!(avg_reset_time.as_millis() < 1,
               "Reset should be fast, got: {:?}", avg_reset_time);
    }

    #[cfg(not(feature = "pio-irq"))]
    {
        println!("PIO IRQ feature not enabled - skipping performance tests");
    }
}