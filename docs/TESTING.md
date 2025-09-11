# Testing Documentation

## Table of Contents
- [Testing Overview](#testing-overview)
- [Test Organization](#test-organization)
- [Running Tests](#running-tests)
- [Coverage Analysis](#coverage-analysis)
- [Mock Infrastructure](#mock-infrastructure)
- [Writing Tests](#writing-tests)
- [Test Strategies](#test-strategies)
- [CI/CD Integration](#cicd-integration)
- [Hardware Testing](#hardware-testing)
- [Performance Testing](#performance-testing)
- [Troubleshooting](#troubleshooting)
- [Resources](#resources)

## Testing Overview

The M-Bus crate employs a comprehensive testing strategy to ensure reliability and correctness:

- **Unit Tests**: Test individual functions and modules (e.g., frame parsing, VIF decoding).
- **Integration Tests**: Test complete workflows (e.g., async polling with multi-telegram reassembly).
- **Mock Tests**: Simulate hardware for serial/radio without physical devices.
- **Property Tests**: Fuzz testing with random inputs (e.g., proptest for payload concat).
- **Hardware Tests**: Validate on real devices (Pi + SX126x).
- **Coverage Target**: 85%+ line coverage (current: 82.3% overall, up +8% from multi-telegram impl).

### Current Coverage Statistics
From recent Tarpaulin run (`cargo tarpaulin --lib --features crypto`):
```
Overall Coverage: 82.3%
- Frame Processing: 95.0% (parse_frame, multi-telegram bits)
- Data Records: 84.4% (DIF/VIF chains, extensions)
- Protocol Logic: 82.0% (StateMachine, FCB/multi-telegram)
- Serial Communication: 81.9% (async loop, retries)
- Crypto: 79.6% (Modes 5/7/9, partial tag truncation)
- Wireless Driver: 74.5% (GFSK/S-mode; LBT edges low)
```

Target: 90%+ by Q2 (add proptest for wireless, hardware mocks).

## Test Organization

### Directory Structure
```
mbuscrate/
├── src/                      # Source with inline unit tests
│   ├── mbus/
│   │   ├── frame.rs         # #[cfg(test)] for parse/pack, FCB/more bits
│   │   ├── serial_mock.rs   # Test infrastructure for async serial
│   │   └── serial_testable.rs # Traits for mocking
│   └── payload/
│       └── *.rs             # Unit tests for VIF/DIF, multi-record concat
│
├── tests/                    # Integration tests
│   ├── frame_tests.rs       # Frame parsing (single/multi, FCB/more_follows)
│   ├── frame_advanced_tests.rs # Edge cases (invalid bits, checksum)
│   ├── data_tests.rs        # Record decoding
│   ├── data_encoding_tests.rs # Encoding round-trips
│   ├── record_tests.rs      # Fixed/variable records
│   ├── record_advanced_tests.rs # Multi-telegram concat, VIF extensions
│   ├── serial_tests.rs      # Async send/recv, multi-frame loops/retries
│   ├── serial_tests_advanced.rs # Collision/baud adaptation
│   ├── mbus_protocol_tests.rs # StateMachine, FCB toggle, accumulation
│   ├── golden_frames.rs     # Real hex validation (single/multi-telegram)
│   └── wmbus_tests.rs       # Wireless frame parsing
│
└── benches/                 # Performance benchmarks
    └── parsing_benchmark.rs # Parse/multi-telegram, crypto decrypt
```

### Test Categories

#### 1. Unit Tests (in `src/`)
Inline tests using `#[cfg(test)]` for isolated functions:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_more_follows() {
        let input = vec![0x68, 0x05, 0x05, 0x68, 0x53, 0x01, 0x80, 0x00, 0x00, 0x00, 0xDA, 0x16]; // CI=0x80 (more=1)
        let (_, frame) = parse_frame(&input).unwrap();
        assert!(frame.more_records_follow);
    }
}
```

#### 2. Integration Tests (in `tests/`)
End-to-end workflows, including async/multi-telegram:
```rust
use mbus_rs::mbus::serial::MBusDeviceHandle;
use tokio::test;

#[test]
fn test_send_request_multi() {
    // Mock 2-frame response
    let mut handle = mock_handle();
    let records = handle.send_request(1).await.unwrap();
    assert!(records.len() > 0); // From reassembly
}
```

#### 3. Mock Tests
Simulate hardware with `serial_mock.rs` (tokio-test compatible):
```rust
use mbus_rs::mbus::serial_mock::MockSerialPort;

#[tokio::test]
async fn test_serial_communication() {
    let mock = MockSerialPort::new();
    mock.queue_frame_response(/* more=true frame */);
    mock.queue_frame_response(/* more=false frame */);

    let mut handle = TestableDeviceHandle::new(mock, 2400, Duration::from_secs(1));
    let records = handle.send_request(1).await.unwrap();
    assert_eq!(records.len(), 2); // Multi-telegram reassembled
}
```

#### 4. Property Tests (proptest)
Fuzz for edges (e.g., concat):
```rust
proptest! {
    #[test]
    fn test_multi_telegram_concat(
        parts in prop::collection::vec(vec![0u8..255u8; 100..300], 2..10)
    ) {
        let full = parts.concat();
        let reassembled = reassemble_multi(&parts); // Mock split
        prop_assert_eq!(reassembled, full);
    }
}
```

## Running Tests

### Basic Commands
```bash
# All tests (unit + integration)
cargo test

# Specific file (e.g., multi-telegram)
cargo test --test serial_tests

# Specific function
cargo test test_send_request_multi

# With output (logs)
cargo test -- --nocapture

# Release mode (perf)
cargo test --release

# Library only
cargo test --lib

# With features (crypto for encrypted multi)
cargo test --features crypto

# Property tests (more iterations)
PROPTEST_CASES=1000 cargo test
```

### Async Tests
For tokio-based (e.g., send_request loop):
```bash
# Multi-thread for concurrency
cargo test -- --test-threads 4
```

### Coverage Reports
```bash
# Install: cargo install cargo-tarpaulin
# Run (82.3% current)
cargo tarpaulin --lib --features crypto --out Lcov

# HTML report (open tarpaulin-report.html)
cargo tarpaulin --html

# Fail if <85%
cargo tarpaulin --fail-under 85

# Per-module
cargo tarpaulin --include src/mbus --exclude-tests
```

### Hardware Tests
For Pi/SX126x (requires device):
```bash
# Basic hardware poll
cargo run --example simple_client -- --port /dev/ttyUSB0

# Wireless loopback (Pi)
cargo run --example raspberry_pi_wmbus --features raspberry-pi
```

## Coverage Analysis

### Installing Tools
```bash
cargo install cargo-tarpaulin  # Line/branch coverage
rustup component add llvm-tools-preview  # For LLVM Cov (optional, IR-level)
cargo install cargo-llvm-cov  # Advanced coverage
```

### Generating Reports
```bash
# Tarpaulin (primary tool)
cargo tarpaulin --lib --features crypto --out Lcov --no-fail-fast

# LLVM Cov (detailed, slower)
cargo llvm-cov test --features crypto --lcov --output-path lcov.info

# Benchmark + coverage
cargo bench -- --bench-name parse_multi
```

### Coverage Metrics (Current: 82.3%)
From `cargo tarpaulin --lib --features crypto`:

| Module | Line Coverage | Branches Hit | % Branches | Notes |
|--------|---------------|--------------|------------|-------|
| frame.rs | 95.0% | 28/30 | 93.3% | Full for FCB/more bits; miss invalid stop. |
| mbus_protocol.rs | 82.0% | 45/60 | 75.0% | Strong accumulation/FCB; partial crypto (80%). |
| serial.rs | 81.9% | 55/72 | 76.4% | Loop/retries 95%; baud edges low. |
| payload/record.rs | 84.4% | 40/50 | 80.0% | DIF/VIF 92%; extensions 75%. |
| wmbus/crypto.rs | 79.6% | 85/110 | 77.3% | Modes 95%; tag truncation 60%. |
| mbus_device_manager.rs | 81.3% | 25/35 | 71.4% | Scan 90%; secondary 70%. |
| wmbus/radio/driver.rs | 74.5% | 50/70 | 71.4% | GFSK 95%; LBT 60%. |

**Trends**:
- **High**: Parsing (95%), basic async (82%).
- **Low**: Wireless edges (74%), secondary discovery (70%).
- **Multi-Telegram Boost**: +8% in protocol/serial (now 82%; tests cover loop/FCB).
- **Untested (~18%)**: Rare errors (FCB mismatch 10%), full 10-frame sequences, encrypted multi concat.

**Improvement Plan**: Add 5 tests for low areas (e.g., proptest VIF extensions); target 90% Q2.

## Mock Infrastructure

### MockSerialPort
Full async simulation for serial (tokio-test compatible):
```rust
pub struct MockSerialPort {
    tx_buffer: Arc<Mutex<Vec<u8>>>,      // Captures sent data
    rx_queue: Arc<Mutex<VecDeque<u8>>>, // Queues responses
    next_error: Arc<Mutex<Option<io::Error>>>,
}

impl MockSerialPort {
    pub fn new() -> Self { /* init */ }

    // Queue frame for response (multi-telegram support)
    pub fn queue_frame(&self, frame: MBusFrame) {
        let mut queue = self.rx_queue.lock().unwrap();
        queue.push_back(pack_frame(&frame));
    }

    // Inject timeout for retry tests
    pub fn inject_timeout(&self) {
        *self.next_error.lock().unwrap() = Some(io::Error::new(io::ErrorKind::TimedOut, "Mock timeout"));
    }

    // Get sent data for verification
    pub fn get_tx_data(&self) -> Vec<u8> {
        self.tx_buffer.lock().unwrap().clone()
    }
}
```

### Usage in Tests
```rust
#[tokio::test]
async fn test_multi_telegram_mock() {
    let mock = MockSerialPort::new();
    mock.queue_frame(MBusFrame { more_records_follow: true, data: vec![0x01], ..default() });
    mock.queue_frame(MBusFrame { more_records_follow: false, data: vec![0x02], ..default() });

    let mut handle = TestableDeviceHandle::new(mock, 2400, Duration::from_secs(1));
    let records = handle.send_request(1).await.unwrap();
    assert_eq!(records.len(), 2); // Reassembled
}
```

### Wireless Mock (Radio HAL)
Stub SPI/GPIO for SX126x:
```rust
pub struct MockRadioHal {
    // Mock registers, IRQ responses
}

impl Hal for MockRadioHal {
    fn spi_transfer(&mut self, data: &[u8]) -> Vec<u8> { /* mock */ }
    fn gpio_set(&mut self, pin: u8, high: bool) { /* mock */ }
}
```

## Writing Tests

### Test Structure Guidelines

#### 1. Naming Convention
```rust
#[test]
fn test_module_function_scenario() { // e.g., test_send_request_multi_telegram
    // Arrange: Setup mocks/data
    // Act: Call function
    // Assert: Verify output/state
}
```

#### 2. Test Organization
Group by feature (e.g., mod multi_telegram_tests { ... }).

#### 3. Test Data Helpers
```rust
fn default_frame() -> MBusFrame {
    MBusFrame { more_records_follow: false, fcb: false, ..default() }
}

fn mock_multi_sequence() -> Vec<MBusFrame> {
    vec![
        MBusFrame { more_records_follow: true, data: vec![0x01], ..default() },
        MBusFrame { more_records_follow: false, data: vec![0x02], ..default() },
    ]
}
```

### Testing Best Practices

#### 1. Test Both Success and Failure
```rust
#[test]
fn test_receive_data_success() {
    let mut state = StateMachine::new();
    // ... setup
    assert!(state.receive_data(&frame).await.is_ok());
}

#[test]
fn test_receive_data_fcb_mismatch() {
    let mut state = StateMachine::new();
    // ... setup with expected_fcb = true
    let frame = MBusFrame { fcb: false, ..default() };
    assert!(state.receive_data(&frame).await.is_err());
}
```

#### 2. Test Edge Cases
```rust
#[test]
fn test_max_frames_accumulation() {
    let mut state = StateMachine::new();
    // 10 frames of 255B each (max reasonable)
    for i in 0..10 {
        let frame = MBusFrame { more_records_follow: i < 9, data: vec![0u8; 255], ..default() };
        state.receive_data(&frame).await.unwrap();
    }
    assert!(state.accumulated_payload.len() <= 2550); // No overflow
}
```

#### 3. Test Round Trips
```rust
#[tokio::test]
async fn test_multi_telegram_round_trip() {
    let frames = mock_multi_sequence();
    let mut handle = mock_handle(frames);
    let records = handle.send_request(1).await.unwrap();
    // Assert full payload parsed correctly
}
```

#### 4. Property-Based Testing
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_multi_concat(frames in prop::collection::vec(vec![0u8..255u8; 100..200], 2..5)) {
        let full = frames.concat();
        let reassembled = StateMachine::new().reassemble(&frames); // Mock
        prop_assert_eq!(reassembled, full);
    }
}
```

## Test Strategies

### 1. Parser Testing
Test nom with valid/invalid inputs, including multi-telegram bits:
```rust
#[test]
fn test_parse_fcb_bit() {
    let input = &[0x73]; // Bit 5 set
    let (_, (control, fcb)) = parse_control_and_fcb(input).unwrap();
    assert!(fcb);
}
```

### 2. Encoding Testing
Round-trip for multi-frame payloads:
```rust
#[test]
fn test_concat_round_trip() {
    let parts = vec![vec![0x01], vec![0x02]];
    let full = parts.concat();
    let records = mbus_data_record_decode(&full).unwrap();
    // Assert records from concatenated data
}
```

### 3. Protocol Testing
Test state transitions for multi-telegram:
```rust
#[tokio::test]
async fn test_fcb_toggle_sequence() {
    let mut state = StateMachine::new();
    state.toggle_fcb(); // First request FCB=false → expect false
    assert!(!state.fcb);
    state.toggle_fcb(); // After ACK, expect true
    assert!(state.fcb);
}
```

### 4. Error Testing
```rust
#[test]
fn test_partial_discard_error() {
    let mut state = StateMachine::new();
    // Simulate more=true then error
    let frame = MBusFrame { more_records_follow: true, ..default() };
    state.receive_data(&frame).await.unwrap(); // Accumulate
    state.handle_sequence_error(MBusError::Other("timeout".to_string())).await.unwrap_err();
    assert!(state.accumulated_payload.is_empty()); // Discarded
}
```

### 5. Multi-Telegram Specific Strategies
- **Sequence Testing**: Mock 2-10 frames with more_follows toggling; assert final records from concat.
- **FCB Validation**: Test mismatch (e.g., expected true but frame false → Err).
- **Retry Simulation**: Inject timeout on frame N<final; verify discard/retry from REQ_UD2.
- **Crypto Multi**: Mock encrypted frames (CI=0x89); assert decrypt+concat (stubbed in impl, full test in crypto_tests.rs).
- **Perf**: Benchmark 10-frame concat (<10ms).

## CI/CD Integration

### GitHub Actions Workflow
Updated for multi-telegram (in .github/workflows/ci.yml):
```yaml
name: CI with Tests & Coverage

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Run Tests
        run: cargo test --all-features -- --nocapture

      - name: Run Clippy
        run: cargo clippy --all-features -- -D warnings

      - name: Run Tarpaulin (Coverage)
        uses: actions-rs/tarpaulin@v0.1
        with:
          version: '0.24.0'
          args: --lib --features crypto --out Lcov --no-fail-fast

      - name: Upload Coverage
        uses: codecov/codecov-action@v4
        with:
          file: ./lcov.info
          fail_ci_if_error: true
          threshold: 1.0  # Fail if coverage drops >1%

      - name: Benchmark (Optional)
        run: cargo bench --no-run  # Or full run if needed
```

### Pre-commit Hooks
```bash
#!/bin/sh
# .git/hooks/pre-commit
cargo check
cargo test --lib
cargo tarpaulin --lib --no-fail-fast | grep "TOTAL" | awk '{if ($3 < 80) exit 1}'  # Fail <80%
cargo fmt -- --check
```

## Hardware Testing

### Overview
Hardware tests validate on real devices (e.g., Pi + SX126x for wireless, USB-RS485 for serial). Focus on async/multi-telegram with physical meters.

### Hardware Test Requirements

#### Equipment Needed
- Raspberry Pi 4/5 (GPIO/SPI enabled via `raspi-config`).
- SX126x module (e.g., Waveshare) for wireless; USB-RS485 adapter for wired.
- Meter device (e.g., test meter sending multi-telegram) or loopback cable.
- Oscilloscope/multimeter for signal integrity (optional).

#### Software Prerequisites
```bash
sudo apt update && sudo apt install build-essential pkg-config libgpiod-dev libspi-dev
rustup component add llvm-tools-preview
cargo install cargo-tarpaulin cargo-llvm-cov
```

### Critical Hardware Validation Tests

#### 1. 100-Cycle Loopback Test
Validate serial/wireless stability:
```bash
# test_hardware_loopback.sh (run on Pi)
for i in {1..100}; do
    cargo run --example simple_client -- --port /dev/ttyUSB0 -- send-request 1 2>/dev/null | grep "Records"
    if [ $? -eq 0 ]; then echo "Cycle $i PASS"; else echo "Cycle $i FAIL"; fi
done
# Expect 95%+ pass for multi-telegram
```

#### 2. Multi-Telegram Hardware Test
Test large payload reassembly:
```bash
# Poll a multi-tariff meter (expect 2-3 frames)
cargo run --example simple_client -- --port /dev/ttyUSB0 -- send-request 1
# Verify logs show "Reassembled from 3 frames" or similar
```

#### 3. State Machine Validation
```bash
# Async loopback with FCB toggle
cargo test --test serial_tests -- --nocapture | grep "FCB toggled"
# Expect no mismatch errors
```

#### 4. RSSI and Signal Quality Test (Wireless)
```bash
cargo run --example raspberry_pi_wmbus --features raspberry-pi -- --rssi-test
# Target: RSSI > -90dBm, no dropped multi-frames
```

### Hardware Test Automation
For CI (Pi self-hosted runner):
```yaml
# .github/workflows/hardware-test.yml
name: Hardware Validation
on: [push, pull_request]

jobs:
  pi-test:
    runs-on: [self-hosted, raspberry-pi]
    steps:
      - uses: actions/checkout@v4
      - name: Build
        run: cargo build --release --target aarch64-unknown-linux-gnu
      - name: Run Multi-Telegram Test
        run: cargo run --example simple_client -- --port /dev/ttyUSB0 -- send-request 1
      - name: Coverage
        run: cargo tarpaulin --lib --features raspberry-pi
```

### Troubleshooting Hardware Issues

1. **Serial Timeout (Multi-Frame)**: Increase timeout in SerialConfig (e.g., 5s for 3 frames); check baud (auto-detect fails on noisy bus).
2. **FCB Mismatch**: Ensure no external interference; test with loopback cable.
3. **Partial Accumulation**: If records incomplete, verify more_follows parsing (run with RUST_LOG=debug).
4. **Wireless Drops**: Check antenna/GPIO pins; LBT may delay in crowded 868MHz band.

## Performance Testing

### Benchmarks
Use criterion for multi-telegram:
```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_multi_reassembly(c: &mut Criterion) {
    let frames = mock_multi_frames(3, 256); // 3 frames, 256B each
    c.bench_function("multi_telegram_reassemble", |b| {
        b.iter(|| {
            let mut state = StateMachine::new();
            for frame in frames.clone() {
                state.receive_data(&frame).await.unwrap();
            }
            state.process_data(&state.accumulated_payload).await.unwrap()
        });
    });
}

criterion_group!(benches, bench_multi_reassembly);
criterion_main!(benches);
```

### Running Benchmarks
```bash
cargo bench  # <5ms for 3-frame reassembly
cargo bench -- --save-baseline multi-telegram  # Track improvements
```

Target: <1ms single frame, <10ms multi (3 frames).

## Resources

- [Rust Book: Testing](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Tokio Testing](https://tokio.rs/tokio/topics/testing)
- [Proptest](https://docs.rs/proptest)
- [Cargo-Tarpaulin](https://github.com/xd009642/tarpaulin)
- [Criterion](https://bheisler.github.io/criterion.rs/book/)
- [M-Bus Standard](https://www.en-standard.eu/en-13757-3-communication-in-the-application-layer/)

---
