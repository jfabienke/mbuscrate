#[cfg(test)]
mod tests {
    use mbus_rs::wmbus::radio::lora::params::{lora_bitrate_hz, class_a_window_delay_sf};
    use mbus_rs::wmbus::radio::modulation::{CodingRate, LoRaBandwidth, SpreadingFactor};

    #[test]
    fn test_lora_bitrate_calc() {
        // SF7, BW125, CR4/5: ~5.47 kbps
        let bitrate = lora_bitrate_hz(
            SpreadingFactor::SF7,
            LoRaBandwidth::BW125,
            CodingRate::CR4_5,
        );
        assert!((bitrate - 5468.0).abs() < 1.0);

        // SF12, BW125, CR4/8: ~0.25 kbps
        let low_bitrate = lora_bitrate_hz(
            SpreadingFactor::SF12,
            LoRaBandwidth::BW125,
            CodingRate::CR4_8,
        );
        assert!((low_bitrate - 250.0).abs() < 1.0);
    }

    #[test]
    fn test_class_a_window_delay() {
        let (delay1, delay2) = class_a_window_delay_sf(SpreadingFactor::SF10);
        assert_eq!(delay1.as_millis(), 100); // Approximate
        assert_eq!(delay2.as_millis(), 1000);
    }
}
