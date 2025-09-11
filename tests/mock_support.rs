// Mock support module for E2E tests
// Placeholder for mock infrastructure

use mbus_rs::error::MBusError;

pub struct MockSerialPort;
pub struct TestableDeviceHandle;

impl MockSerialPort {
    pub fn new() -> Self {
        MockSerialPort
    }
}

impl TestableDeviceHandle {
    pub fn from_mock(_mock: MockSerialPort) -> Self {
        TestableDeviceHandle
    }
    
    pub async fn send_request(&mut self, _address: u8) -> Result<Vec<mbus_rs::payload::record::MBusRecord>, MBusError> {
        Ok(Vec::new())
    }
}