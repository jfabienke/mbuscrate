use clap::{Parser, Subcommand};
use mbus_rs::{init_logger, log_info, MBusDeviceManager, MBusError};

#[derive(Parser)]
#[command(name = "mbus-cli")]
#[command(about = "CLI tool for M-Bus protocol")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Connect {
        port: String,
        #[arg(short, long, default_value = "2400")]
        baudrate: u32,
    },
    SendRequest {
        address: u8,
    },
    ScanDevices,
    Disconnect,
}

#[tokio::main]
async fn main() -> Result<(), MBusError> {
    init_logger();

    let cli = Cli::parse();
    let mut manager = MBusDeviceManager::new().await?;

    match cli.command {
        Commands::Connect { port, baudrate } => {
            manager.add_mbus_handle_with_config(&port, baudrate).await?;
            log_info("Connected to M-Bus device");
        }
        Commands::SendRequest { address } => {
            let records = manager.send_request(address).await?;
            for record in records {
                log_info(&format!(
                    "Record: {:?} {} {}",
                    record.value, record.unit, record.quantity
                ));
            }
        }
        Commands::ScanDevices => {
            let addresses = manager.scan_devices().await?;
            for addr in addresses {
                log_info(&format!("Device: {addr}"));
            }
        }
        Commands::Disconnect => {
            manager.disconnect_all().await?;
            log_info("Disconnected");
        }
    }

    Ok(())
}
