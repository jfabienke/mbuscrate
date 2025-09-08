use tokio::net::TcpStream;
use crate::error::MBusError;
use crate::mbus::frame::{parse_frame, pack_frame, MBusFrame};

pub struct MBusTcpHandle {
    stream: TcpStream,
}

impl MBusTcpHandle {
    pub async fn connect(addr: &str) -> Result<Self, MBusError> {
        let stream = TcpStream::connect(addr).await.map_err(|e| MBusError::SerialPortError(e.to_string()))?;
        Ok(Self { stream })
    }

    pub async fn send_frame(&mut self, frame: &MBusFrame) -> Result<(), MBusError> {
        let data = pack_frame(frame);
        self.stream.write_all(&data).await.map_err(|e| MBusError::SerialPortError(e.to_string()))?;
        Ok(())
    }

    pub async fn recv_frame(&mut self) -> Result<MBusFrame, MBusError> {
        let mut buf = [0u8; 256]; // Max frame size
        let n = self.stream.read(&mut buf).await.map_err(|e| MBusError::SerialPortError(e.to_string()))?;
        let (frame, _) = parse_frame(&buf[..n]).map_err(|e| MBusError::FrameParseError(format!("{:?}", e)))?;
        Ok(frame)
    }
}