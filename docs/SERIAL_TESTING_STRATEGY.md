# Serial Module Testing Strategy

## Overview
The M-Bus serial module (`src/mbus/serial.rs`) handles communication with M-Bus devices over serial ports. Testing this module presents unique challenges because it requires either physical hardware or sophisticated mocking.

## Testing Approach Implemented

### 1. Mock Serial Port (`src/mbus/serial_mock.rs`)
- **Purpose**: Simulates a bidirectional serial port in memory
- **Features**:
  - Separate TX/RX buffers for data inspection
  - Frame response generation for all M-Bus frame types
  - Error injection for testing error handling
  - Configurable read delays for timing tests

### 2. Testable Serial Interface (`src/mbus/serial_testable.rs`)
- **Purpose**: Provides dependency injection for serial port operations
- **Key Design**:
  - `SerialPort` trait that abstracts AsyncRead + AsyncWrite operations
  - `TestableDeviceHandle<P>` generic over any SerialPort implementation
  - Works with both real `tokio_serial::SerialStream` and `MockSerialPort`

### 3. Test Coverage Achieved

#### Unit Tests for Mock Infrastructure
- Mock serial port creation and buffer management
- Frame response generation for all types (ACK, Short, Long, Control)
- Buffer clearing and data queueing

#### Integration Tests for Serial Communication
- **Send Operations**:
  - ACK frame transmission
  - Short frame transmission
  - Long frame transmission with data payload

- **Receive Operations**:
  - ACK frame reception (1 byte)
  - Short frame reception (5 bytes)
  - Long/Control frame reception (variable length)
  - Timeout handling for different baud rates
  - Invalid frame start byte handling

- **Request/Response Patterns**:
  - Send request and wait for response
  - Error propagation from underlying I/O

#### Baud Rate Timeout Mapping Tests
- Verified timeout calculations for all standard baud rates:
  - 300 baud  → 1300ms timeout
  - 600 baud   → 800ms timeout
  - 1200 baud  → 500ms timeout
  - 2400 baud  → 300ms timeout
  - 4800 baud  → 300ms timeout
  - 9600 baud  → 200ms timeout
  - 19200 baud → 200ms timeout
  - 38400 baud → 200ms timeout
  - Other      → 500ms default

## How to Use for Testing

### Example: Testing Custom Frame Handling
```rust
#[tokio::test]
async fn test_custom_protocol() {
    let mock = MockSerialPort::new();

    // Queue a custom response
    mock.queue_frame_response(
        FrameType::Long {
            control: 0x08,
            address: 0x01,
            ci: 0x72,
            data: Some(vec![0x01, 0x02, 0x03])
        },
        None
    );

    let mut handle = TestableDeviceHandle::new(
        mock.clone(),
        2400,
        Duration::from_secs(1)
    );

    // Send request
    let request = create_request_frame();
    handle.send_frame(&request).await.unwrap();

    // Verify sent data
    let tx_data = mock.get_tx_data();
    assert_eq!(tx_data, expected_bytes);

    // Receive response
    let response = handle.recv_frame().await.unwrap();
    assert_eq!(response.frame_type, MBusFrameType::Long);
}
```

### Example: Testing Error Conditions
```rust
#[tokio::test]
async fn test_serial_error() {
    let mock = MockSerialPort::new();

    // Inject an error
    mock.set_next_error(
        std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "Connection lost"
        )
    );

    let mut handle = TestableDeviceHandle::new(mock, 2400, Duration::from_secs(1));

    let result = handle.recv_frame().await;
    assert!(matches!(result, Err(MBusError::SerialPortError(_))));
}
```

## Testing Limitations

### Current Limitations
1. **Actual Hardware**: The real `MBusDeviceHandle::connect()` cannot be tested without hardware
2. **Async Timing**: Precise timing tests are difficult in async contexts
3. **Serial Port Settings**: Parity, stop bits, data bits settings are not fully tested

### Future Improvements
1. **Serial Port Mock Library**: Consider using `serialport-mock` crate for more realistic simulation
2. **Hardware-in-Loop Tests**: Create optional integration tests with real M-Bus devices
3. **Timing Verification**: Add tests that verify actual timeout behavior with delays
4. **Protocol State Machine**: Test complete M-Bus communication sequences

## Running the Tests

```bash
# Run all serial tests
cargo test --lib serial

# Run specific test suites
cargo test --lib serial_mock
cargo test --lib serial_testable

# Run with output for debugging
cargo test --lib test_recv_frame_long -- --nocapture

# Generate coverage report
cargo llvm-cov --lib --html
```

## Key Files
- `src/mbus/serial.rs` - Original serial implementation
- `src/mbus/serial_mock.rs` - Mock serial port for testing
- `src/mbus/serial_testable.rs` - Testable wrapper with dependency injection
- `tests/serial_tests_advanced.rs` - Additional serial tests

## Metrics
- **Test Count**: 16 tests for serial functionality
- **Code Coverage**: Limited for actual serial.rs due to hardware dependencies
- **Mock Coverage**: 100% coverage of mock and testable implementations
