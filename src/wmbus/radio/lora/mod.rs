//! LoRa support for SX126x radio
//!
//! This module provides LoRa-specific types and utilities for the SX126x driver.
//! It includes parameter definitions, packet parsing, payload decoders, and helpers
//! for OTAA/ABP handling in non-LoRaWAN metering gateways.

pub mod adr;
pub mod cad;
pub mod channel_hopping;
pub mod class_bc;
pub mod decoder;
pub mod decoder_nom;
pub mod decoders;
pub mod duty_cycle;
pub mod format_detector;
pub mod irq_queue;
pub mod lbm;
pub mod packet;
pub mod params;
pub mod single_channel;
pub mod smart_decoder;

// SX1262 driver with PIO IRQ integration
#[cfg(feature = "pio-irq")]
pub mod sx1262;

pub use adr::{AdrController, AdrConfig, AdrDecision, AdrReason, SignalMetrics};
pub use cad::{CadExitMode, CadStats, LoRaCadParams};
pub use channel_hopping::{ChannelHopper, Channel, HoppingStrategy, ChannelStats};
pub use class_bc::{ClassBCController, DeviceClass, BeaconConfig, MulticastSession, ClassBCStatus};
pub use decoder::{
    BatteryStatus, DecentlabConfig, DecoderType, DeviceStatus, DraginoModel, ElvacoModel,
    GenericCounterConfig, LoRaDecodeError, LoRaDeviceManager, LoRaPayloadDecoder, MeteringData,
    Reading,
};
pub use decoder_nom::NomDecoderAdapter;
pub use duty_cycle::{DutyCycleManager, PowerMode, PowerStats};
pub use format_detector::{Confidence, DetectionResult, FormatDetector};
pub use irq_queue::{IrqEventQueue, IrqEvent, IrqStats, irq_processor_task};
pub use lbm::{LbmCore, MeshMessage, QoS, NodeInfo, MeshStats};
pub use packet::{
    build_trigger_frame, calc_cumulative_delta, decode_lora_packet, parse_abp_data, parse_otaa_join,
};
pub use params::{
    CodingRate, LoRaBandwidth, LoRaModParams, LoRaModParamsExt, LoRaPacketParams, SpreadingFactor,
};
pub use single_channel::{SingleChannelConfig, DutyCycleLimiter};
pub use smart_decoder::{DeviceStats, SmartDecoder};

// Export SX1262 driver when PIO IRQ feature is enabled
#[cfg(feature = "pio-irq")]
pub use sx1262::{Sx1262Driver, Sx1262Error, LoRaConfig};
