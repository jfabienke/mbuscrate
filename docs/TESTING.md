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

## Testing Overview

The M-Bus crate employs a comprehensive testing strategy to ensure reliability and correctness:

- **Unit Tests**: Test individual functions and modules
- **Integration Tests**: Test complete workflows
- **Mock Tests**: Test hardware-dependent code without hardware
- **Property Tests**: Fuzz testing with random inputs
- **Coverage Target**: 80%+ line coverage

### Current Coverage Statistics
```
Overall Coverage: 78.19%
- Frame Processing: 97.58%
- Data Records: 93.71%
- VIF Processing: 94.74%
- Data Encoding: 79.91%
- Protocol Logic: 74.88%
```

## Test Organization

### Directory Structure
```
mbuscrate/
├── src/                      # Source with inline unit tests
│   ├── mbus/
│   │   ├── frame.rs         # Contains #[cfg(test)] mod tests
│   │   ├── serial_mock.rs   # Test infrastructure
│   │   └── serial_testable.rs
│   └── payload/
│       └── *.rs             # Each with unit tests
│
├── tests/                    # Integration tests
│   ├── frame_tests.rs       # Basic frame tests
│   ├── frame_advanced_tests.rs
│   ├── data_tests.rs
│   ├── data_encoding_tests.rs
│   ├── record_tests.rs
│   ├── record_advanced_tests.rs
│   ├── serial_tests.rs
│   ├── serial_tests_advanced.rs
│   └── golden_frames.rs    # Real-world frame tests
│
└── benches/                 # Performance benchmarks
    └── parsing_benchmark.rs
```

### Test Categories

#### 1. Unit Tests (in `src/`)
Located within source files using `#[cfg(test)]` modules:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_function() {
        // Test implementation
    }
}
```

#### 2. Integration Tests (in `tests/`)
Separate files testing complete functionality:
```rust
use mbus_rs::mbus::frame::{parse_frame, pack_frame};

#[test]
fn test_frame_round_trip() {
    let frame = create_test_frame();
    let packed = pack_frame(&frame);
    let (_, parsed) = parse_frame(&packed).unwrap();
    assert_eq!(frame, parsed);
}
```

#### 3. Mock Tests
Using the mock infrastructure for hardware simulation:
```rust
use mbus_rs::mbus::serial_mock::MockSerialPort;

#[tokio::test]
async fn test_serial_communication() {
    let mock = MockSerialPort::new();
    mock.queue_frame_response(FrameType::Ack, None);
    // Test communication
}
```

## Running Tests

### Basic Commands

```bash
# Run all tests
cargo test

# Run specific test file
cargo test --test frame_tests

# Run specific test function
cargo test test_parse_ack_frame

# Run tests with output
cargo test -- --nocapture

# Run tests in release mode
cargo test --release

# Run only library tests
cargo test --lib

# Run only integration tests
cargo test --tests
```

### Async Tests
For async functions, use `tokio::test`:
```bash
# Run async tests
cargo test --features tokio/full
```

### Property Tests
```bash
# Run property tests with more iterations
PROPTEST_CASES=10000 cargo test
```

## Coverage Analysis

### Installing Coverage Tools
```bash
# Install llvm-cov
cargo install cargo-llvm-cov
```

### Generating Coverage Reports

```bash
# Generate summary
cargo llvm-cov --summary-only

# Generate detailed report
cargo llvm-cov --lib --bins --tests

# Generate HTML report
cargo llvm-cov --html
# Open target/llvm-cov/html/index.html

# Generate lcov format for CI
cargo llvm-cov --lcov --output-path lcov.info
```

### Coverage Metrics

| Module | Line Coverage | Test Count | Priority |
|--------|--------------|------------|----------|
| frame.rs | 97.58% | 20 | Critical |
| record.rs | 93.71% | 25 | Critical |
| data.rs | 93.64% | 17 | High |
| vif.rs | 94.74% | 15 | High |
| data_encoding.rs | 79.91% | 18 | High |
| mbus_protocol.rs | 74.88% | 12 | Medium |
| serial.rs | 18.92% | 16 | Low (HW) |

## Mock Infrastructure

### MockSerialPort
Complete serial port simulation:

```rust
pub struct MockSerialPort {
    pub tx_buffer: Arc<Mutex<Vec<u8>>>,      // Sent data
    pub rx_buffer: Arc<Mutex<VecDeque<u8>>>, // Received data
    pub next_error: Arc<Mutex<Option<io::Error>>>,
}

impl MockSerialPort {
    // Queue response data
    pub fn queue_rx_data(&self, data: &[u8])
    
    // Queue M-Bus frame response
    pub fn queue_frame_response(&self, frame_type: FrameType, data: Option<Vec<u8>>)
    
    // Get transmitted data
    pub fn get_tx_data(&self) -> Vec<u8>
    
    // Inject error
    pub fn set_next_error(&self, error: io::Error)
}
```

### Usage Example
```rust
#[tokio::test]
async fn test_device_communication() {
    let mock = MockSerialPort::new();
    
    // Queue expected response
    mock.queue_frame_response(
        FrameType::Long {
            control: 0x08,
            address: 0x01,
            ci: 0x72,
            data: Some(vec![0x01, 0x02, 0x03])
        },
        None
    );
    
    // Create testable handle
    let mut handle = TestableDeviceHandle::new(mock.clone(), 2400, Duration::from_secs(1));
    
    // Send request
    let request = create_request_frame();
    handle.send_frame(&request).await.unwrap();
    
    // Verify sent data
    let tx_data = mock.get_tx_data();
    assert_eq!(tx_data[0], 0x10); // Short frame start
    
    // Receive response
    let response = handle.recv_frame().await.unwrap();
    assert_eq!(response.frame_type, MBusFrameType::Long);
}
```

## Writing Tests

### Test Structure Guidelines

#### 1. Naming Convention
```rust
#[test]
fn test_module_function_scenario() {
    // Example: test_frame_parse_ack_valid()
}
```

#### 2. Test Organization
```rust
// Group related tests
mod parse_tests {
    #[test]
    fn test_parse_valid() { }
    
    #[test]
    fn test_parse_invalid() { }
}

mod pack_tests {
    #[test]
    fn test_pack_frame() { }
}
```

#### 3. Test Data Helpers
```rust
fn create_test_frame() -> MBusFrame {
    MBusFrame {
        frame_type: MBusFrameType::Short,
        control: 0x53,
        address: 0x01,
        // ...
    }
}

fn create_test_data() -> Vec<u8> {
    vec![0x68, 0x03, 0x03, 0x68, ...]
}
```

### Testing Best Practices

#### 1. Test Both Success and Failure
```rust
#[test]
fn test_parse_frame_valid() {
    let data = valid_frame_bytes();
    let result = parse_frame(&data);
    assert!(result.is_ok());
}

#[test]
fn test_parse_frame_invalid() {
    let data = invalid_frame_bytes();
    let result = parse_frame(&data);
    assert!(result.is_err());
    assert!(matches!(result, Err(MBusError::FrameParseError(_))));
}
```

#### 2. Test Edge Cases
```rust
#[test]
fn test_frame_max_length() {
    // Test with 252 bytes (maximum data)
    let data = vec![0x01; 252];
    let frame = MBusFrame {
        frame_type: MBusFrameType::Long,
        data: data.clone(),
        // ...
    };
    let packed = pack_frame(&frame);
    assert_eq!(packed[1], 0xFF); // Length = 255
}

#[test]
fn test_frame_empty_data() {
    let frame = MBusFrame {
        data: vec![],
        // ...
    };
    assert!(verify_frame(&frame).is_ok());
}
```

#### 3. Test Round Trips
```rust
#[test]
fn test_bcd_round_trip() {
    let values = vec![0, 1, 42, 99, 1234, 999999];
    for value in values {
        let encoded = encode_bcd(value);
        let (_, decoded) = decode_bcd(&encoded).unwrap();
        assert_eq!(decoded, value);
    }
}
```

#### 4. Property-Based Testing
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_frame_pack_unpack(
        control in 0u8..255,
        address in 0u8..255,
        data in prop::collection::vec(0u8..255, 0..252)
    ) {
        let frame = MBusFrame {
            frame_type: MBusFrameType::Long,
            control,
            address,
            data: data.clone(),
            // ...
        };
        let packed = pack_frame(&frame);
        let (_, unpacked) = parse_frame(&packed).unwrap();
        assert_eq!(unpacked.control, control);
        assert_eq!(unpacked.address, address);
        assert_eq!(unpacked.data, data);
    }
}
```

## Test Strategies

### 1. Parser Testing
Test nom parsers with various inputs:
```rust
#[test]
fn test_parser_valid_input() {
    let input = &[0xE5];
    let (remaining, frame) = parse_frame(input).unwrap();
    assert_eq!(frame.frame_type, MBusFrameType::Ack);
    assert!(remaining.is_empty());
}

#[test]
fn test_parser_partial_input() {
    let input = &[0x10, 0x53]; // Incomplete short frame
    let result = parse_frame(input);
    assert!(result.is_err());
}
```

### 2. Encoding Testing
Test all encoding formats:
```rust
#[test]
fn test_bcd_encoding() {
    assert_eq!(encode_bcd(0), vec![0x00, 0x00, 0x00, 0x00]);
    assert_eq!(encode_bcd(1234), vec![0x00, 0x00, 0x12, 0x34]);
    assert_eq!(encode_bcd(99999999), vec![0x99, 0x99, 0x99, 0x99]);
}
```

### 3. Protocol Testing
Test communication sequences:
```rust
#[tokio::test]
async fn test_init_sequence() {
    let mock = MockSerialPort::new();
    mock.queue_frame_response(FrameType::Ack, None);
    
    let mut manager = DataRetrievalManager::default();
    let result = manager.initialize_device(&mut handle, 0x01).await;
    assert!(result.is_ok());
}
```

### 4. Error Testing
Test error conditions:
```rust
#[test]
fn test_checksum_error() {
    let mut frame = create_valid_frame();
    frame.checksum = 0xFF; // Invalid
    let result = verify_frame(&frame);
    assert!(matches!(result, Err(MBusError::InvalidChecksum { .. })));
}
```

## CI/CD Integration

### GitHub Actions Workflow
```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          components: llvm-tools-preview
      
      - name: Run tests
        run: cargo test --all-features
      
      - name: Generate coverage
        run: |
          cargo install cargo-llvm-cov
          cargo llvm-cov --lcov --output-path lcov.info
      
      - name: Upload coverage
        uses: codecov/codecov-action@v2
        with:
          files: ./lcov.info
```

### Pre-commit Hooks
```bash
#!/bin/sh
# .git/hooks/pre-commit

# Run tests
cargo test --quiet

# Check coverage
cargo llvm-cov --summary-only | grep "TOTAL" | awk '{if ($3 < 75) exit 1}'

# Format check
cargo fmt -- --check
```

## Debugging Tests

### Enable Debug Output
```rust
#[test]
fn test_with_debug() {
    env::set_var("RUST_LOG", "debug");
    init_logger();
    
    // Test code
    log::debug!("Debug information: {:?}", data);
}
```

### Use `dbg!` Macro
```rust
#[test]
fn test_debugging() {
    let value = calculate_something();
    dbg!(&value); // Prints to stderr
    assert_eq!(value, expected);
}
```

### Capture Output
```rust
#[test]
fn test_with_output() {
    let output = std::process::Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("test-command")
        .output()
        .expect("Failed to execute");
    
    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
}
```

## Test Maintenance

### Adding New Tests
1. Create test file in appropriate directory
2. Follow naming conventions
3. Include positive and negative cases
4. Document complex test scenarios
5. Update coverage targets

### Refactoring Tests
1. Keep tests DRY with helper functions
2. Use test fixtures for common data
3. Group related tests in modules
4. Update tests when implementation changes

### Test Review Checklist
- [ ] Tests compile without warnings
- [ ] Tests pass consistently
- [ ] Edge cases covered
- [ ] Error conditions tested
- [ ] Documentation updated
- [ ] Coverage maintained/improved

## Performance Testing

### Benchmarks
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_parse_frame(c: &mut Criterion) {
    let frame_data = create_test_frame_bytes();
    
    c.bench_function("parse_frame", |b| {
        b.iter(|| {
            parse_frame(black_box(&frame_data))
        });
    });
}

criterion_group!(benches, bench_parse_frame);
criterion_main!(benches);
```

### Running Benchmarks
```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench parse_frame

# Save baseline
cargo bench -- --save-baseline master

# Compare with baseline
cargo bench -- --baseline master
```

## Troubleshooting

### Common Issues

#### 1. Async Test Timeout
```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_with_timeout() {
    tokio::time::timeout(
        Duration::from_secs(5),
        async_operation()
    ).await.unwrap();
}
```

#### 2. Test Isolation
```rust
#[test]
fn test_isolated() {
    // Use local state, not global
    let state = TestState::new();
    // Test with local state
}
```

#### 3. Flaky Tests
- Add retries for timing-sensitive tests
- Use deterministic test data
- Mock system dependencies
- Avoid hardcoded delays

## Hardware Testing

### Overview

With the completion of hardware register mapping fixes (v1.1.0), the SX126x radio driver is now ready for real hardware validation on Raspberry Pi platforms. This section covers procedures for testing with actual hardware.

### Hardware Test Requirements

#### Equipment Needed
- Raspberry Pi 4/5 with GPIO and SPI enabled
- SX126x-based radio module (e.g., E22-900M22S, RFM95W)
- Proper wiring per [HARDWARE.md](HARDWARE.md) documentation
- At least two devices for bidirectional testing

#### Software Prerequisites
```bash
# Enable SPI and GPIO on Pi
sudo raspi-config
# Navigate to Interface Options -> SPI -> Yes
# Navigate to Interface Options -> GPIO -> Yes

# Install required dependencies
sudo apt update
sudo apt install build-essential pkg-config
```

### Critical Hardware Validation Tests

#### 1. 100-Cycle Loopback Test
This test validates the hardware register mapping fixes and ensures reliable operation:

```bash
# Create hardware loopback test script
cat > test_hardware_loopback.sh << 'EOF'
#!/bin/bash
# Hardware validation: 100-cycle loopback test

PASS_COUNT=0
TOTAL_CYCLES=100

echo "Starting 100-cycle hardware loopback test..."

for i in $(seq 1 $TOTAL_CYCLES); do
    echo -n "Cycle $i/$TOTAL_CYCLES: "
    
    # Run the Pi quick start example in loopback mode
    timeout 10s cargo run --example pi_quick_start -- --loopback 2>&1 | \
        grep -q "Frame transmitted and received successfully"
    
    if [ $? -eq 0 ]; then
        echo "PASS"
        ((PASS_COUNT++))
    else
        echo "FAIL"
    fi
done

PASS_RATE=$((PASS_COUNT * 100 / TOTAL_CYCLES))
echo ""
echo "Results: $PASS_COUNT/$TOTAL_CYCLES cycles passed ($PASS_RATE%)"

if [ $PASS_RATE -ge 95 ]; then
    echo "✅ Hardware validation PASSED (≥95% success rate)"
    exit 0
else
    echo "❌ Hardware validation FAILED (<95% success rate)"
    exit 1
fi
EOF

chmod +x test_hardware_loopback.sh
./test_hardware_loopback.sh
```

#### 2. State Machine Validation
Verify that the corrected RadioState enum values work with real hardware:

```bash
# Test state transitions with actual SX126x chip
cargo test --test hardware_state_tests -- --nocapture

# Expected output should show:
# - Sleep -> StandbyRc (state value 0x2)  
# - StandbyRc -> Rx (state value 0x5) ✅ Fixed mapping
# - Rx -> StandbyRc -> Tx (state value 0x6) ✅ Fixed mapping
# - No "stuck" RX states or invalid guards
```

#### 3. IRQ Register Validation
Test the corrected interrupt bit mappings:

```bash
# Monitor IRQ events during transmission/reception
RUST_LOG=debug cargo run --example raspberry_pi_wmbus 2>&1 | \
    grep -E "(IRQ|interrupt|RxDone|TxDone)"

# Expected patterns:
# - RxDone interrupt on bit 0 (0x0001) ✅ Fixed mapping  
# - TxDone interrupt on bit 1 (0x0002) ✅ Fixed mapping
# - No spurious interrupts or missed events
```

#### 4. RSSI and Signal Quality Test
Validate radio performance with real RF conditions:

```bash
# Test signal strength and quality metrics
cargo run --example pi_quick_start -- --rssi-test

# Target metrics:
# - RSSI > -90dBm in loopback at 1m distance
# - Packet Error Rate < 5% under normal conditions  
# - CRC pass rate ~90% with enhanced decoding
```

### Hardware Test Automation

#### CI/CD Integration for Hardware
For automated hardware testing in CI/CD pipelines:

```yaml
# .github/workflows/hardware-test.yml (Pi-based runner)
name: Hardware Validation
on: [push, pull_request]

jobs:
  pi-hardware-test:
    runs-on: [self-hosted, raspberry-pi]
    steps:
      - uses: actions/checkout@v3
      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          target: aarch64-unknown-linux-gnu
      
      - name: Build for Pi
        run: cargo build --target aarch64-unknown-linux-gnu --release
      
      - name: Hardware Loopback Test
        run: ./test_hardware_loopback.sh
      
      - name: State Machine Validation
        run: cargo test --target aarch64-unknown-linux-gnu hardware_state_tests
```

### Troubleshooting Hardware Issues

#### Common Problems and Solutions

1. **"Stuck" RX State (Pre-v1.1.0 symptom)**:
   - **Symptom**: Radio enters RX but never transitions to other states
   - **Cause**: Incorrect enum values (Tx=0x5, Rx=0x6)
   - **Fix**: ✅ Fixed in v1.1.0 (Rx=0x5, Tx=0x6)

2. **Missing IRQ Events**:
   - **Symptom**: TxDone/RxDone events not detected
   - **Cause**: Incorrect bitflag positions
   - **Fix**: ✅ Fixed in v1.1.0 (RxDone=bit0, TxDone=bit1)

3. **SPI Communication Errors**:
   - Check wiring per [HARDWARE.md](HARDWARE.md)
   - Verify SPI is enabled: `ls /dev/spidev*`
   - Test with loopback: `sudo usermod -a -G spi,gpio $USER`

4. **GPIO Permission Issues**:
   - Add user to gpio group: `sudo usermod -a -G gpio $USER`
   - Or run with sudo (not recommended for production)

### Performance Baselines

#### Expected Hardware Performance (Pi 4/5 + SX126x)
- **SPI Speed**: Up to 16 MHz
- **Command Latency**: <1ms for basic operations
- **State Transition Time**: <10ms (Sleep -> RX)
- **Packet Transmission Rate**: ~100 packets/sec
- **Power Consumption**: 
  - Sleep: ~160nA
  - RX: ~4.6mA  
  - TX (14dBm): ~44mA

#### Validation Criteria
- ✅ 100-cycle loopback test: ≥95% success rate
- ✅ State transitions: All enum values correctly mapped
- ✅ IRQ handling: All interrupt events properly detected
- ✅ No crashes or hangs during extended operation (>1 hour)

### Hardware Test Documentation
- See [HARDWARE.md](HARDWARE.md) for wiring diagrams
- See [DEPLOYMENT.md](DEPLOYMENT.md) for Pi setup procedures
- See [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for debug procedures

## Resources

- [Rust Book: Testing](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [tokio Testing](https://tokio.rs/tokio/topics/testing)
- [proptest Documentation](https://docs.rs/proptest)
- [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)
- [criterion.rs](https://bheisler.github.io/criterion.rs/book/)