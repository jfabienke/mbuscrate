# Hardware Compatibility Guide

This guide covers M-Bus hardware compatibility, tested devices, and setup instructions for use with the mbus-rs crate.

## Table of Contents
- [M-Bus Converters](#m-bus-converters)
- [Tested Utility Meters](#tested-utility-meters)
- [Wiring and Installation](#wiring-and-installation)
- [Power Requirements](#power-requirements)
- [Troubleshooting Hardware](#troubleshooting-hardware)
- [Wireless M-Bus Hardware](#wireless-m-bus-hardware)

## M-Bus Converters

M-Bus devices require a level converter to communicate with computers via RS-232/USB. Here are tested and compatible converters:

### USB to M-Bus Converters

#### Recommended Converters

| Model                | Manufacturer | Max Devices | Baud Rates | Price Range | Notes                         | Warranty |
|----------------------|--------------|-------------|------------|-------------|-------------------------------|----------|
| **USB-MBUS-M60**     | Relay        | 60          | 300-38400  | $150-200    | Best for medium installations | 1 year   |
| **USB-MBUS-M120**    | Relay        | 120         | 300-38400  | $250-300    | Industrial grade              | 2 years  |
| **PadPuls M1**       | Relay        | 3           | 300-9600   | $80-100     | Good for testing/small setups | 1 year   |
| **MBUSCONV-USB**     | Solvimus     | 20          | 300-9600   | $120-150    | Compact design                | 1 year   |
| **M-Bus Master USB** | Techbase     | 60          | 300-38400  | $180-220    | DIN rail mountable            | 2 years  |

#### Budget Options

| Model             | Manufacturer | Max Devices | Notes                                 |
|-------------------|--------------|-------------|---------------------------------------|
| Generic USB-MBUS  | Various      | 5-10        | Available on AliExpress/eBay ($30-50) |
| DIY Arduino-based | Open source  | 1-5         | Build your own (~$20)                 |

### RS-232 to M-Bus Converters

| Model                     | Manufacturer | Max Devices | Power Supply    |
|---------------------------|--------------|-------------|-----------------|
| **Level Converter RS232** | Relay        | 60          | External 24V    |
| **M-Bus Master**          | ADFweb       | 120         | External 24-42V |
| **HD67190**               | ADFweb       | 250         | DIN rail power  |

### Setup Instructions

#### Linux Setup

```bash
# 1. Connect converter
# 2. Check detection
dmesg | tail
ls /dev/ttyUSB*

# 3. Set permissions
sudo chmod 666 /dev/ttyUSB0
# Or add user to dialout group
sudo usermod -a -G dialout $USER

# 4. Test connection
cargo run --example simple_client -- /dev/ttyUSB0
```

#### Windows Setup

1. Install driver (usually FTDI or CH340)
2. Check Device Manager for COM port
3. Use port name like "COM3" in application

#### macOS Setup

```bash
# 1. Install driver if needed (FTDI/CH340)
# 2. Find port
ls /dev/tty.usbserial*

# 3. Test
cargo run --example simple_client -- /dev/tty.usbserial-1410
```

## Tested Utility Meters

### Heat Meters

| Manufacturer  | Model        | Protocol Version | Address Range | Notes                |
|---------------|--------------|------------------|---------------|----------------------|
| **Kamstrup**  | MULTICAL 302 | EN 13757-3       | 1-250         | Very reliable        |
| **Kamstrup**  | MULTICAL 403 | EN 13757-3       | 1-250         | High precision       |
| **Engelmann** | SensoStar    | EN 1434-3        | 1-250         | Good documentation   |
| **Techem**    | compact V    | EN 13757-3       | 1-250         | Common in EU         |
| **Diehl**     | Sharky 775   | EN 13757-3       | 1-250         | Multiple data points |

### Water Meters

| Manufacturer | Model       | Protocol Version | Features        |
|--------------|-------------|------------------|-----------------|
| **Sensus**   | iPERL       | EN 13757-3       | Leak detection  |
| **Elster**   | Q3          | EN 13757-3       | High accuracy   |
| **Itron**    | Cyble M-Bus | EN 13757-3       | Retrofit module |
| **Zenner**   | Minomess    | EN 13757-3       | Compact design  |

### Electricity Meters

| Manufacturer      | Model   | Protocol Version | Notes           |
|-------------------|---------|------------------|-----------------|
| **EMH**           | LZQJ-XC | EN 13757-3       | 3-phase support |
| **Eastron**       | SDM630  | Modbus/M-Bus     | Dual protocol   |
| **Carlo Gavazzi** | EM24    | EN 13757-3       | DIN rail mount  |

### Gas Meters

| Manufacturer | Model       | Protocol Version | Safety Features     |
|--------------|-------------|------------------|---------------------|
| **Elster**   | BK-G4 M-Bus | EN 13757-3       | ATEX certified      |
| **Itron**    | G4 RF1      | EN 13757-3       | Pulse output option |

## Wiring and Installation

### Basic M-Bus Wiring

```
┌─────────────┐     ┌──────────┐     ┌──────────┐
│   PC/USB    │     │ Converter│     │  Meter 1 │
│             ├─────┤  M-Bus   ├─────┤  Address │
│             │ USB │  Master  │ Bus │    01    │
└─────────────┘     └──────────┘     └─────┬────┘
                           │               │
                           │         ┌─────┴────┐
                           │         │  Meter 2 │
                           │         │  Address │
                           │         │    02    │
                           │         └─────┬────┘
                           │               │
                           └───────────────┘
```

### Cable Specifications

| Parameter | Specification |
|-----------|---------------|
| **Cable Type** | Twisted pair (telephone cable) |
| **Wire Gauge** | 0.5-1.5 mm² (AWG 20-16) |
| **Max Length** | 1000m @ 2400 baud |
| **Max Length** | 350m @ 9600 baud |
| **Topology** | Bus (parallel) or Star |
| **Termination** | Not required for M-Bus |

### Connection Diagram

```
M-Bus Master (Converter)          M-Bus Slave (Meter)
┌────────────────┐                ┌────────────────┐
│                │                │                │
│     +24-42V ───┼────────────────┼─── M-Bus (+)   │
│                │                │                │
│     GND ───────┼────────────────┼─── M-Bus (-)   │
│                │                │                │
└────────────────┘                └────────────────┘

Note: Polarity doesn't matter for M-Bus
```

### Installation Best Practices

1. **Cable Routing**
   - Avoid parallel runs with power cables
   - Maintain 30cm separation from AC lines
   - Use shielded cable in noisy environments

2. **Grounding**
   - Connect shield to ground at one end only
   - Use converter's ground terminal
   - Avoid ground loops

3. **Device Addressing**
   - Set unique primary addresses (1-250)
   - Document address assignments
   - Leave gaps for future expansion

## Power Requirements

### Power Consumption

| Component    | Current Draw      | Voltage   |
|--------------|-------------------|-----------|
| M-Bus Master | 50-200mA          | 24-42V DC |
| Each Meter   | 1.5mA (unit load) | From bus  |
| Maximum Load | 350mA typical     | 24-42V DC |

### Power Supply Sizing

```
Required Power = Base Power + (Number of Meters × 1.5mA × Voltage)

Example for 60 meters:
Power = 5W + (60 × 1.5mA × 36V) = 5W + 3.24W = 8.24W
Use: 15W power supply for safety margin
```

### Recommended Power Supplies

| Meters  | Power Supply | Model Examples       |
|---------|--------------|----------------------|
| 1-20    | 15W, 24V     | Mean Well MDR-15-24  |
| 20-60   | 30W, 36V     | Mean Well MDR-30-36  |
| 60-120  | 60W, 36V     | Mean Well MDR-60-36  |
| 120-250 | 100W, 42V    | Mean Well SDR-100-42 |

## Troubleshooting Hardware

### No Communication

**Check List:**
```bash
# 1. Power LED on converter
# 2. Correct wiring (use multimeter)
# 3. Bus voltage (should be 24-42V)
# 4. Device addressing
# 5. Termination (usually not needed)
```

### Intermittent Communication

**Common Causes:**
- Voltage drop on long cables
- Too many devices for power supply
- Electromagnetic interference
- Bad connections

**Diagnostic Commands:**
```rust
// Test communication quality
for i in 0..100 {
    match handle.recv_frame().await {
        Ok(_) => success_count += 1,
        Err(_) => error_count += 1,
    }
}
println!("Success rate: {}%", success_count);
```

### Electrical Measurements

**Required Tools:**
- Multimeter
- Oscilloscope (optional)

**Measurements:**
| Parameter            | Expected Value |
|----------------------|----------------|
| Bus Voltage (idle)   | 24-42V DC      |
| Bus Voltage (active) | 12-20V DC      |
| Current per meter    | 1-2mA          |
| Signal frequency     | Baud rate      |

## Wireless M-Bus Hardware

### Wireless M-Bus Adapters

| Model                      | Manufacturer | Frequency | Range | Protocol   |
|----------------------------|--------------|-----------|-------|------------|
| **AMBER Wireless AMB8465** | AMBER        | 868 MHz   | 500m  | EN 13757-4 |
| **IMST iM871A**            | IMST         | 868 MHz   | 1km   | EN 13757-4 |
| **RadioCrafts RC1701**     | RadioCrafts  | 868 MHz   | 2km   | EN 13757-4 |

### Antenna Considerations

```
Antenna Gain vs Range:
- 0 dBi (chip antenna): 100-200m urban
- 3 dBi (whip antenna): 300-500m urban
- 6 dBi (external): 500-1000m urban
- 9 dBi (directional): 1-2km line of sight
```

### Frequency Bands

| Region | Frequency | Power Limit | Duty Cycle |
|--------|-----------|-------------|------------|
| Europe | 868 MHz   | 25mW        | 1%         |
| Europe | 433 MHz   | 10mW        | 10%        |
| USA    | 915 MHz   | 30mW        | No limit   |

## Performance Optimization

### Maximum Device Count

```rust
// Calculate max devices based on power supply
fn max_devices(power_supply_watts: f32, voltage: f32) -> u32 {
    let base_consumption = 5.0; // Watts for converter
    let per_device = 0.0015 * voltage; // 1.5mA per device
    let available = power_supply_watts - base_consumption;
    (available / per_device) as u32
}
```

### Response Time Optimization

| Factor       | Impact    | Optimization            |
|--------------|-----------|-------------------------|
| Baud rate    | Direct    | Use highest stable rate |
| Cable length | Inverse   | Minimize length         |
| Device count | Linear    | Use segmentation        |
| Power supply | Threshold | Ensure adequate power   |

## Certification and Standards

### Relevant Standards

- **EN 13757-2**: Physical and link layer
- **EN 13757-3**: Application layer
- **EN 13757-4**: Wireless M-Bus
- **EN 1434-3**: Heat meters
- **IEC 60870-5**: Data transmission

### Certification Marks

Look for these on M-Bus devices:
- CE marking (Europe)
- MID approval (Measuring Instruments Directive)
- PTB approval (Germany)
- NMi approval (Netherlands)

## Manufacturer Resources

### Documentation Links

- [Kamstrup Technical Documentation](https://www.kamstrup.com/en-en/technical-support)
- [Engelmann M-Bus Guide](https://www.engelmann.de/en/downloads/)
- [Relay M-Bus Converters](https://www.relay.de/en/products/m-bus/)
- [M-Bus.com Protocol Information](http://www.m-bus.com/)

### Support Contacts

For hardware-specific issues:
- Check manufacturer documentation first
- Contact manufacturer technical support
- Join M-Bus user forums
- Consult system integrators

## Related Documentation

- [Troubleshooting Guide](TROUBLESHOOTING.md) - Solving communication issues
- [Protocol Reference](PROTOCOL.md) - M-Bus protocol details
- [Deployment Guide](DEPLOYMENT.md) - Production setup
- [Examples](EXAMPLES.md) - Code examples for different hardware
