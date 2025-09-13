// Add to DeviceInfo (assume it exists; if not, create src/mbus/device.rs with:
use crate::payload::data_encoding;

#[derive(Debug)]
pub struct DeviceInfo {
    pub manufacturer_id: String,
    pub serial_number: u32,
    pub version: u8,
    pub device_type: u8,
}

impl DeviceInfo {
    pub fn parse_from_frame(frame: &MBusFrame) -> Result<Self, MBusError> {
        // Parse manufacturer from data[0..2], etc.
        let manufacturer = data_encoding::mbus_decode_manufacturer(frame.data[0], frame.data[1]);
        Ok(Self {
            manufacturer_id: manufacturer,
            serial_number: 0, // Parse BCD
            version: frame.data[2],
            device_type: frame.data[3],
        })
    }
}

// Then in lib.rs or mbus/mod.rs: pub use device::DeviceInfo;
