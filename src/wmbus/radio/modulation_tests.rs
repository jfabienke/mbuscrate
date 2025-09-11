//! Tests for enhanced ToA calculator and LBT support

#[cfg(test)]
mod tests {
    use crate::wmbus::radio::modulation::{TimeOnAir, EncodingType, ListenBeforeTalk};
    
    #[test]
    fn test_toa_s_mode() {
        let toa = TimeOnAir::s_mode(100); // 100-byte frame
        let time_ms = toa.calculate_ms();
        
        // S-mode: Manchester encoding doubles the bits
        // (48 preamble + 16 sync + (100 + 2 CRC) * 8 data) * 2 encoding / 32768 bps * 1000 ms
        let expected_bits = (48 + 16 + 102 * 8) * 2; // 1760 bits
        let expected_ms = (expected_bits as f64 / 32768.0) * 1000.0;
        
        assert!((time_ms - expected_ms).abs() < 0.1);
        println!("S-mode ToA for 100 bytes: {:.2} ms", time_ms);
    }
    
    #[test]
    fn test_toa_t_mode() {
        let toa = TimeOnAir::t_mode(100); // 100-byte frame
        let time_ms = toa.calculate_ms();
        
        // T-mode: 3-out-of-6 encoding multiplies by 1.5
        // (48 preamble + 16 sync + (100 + 2 CRC) * 8 data) * 1.5 encoding / 100000 bps * 1000 ms
        let expected_bits = ((48 + 16 + 102 * 8) * 3) / 2; // 1320 bits
        let expected_ms = (expected_bits as f64 / 100000.0) * 1000.0;
        
        assert!((time_ms - expected_ms).abs() < 0.1);
        println!("T-mode ToA for 100 bytes: {:.2} ms", time_ms);
    }
    
    #[test]
    fn test_toa_c_mode() {
        let toa = TimeOnAir::c_mode(100); // 100-byte frame
        let time_ms = toa.calculate_ms();
        
        // C-mode: NRZ encoding has no overhead
        // (48 preamble + 16 sync + (100 + 2 CRC) * 8 data) / 100000 bps * 1000 ms
        let expected_bits = 48 + 16 + 102 * 8; // 880 bits
        let expected_ms = (expected_bits as f64 / 100000.0) * 1000.0;
        
        assert!((time_ms - expected_ms).abs() < 0.1);
        println!("C-mode ToA for 100 bytes: {:.2} ms", time_ms);
        
        // C-mode should be faster than T-mode for same data
        let t_mode_toa = TimeOnAir::t_mode(100);
        assert!(time_ms < t_mode_toa.calculate_ms());
    }
    
    #[test]
    fn test_duty_cycle_compliance() {
        // Test with 50-byte frame in different modes
        let s_mode = TimeOnAir::s_mode(50);
        let t_mode = TimeOnAir::t_mode(50);
        let c_mode = TimeOnAir::c_mode(50);
        
        // Calculate max transmissions per hour for 0.9% duty cycle
        let s_max = s_mode.max_transmissions_per_hour();
        let t_max = t_mode.max_transmissions_per_hour();
        let c_max = c_mode.max_transmissions_per_hour();
        
        println!("Max transmissions per hour (0.9% duty cycle):");
        println!("  S-mode: {} transmissions", s_max);
        println!("  T-mode: {} transmissions", t_max);
        println!("  C-mode: {} transmissions", c_max);
        
        // C-mode should allow most transmissions (least overhead)
        assert!(c_max > t_max);
        assert!(c_max > s_max);
        
        // Verify duty cycle calculation
        assert!(s_mode.check_duty_cycle(s_max));
        assert!(!s_mode.check_duty_cycle(s_max + 1)); // Exceeds limit
    }
    
    #[test]
    fn test_lbt_channel_clear() {
        let lbt = ListenBeforeTalk::new_etsi();
        
        // Test channel clear detection
        assert!(lbt.is_channel_clear(-90)); // -90 dBm < -85 dBm threshold
        assert!(lbt.is_channel_clear(-86)); // -86 dBm < -85 dBm threshold
        assert!(!lbt.is_channel_clear(-84)); // -84 dBm > -85 dBm threshold
        assert!(!lbt.is_channel_clear(-80)); // -80 dBm > -85 dBm threshold
    }
    
    #[test]
    fn test_lbt_exponential_backoff() {
        let mut lbt = ListenBeforeTalk::new_etsi();
        
        // Test exponential backoff
        let backoff1 = lbt.calculate_backoff_ms(); // 2^0 * 10 = 10ms
        assert_eq!(backoff1, 10);
        
        let backoff2 = lbt.calculate_backoff_ms(); // 2^1 * 10 = 20ms
        assert_eq!(backoff2, 20);
        
        let backoff3 = lbt.calculate_backoff_ms(); // 2^2 * 10 = 40ms
        assert_eq!(backoff3, 40);
        
        // Test max backoff cap
        for _ in 0..20 {
            lbt.calculate_backoff_ms();
        }
        let backoff_max = lbt.calculate_backoff_ms();
        assert_eq!(backoff_max, 1000); // Capped at max_backoff_ms
        
        // Test reset
        lbt.reset_backoff();
        let backoff_reset = lbt.calculate_backoff_ms();
        assert_eq!(backoff_reset, 10); // Back to initial
    }
    
    #[test]
    fn test_lbt_total_access_time() {
        let lbt = ListenBeforeTalk::new_etsi();
        let toa = TimeOnAir::c_mode(50);
        let toa_ms = toa.calculate_ms();
        
        // Total access time includes LBT listening time
        let total_ms = lbt.total_access_time_ms(toa_ms);
        assert_eq!(total_ms, lbt.min_listen_time_ms as f64 + toa_ms);
        
        println!("C-mode 50-byte frame:");
        println!("  ToA: {:.2} ms", toa_ms);
        println!("  LBT: {} ms", lbt.min_listen_time_ms);
        println!("  Total: {:.2} ms", total_ms);
    }
    
    #[test]
    fn test_encoding_comparison() {
        // Compare all encoding types for same frame size
        let frame_size = 75;
        
        let manchester = TimeOnAir {
            frame_bytes: frame_size,
            preamble_bits: 48,
            sync_bits: 16,
            crc_bytes: 2,
            bitrate: 100000, // Same bitrate for fair comparison
            encoding: EncodingType::Manchester,
        };
        
        let three_six = TimeOnAir {
            frame_bytes: frame_size,
            preamble_bits: 48,
            sync_bits: 16,
            crc_bytes: 2,
            bitrate: 100000,
            encoding: EncodingType::ThreeOutOfSix,
        };
        
        let nrz = TimeOnAir {
            frame_bytes: frame_size,
            preamble_bits: 48,
            sync_bits: 16,
            crc_bytes: 2,
            bitrate: 100000,
            encoding: EncodingType::Nrz,
        };
        
        let manchester_ms = manchester.calculate_ms();
        let three_six_ms = three_six.calculate_ms();
        let nrz_ms = nrz.calculate_ms();
        
        println!("Encoding comparison for {}-byte frame:", frame_size);
        println!("  Manchester (2×): {:.2} ms", manchester_ms);
        println!("  3-out-of-6 (1.5×): {:.2} ms", three_six_ms);
        println!("  NRZ (1×): {:.2} ms", nrz_ms);
        
        // Verify encoding overhead ratios
        assert!((manchester_ms / nrz_ms - 2.0).abs() < 0.01); // 2× overhead
        assert!((three_six_ms / nrz_ms - 1.5).abs() < 0.01); // 1.5× overhead
    }
}