//! Tests for LoRa enhancements based on SX126x application notes

#[cfg(test)]
mod tests {
    use mbus_rs::wmbus::radio::lora::{
        cad::{CadExitMode, CadStats, LoRaCadParams},
        params::{SyncWords, LoRaModParams, LoRaModParamsExt, LoRaPacketParams},
        CodingRate, LoRaBandwidth, SpreadingFactor,
    };

    // ========================== DEFAULT CONFIGURATION TESTS ==========================

    #[test]
    fn test_default_mod_params() {
        let params = LoRaModParams::default();

        // Should match SX126x User Guide defaults
        assert_eq!(params.sf, SpreadingFactor::SF7);
        assert_eq!(params.bw, LoRaBandwidth::BW500);
        assert_eq!(params.cr, CodingRate::CR4_5);
        assert!(!params.low_data_rate_optimize);
    }

    #[test]
    fn test_default_packet_params() {
        let params = LoRaPacketParams::default();

        // Should have sensible defaults
        assert_eq!(params.preamble_len, 8);
        assert!(!params.implicit_header);
        assert_eq!(params.payload_len, 64);
        assert!(params.crc_on);
        assert!(!params.iq_inverted);
    }

    #[test]
    fn test_regional_defaults() {
        // EU868 defaults
        let eu_params = LoRaModParams::eu868_defaults();
        assert_eq!(eu_params.sf, SpreadingFactor::SF9);
        assert_eq!(eu_params.bw, LoRaBandwidth::BW125);

        // US915 defaults (should match standard defaults)
        let us_params = LoRaModParams::us915_defaults();
        assert_eq!(us_params.sf, SpreadingFactor::SF7);
        assert_eq!(us_params.bw, LoRaBandwidth::BW500);

        // AS923 defaults
        let as_params = LoRaModParams::as923_defaults();
        assert_eq!(as_params.sf, SpreadingFactor::SF8);
        assert_eq!(as_params.bw, LoRaBandwidth::BW125);
    }

    #[test]
    fn test_param_validation() {
        // Valid configuration
        let valid = LoRaModParams {
            sf: SpreadingFactor::SF9,
            bw: LoRaBandwidth::BW125,
            cr: CodingRate::CR4_5,
            low_data_rate_optimize: false,
        };
        assert!(valid.validate().is_ok());

        // Invalid: SF12 with BW500 (excessive time on air)
        let invalid1 = LoRaModParams {
            sf: SpreadingFactor::SF12,
            bw: LoRaBandwidth::BW500,
            cr: CodingRate::CR4_5,
            low_data_rate_optimize: false,
        };
        assert!(invalid1.validate().is_err());

        // Invalid: SF12 with very low bandwidth (ultra-low data rate)
        let invalid2 = LoRaModParams {
            sf: SpreadingFactor::SF12,
            bw: LoRaBandwidth::BW7_8,
            cr: CodingRate::CR4_8,
            low_data_rate_optimize: true,
        };
        assert!(invalid2.validate().is_err());
    }

    // ========================== CAD PARAMETER TESTS ==========================

    #[test]
    fn test_cad_optimal_params() {
        // Test BW125 optimal values from AN1200.48 Table 1
        let params_sf7 = LoRaCadParams::optimal(SpreadingFactor::SF7, LoRaBandwidth::BW125);
        assert_eq!(params_sf7.symbol_num, 2);
        assert_eq!(params_sf7.det_peak, 22);
        assert_eq!(params_sf7.det_min, 10);
        assert_eq!(params_sf7.exit_mode, CadExitMode::CadOnly);

        let params_sf10 = LoRaCadParams::optimal(SpreadingFactor::SF10, LoRaBandwidth::BW125);
        assert_eq!(params_sf10.symbol_num, 4);
        assert_eq!(params_sf10.det_peak, 21);

        let params_sf12 = LoRaCadParams::optimal(SpreadingFactor::SF12, LoRaBandwidth::BW125);
        assert_eq!(params_sf12.symbol_num, 8);
        assert_eq!(params_sf12.det_peak, 20);

        // Test BW500 optimal values from AN1200.48 Table 43
        let params_sf7_bw500 = LoRaCadParams::optimal(SpreadingFactor::SF7, LoRaBandwidth::BW500);
        assert_eq!(params_sf7_bw500.symbol_num, 4);
        assert_eq!(params_sf7_bw500.det_peak, 21);

        let params_sf12_bw500 = LoRaCadParams::optimal(SpreadingFactor::SF12, LoRaBandwidth::BW500);
        assert_eq!(params_sf12_bw500.symbol_num, 16);
        assert_eq!(params_sf12_bw500.det_peak, 19);
    }

    #[test]
    fn test_cad_fast_detect() {
        let params = LoRaCadParams::fast_detect(SpreadingFactor::SF10, LoRaBandwidth::BW125);

        // Should use minimal symbols for speed
        assert_eq!(params.symbol_num, 2);
        // Should have slightly stricter threshold
        assert_eq!(params.det_peak, 22);
    }

    #[test]
    fn test_cad_high_reliability() {
        let params = LoRaCadParams::high_reliability(SpreadingFactor::SF7, LoRaBandwidth::BW125);

        // Should use more symbols for accuracy
        assert_eq!(params.symbol_num, 4);
        // Should have raised noise floor
        assert_eq!(params.det_min, 12);
    }

    #[test]
    fn test_cad_duration() {
        let params = LoRaCadParams::optimal(SpreadingFactor::SF7, LoRaBandwidth::BW125);
        let duration = params.duration_ms(SpreadingFactor::SF7, LoRaBandwidth::BW125);

        // SF7, BW125: T_sym = 128/125000 * 1000 = ~1ms
        // 2 symbols = ~2ms
        assert!(duration >= 1 && duration <= 3);

        let params_sf12 = LoRaCadParams::optimal(SpreadingFactor::SF12, LoRaBandwidth::BW125);
        let duration_sf12 = params_sf12.duration_ms(SpreadingFactor::SF12, LoRaBandwidth::BW125);

        // SF12, BW125: T_sym = 4096/125000 * 1000 = ~33ms
        // 8 symbols = ~264ms
        assert!(duration_sf12 >= 200 && duration_sf12 <= 300);
    }

    #[test]
    fn test_cad_narrow_bandwidth() {
        // Test narrow bandwidth configurations
        let params = LoRaCadParams::optimal(SpreadingFactor::SF10, LoRaBandwidth::BW62_5);

        // Narrow BW should use more symbols
        assert!(params.symbol_num >= 4);

        // Very narrow bandwidth
        let params_narrow = LoRaCadParams::optimal(SpreadingFactor::SF12, LoRaBandwidth::BW7_8);
        assert_eq!(params_narrow.symbol_num, 16); // Maximum symbols for reliability
    }

    // ========================== CAD STATISTICS TESTS ==========================

    #[test]
    fn test_cad_stats() {
        let mut stats = CadStats::default();

        // Record some CAD operations
        stats.record_cad(true, 10);
        stats.record_cad(false, 12);
        stats.record_cad(true, 8);
        stats.record_cad(false, 14);

        assert_eq!(stats.total_cad_operations, 4);
        assert_eq!(stats.activity_detected, 2);
        assert_eq!(stats.channel_clear, 2);

        // Check detection rate (50%)
        assert!((stats.detection_rate() - 0.5).abs() < 0.01);

        // Check average duration
        assert!((stats.avg_duration_ms - 11.0).abs() < 0.1);
    }

    #[test]
    fn test_cad_stats_reset() {
        let mut stats = CadStats::default();

        stats.record_cad(true, 10);
        stats.record_cad(false, 20);

        assert_eq!(stats.total_cad_operations, 2);

        stats.reset();

        assert_eq!(stats.total_cad_operations, 0);
        assert_eq!(stats.activity_detected, 0);
        assert_eq!(stats.channel_clear, 0);
        assert_eq!(stats.avg_duration_ms, 0.0);
    }

    // ========================== SYNC WORD TESTS ==========================

    #[test]
    fn test_sync_words() {
        // Test predefined sync words
        assert_eq!(SyncWords::PUBLIC, [0x34, 0x44]);
        assert_eq!(SyncWords::PRIVATE, [0x14, 0x24]);
        assert_eq!(SyncWords::CUSTOM, [0x12, 0x34]);
    }

    // ========================== INTEGRATION TESTS ==========================

    #[test]
    fn test_sf_bw_combinations() {
        // Test various SF/BW combinations for CAD
        let test_cases = vec![
            (SpreadingFactor::SF5, LoRaBandwidth::BW500),
            (SpreadingFactor::SF7, LoRaBandwidth::BW125),
            (SpreadingFactor::SF9, LoRaBandwidth::BW250),
            (SpreadingFactor::SF10, LoRaBandwidth::BW62_5),
            (SpreadingFactor::SF11, LoRaBandwidth::BW31_2),
            (SpreadingFactor::SF12, LoRaBandwidth::BW125),
        ];

        for (sf, bw) in test_cases {
            let params = LoRaCadParams::optimal(sf, bw);

            // All should have valid parameters
            assert!(params.symbol_num >= 2 && params.symbol_num <= 16);
            assert!(params.det_peak >= 19 && params.det_peak <= 22);
            assert!(params.det_min >= 10 && params.det_min <= 15);

            // Duration should be reasonable (up to 2 seconds for SF12 with many symbols)
            let duration = params.duration_ms(sf, bw);
            assert!(duration > 0 && duration < 2000,
                    "Unexpected duration for SF{:?}/BW{:?}: {}ms", sf, bw, duration);
        }
    }

    #[test]
    fn test_ldro_with_defaults() {
        use mbus_rs::wmbus::radio::lora::params::requires_ldro;

        // LDRO should be required for SF11/SF12 with BW <= 125kHz
        assert!(requires_ldro(SpreadingFactor::SF11, LoRaBandwidth::BW125));
        assert!(requires_ldro(SpreadingFactor::SF12, LoRaBandwidth::BW125));
        assert!(requires_ldro(SpreadingFactor::SF11, LoRaBandwidth::BW62_5));

        // LDRO not required for higher bandwidths
        assert!(!requires_ldro(SpreadingFactor::SF11, LoRaBandwidth::BW250));
        assert!(!requires_ldro(SpreadingFactor::SF12, LoRaBandwidth::BW500));

        // LDRO not required for lower SF
        assert!(!requires_ldro(SpreadingFactor::SF10, LoRaBandwidth::BW125));
        assert!(!requires_ldro(SpreadingFactor::SF7, LoRaBandwidth::BW62_5));
    }
}