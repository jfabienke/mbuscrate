use mbus_rs::{connect, send_request};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to M-Bus device on serial port
    let mut handle = connect("/dev/ttyUSB0").await?;
    
    // Send request to device at address 0x01
    let records = send_request(&mut handle, 0x01).await?;
    
    for record in records {
        println!("Record: {record:?}");
    }
    
    Ok(())
}
