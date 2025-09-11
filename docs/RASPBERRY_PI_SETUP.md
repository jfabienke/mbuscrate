# Raspberry Pi Setup Guide

This guide covers how to set up and use the SX126x radio driver on Raspberry Pi 4 and 5 platforms.

## Supported Platforms

| Platform                | Architecture            | Target Triple                   | Status           |
|-------------------------|-------------------------|---------------------------------|------------------|
| Raspberry Pi 5          | ARM Cortex-A76 (64-bit) | `aarch64-unknown-linux-gnu`     | ✅ Supported     |
| Raspberry Pi 4          | ARM Cortex-A72 (64-bit) | `aarch64-unknown-linux-gnu`     | ✅ Supported     |
| Raspberry Pi 4          | ARM Cortex-A72 (32-bit) | `armv7-unknown-linux-gnueabihf` | ✅ Supported     |
| Raspberry Pi 3/2/Zero W | ARM Cortex-A53/A7       | `armv7-unknown-linux-gnueabihf` | 🔶 Compatible    |
| Raspberry Pi 1/Zero     | ARM1176JZF-S            | `arm-unknown-linux-gnueabihf`   | 🔶 Basic Support |

## Hardware Requirements

### SX126x Module Connection

Connect your SX126x radio module to the Raspberry Pi GPIO header:

```text
┌───────────────────────────────────────┐
│ Raspberry Pi 4/5 GPIO Header (40-pin) │
├───────────────────────────────────────┤
│ Pin │ BCM │ Function │ SX126x Pin     │
├─────┼─────┼──────────┼────────────────┤
│ 19  │ 10  │ MOSI     │ MOSI           │
│ 21  │ 9   │ MISO     │ MISO           │
│ 23  │ 11  │ SCLK     │ SCLK           │
│ 24  │ 8   │ CE0      │ NSS            │
│ 22  │ 25  │ GPIO     │ BUSY (input)   │
│ 18  │ 24  │ GPIO     │ DIO1 (input)   │
│ 16  │ 23  │ GPIO     │ DIO2 (optional)│
│ 15  │ 22  │ GPIO     │ NRESET (opt.)  │
│ 1   │ 3V3 │ Power    │ VCC (3.3V)     │
│ 6   │ GND │ Ground   │ GND            │
└─────┴─────┴──────────┴────────────────┘
```

### Power Supply Requirements

- **Voltage**: 3.3V (use Pi's 3.3V rail)
- **Current**: Up to 120mA during transmission
- **Regulation**: Clean supply recommended (add decoupling capacitors)

### SPI Configuration

Enable SPI interface in Raspberry Pi OS:

1. Edit `/boot/config.txt`:
   ```bash
   sudo nano /boot/config.txt
   ```

2. Add or uncomment:
   ```
   dtparam=spi=on
   ```

3. Reboot the system:
   ```bash
   sudo reboot
   ```

4. Verify SPI devices exist:
   ```bash
   ls /dev/spi*
   # Should show: /dev/spidev0.0  /dev/spidev0.1
   ```

## Software Setup

### 1. Install Rust Cross-Compilation Tools

For cross-compilation from another host:

```bash
# Install cross-compilation targets
rustup target add aarch64-unknown-linux-gnu    # Pi 4/5 64-bit
rustup target add armv7-unknown-linux-gnueabihf # Pi 4 32-bit
rustup target add arm-unknown-linux-gnueabihf   # Pi 1/Zero

# Install cross-compilation toolchain (Ubuntu/Debian)
sudo apt install gcc-aarch64-linux-gnu gcc-arm-linux-gnueabihf

# Or install via cargo cross (easier)
cargo install cross
```

### 2. Native Compilation on Pi

For building directly on Raspberry Pi:

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install system dependencies
sudo apt install -y build-essential pkg-config libssl-dev

# Enable access to GPIO/SPI for non-root users
sudo usermod -a -G spi,gpio $USER
# Log out and back in for group changes to take effect
```

## Building and Running

### Method 1: Cross-Compilation (Recommended)

From your development machine:

```bash
# Clone and build for Raspberry Pi 4/5 (64-bit)
git clone https://github.com/your-repo/mbus-rs
cd mbus-rs

# Build with cross
cross build --target aarch64-unknown-linux-gnu --features raspberry-pi --release

# Or build with standard cargo (requires toolchain setup)
cargo build --target aarch64-unknown-linux-gnu --features raspberry-pi --release

# Copy binary to Pi
scp target/aarch64-unknown-linux-gnu/release/examples/raspberry_pi_wmbus pi@raspberrypi.local:~/
```

### Method 2: Native Compilation

On the Raspberry Pi:

```bash
# Clone repository
git clone https://github.com/your-repo/mbus-rs
cd mbus-rs

# Build with Raspberry Pi features
cargo build --features raspberry-pi --release

# Build examples
cargo build --examples --features raspberry-pi --release
```

### Build Features

Use these feature flags for different configurations:

```bash
# Basic Raspberry Pi support
cargo build --features raspberry-pi

# With GPIO interrupts (advanced)
cargo build --features raspberry-pi,gpio-interrupt

# Hardware SPI optimization
cargo build --features raspberry-pi,hardware-spi

# All Raspberry Pi features
cargo build --features raspberry-pi,gpio-interrupt,hardware-spi
```

## Running Examples

### Quick Start Example

```bash
# Run the quick start example (requires sudo for GPIO access)
sudo ./target/release/examples/pi_quick_start

# Or with logging
sudo RUST_LOG=info ./target/release/examples/pi_quick_start
```

### Full wM-Bus Example

```bash
# Receiver mode (default)
sudo ./target/release/examples/raspberry_pi_wmbus receive

# Transmitter mode
sudo ./target/release/examples/raspberry_pi_wmbus transmit

# Hardware test
sudo ./target/release/examples/raspberry_pi_wmbus test
```

### Systemd Service (Optional)

Create a systemd service for automatic startup:

1. Create service file:
   ```bash
   sudo nano /etc/systemd/system/wmbus-receiver.service
   ```

2. Add configuration:
   ```ini
   [Unit]
   Description=wM-Bus Receiver Service
   After=network.target

   [Service]
   Type=simple
   User=root
   ExecStart=/home/pi/raspberry_pi_wmbus receive
   Restart=always
   RestartSec=10
   Environment=RUST_LOG=info

   [Install]
   WantedBy=multi-user.target
   ```

3. Enable and start:
   ```bash
   sudo systemctl daemon-reload
   sudo systemctl enable wmbus-receiver
   sudo systemctl start wmbus-receiver
   ```

## Performance Tuning

### CPU Governor

Set performance governor for better real-time performance:

```bash
# Check current governor
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor

# Set performance governor
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

### Memory Settings

For high-throughput applications, consider adjusting:

```bash
# Add to /boot/config.txt for Pi 4/5
gpu_mem=16          # Reduce GPU memory
disable_camera=1    # Disable camera if not used
```

### Real-Time Priority

For time-critical applications:

```bash
# Run with real-time priority
sudo chrt -f 10 ./your_wmbus_app

# Or set in systemd service
# Add: Nice=-10
```

## Troubleshooting

### Permission Issues

```bash
# Add user to required groups
sudo usermod -a -G spi,gpio,i2c $USER

# Or run with sudo (not recommended for production)
sudo ./your_application
```

### SPI Not Working

```bash
# Check SPI is enabled
lsmod | grep spi
# Should show: spi_bcm2835

# Check device nodes
ls -l /dev/spi*
# Should show readable devices

# Test SPI loopback (connect MOSI to MISO)
sudo apt install spi-tools
spi-config -d /dev/spidev0.0 -s 1000000 -b 8 -l
```

### GPIO Access Issues

```bash
# Check GPIO groups
groups $USER
# Should include 'gpio'

# Manual GPIO permission (temporary)
sudo chmod 666 /dev/gpiomem
```

### Radio Not Responding

1. Check power supply voltage (should be 3.3V ±5%)
2. Verify SPI connections with multimeter
3. Check BUSY pin behavior (should go high during commands)
4. Use hardware test mode: `sudo ./app test`

### Performance Issues

```bash
# Check CPU frequency scaling
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq

# Monitor system load
htop

# Check for USB interference on Pi 4
dmesg | grep -i usb
```

## Advanced Configuration

### Custom GPIO Pin Assignment

```rust
use mbus_rs::wmbus::radio::hal::{RaspberryPiHalBuilder, GpioPins};

// Custom pin configuration
let hal = RaspberryPiHalBuilder::new()
    .spi_bus(1)           // Use auxiliary SPI
    .busy_pin(20)         // Different BUSY pin
    .dio1_pin(21)         // Different DIO1 pin
    .no_dio2()            // No DIO2 connection
    .reset_pin(16)        // Hardware reset on GPIO 16
    .spi_speed(12_000_000) // 12 MHz SPI
    .build()?;
```

### Multiple Radio Support

```rust
// Support multiple SX126x modules
let radio1_hal = RaspberryPiHalBuilder::new()
    .spi_bus(0)
    .busy_pin(25)
    .dio1_pin(24)
    .build()?;

let radio2_hal = RaspberryPiHalBuilder::new()
    .spi_bus(1)           // Use second SPI bus
    .busy_pin(20)         // Different pins
    .dio1_pin(21)
    .build()?;
```

## Regulatory Considerations

### EU (ETSI EN 300 220)

- **Frequency**: 868.95 MHz (wM-Bus S-mode)
- **Max Power**: +14 dBm (25 mW)
- **Duty Cycle**: 1% per hour

### US (FCC Part 15)

- **Frequency**: 915 MHz ISM band
- **Max Power**: +30 dBm (1W) with antenna restrictions

### Implementation

```rust
// EU configuration
driver.configure_for_wmbus(868_950_000, 100_000)?;
driver.set_tx_params(14, 0x07)?; // +14 dBm

// US configuration
driver.configure_for_wmbus(915_000_000, 100_000)?;
driver.set_tx_params(20, 0x07)?; // +20 dBm
```

## Support

For issues specific to Raspberry Pi:

1. Check the [troubleshooting](#troubleshooting) section
2. Enable debug logging: `RUST_LOG=debug`
3. Run hardware test: `sudo ./app test`
4. Check GPIO/SPI permissions and configuration

For general radio driver issues, see the main [README](../README.md) and [radio driver documentation](../src/wmbus/radio/README.md).
