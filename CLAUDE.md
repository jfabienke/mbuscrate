# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**mbus-rs** is a Rust implementation of the M-Bus (Meter-Bus) protocol, a European standard for remote reading of utility meters. It supports both wired M-Bus (via serial) and wireless M-Bus (wM-Bus) communication.

## Development Commands

### Build & Test
```bash
# Build the project
cargo build

# Run all tests
cargo test

# Run a specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Run tests in a specific module
cargo test --test golden_frames
cargo test --test vif_tests
cargo test --test record_tests

# Run benchmarks
cargo bench

# Run with debug logging
RUST_LOG=debug cargo run

# Run examples
cargo run --example parse_frame
cargo run --example simple_client
```

### Code Quality
```bash
# Format code
cargo fmt --all

# Check formatting
cargo fmt --all -- --check

# Run clippy linting
cargo clippy --all-targets -- -D warnings

# Security audit
cargo deny check

# Test coverage (requires tarpaulin)
cargo tarpaulin --out lcov
```

## Architecture

### Module Organization

- **`src/mbus/`**: Wired M-Bus implementation
  - `frame.rs`: Frame parsing/packing using `nom` parser combinators
  - `mbus_protocol.rs`: Protocol state machine and data retrieval
  - `serial.rs`: Serial port communication via `tokio-serial`

- **`src/wmbus/`**: Wireless M-Bus implementation
  - `frame.rs`: Wireless frame handling
  - `encryption.rs`: AES-128 encryption support
  - `network.rs`: Network management
  - `protocol.rs`: Wireless protocol logic

- **`src/payload/`**: Data parsing and normalization
  - `data.rs`: Data record decoding
  - `data_encoding.rs`: Type encoding/decoding
  - `record.rs`: M-Bus record structures
  - `vif.rs`: Value Information Field parsing
  - `vif_maps.rs`: VIF lookup tables

- **`src/lib.rs`**: Public API surface - high-level async functions for device interaction
- **`src/main.rs`**: CLI tool for testing M-Bus communication
- **`src/error.rs`**: Error types using `thiserror`

### Key Design Patterns

1. **Parser Combinators**: All frame parsing uses `nom` for robustness
2. **Async-First**: All I/O operations are async using `tokio`
3. **Layered Architecture**: Clear separation between transport, protocol, and application layers
4. **Type Safety**: Strong typing for M-Bus values and units

### Testing Strategy

- **Golden Frames**: Real device frames in `tests/golden_frames.rs` from manufacturers (EDC, Engelmann, Elster)
- **Property Testing**: Uses `proptest` for edge cases
- **Test Data**: `.hex` files in `tests/` contain real M-Bus frames
- **Integration Tests**: `tests/lib_tests.rs` for end-to-end testing

### Error Handling

All errors derive from `MBusError` enum in `src/error.rs`:
- `SerialPortError`: Communication failures
- `FrameParseError`: Invalid frame structure
- `UnknownVif`/`UnknownVife`: Unknown VIF codes
- `InvalidChecksum`: Data integrity issues
- `DeviceDiscoveryError`: Scanning failures

### Frame Structure

M-Bus frames follow this pattern:
- Start byte (0x68 for long, 0x10 for short)
- Length fields
- Control field (C)
- Address field (A)
- Control information (CI)
- Data records (DIF, VIF, data)
- Checksum
- Stop byte (0x16)

### Data Record Format

Each record contains:
- DIF (Data Information Field): Data type and storage
- VIF (Value Information Field): Unit and scaling
- VIFE (Extended VIF): Additional information
- Data: Actual value

## Important Notes

- **Standards**: Implements EN 13757-3 M-Bus specification
- **Async Runtime**: Uses `tokio` with full features
- **Logging**: Set `RUST_LOG` environment variable for debug output
- **Serial Ports**: Linux (`/dev/ttyUSB0`), macOS (`/dev/tty.usbserial-*`), Windows (`COM*`)
- **Test Coverage**: Run `cargo tarpaulin` to generate `lcov.info` coverage report
- **CI Pipeline**: GitHub Actions runs formatting, linting, tests on Ubuntu/macOS/Windows