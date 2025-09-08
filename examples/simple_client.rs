use mbus_rs::{connect, send_request};
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut handle = connect("/dev/ttyUSB0").await?;
    let records = send_request(&mut handle, 0x01).await?;
    for record in records {
        println!("Record: {:?}", record);
    }
    Ok(())
}
