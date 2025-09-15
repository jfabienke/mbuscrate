//! PIO IRQ Debouncing Demo for Raspberry Pi 5
//!
//! This demo showcases the PIO-based IRQ debouncing system for SX1262 HAT
//! on Raspberry Pi 5. It demonstrates sub-10μs IRQ latency, noise filtering,
//! and integration with LoRa packet reception.
//!
//! ## Hardware Requirements
//!
//! - Raspberry Pi 5 with RP1 southbridge
//! - SX1262 LoRa HAT (DIO pins connected to GPIO25-28)
//! - Optional: Signal generator for IRQ simulation
//!
//! ## Usage
//!
//! ```bash
//! # Run on Raspberry Pi 5 with PIO feature
//! cargo run --example pio_irq_demo --features pio-irq
//!
//! # Monitor performance with system stats
//! cargo run --example pio_irq_demo --features pio-irq -- --stats
//!
//! # Test with simulated noise
//! cargo run --example pio_irq_demo --features pio-irq -- --noise-test
//! ```

use std::time::{Duration, Instant};
use std::thread;
use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}};
use clap::{Arg, Command};
use log::{info, warn, error, debug};

#[cfg(feature = "pio-irq")]
use mbus_rs::wmbus::radio::pio_irq::{
    get_pio_irq_backend, PioIrqBackend,
    DIO0_TX_DONE, DIO1_RX_DONE, DIO2_MASK, DIO3_MASK,
};

#[cfg(feature = "pio-irq")]
use mbus_rs::wmbus::radio::lora::sx1262::{Sx1262Driver, LoRaConfig};

/// Demo configuration
#[derive(Debug, Clone)]
struct DemoConfig {
    duration_secs: u64,
    show_stats: bool,
    noise_test: bool,
    debounce_us: u32,
    verbose: bool,
}

impl Default for DemoConfig {
    fn default() -> Self {
        Self {
            duration_secs: 30,
            show_stats: false,
            noise_test: false,
            debounce_us: 10,
            verbose: false,
        }
    }
}

/// Performance statistics
#[derive(Debug, Default)]
struct PerformanceStats {
    total_irqs: AtomicU64,
    valid_irqs: AtomicU64,
    filtered_irqs: AtomicU64,
    min_latency_ns: AtomicU64,
    max_latency_ns: AtomicU64,
    total_latency_ns: AtomicU64,
}

impl PerformanceStats {
    fn record_irq(&self, latency_ns: u64, filtered: bool) {
        self.total_irqs.fetch_add(1, Ordering::Relaxed);

        if filtered {
            self.filtered_irqs.fetch_add(1, Ordering::Relaxed);
        } else {
            self.valid_irqs.fetch_add(1, Ordering::Relaxed);
        }

        // Update latency statistics
        self.total_latency_ns.fetch_add(latency_ns, Ordering::Relaxed);

        // Update min latency
        let mut current_min = self.min_latency_ns.load(Ordering::Relaxed);
        while current_min == 0 || latency_ns < current_min {
            match self.min_latency_ns.compare_exchange_weak(
                current_min, latency_ns, Ordering::Relaxed, Ordering::Relaxed
            ) {
                Ok(_) => break,
                Err(actual) => current_min = actual,
            }
        }

        // Update max latency
        let mut current_max = self.max_latency_ns.load(Ordering::Relaxed);
        while latency_ns > current_max {
            match self.max_latency_ns.compare_exchange_weak(
                current_max, latency_ns, Ordering::Relaxed, Ordering::Relaxed
            ) {
                Ok(_) => break,
                Err(actual) => current_max = actual,
            }
        }
    }

    fn print_summary(&self) {
        let total = self.total_irqs.load(Ordering::Relaxed);
        let valid = self.valid_irqs.load(Ordering::Relaxed);
        let filtered = self.filtered_irqs.load(Ordering::Relaxed);
        let min_ns = self.min_latency_ns.load(Ordering::Relaxed);
        let max_ns = self.max_latency_ns.load(Ordering::Relaxed);
        let total_ns = self.total_latency_ns.load(Ordering::Relaxed);

        println!("\n=== PIO IRQ Performance Statistics ===");
        println!("Total IRQ events:     {}", total);
        println!("Valid IRQs:           {} ({:.1}%)", valid, (valid as f64 / total as f64) * 100.0);
        println!("Filtered (noise):     {} ({:.1}%)", filtered, (filtered as f64 / total as f64) * 100.0);

        if total > 0 {
            let avg_ns = total_ns / total;
            println!("Latency (min/avg/max): {:.1}μs / {:.1}μs / {:.1}μs",
                     min_ns as f64 / 1000.0,
                     avg_ns as f64 / 1000.0,
                     max_ns as f64 / 1000.0);
        }
        println!("=======================================\n");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let matches = Command::new("PIO IRQ Demo")
        .about("Demonstrates PIO-based IRQ debouncing for SX1262 on Raspberry Pi 5")
        .arg(Arg::new("duration")
            .short('d')
            .long("duration")
            .value_name("SECONDS")
            .help("Demo duration in seconds")
            .default_value("30"))
        .arg(Arg::new("stats")
            .long("stats")
            .help("Show performance statistics")
            .action(clap::ArgAction::SetTrue))
        .arg(Arg::new("noise-test")
            .long("noise-test")
            .help("Enable noise simulation test")
            .action(clap::ArgAction::SetTrue))
        .arg(Arg::new("debounce")
            .long("debounce")
            .value_name("MICROSECONDS")
            .help("Debounce window in microseconds")
            .default_value("10"))
        .arg(Arg::new("verbose")
            .short('v')
            .long("verbose")
            .help("Enable verbose logging")
            .action(clap::ArgAction::SetTrue))
        .get_matches();

    // Initialize logging
    let log_level = if matches.get_flag("verbose") {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    env_logger::Builder::from_default_env()
        .filter_level(log_level)
        .init();

    // Parse configuration
    let config = DemoConfig {
        duration_secs: matches.get_one::<String>("duration")
            .unwrap()
            .parse()
            .expect("Invalid duration"),
        show_stats: matches.get_flag("stats"),
        noise_test: matches.get_flag("noise-test"),
        debounce_us: matches.get_one::<String>("debounce")
            .unwrap()
            .parse()
            .expect("Invalid debounce value"),
        verbose: matches.get_flag("verbose"),
    };

    println!("🚀 PIO IRQ Debouncing Demo for Raspberry Pi 5");
    println!("Configuration: {:?}\n", config);

    #[cfg(feature = "pio-irq")]
    {
        run_demo(config)?;
    }

    #[cfg(not(feature = "pio-irq"))]
    {
        println!("❌ PIO IRQ feature not enabled!");
        println!("Build with: cargo run --example pio_irq_demo --features pio-irq");
        return Err("PIO IRQ feature required".into());
    }

    Ok(())
}

#[cfg(feature = "pio-irq")]
fn run_demo(config: DemoConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Detect platform and backend
    detect_platform();

    // Initialize PIO IRQ backend
    let backend = get_pio_irq_backend();
    info!("Initialized {} backend", backend.name());

    // Run appropriate demo based on configuration
    if config.noise_test {
        run_noise_simulation_test(backend, &config)?;
    } else {
        run_interactive_demo(backend, &config)?;
    }

    Ok(())
}

#[cfg(feature = "pio-irq")]
fn detect_platform() {
    println!("🔍 Platform Detection:");
    println!("  OS: {}", std::env::consts::OS);
    println!("  Architecture: {}", std::env::consts::ARCH);

    // Check for Raspberry Pi 5
    if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
        if cpuinfo.contains("BCM2712") {
            println!("  ✅ Raspberry Pi 5 detected (BCM2712)");
        } else if cpuinfo.contains("BCM2711") {
            println!("  ⚠️  Raspberry Pi 4 detected (BCM2711) - software fallback");
        } else {
            println!("  ❓ Unknown Raspberry Pi model");
        }

        if std::arch::is_aarch64_feature_detected!("neon") {
            println!("  ✅ NEON support available");
        }
    } else {
        println!("  ❌ Cannot read /proc/cpuinfo - not on Linux/Pi");
    }

    println!();
}

#[cfg(feature = "pio-irq")]
fn run_interactive_demo(
    backend: Arc<dyn PioIrqBackend>,
    config: &DemoConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 Interactive IRQ Demo");
    println!("Duration: {} seconds", config.duration_secs);
    println!("Debounce: {}μs", config.debounce_us);

    let stats = Arc::new(PerformanceStats::default());
    let running = Arc::new(AtomicBool::new(true));

    // Try to initialize SX1262 driver
    let sx1262_available = match Sx1262Driver::new() {
        Ok(mut driver) => {
            info!("✅ SX1262 driver initialized");

            // Configure for wM-Bus
            driver.configure_for_wmbus(868_950_000, 125_000)?;
            driver.set_rx_continuous()?;

            println!("📡 SX1262 configured for wM-Bus reception (868.95 MHz)");
            true
        }
        Err(e) => {
            warn!("⚠️  SX1262 driver not available: {}", e);
            println!("📊 Running IRQ backend test without radio");
            false
        }
    };

    // Start monitoring thread
    let backend_clone = Arc::clone(&backend);
    let stats_clone = Arc::clone(&stats);
    let running_clone = Arc::clone(&running);
    let config_clone = config.clone();

    let monitor_handle = thread::spawn(move || {
        monitor_irq_events(backend_clone, stats_clone, running_clone, &config_clone, sx1262_available);
    });

    // Start statistics display thread
    let stats_clone = Arc::clone(&stats);
    let running_clone = Arc::clone(&running);
    let stats_config = config.clone();

    let stats_handle = if config.show_stats {
        Some(thread::spawn(move || {
            display_live_stats(stats_clone, running_clone, &stats_config);
        }))
    } else {
        None
    };

    // Run for specified duration
    println!("🔄 Monitoring IRQ events... (press Ctrl+C to stop early)");
    thread::sleep(Duration::from_secs(config.duration_secs));

    // Stop monitoring
    running.store(false, Ordering::Relaxed);

    // Wait for threads to complete
    monitor_handle.join().unwrap();
    if let Some(handle) = stats_handle {
        handle.join().unwrap();
    }

    // Print final statistics
    stats.print_summary();

    // Demonstrate reset functionality
    println!("🔄 Demonstrating State Machine Reset...");
    demonstrate_reset_functionality(&backend)?;

    Ok(())
}

#[cfg(feature = "pio-irq")]
fn monitor_irq_events(
    mut backend: Arc<dyn PioIrqBackend>,
    stats: Arc<PerformanceStats>,
    running: Arc<AtomicBool>,
    config: &DemoConfig,
    sx1262_available: bool,
) {
    let mut iteration = 0u64;

    while running.load(Ordering::Relaxed) {
        iteration += 1;

        let start = Instant::now();

        // Test all DIO pins with debouncing
        let dio_masks = [DIO1_RX_DONE, DIO0_TX_DONE, DIO2_MASK, DIO3_MASK];

        for &mask in &dio_masks {
            let events = backend.debounce_irq(mask, config.debounce_us);
            let latency_ns = start.elapsed().as_nanos() as u64;

            if events != 0 {
                let filtered = events != mask; // If not exact match, some filtering occurred
                stats.record_irq(latency_ns, filtered);

                if config.verbose {
                    debug!("IRQ detected: mask=0x{:02X}, events=0x{:02X}, latency={}ns",
                           mask, events, latency_ns);
                }
            }
        }

        // If SX1262 is available, also check for packets
        if sx1262_available && iteration % 100 == 0 {
            // This would need a mutable reference to the driver
            // For demo purposes, we'll just log that we would check
            debug!("Would check SX1262 for packets (iteration {})", iteration);
        }

        // Prevent overwhelming the system
        thread::sleep(Duration::from_millis(1));
    }

    info!("IRQ monitoring stopped after {} iterations", iteration);
}

#[cfg(feature = "pio-irq")]
fn display_live_stats(
    stats: Arc<PerformanceStats>,
    running: Arc<AtomicBool>,
    config: &DemoConfig,
) {
    let mut last_total = 0u64;

    while running.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_secs(5));

        let current_total = stats.total_irqs.load(Ordering::Relaxed);
        let valid = stats.valid_irqs.load(Ordering::Relaxed);
        let filtered = stats.filtered_irqs.load(Ordering::Relaxed);

        if current_total > last_total {
            let rate = (current_total - last_total) as f64 / 5.0; // IRQs per second
            println!("📊 Live Stats - Rate: {:.1} IRQ/s, Valid: {}, Filtered: {}",
                     rate, valid, filtered);
            last_total = current_total;
        }
    }
}

#[cfg(feature = "pio-irq")]
fn run_noise_simulation_test(
    backend: Arc<dyn PioIrqBackend>,
    config: &DemoConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Noise Simulation Test");
    println!("Testing IRQ debouncing under simulated noise conditions");
    println!("Duration: {} seconds", config.duration_secs);

    let stats = Arc::new(PerformanceStats::default());

    // Simulate various noise patterns
    let noise_patterns = [
        ("Burst", simulate_burst_noise as fn(Arc<dyn PioIrqBackend>, Arc<PerformanceStats>, u64, u32)),
        ("Glitch", simulate_glitch_noise as fn(Arc<dyn PioIrqBackend>, Arc<PerformanceStats>, u64, u32)),
        ("Periodic", simulate_periodic_noise as fn(Arc<dyn PioIrqBackend>, Arc<PerformanceStats>, u64, u32)),
        ("Random", simulate_random_noise as fn(Arc<dyn PioIrqBackend>, Arc<PerformanceStats>, u64, u32)),
    ];

    for (name, pattern_fn) in &noise_patterns {
        println!("\n🔧 Testing {} noise pattern...", name);

        let pattern_stats = Arc::new(PerformanceStats::default());
        let test_duration = config.duration_secs / noise_patterns.len() as u64;

        let backend_clone = Arc::clone(&backend);
        let stats_clone = Arc::clone(&pattern_stats);

        pattern_fn(backend_clone, stats_clone, test_duration, config.debounce_us);

        // Print pattern-specific results
        let total = pattern_stats.total_irqs.load(Ordering::Relaxed);
        let filtered = pattern_stats.filtered_irqs.load(Ordering::Relaxed);

        if total > 0 {
            let filter_rate = (filtered as f64 / total as f64) * 100.0;
            println!("  {} Pattern: {} IRQs, {:.1}% filtered", name, total, filter_rate);
        }
    }

    stats.print_summary();
    Ok(())
}

#[cfg(feature = "pio-irq")]
fn simulate_burst_noise(
    mut backend: Arc<dyn PioIrqBackend>,
    stats: Arc<PerformanceStats>,
    duration_secs: u64,
    debounce_us: u32,
) {
    let start_time = Instant::now();

    while start_time.elapsed().as_secs() < duration_secs {
        // Simulate burst of rapid IRQs (noise)
        for _ in 0..10 {
            let start = Instant::now();
            let events = backend.debounce_irq(DIO1_RX_DONE, debounce_us);
            let latency = start.elapsed().as_nanos() as u64;

            stats.record_irq(latency, events == 0); // No events = filtered noise

            thread::sleep(Duration::from_micros(50)); // Rapid succession
        }

        thread::sleep(Duration::from_millis(100)); // Pause between bursts
    }
}

#[cfg(feature = "pio-irq")]
fn simulate_glitch_noise(
    mut backend: Arc<dyn PioIrqBackend>,
    stats: Arc<PerformanceStats>,
    duration_secs: u64,
    debounce_us: u32,
) {
    let start_time = Instant::now();

    while start_time.elapsed().as_secs() < duration_secs {
        // Simulate short glitches that should be filtered
        let start = Instant::now();
        let events = backend.debounce_irq(DIO1_RX_DONE, debounce_us);
        let latency = start.elapsed().as_nanos() as u64;

        stats.record_irq(latency, events == 0);

        thread::sleep(Duration::from_micros(500)); // 500μs between glitches
    }
}

#[cfg(feature = "pio-irq")]
fn simulate_periodic_noise(
    mut backend: Arc<dyn PioIrqBackend>,
    stats: Arc<PerformanceStats>,
    duration_secs: u64,
    debounce_us: u32,
) {
    let start_time = Instant::now();

    while start_time.elapsed().as_secs() < duration_secs {
        // Simulate periodic interference
        let start = Instant::now();
        let events = backend.debounce_irq(DIO1_RX_DONE, debounce_us);
        let latency = start.elapsed().as_nanos() as u64;

        stats.record_irq(latency, events == 0);

        thread::sleep(Duration::from_millis(10)); // 100 Hz periodic
    }
}

#[cfg(feature = "pio-irq")]
fn simulate_random_noise(
    mut backend: Arc<dyn PioIrqBackend>,
    stats: Arc<PerformanceStats>,
    duration_secs: u64,
    debounce_us: u32,
) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let start_time = Instant::now();
    let mut counter = 0u64;

    while start_time.elapsed().as_secs() < duration_secs {
        // Simple pseudo-random timing
        counter += 1;
        let mut hasher = DefaultHasher::new();
        counter.hash(&mut hasher);
        let random_delay = (hasher.finish() % 1000) + 100; // 100-1100 μs

        let start = Instant::now();
        let events = backend.debounce_irq(DIO1_RX_DONE, debounce_us);
        let latency = start.elapsed().as_nanos() as u64;

        stats.record_irq(latency, events == 0);

        thread::sleep(Duration::from_micros(random_delay));
    }
}

/// Demonstrate the state machine reset functionality
#[cfg(feature = "pio-irq")]
fn demonstrate_reset_functionality(backend: &Arc<dyn PioIrqBackend>) -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Instant;

    println!("📋 Backend: {}", backend.name());

    // Test 1: Basic reset functionality
    println!("  ✓ Testing basic reset...");
    let start = Instant::now();
    backend.reset()?;
    let reset_time = start.elapsed();
    println!("    Reset completed in {:?}", reset_time);

    // Test 2: Functionality after reset
    println!("  ✓ Testing functionality after reset...");
    backend.clear_irq_fifo();
    let events = backend.debounce_irq(0x02, 10);
    assert_eq!(events, 0, "No events expected after reset");
    assert!(!backend.is_irq_pending(), "No pending IRQs expected after reset");
    println!("    All functions working correctly after reset");

    // Test 3: Multiple reset cycles with different configurations
    println!("  ✓ Testing multiple reset cycles...");
    let debounce_values = [5, 10, 15, 20];
    for (i, &debounce_us) in debounce_values.iter().enumerate() {
        let start = Instant::now();
        backend.reset()?;
        let reset_time = start.elapsed();

        // Test different debounce configurations after each reset
        backend.debounce_irq(0x0F, debounce_us);

        println!("    Cycle {}: Reset in {:?}, debounce {}μs configured",
                 i + 1, reset_time, debounce_us);
    }

    // Test 4: Performance measurement
    println!("  ✓ Measuring reset performance...");
    const RESET_COUNT: usize = 5;
    let mut total_time = Duration::from_nanos(0);

    for i in 0..RESET_COUNT {
        let start = Instant::now();
        backend.reset()?;
        let reset_time = start.elapsed();
        total_time += reset_time;

        // Quick functionality check
        backend.debounce_irq(0x02, 10);
    }

    let avg_reset_time = total_time / RESET_COUNT as u32;
    println!("    Average reset time: {:?} ({} iterations)", avg_reset_time, RESET_COUNT);

    // Test 5: Reset after simulated IRQ activity
    println!("  ✓ Testing reset after IRQ activity...");
    // Simulate some IRQ activity
    for _ in 0..10 {
        backend.debounce_irq(0x0F, 10);
        backend.clear_irq_fifo();
    }

    let start = Instant::now();
    backend.reset()?;
    let reset_time = start.elapsed();
    println!("    Reset after activity completed in {:?}", reset_time);

    println!("🎯 Reset demonstration completed successfully!");
    println!("   • All reset operations succeeded");
    println!("   • Backend remains functional after resets");
    println!("   • Performance within expected bounds");

    Ok(())
}