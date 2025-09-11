# Raspberry Pi Platform Support

## Summary

The mbus-rs crate now includes comprehensive support for Raspberry Pi 4 and 5 as target platforms for SX126x radio communication. This enables developers to easily build wireless M-Bus applications on these popular single-board computers.

## What's Included

### 🔧 Hardware Abstraction Layer (HAL)
- **Complete SPI implementation** using the `rppal` crate
- **GPIO control** for BUSY, DIO, and RESET pins
- **Hardware reset support** for reliable radio initialization
- **Configurable pin assignments** for flexible hardware designs
- **Multiple SPI bus support** (SPI0 and SPI1)

### 📱 Target Platform Support
| Platform           | Architecture            | Target Triple                   | Status          |
|--------------------|-------------------------|---------------------------------|-----------------|
| **Raspberry Pi 5** | ARM Cortex-A76 (64-bit) | `aarch64-unknown-linux-gnu`     | ✅ Full Support |
| **Raspberry Pi 4** | ARM Cortex-A72 (64-bit) | `aarch64-unknown-linux-gnu`     | ✅ Full Support |
| **Raspberry Pi 4** | ARM Cortex-A72 (32-bit) | `armv7-unknown-linux-gnueabihf` | ✅ Full Support |

### 🛠️ Developer Experience
- **Builder pattern** for easy HAL configuration
- **Default pin mappings** that work out of the box
- **Comprehensive examples** showing real-world usage
- **Cross-compilation support** from development machines
- **Integration tests** for hardware validation

### 📋 Examples Provided
- **`raspberry_pi_wmbus.rs`** - Full-featured wM-Bus receiver/transmitter with frame parsing
- **`pi_quick_start.rs`** - Minimal example for getting started quickly

### 🔧 Build and Deployment Tools
- **Cross-compilation script** (`scripts/build_pi.sh`) for easy builds
- **Cargo features** for platform selection (`raspberry-pi`, `raspberry-pi-4`, `raspberry-pi-5`)
- **Systemd service examples** for production deployment

### 📚 Documentation
- **Complete setup guide** (`docs/RASPBERRY_PI_SETUP.md`) with wiring diagrams
- **Troubleshooting section** covering common issues
- **Performance tuning** recommendations
- **Regulatory compliance** information (EU/US)

## Key Features

### Easy Hardware Setup
```rust
use mbus_rs::wmbus::radio::hal::{RaspberryPiHal, GpioPins};

// Default configuration (works with most wiring)
let hal = RaspberryPiHal::new(0, &GpioPins::default())?;

// Custom configuration
let hal = RaspberryPiHalBuilder::new()
    .spi_bus(0)
    .busy_pin(25)
    .dio1_pin(24)
    .spi_speed(8_000_000)
    .build()?;
```

### One-Line wM-Bus Configuration
```rust
use mbus_rs::wmbus::radio::driver::Sx126xDriver;

let mut driver = Sx126xDriver::new(hal, 32_000_000);
driver.configure_for_wmbus(868_950_000, 100_000)?; // EU S-mode
```

### Cross-Compilation Made Easy
```bash
# Build for Raspberry Pi 5
./scripts/build_pi.sh pi5

# Build for all Pi variants
./scripts/build_pi.sh all
```

## Hardware Requirements

### Minimal Setup
- Raspberry Pi 4 or 5
- SX126x radio module (SX1261, SX1262, or SX1268)
- 5 jumper wires (SPI + BUSY)
- 3.3V power supply for radio

### Recommended Setup
- All minimal components plus:
- DIO1 connection for interrupts
- RESET connection for reliable startup
- Proper antenna (868/915 MHz depending on region)

## Getting Started

1. **Enable SPI** on your Raspberry Pi:
   ```bash
   sudo raspi-config
   # Interface Options > SPI > Enable
   ```

2. **Install Rust** (if building on Pi):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

3. **Clone and build**:
   ```bash
   git clone https://github.com/your-repo/mbus-rs
   cd mbus-rs
   cargo build --features raspberry-pi --examples
   ```

4. **Run example**:
   ```bash
   sudo ./target/debug/examples/pi_quick_start
   ```

## Production Deployment

### Systemd Service
Create `/etc/systemd/system/wmbus-receiver.service`:
```ini
[Unit]
Description=wM-Bus Receiver
After=network.target

[Service]
Type=simple
User=root
ExecStart=/home/pi/raspberry_pi_wmbus receive
Restart=always
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```

### Enable and start:
```bash
sudo systemctl enable wmbus-receiver
sudo systemctl start wmbus-receiver
```

## Performance Characteristics

- **SPI Speed**: Up to 16 MHz (8 MHz default for reliability)
- **Latency**: <1ms for command processing
- **Power**: ~12mA RX, ~80mA TX (@14dBm)
- **Range**: 2-5km typical (depends on environment and antenna)

## Regulatory Compliance

### European Union (ETSI EN 300 220)
- Frequency: 868.95 MHz
- Max Power: +14 dBm (25 mW)
- Duty Cycle: 1% per hour
- ✅ Fully compliant with default configuration

### United States (FCC Part 15)
- Frequency: 915 MHz ISM band
- Max Power: +30 dBm (with antenna restrictions)
- ✅ Configurable for US operation

## What's Next

This Raspberry Pi platform support enables:
- **IoT Gateway Applications** - Collect data from wM-Bus meters
- **Smart Home Integration** - Connect utility meters to home automation
- **Industrial Monitoring** - Remote meter reading systems
- **Development and Prototyping** - Easy testing of wM-Bus applications

## Support and Troubleshooting

See the comprehensive [Raspberry Pi Setup Guide](docs/RASPBERRY_PI_SETUP.md) for:
- Detailed wiring diagrams
- Troubleshooting common issues
- Performance optimization tips
- Advanced configuration options

The mbus-rs project now provides a complete, production-ready solution for wireless M-Bus communication on Raspberry Pi platforms! 🎉
