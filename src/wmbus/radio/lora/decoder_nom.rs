//! Nom-based decoder utilities and adapter for LoRa payloads
//!
//! This module provides integration between nom parser combinators and the LoRa decoder trait.

use super::decoder::{LoRaDecodeError, LoRaPayloadDecoder, MeteringData};
use nom::IResult;

/// Adapter to convert nom parsers into LoRaPayloadDecoder trait
#[derive(Clone)]
pub struct NomDecoderAdapter<F> {
    parser: F,
    decoder_type: String,
}

impl<F> NomDecoderAdapter<F> {
    /// Create a new adapter from a nom parser function
    pub fn new(parser: F, decoder_type: impl Into<String>) -> Self {
        Self {
            parser,
            decoder_type: decoder_type.into(),
        }
    }
}

impl<F> std::fmt::Debug for NomDecoderAdapter<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NomDecoderAdapter")
            .field("decoder_type", &self.decoder_type)
            .finish()
    }
}

impl<F> LoRaPayloadDecoder for NomDecoderAdapter<F>
where
    F: Fn(&[u8]) -> IResult<&[u8], MeteringData> + Send + Sync + Clone + 'static,
{
    fn decode(&self, payload: &[u8], _f_port: u8) -> Result<MeteringData, LoRaDecodeError> {
        match (self.parser)(payload) {
            Ok((remaining, data)) => {
                // Check if entire payload was consumed
                if !remaining.is_empty() {
                    // Log warning but still return data
                    log::debug!(
                        "Decoder {} left {} bytes unparsed",
                        self.decoder_type,
                        remaining.len()
                    );
                }
                Ok(data)
            }
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => {
                Err(LoRaDecodeError::InvalidData {
                    offset: payload.len() - e.input.len(),
                    reason: format!("{:?}", e.code),
                })
            }
            Err(nom::Err::Incomplete(needed)) => {
                let expected = match needed {
                    nom::Needed::Unknown => payload.len() + 1,
                    nom::Needed::Size(n) => payload.len() + n.get(),
                };
                Err(LoRaDecodeError::InvalidLength {
                    expected,
                    actual: payload.len(),
                })
            }
        }
    }

    fn decoder_type(&self) -> &str {
        &self.decoder_type
    }

    fn clone_box(&self) -> Box<dyn LoRaPayloadDecoder> {
        Box::new(self.clone())
    }
}

/// Helper macro to create a nom-based decoder
#[macro_export]
macro_rules! nom_decoder {
    ($name:ident, $parser:expr) => {
        Box::new(
            $crate::wmbus::radio::lora::decoder_nom::NomDecoderAdapter::new(
                $parser,
                stringify!($name),
            ),
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::record::MBusRecordValue;
    use crate::wmbus::radio::lora::decoder::{BatteryStatus, DeviceStatus, Reading};
    use nom::number::complete::le_u32;
    use std::time::SystemTime;

    #[test]
    fn test_nom_adapter() {
        // Simple parser that reads a u32 counter
        fn parse_counter(input: &[u8]) -> IResult<&[u8], MeteringData> {
            let (input, counter) = le_u32(input)?;

            Ok((
                input,
                MeteringData {
                    timestamp: SystemTime::now(),
                    readings: vec![Reading {
                        value: MBusRecordValue::Numeric(counter as f64),
                        unit: "units".to_string(),
                        quantity: "Count".to_string(),
                        tariff: None,
                        storage_number: None,
                        description: None,
                    }],
                    battery: None,
                    status: DeviceStatus::default(),
                    raw_payload: input.to_vec(),
                    decoder_type: "TestCounter".to_string(),
                },
            ))
        }

        let adapter = NomDecoderAdapter::new(parse_counter, "TestCounter");

        // Test successful parse
        let payload = vec![0x10, 0x00, 0x00, 0x00]; // 16 in little-endian
        let result = adapter.decode(&payload, 1).unwrap();

        assert_eq!(result.readings.len(), 1);
        match &result.readings[0].value {
            MBusRecordValue::Numeric(val) => assert_eq!(*val, 16.0),
            _ => panic!("Expected numeric value"),
        }

        // Test incomplete data
        let short_payload = vec![0x10, 0x00];
        let err = adapter.decode(&short_payload, 1);
        assert!(matches!(err, Err(LoRaDecodeError::InvalidLength { .. })));
    }
}
