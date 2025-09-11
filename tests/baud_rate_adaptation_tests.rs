use mbus_rs::mbus::serial::{MBusBaudRate, SerialConfig, CollisionConfig, CollisionStatistics};
use std::time::Duration;

#[test]
fn test_mbus_baud_rate_enum() {
    // Test baud rate enum functionality
    assert_eq!(MBusBaudRate::Baud2400.as_u32(), 2400);
    assert_eq!(MBusBaudRate::Baud9600.as_u32(), 9600);
    
    // Test conversion from u32
    assert_eq!(MBusBaudRate::from(2400), MBusBaudRate::Baud2400);
    assert_eq!(MBusBaudRate::from(9600), MBusBaudRate::Baud9600);
    assert_eq!(MBusBaudRate::from(999999), MBusBaudRate::Baud2400); // Fallback
}

#[test]
fn test_baud_rate_timeouts() {
    // Test that timeout calculation is proper for different baud rates
    assert!(MBusBaudRate::Baud300.timeout() > MBusBaudRate::Baud9600.timeout());
    assert!(MBusBaudRate::Baud9600.timeout() > MBusBaudRate::Baud38400.timeout());
    
    // Verify reasonable timeout ranges
    assert!(MBusBaudRate::Baud300.timeout() >= Duration::from_millis(1000));
    assert!(MBusBaudRate::Baud38400.timeout() <= Duration::from_millis(200));
}

#[test]
fn test_inter_frame_delay() {
    // Test inter-frame delay calculation
    assert!(MBusBaudRate::Baud300.inter_frame_delay() > MBusBaudRate::Baud9600.inter_frame_delay());
    assert!(MBusBaudRate::Baud9600.inter_frame_delay() > MBusBaudRate::Baud38400.inter_frame_delay());
    
    // Verify reasonable delay ranges
    assert!(MBusBaudRate::Baud300.inter_frame_delay() >= Duration::from_millis(50));
    assert!(MBusBaudRate::Baud38400.inter_frame_delay() <= Duration::from_millis(10));
}

#[test]
fn test_collision_config_default() {
    let config = CollisionConfig::default();
    assert_eq!(config.max_collision_retries, 5);
    assert_eq!(config.initial_backoff_ms, 10);
    assert_eq!(config.max_backoff_ms, 500);
    assert_eq!(config.collision_threshold, 2);
}

#[test]
fn test_collision_statistics() {
    let mut stats = CollisionStatistics::default();
    
    // Initial state
    assert_eq!(stats.collision_rate, 0.0);
    assert!(!stats.is_high_collision_rate(30.0));
    
    // Add some collisions
    stats.total_collisions = 3;
    stats.successful_communications = 7;
    stats.update_collision_rate();
    
    // Should be 30% collision rate
    assert_eq!(stats.collision_rate, 30.0);
    assert!(!stats.is_high_collision_rate(30.0)); // Equal to threshold
    assert!(stats.is_high_collision_rate(25.0));  // Above threshold
    
    // Add more collisions
    stats.total_collisions = 5;
    stats.update_collision_rate();
    
    // Should be 5/12 = 41.67% collision rate
    assert!((stats.collision_rate - 41.666666666666664).abs() < 0.001);
    assert!(stats.is_high_collision_rate(30.0));
}

#[test]
fn test_serial_config_with_auto_baud() {
    let config = SerialConfig {
        baudrate: 2400,
        timeout: Duration::from_secs(5),
        auto_baud_detection: true,
        collision_config: CollisionConfig::default(),
    };
    
    assert_eq!(config.baudrate, 2400);
    assert!(config.auto_baud_detection);
    assert_eq!(config.collision_config.max_collision_retries, 5);
}

#[test]
fn test_baud_rate_priority_order() {
    // Test that the baud rate priority order makes sense
    // 2400 should be first (most common)
    assert_eq!(MBusBaudRate::ALL_RATES[0], MBusBaudRate::Baud2400);
    
    // 9600 should be second (second most common)
    assert_eq!(MBusBaudRate::ALL_RATES[1], MBusBaudRate::Baud9600);
    
    // Should have all 8 standard rates
    assert_eq!(MBusBaudRate::ALL_RATES.len(), 8);
    
    // All rates should be unique
    let mut rates = MBusBaudRate::ALL_RATES.to_vec();
    rates.sort_by_key(|r| r.as_u32());
    rates.dedup();
    assert_eq!(rates.len(), 8);
}

#[test]
fn test_standards_compliance() {
    // EN 13757-2 Section 4.2.8 specifies these exact baud rates
    let expected_rates = [300, 600, 1200, 2400, 4800, 9600, 19200, 38400];
    let actual_rates: Vec<u32> = MBusBaudRate::ALL_RATES.iter().map(|r| r.as_u32()).collect();
    
    for expected in &expected_rates {
        assert!(actual_rates.contains(expected), "Missing standard baud rate: {}", expected);
    }
}

#[test]
fn test_timeout_scaling() {
    // Lower baud rates should have proportionally longer timeouts
    let baud_300_timeout = MBusBaudRate::Baud300.timeout().as_millis();
    let baud_9600_timeout = MBusBaudRate::Baud9600.timeout().as_millis();
    
    // 300 baud should have at least 3x longer timeout than 9600 baud
    assert!(baud_300_timeout >= baud_9600_timeout * 3);
}