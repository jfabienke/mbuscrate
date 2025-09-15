# Debugging and Logging Guide for device-config on Raspberry Pi

This document covers the RTT (Real-Time Transfer) + defmt logging system implemented for Raspberry Pi 4/5 deployments. RTT provides low-overhead, structured logging over SWO (Serial Wire Output on GPIO13) without UART/USB blocking, while defmt encodes compact binary logs for efficient decoding. This is integrated into mbuscrate (core logging) and device-config (e.g., LoRa/SX1262 IRQs, CryptoService events). Logs are non-blocking (<0.1W overhead) and decode to human-readable formats via probe-rs.

The system routes `tracing` crate logs to defmt for binary encoding over RTT/ITM (ARM CoreSight), with fallbacks to file/stdout. It's designed for real-time monitoring in low-power gateways (e.g., IRQ debouncing, AES offload stats) without performance impact.

## Prerequisites

- **Hardware**: Raspberry Pi 4B/5B (SWO on GPIO13; expose via header for probe). For live decode, connect via SWD adapter (e.g., Raspberry Pi Pico probe: SWDIO GPIO10, SWCLK GPIO12, SWO GPIO13, GND).
- **Rust Toolchain**: `aarch64-unknown-linux-gnu` target (cross-compile on host if needed).
- **Probe Tools**: Install via `cargo install probe-rs` (free, open-source; supports Pi via SWD). Alternatives: J-Link or Segger for probe-rs, or VSCode probe-rs extension.

## Setup

### 1. Dependencies in Cargo.toml
Add to both mbuscrate and device-config `Cargo.toml` (or root workspace if shared):

```toml
[dependencies]
defmt = "0.3"
defmt-rtt = "0.4"
cortex-a = "0.3"
tracing = { version = "0.1", features = ["log"} }
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[features]
default = ["logging"]
logging = ["defmt", "defmt-rtt", "cortex-a", "tracing", "tracing-subscriber"]
```

Build with logging enabled:
```bash
cargo build --features logging
```

### 2. Initialize Logging
Call once early in your application (e.g., in `gateway/src/main.rs` or `core/src/lib.rs` init):

```rust
// In main.rs or a shared init function
use defmt_rtt as _;
use tracing_subscriber::prelude::*;

fn init_logging() {
    // Init RTT/ITM (SWO channel 0 for defmt)
    // (Implementation details in core/src/logging/mod.rs)
    init_rtt_logging();

    // Route tracing to defmt (binary writer)
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(defmt_writer())  // Custom defmt writer to ITM port 0
        .with_max_level(tracing::Level::INFO)
        .init();
}
```

For Pi-specific setup (e.g., ITM/SWO config), see `core/src/logging/rtt_init.rs` (platform detection for Pi 4/5).

### 3. Logging Configuration
Set log levels via environment variable `RUST_LOG` (e.g., `RUST_LOG=info` for production, `debug` for development). defmt encodes structured logs as binary (compact: ~10-20 bytes/entry vs. 50+ text).

- **Levels**: `trace` (low-level, e.g., raw IRQ edges), `debug` (timings), `info` (events like RX complete), `warn` (timeouts), `error` (failures).
- **Structured Fields**: Use `field::display` or `%` for numbers/strings (e.g., `{irq_mask=0x02, device=0x1234}`).

## Usage

### 1. Basic Logging
Import and use `tracing` macros, which route to defmt/RTT:

```rust
use tracing::{info, warn, debug, error, trace};

info!("Starting LoRa RX: timeout 100ms");  // Decodes to: "[INFO] Starting LoRa RX: timeout 100ms"
warn!("IRQ timeout: channel=5, SF=7");  // Structured: {channel=5, sf=7}
error!("CRC fail on packet len=255");  // Binary error for filtering
trace!("Raw edge at 123μs");  // High-res timestamp via ARM cycle counter
```

### 2. SX1262/LoRa-Specific Logging (lora_v2.rs)
In `LoraHandler` methods (e.g., `receive()`/`transmit()`), log IRQ events, offloads, and metrics:

```rust
pub async fn receive(&mut self, timeout_ms: u32) -> Result<LoraPacket, anyhow::Error> {
    info!("SX1262 RX start: timeout={}ms, channel={}", timeout_ms, self.channel);  // Event start

    if self.wait_irq(timeout_ms) {
        debug!("IRQ detected: DIO1 mask=0x02, timestamp=123μs");  // Timing
        let rx_data = self.spi_burst(&[0x10, 0x00, 255]);
        info!("RX packet: len={} bytes, RSSI={}dBm", rx_data.len(), self.get_rssi());

        if !self.validate_crc(&rx_data) {
            error!("CRC validation failed: expected=0x{:04X}, got=0x{:04X}", expected_crc, computed_crc);
            return Err(anyhow!("CRC fail"));
        }

        let de_whitened = self.dewhiten_lora(&rx_data);  // NEON offload log
        debug!("De-whitening: {} bytes processed", de_whitened.len());

        let decrypted = self.crypto_decrypt(&de_whitened);  // AES hardware
        info!("AES decrypt: backend=hardware, success=true");

        trace!("Packet decode: SF=7 BW=125kHz, symbols=14");  // Low-level

        info!("RX complete: device_id=0x{:08X}, payload_len={}", self.device_id, decrypted.len());
        Ok(LoraPacket::from_bytes(&decrypted))
    } else {
        warn!("RX timeout: {}ms elapsed, no IRQ event", timeout_ms);
        error!("Timeout details: channel={} SF={} RSSI=low", self.channel, self.sf as u8);
        Err(anyhow!("RX timeout"))
    }
}
```

- **IRQ Events**: Log debounced IRQs (`debug!("IRQ: mask=0x02")`), raw edges (`trace!("Edge at {}μs")`), and failures (`error!("Stale IRQ")`).
- **Offload Logs**: Track AES/NEON usage (`info!("AES: hardware")`), timings (`debug!("SPI burst: {} bytes, 0.13ms")`).

### 3. Crypto and Device Logging (CryptoService)
In `core/src/services/crypto_service.rs` or `LoraHandler`:

```rust
pub async fn authenticate(&self, device_id: u32, challenge: &[u8]) -> Result<CryptoResult, CryptoServiceError> {
    debug!("Auth challenge: len={} bytes for device 0x{:08X}", challenge.len(), device_id);
    let hmac = self.crypto.hmac_auth(&key, challenge);  // Hardware SHA
    info!("HMAC auth: success=true, backend=hardware, digest=0x{:02X?}", &hmac[..4]);  // Partial digest
    // ...
}
```

- **Structured**: `{device_id=0x1234, backend=hardware, op=encrypt, len=16}` for decode to JSON.

### 4. Gateway-Wide Usage (gateway/src/main.rs)
```rust
#[tokio::main]
async fn main() {
    init_logging();  // Early init (SWO/ITM setup)

    let mut lora = LoraHandler::new().await;
    info!("Gateway started: Pi5 detected, offloads=AES+NEON");

    loop {
        if let Ok(packet) = lora.receive(1000).await {
            info!("Gateway RX: {} bytes from channel {}", packet.len(), lora.channel);
            // Process HCA...
        } else {
            warn!("Gateway event: RX fail, retrying...");
        }
    }
}
```

- **Global Events**: Gateway lifecycle (`info!("OTA sync: 50 devices")`), errors (`error!("Sync fail: device=0x1234")`).

## Decoding and Monitoring

### 1. Live Streaming with probe-rs
Connect SWD (Pico probe or direct: GPIO10=SWDIO, 12=SWCLK, 13=SWO, GND). Run on host PC:

```bash
# Install: cargo install probe-rs
probe-rs run --chip BCM2712 --defmt --target aarch64-unknown-linux-gnu --baud-rate 1M
# Output (ANSI/JSON): "[INFO] Gateway started: Pi5 detected, offloads=AES+NEON"
# For JSON: probe-rs run --defmt --format json > logs.json
```

- **Configuration** (`.probe.toml` in project root):
  ```toml
  [probe]
  chip = "BCM2712"  # Or custom for Pi
  speed = "1M"  # SWO baud (1 MHz safe)

  [defmt]
  enabled = true
  max-level = "INFO"  # Filter

  [[swd-pins]]
  swdio = "GPIO10"
  swclk = "GPIO12"
  swo = "GPIO13"
  ```

### 2. Scripts for Automation
- **setup-probe-rs.sh** (in `scripts/`):
  ```bash
  #!/bin/bash
  cargo install probe-rs --version 0.22
  probe-rs chip --list-profiles | grep BCM2712  # Verify
  echo "Probe ready: Connect SWD to Pi GPIO10/12/13"
  ```
- **rtt-monitor.sh** (live tail):
  ```bash
  #!/bin/bash
  probe-rs run --chip BCM2712 --defmt --target aarch64-unknown-linux-gnu --attach --wait-halt | defmt-print --colored-output
  # Or for file: probe-rs run --chip BCM2712 --defmt --output rtt.bin
  # defmt-print --reader rtt rtt.bin
  ```
- **test-rtt-logging.sh** (validation):
  ```bash
  #!/bin/bash
  RUST_LOG=debug cargo run --features logging --bin rtt_demo | grep "IRQ debounced" | wc -l  # Count logs
  probe-rs run --chip BCM2712 --defmt --target aarch64-unknown-linux-gnu --bin rtt_demo
  # Assert: 10+ logs, no overflow
  ```

### 3. Output Examples (Decoded)
- ANSI: `[INFO gateway] Gateway started: Pi5 detected, offloads=AES+NEON`
- JSON (via `--format json`): `{"timestamp":"1234567890.123","level":"INFO","target":"gateway","fields":{"offloads":"AES+NEON"}}`
- Binary Size: "IRQ: mask=2" → 12 bytes (vs. 30+ text).

## Troubleshooting

- **No SWO Output**: Check GPIO13 exposed; run `probe-rs list` to detect probe. Verify ITM enable (DEMCR 0xE000EDFC bit24=1).
- **High Overhead**: Set `RUST_LOG=info` (filters trace/debug); defmt truncates long strings.
- **Pi 4/5 Diff**: Identical; Pi 5 SWO faster (2.4GHz vs. 1.5GHz). Non-RPi: Logs to stderr/file.
- **Decode Errors**: Update probe-rs to 0.22+; ensure baud 1M (match ITM prescaler). For high-rate: Increase SWO baud to 2M.
- **SWD Access**: If header soldered, use Pico as probe (SWD passthrough). Secure pins (no public exposure).

This system delivers high-fidelity, real-time logs for production debugging (e.g., SX1262 IRQs, AES offloads). For issues, check `RTT_LOGGING.md` in mbuscrate docs.
