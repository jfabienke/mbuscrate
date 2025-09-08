use crate::error::MBusError;
use crate::wmbus::frame::WMBusFrame;

pub trait WMBusProtocolTrait {
    fn decode_frame(&self, data: &[u8]) -> Result<WMBusFrame, MBusError>;
    fn encode_frame(&self, frame: &WMBusFrame) -> Vec<u8>;
}

// Protocol-agnostic interface