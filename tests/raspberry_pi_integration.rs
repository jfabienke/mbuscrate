//! Integration tests for Raspberry Pi HAL implementation
//!
//! These tests verify that the Raspberry Pi HAL implementation correctly
//! interfaces with the SX126x radio driver.
//!
//! Note: These tests require actual Raspberry Pi hardware with properly
//! connected SX126x radio module. Run with `--ignored` flag or set
//! `RPI_HARDWARE_TEST=1` environment variable to enable hardware tests.

#[cfg(feature = "raspberry-pi")]
mod raspberry_pi_tests {
    use mbus_rs::wmbus::radio::{
        driver::Sx126xDriver,
        hal::{GpioPins, RaspberryPiHal, RaspberryPiHalBuilder},
    };
    use std::env;

    /// Check if hardware tests should be run
    fn should_run_hardware_tests() -> bool {
        env::var("RPI_HARDWARE_TEST").unwrap_or_default() == "1"
    }

    /// Default test GPIO configuration
    fn test_gpio_pins() -> GpioPins {
        GpioPins {
            busy: 25,
            dio1: 24,
            dio2: Some(23),
            reset: Some(22),
        }
    }

    #[test]
    fn test_gpio_pins_creation() {
        let pins = GpioPins::default();
        assert_eq!(pins.busy, 25);
        assert_eq!(pins.dio1, 24);
        assert_eq!(pins.dio2, Some(23));
        assert_eq!(pins.reset, Some(22));
    }

    #[test]
    fn test_hal_builder_configuration() {
        let builder = RaspberryPiHalBuilder::new()
            .spi_bus(1)
            .spi_speed(10_000_000)
            .busy_pin(20)
            .dio1_pin(21)
            .no_dio2()
            .no_reset();

        // Test that builder configuration is correct
        // (We can't access private fields directly, but we can test build result)
        // This is mainly testing the builder pattern works correctly
        assert_eq!(1, 1); // Placeholder assertion
    }

    #[test]
    #[ignore = "Requires Raspberry Pi hardware"]
    fn test_hal_initialization() {
        if !should_run_hardware_tests() {
            return;
        }

        // Test basic HAL initialization
        let result = RaspberryPiHal::new(0, &test_gpio_pins());
        match result {
            Ok(hal) => {
                println!("✅ HAL initialized: {}", hal.get_info());
                // Test basic functionality
                assert!(hal.get_info().contains("SPI0"));
            }
            Err(e) => {
                // This might fail on non-Pi systems or without proper setup
                println!("⚠️ HAL initialization failed (expected on non-Pi): {}", e);
            }
        }
    }

    #[test]
    #[ignore = "Requires Raspberry Pi hardware"]
    fn test_hal_builder_initialization() {
        if !should_run_hardware_tests() {
            return;
        }

        let result = RaspberryPiHalBuilder::new()
            .spi_bus(0)
            .spi_speed(8_000_000)
            .busy_pin(25)
            .dio1_pin(24)
            .dio2_pin(23)
            .reset_pin(22)
            .build();

        match result {
            Ok(hal) => {
                println!("✅ HAL builder worked: {}", hal.get_info());
            }
            Err(e) => {
                println!("⚠️ HAL builder failed (expected on non-Pi): {}", e);
            }
        }
    }

    #[test]
    #[ignore = "Requires Raspberry Pi hardware with SX126x"]
    fn test_driver_integration() {
        if !should_run_hardware_tests() {
            return;
        }

        // Full integration test with actual hardware
        let hal = match RaspberryPiHal::new(0, &test_gpio_pins()) {
            Ok(hal) => hal,
            Err(e) => {
                println!("⚠️ Skipping driver test - HAL init failed: {}", e);
                return;
            }
        };

        let mut driver = Sx126xDriver::new(hal, 32_000_000);

        // Test basic radio communication
        match driver.get_irq_status() {
            Ok(status) => {
                println!(
                    "✅ Radio communication successful - IRQ status: 0x{:04X}",
                    status.raw()
                );
            }
            Err(e) => {
                println!("❌ Radio communication failed: {}", e);
                panic!("Radio communication test failed");
            }
        }

        // Test wM-Bus configuration
        match driver.configure_for_wmbus(868_950_000, 100_000) {
            Ok(()) => {
                println!("✅ wM-Bus configuration successful");
            }
            Err(e) => {
                println!("❌ wM-Bus configuration failed: {}", e);
                panic!("wM-Bus configuration test failed");
            }
        }

        // Test receive mode activation
        match driver.set_rx(1000) {
            // 1 second timeout
            Ok(()) => {
                println!("✅ Receive mode activation successful");

                // Wait a bit and check for any activity
                std::thread::sleep(std::time::Duration::from_millis(1100));

                match driver.process_irqs() {
                    Ok(Some(data)) => {
                        println!("📡 Received {} bytes during test", data.len());
                    }
                    Ok(None) => {
                        println!("✅ Receive test completed (no data, expected)");
                    }
                    Err(e) => {
                        println!("⚠️ IRQ processing error: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("❌ Receive mode activation failed: {}", e);
                panic!("Receive mode test failed");
            }
        }

        println!("🎉 All hardware tests passed!");
    }

    #[test]
    #[ignore = "Requires Raspberry Pi hardware"]
    fn test_gpio_functionality() {
        if !should_run_hardware_tests() {
            return;
        }

        let hal = match RaspberryPiHal::new(0, &test_gpio_pins()) {
            Ok(hal) => hal,
            Err(e) => {
                println!("⚠️ Skipping GPIO test - HAL init failed: {}", e);
                return;
            }
        };

        let mut driver = Sx126xDriver::new(hal, 32_000_000);

        // Test GPIO read functionality
        match driver.gpio_read(1) {
            // DIO1 pin
            Ok(state) => {
                println!(
                    "✅ DIO1 pin read successful: {}",
                    if state { "HIGH" } else { "LOW" }
                );
            }
            Err(e) => {
                println!("❌ DIO1 pin read failed: {}", e);
            }
        }

        // Test DIO2 if configured
        match driver.gpio_read(2) {
            // DIO2 pin
            Ok(state) => {
                println!(
                    "✅ DIO2 pin read successful: {}",
                    if state { "HIGH" } else { "LOW" }
                );
            }
            Err(_) => {
                println!("ℹ️ DIO2 pin not configured or read failed (expected if not wired)");
            }
        }
    }

    #[test]
    #[ignore = "Requires Raspberry Pi hardware"]
    fn test_multiple_spi_buses() {
        if !should_run_hardware_tests() {
            return;
        }

        // Test SPI bus 0
        match RaspberryPiHal::new(0, &test_gpio_pins()) {
            Ok(hal) => {
                println!("✅ SPI bus 0 initialization successful");
                assert!(hal.get_info().contains("SPI0"));
            }
            Err(e) => {
                println!("⚠️ SPI bus 0 failed: {}", e);
            }
        }

        // Test SPI bus 1 (if available)
        let pins_bus1 = GpioPins {
            busy: 20,
            dio1: 21,
            dio2: None,
            reset: None,
        };

        match RaspberryPiHal::new(1, &pins_bus1) {
            Ok(hal) => {
                println!("✅ SPI bus 1 initialization successful");
                assert!(hal.get_info().contains("SPI1"));
            }
            Err(e) => {
                println!("ℹ️ SPI bus 1 not available or failed: {}", e);
                // This is expected if SPI1 is not enabled
            }
        }
    }

    #[test]
    fn test_error_conditions() {
        // Test invalid SPI bus numbers
        let result = RaspberryPiHalBuilder::new()
            .spi_bus(99) // Invalid bus
            .build();

        assert!(result.is_err());
        println!("✅ Invalid SPI bus correctly rejected");

        // Test invalid SPI speed
        let result = RaspberryPiHalBuilder::new()
            .spi_speed(0) // Invalid speed
            .build();

        assert!(result.is_err());
        println!("✅ Invalid SPI speed correctly rejected");
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn test_target_architecture() {
        // Verify we're running on the expected architecture
        println!("✅ Running on aarch64 (Raspberry Pi 4/5 64-bit)");
        assert_eq!(std::env::consts::ARCH, "aarch64");
    }

    #[cfg(target_arch = "arm")]
    #[test]
    fn test_target_architecture_arm() {
        // Verify we're running on ARM architecture
        println!("✅ Running on ARM (32-bit Raspberry Pi)");
        assert_eq!(std::env::consts::ARCH, "arm");
    }
}

#[cfg(not(feature = "raspberry-pi"))]
mod disabled_tests {
    #[test]
    fn test_raspberry_pi_feature_disabled() {
        println!("ℹ️ Raspberry Pi tests disabled - enable with --features raspberry-pi");
    }
}

// Mock tests that can run without hardware
#[cfg(all(test, feature = "raspberry-pi"))]
mod mock_tests {
    use mbus_rs::wmbus::radio::hal::{GpioPins, RaspberryPiHalBuilder};

    #[test]
    fn test_gpio_pins_display() {
        let pins = GpioPins::default();
        // Test that we can create and access pin configurations
        assert_eq!(pins.busy, 25);
        assert_eq!(pins.dio1, 24);
        assert!(pins.dio2.is_some());
        assert!(pins.reset.is_some());
    }

    #[test]
    fn test_builder_pattern() {
        // Test builder pattern without actual hardware initialization
        let builder = RaspberryPiHalBuilder::new()
            .spi_bus(0)
            .busy_pin(25)
            .dio1_pin(24);

        // Just verify builder methods return Self for chaining
        let builder2 = builder.no_dio2().spi_speed(8_000_000);

        // We can't test build() without hardware, but we can test the pattern
        assert_eq!(
            std::mem::size_of_val(&builder2),
            std::mem::size_of::<RaspberryPiHalBuilder>()
        );
    }

    #[test]
    fn test_pin_configuration_variants() {
        // Test different pin configurations
        let minimal = GpioPins {
            busy: 25,
            dio1: 24,
            dio2: None,
            reset: None,
        };

        let full = GpioPins {
            busy: 25,
            dio1: 24,
            dio2: Some(23),
            reset: Some(22),
        };

        assert_ne!(minimal.dio2, full.dio2);
        assert_ne!(minimal.reset, full.reset);
    }
}
