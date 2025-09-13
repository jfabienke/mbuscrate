//! Integration tests for LoRa payload decoders

#[cfg(test)]
mod tests {
    use mbuscrate::payload::record::MBusRecordValue;
    use mbuscrate::wmbus::radio::lora::decoders::*;
    use mbuscrate::wmbus::radio::lora::{
        DecentlabChannel, DecentlabConfig, DraginoModel, ElvacoModel, GenericCounterConfig,
        LoRaDeviceManager, LoRaPayloadDecoder,
    };

    #[test]
    fn test_device_manager_registration() {
        let mut manager = LoRaDeviceManager::new();

        // Register multiple devices
        manager.register_device(
            "device1".to_string(),
            Box::new(GenericCounterDecoder::new(GenericCounterConfig::default())),
        );

        manager.register_device(
            "device2".to_string(),
            Box::new(DraginoDecoder::new(DraginoModel::SW3L)),
        );

        // Test that devices use their specific decoders
        let payload = vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 85];

        let result1 = manager.decode_payload("device1", &payload, 1);
        assert!(result1.is_ok());
        assert_eq!(result1.unwrap().decoder_type, "GenericCounter");

        // Unknown device should use default (RawBinary)
        let result3 = manager.decode_payload("unknown", &payload, 1);
        assert!(result3.is_ok());
        assert_eq!(result3.unwrap().decoder_type, "RawBinary");
    }

    #[test]
    fn test_compact_frame_decoder() {
        let decoder = CompactFrameDecoder::default();

        // Test simple compact format
        let payload = vec![
            0x12, 0x34, 0x56, 0x78, // Device ID
            0x64, 0x00, 0x00, 0x00, // Counter = 100
            0x00, 0x00, // Status
            75,   // Battery
            0x00, 0x01, // Temperature = 25.6°C
        ];

        let result = decoder.decode(&payload, 1).unwrap();
        assert_eq!(result.readings.len(), 2); // Counter + Temperature
        assert_eq!(result.battery.as_ref().unwrap().percentage, Some(75));
    }

    #[test]
    fn test_generic_counter_scaling() {
        // Test with gas meter scaling
        let decoder = GenericCounterDecoder::gas_meter(1000.0); // 1000 pulses/m³

        let payload = vec![
            0xE8, 0x03, 0x00, 0x00, // 1000 pulses
            0x64, 0x00, // Delta = 100 pulses
            0x00, // Status
            90,   // Battery
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        // Check scaled value
        match &result.readings[0].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 1.0), // 1000/1000 = 1 m³
            _ => panic!("Expected numeric value"),
        }
        assert_eq!(result.readings[0].unit, "m³");

        // Check delta
        match &result.readings[1].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 0.1), // 100/1000 = 0.1 m³
            _ => panic!("Expected numeric delta"),
        }
    }

    #[test]
    fn test_decentlab_channel_parsing() {
        // Custom Decentlab configuration
        let config = DecentlabConfig {
            protocol_version: 2,
            channels: vec![
                DecentlabChannel {
                    name: "Moisture".to_string(),
                    unit: "%".to_string(),
                    scale_factor: 0.1,
                    offset: 0.0,
                },
                DecentlabChannel {
                    name: "EC".to_string(),
                    unit: "µS/cm".to_string(),
                    scale_factor: 1.0,
                    offset: 0.0,
                },
            ],
        };

        let decoder = DecentlabDecoder::new(config);

        let payload = vec![
            0x02, // Protocol
            0xAB, 0xCD, // Device ID
            0x03, // Both channels active
            0x01, 0xF4, // Moisture = 500 * 0.1 = 50%
            0x03, 0xE8, // EC = 1000 µS/cm
            0x0C, 0x1C, // Battery = 3100mV
        ];

        let result = decoder.decode(&payload, 1).unwrap();
        assert_eq!(result.readings.len(), 2);

        // Check moisture
        match &result.readings[0].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 50.0),
            _ => panic!("Expected numeric moisture"),
        }
        assert_eq!(result.readings[0].unit, "%");

        // Check EC
        match &result.readings[1].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 1000.0),
            _ => panic!("Expected numeric EC"),
        }
        assert_eq!(result.readings[1].unit, "µS/cm");
    }

    #[test]
    fn test_dragino_leak_sensor() {
        let decoder = DraginoDecoder::new(DraginoModel::LWL03A);

        // Simulate leak detected
        let payload = vec![
            0xFF, 0xFF, // Device ID
            0x01, // Leak detected
            0x03, 0x00, // 3 leak events
            0x0F, 0x00, // 15 minutes duration
            0x70, 0x0B, // Battery = 2928mV
        ];

        let result = decoder.decode(&payload, 1).unwrap();

        // Check leak status
        assert!(result.status.leak);
        assert!(result.status.alarm);

        // Check readings
        let leak_count = result
            .readings
            .iter()
            .find(|r| r.description == Some("LeakTimes".to_string()))
            .unwrap();
        match &leak_count.value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 3.0),
            _ => panic!("Expected numeric leak count"),
        }

        // Check low battery warning (2.928V is getting low)
        assert!(result.battery.as_ref().unwrap().low_battery);
    }

    #[test]
    fn test_sensative_tlv_parsing() {
        let decoder = SensativeDecoder::new();

        // TLV encoded payload with temperature and humidity
        let payload = vec![
            0x01, 0x02, 0x10, 0x09, // Temperature: Type=1, Len=2, Value=2320 (23.20°C)
            0x02, 0x01, 0x64, // Humidity: Type=2, Len=1, Value=100 (50%)
        ];

        let result = decoder.decode(&payload, 1).unwrap();
        assert_eq!(result.readings.len(), 2);

        // Check temperature
        let temp = result
            .readings
            .iter()
            .find(|r| r.quantity == "Temperature")
            .unwrap();
        match &temp.value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 23.20),
            _ => panic!("Expected numeric temperature"),
        }

        // Check humidity
        let humidity = result
            .readings
            .iter()
            .find(|r| r.quantity == "Humidity")
            .unwrap();
        match &humidity.value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 50.0),
            _ => panic!("Expected numeric humidity"),
        }
    }

    #[test]
    fn test_error_handling() {
        let decoder = GenericCounterDecoder::new(GenericCounterConfig::default());

        // Too short payload
        let short_payload = vec![0x00, 0x00];
        let result = decoder.decode(&short_payload, 1);
        assert!(result.is_err());

        // Check error type
        match result.unwrap_err() {
            mbuscrate::wmbus::radio::lora::LoRaDecodeError::InvalidLength { expected, actual } => {
                assert!(expected > actual);
            }
            _ => panic!("Expected InvalidLength error"),
        }
    }

    #[test]
    fn test_battery_voltage_conversion() {
        use mbuscrate::wmbus::radio::lora::decoder::helpers;

        // Test ADC to voltage conversion
        let voltage = helpers::adc_to_voltage(200, 3.6, 255);
        assert!((voltage - 2.824).abs() < 0.01);

        // Test voltage to percentage
        let percentage = helpers::voltage_to_percentage(3.0, 2.4, 3.6);
        assert_eq!(percentage, 50);

        // Test edge cases
        assert_eq!(helpers::voltage_to_percentage(2.4, 2.4, 3.6), 0);
        assert_eq!(helpers::voltage_to_percentage(3.6, 2.4, 3.6), 100);
        assert_eq!(helpers::voltage_to_percentage(4.0, 2.4, 3.6), 100); // Clamp to 100
    }

    #[test]
    fn test_auto_detection() {
        let mut manager = LoRaDeviceManager::new();

        // Register a Decentlab decoder
        let decentlab = Box::new(DecentlabDecoder::dl_pr26());
        manager.register_device("test".to_string(), decentlab);

        // Try to auto-detect with a Decentlab-formatted payload
        let decentlab_payload = vec![
            0x02, // Protocol v2
            0x00, 0x01, // Device ID
            0x01, // Channel flags
            0x00, 0x00, // Data
            0x0C, 0x1C, // Battery
        ];

        let detected = manager.auto_detect_decoder(&decentlab_payload, 1);
        assert!(detected.is_some());
        assert_eq!(detected.unwrap(), "Decentlab");
    }
}
