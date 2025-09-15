#[cfg(test)]
mod tests {
    use mbus_rs::wmbus::radio::lora::params::{
        lora_bitrate_hz, class_a_window_delay_sf, get_lora_sensitivity_dbm,
        get_min_snr_db, requires_ldro
    };
    use mbus_rs::wmbus::radio::lora::{CodingRate, LoRaBandwidth, SpreadingFactor};

    #[test]
    fn test_lora_bitrate_calc() {
        // SF7, BW125, CR4/5: ~5.47 kbps
        // BR = 7 * (125000 / 2^7) * (4/5) = 7 * 976.5625 * 0.8 = 5468.75
        let bitrate = lora_bitrate_hz(
            SpreadingFactor::SF7,
            LoRaBandwidth::BW125,
            CodingRate::CR4_5,
        );
        assert!((bitrate - 5469.0).abs() < 10.0, "SF7 bitrate: {}", bitrate);

        // SF12, BW125, CR4/8: ~183 bps
        // BR = 12 * (125000 / 2^12) * (4/8) = 12 * 30.52 * 0.5 = 183.12
        let low_bitrate = lora_bitrate_hz(
            SpreadingFactor::SF12,
            LoRaBandwidth::BW125,
            CodingRate::CR4_8,
        );
        assert!((low_bitrate - 183.0).abs() < 10.0, "SF12 bitrate: {}", low_bitrate);
    }

    #[test]
    fn test_class_a_window_delay() {
        let (delay1, delay2) = class_a_window_delay_sf(SpreadingFactor::SF10);
        assert_eq!(delay1.as_millis(), 100); // Approximate
        assert_eq!(delay2.as_millis(), 1000);
    }

    #[test]
    fn test_ldro_requirements() {
        // Test LDRO required for SF11/SF12 with BW <= 125kHz
        assert!(requires_ldro(SpreadingFactor::SF11, LoRaBandwidth::BW125));
        assert!(requires_ldro(SpreadingFactor::SF12, LoRaBandwidth::BW125));
        assert!(requires_ldro(SpreadingFactor::SF11, LoRaBandwidth::BW62_5));
        assert!(requires_ldro(SpreadingFactor::SF12, LoRaBandwidth::BW31_2));
        assert!(requires_ldro(SpreadingFactor::SF11, LoRaBandwidth::BW7_8));

        // Test LDRO not required for SF11/SF12 with BW > 125kHz
        assert!(!requires_ldro(SpreadingFactor::SF11, LoRaBandwidth::BW250));
        assert!(!requires_ldro(SpreadingFactor::SF12, LoRaBandwidth::BW500));

        // Test LDRO not required for lower SF regardless of BW
        assert!(!requires_ldro(SpreadingFactor::SF7, LoRaBandwidth::BW125));
        assert!(!requires_ldro(SpreadingFactor::SF8, LoRaBandwidth::BW62_5));
        assert!(!requires_ldro(SpreadingFactor::SF9, LoRaBandwidth::BW31_2));
        assert!(!requires_ldro(SpreadingFactor::SF10, LoRaBandwidth::BW7_8));
    }

    #[test]
    fn test_sensitivity_values() {
        // Test sensitivity at 125kHz bandwidth (reference values from AN1200.22)
        assert_eq!(get_lora_sensitivity_dbm(SpreadingFactor::SF5, LoRaBandwidth::BW125), -124);
        assert_eq!(get_lora_sensitivity_dbm(SpreadingFactor::SF6, LoRaBandwidth::BW125), -127);
        assert_eq!(get_lora_sensitivity_dbm(SpreadingFactor::SF7, LoRaBandwidth::BW125), -130);
        assert_eq!(get_lora_sensitivity_dbm(SpreadingFactor::SF8, LoRaBandwidth::BW125), -133);
        assert_eq!(get_lora_sensitivity_dbm(SpreadingFactor::SF9, LoRaBandwidth::BW125), -136);
        assert_eq!(get_lora_sensitivity_dbm(SpreadingFactor::SF10, LoRaBandwidth::BW125), -139);
        assert_eq!(get_lora_sensitivity_dbm(SpreadingFactor::SF11, LoRaBandwidth::BW125), -141);
        assert_eq!(get_lora_sensitivity_dbm(SpreadingFactor::SF12, LoRaBandwidth::BW125), -144);

        // Test bandwidth adjustments
        // Lower BW = better sensitivity
        assert_eq!(get_lora_sensitivity_dbm(SpreadingFactor::SF7, LoRaBandwidth::BW62_5), -131);
        assert_eq!(get_lora_sensitivity_dbm(SpreadingFactor::SF7, LoRaBandwidth::BW31_2), -132);
        assert_eq!(get_lora_sensitivity_dbm(SpreadingFactor::SF7, LoRaBandwidth::BW7_8), -136);

        // Higher BW = worse sensitivity
        assert_eq!(get_lora_sensitivity_dbm(SpreadingFactor::SF7, LoRaBandwidth::BW250), -127);
        assert_eq!(get_lora_sensitivity_dbm(SpreadingFactor::SF7, LoRaBandwidth::BW500), -124);
    }

    #[test]
    fn test_min_snr_requirements() {
        // Test minimum SNR values from AN1200.22
        assert_eq!(get_min_snr_db(SpreadingFactor::SF5), -5.0);
        assert_eq!(get_min_snr_db(SpreadingFactor::SF6), -7.5);
        assert_eq!(get_min_snr_db(SpreadingFactor::SF7), -7.5);
        assert_eq!(get_min_snr_db(SpreadingFactor::SF8), -10.0);
        assert_eq!(get_min_snr_db(SpreadingFactor::SF9), -12.5);
        assert_eq!(get_min_snr_db(SpreadingFactor::SF10), -15.0);
        assert_eq!(get_min_snr_db(SpreadingFactor::SF11), -17.5);
        assert_eq!(get_min_snr_db(SpreadingFactor::SF12), -20.0);
    }

    #[test]
    fn test_bitrate_variations() {
        // Test various SF/BW/CR combinations
        struct TestCase {
            sf: SpreadingFactor,
            bw: LoRaBandwidth,
            cr: CodingRate,
            expected_bps: f64,
            tolerance: f64,
        }

        let test_cases = vec![
            // Fast data rates
            TestCase {
                sf: SpreadingFactor::SF5,
                bw: LoRaBandwidth::BW500,
                cr: CodingRate::CR4_5,
                expected_bps: 62500.0,  // 5 * (500000/32) * 0.8
                tolerance: 100.0,
            },
            TestCase {
                sf: SpreadingFactor::SF7,
                bw: LoRaBandwidth::BW500,
                cr: CodingRate::CR4_5,
                expected_bps: 21875.0,  // 7 * (500000/128) * 0.8
                tolerance: 100.0,
            },
            // Medium data rates
            TestCase {
                sf: SpreadingFactor::SF9,
                bw: LoRaBandwidth::BW125,
                cr: CodingRate::CR4_5,
                expected_bps: 1758.0,  // 9 * (125000/512) * 0.8
                tolerance: 10.0,
            },
            // Slow data rates
            TestCase {
                sf: SpreadingFactor::SF12,
                bw: LoRaBandwidth::BW62_5,
                cr: CodingRate::CR4_8,
                expected_bps: 92.0,  // 12 * (62500/4096) * 0.5
                tolerance: 5.0,
            },
            // Ultra-slow for maximum range
            TestCase {
                sf: SpreadingFactor::SF12,
                bw: LoRaBandwidth::BW7_8,
                cr: CodingRate::CR4_8,
                expected_bps: 11.0,  // 12 * (7800/4096) * 0.5
                tolerance: 2.0,
            },
        ];

        for tc in test_cases {
            let bitrate = lora_bitrate_hz(tc.sf, tc.bw, tc.cr);
            assert!(
                (bitrate - tc.expected_bps).abs() < tc.tolerance,
                "SF{:?} BW{:?} CR{:?}: expected {} bps, got {} bps",
                tc.sf, tc.bw, tc.cr, tc.expected_bps, bitrate
            );
        }
    }
}
