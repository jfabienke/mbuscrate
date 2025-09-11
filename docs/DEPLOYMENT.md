# Raspberry Pi Deployment Guide

This guide covers deploying mbus-rs applications on Raspberry Pi for wireless M-Bus communication.

## Supported Hardware

- **Raspberry Pi 4** (2GB+ RAM recommended)
- **Raspberry Pi 5** (4GB+ RAM recommended)
- **Radio Module**: SX1262 or RFM69HCW
- **OS**: Raspberry Pi OS (64-bit recommended)

## Prerequisites

### System Setup

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install build dependencies
sudo apt install -y build-essential pkg-config libssl-dev

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Enable SPI interface
sudo raspi-config
# Navigate to: Interface Options > SPI > Enable
```

### Hardware Connections

Connect your SX126x module to the Raspberry Pi:

| SX126x Pin | Pi GPIO | Physical Pin |
|------------|---------|--------------|
| VCC        | 3.3V    | Pin 1        |
| GND        | GND     | Pin 6        |
| MISO       | GPIO 9  | Pin 21       |
| MOSI       | GPIO 10 | Pin 19       |
| SCK        | GPIO 11 | Pin 23       |
| NSS/CS     | GPIO 8  | Pin 24       |
| RESET      | GPIO 17 | Pin 11       |
| BUSY       | GPIO 24 | Pin 18       |
| DIO1       | GPIO 25 | Pin 22       |

## Building and Running

### Native Compilation (on Pi)

```bash
# Clone the repository
git clone https://github.com/your-org/your-mbus-app.git
cd your-mbus-app

# Add mbus-rs dependency to Cargo.toml
cat >> Cargo.toml << 'EOF'
[dependencies]
mbus-rs = { version = "1.0", features = ["crypto", "raspberry-pi"] }
tokio = { version = "1", features = ["full"] }
EOF

# Build in release mode
cargo build --release --features raspberry-pi

# Run the application
sudo ./target/release/your-mbus-app
```

### Cross-Compilation (from development machine)

```bash
# Install cross-compilation toolchain
rustup target add aarch64-unknown-linux-gnu

# Install linker
sudo apt install gcc-aarch64-linux-gnu  # Ubuntu/Debian
# or
brew install aarch64-elf-gcc  # macOS

# Configure cargo for cross-compilation
mkdir -p .cargo
cat > .cargo/config.toml << 'EOF'
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
EOF

# Build for Pi
cargo build --release --target aarch64-unknown-linux-gnu --features raspberry-pi

# Copy to Pi
scp target/aarch64-unknown-linux-gnu/release/your-mbus-app pi@raspberrypi.local:~/
```

## Example Application

Create a simple wM-Bus receiver:

```rust
use mbus_rs::wmbus::radio::hal::{RaspberryPiHal, GpioPins};
use mbus_rs::wmbus::radio::driver::Sx126xDriver;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize HAL with default GPIO pins
    let gpio_pins = GpioPins {
        reset: 17,
        busy: 24,
        dio1: 25,
        nss: 8,
    };
    
    let hal = RaspberryPiHal::new(0, gpio_pins)?;
    let mut driver = Sx126xDriver::new(hal, 32_000_000);
    
    // Configure for EU wM-Bus S-mode (868.95 MHz, 100 kbps)
    driver.configure_for_wmbus(868_950_000, 100_000)?;
    
    // Start continuous reception
    driver.set_rx_continuous()?;
    println!("Listening for wM-Bus frames...");
    
    loop {
        // Check for received frames
        if let Some((frame, rssi)) = driver.check_rx()? {
            println!("Frame received: {} bytes, RSSI: {} dBm", frame.len(), rssi);
            // Process frame with mbus-rs parsing
        }
        
        sleep(Duration::from_millis(10)).await;
    }
}
```

## Systemd Service

Create a service to run at boot:

```bash
# Create service file
sudo nano /etc/systemd/system/wmbus-receiver.service
```

```ini
[Unit]
Description=Wireless M-Bus Receiver
After=network.target

[Service]
Type=simple
User=pi
WorkingDirectory=/home/pi
ExecStart=/home/pi/your-mbus-app
Restart=on-failure
RestartSec=10

# Grant access to SPI and GPIO
SupplementaryGroups=spi gpio

# Logging
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

```bash
# Enable and start service
sudo systemctl daemon-reload
sudo systemctl enable wmbus-receiver
sudo systemctl start wmbus-receiver

# View logs
sudo journalctl -u wmbus-receiver -f
```

## Performance Tuning

### SPI Speed Configuration

```rust
// Adjust SPI speed based on wire length and signal quality
let hal = RaspberryPiHal::builder()
    .spi_bus(0)
    .spi_speed_hz(8_000_000)  // 8 MHz for short wires
    // .spi_speed_hz(4_000_000)  // 4 MHz for longer wires
    .gpio_pins(gpio_pins)
    .build()?;
```

### CPU Governor

```bash
# Set performance mode for consistent timing
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Make permanent
sudo apt install cpufrequtils
echo 'GOVERNOR="performance"' | sudo tee /etc/default/cpufrequtils
```

## Troubleshooting

### Common Issues

**SPI Permission Denied**
```bash
# Add user to spi group
sudo usermod -aG spi $USER
# Logout and login again
```

**GPIO Access Error**
```bash
# Add user to gpio group
sudo usermod -aG gpio $USER
# Logout and login again
```

**Radio Not Responding**
```bash
# Check SPI is enabled
ls /dev/spidev*
# Should show: /dev/spidev0.0 /dev/spidev0.1

# Test GPIO access
gpio readall  # Shows pin states
```

**High CPU Usage**
- Increase sleep duration in main loop
- Use interrupt-driven reception instead of polling
- Check for proper error handling

### Debug Logging

```bash
# Enable debug output
RUST_LOG=debug ./your-mbus-app

# Or in systemd service
Environment="RUST_LOG=debug,mbus_rs=trace"
```

## Power Management

For battery-powered deployments:

```rust
// Use duty-cycled reception
driver.set_rx_duty_cycle(
    100_000,  // 100ms active
    900_000   // 900ms sleep
)?;
```

## Related Documentation

- [Raspberry Pi Setup Guide](RASPBERRY_PI_SETUP.md) - Detailed hardware setup
- [Examples](EXAMPLES.md) - More code examples
- [Troubleshooting](TROUBLESHOOTING.md) - Extended troubleshooting guide