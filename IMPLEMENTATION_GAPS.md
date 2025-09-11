# Implementation Gaps Documentation

**Generated: 2025-01-11 | Version: 1.0.0**

This document provides a comprehensive list of implementation gaps in the mbuscrate project, organized by priority and implementation status. It serves as a roadmap for completing the M-Bus protocol implementation.

## Executive Summary

The mbuscrate project has a well-structured architecture but significant implementation gaps. While basic frame parsing works, most advanced features are either partially implemented, stubbed, or completely missing.

### Current Status Overview
- **✅ Working**: Basic frame parsing, simple device scanning, HAL trait definition
- **⚠️ Partial**: Protocol support, wireless M-Bus, testing infrastructure  
- **🚧 Stubbed**: Async I/O, encryption, protocol management
- **❌ Missing**: Event system, OMS features, performance verification

### Critical Impact Areas
1. **No async operations** - Blocking I/O limits scalability
2. **No encryption** - Security features completely stubbed
3. **Incomplete protocol** - Missing multi-telegram, compact frames
4. **Unverified performance** - No benchmarks or measurements

## Priority 1: Core Functionality Gaps (STUBBED)

These are interfaces that exist but return errors or placeholder values. Must be implemented for basic functionality.

### 1.1 Async I/O Implementation
**Status**: 🚧 STUBBED  
**Files**: `src/mbus/serial.rs`, `src/mbus/serial_testable.rs`
**Current State**: 
- Tokio dependency added but not used
- No `async_trait` implementations
- All I/O operations are blocking

**Missing**:
- `async fn send_frame()` implementation
- `async fn recv_frame()` with timeout
- `AsyncRead`/`AsyncWrite` traits
- Concurrent device polling

**Dependencies**: None

### 1.2 Encryption/Security (Modes 5/7/9)
**Status**: 🚧 STUBBED  
**Files**: `src/wmbus/crypto.rs`, `src/wmbus/encryption.rs`
**Current State**:
- All functions return "not implemented"
- Interface defined but no logic

**Missing**:
- Mode 5 AES-128-CTR implementation
- Mode 7 AES-128-CBC implementation  
- Mode 9 AES-128-GCM with:
  - 11-byte AAD construction
  - 12-byte IV derivation
  - Tag truncation to 12 bytes
  - CRC pre-encrypt
- Key derivation (XOR with manufacturer ID)
- Access number tracking

**Dependencies**: AES crate integration

### 1.3 Protocol Layer Completion
**Status**: 🚧 STUBBED  
**Files**: `src/mbus/mbus_protocol.rs`
**Current State**:
- `DataRetrievalManager` returns empty results
- `DataRequestor` has structure only
- `ResponseParser` not implemented
- `PrimaryAddressScanner` is basic loop

**Missing**:
- Request frame creation
- Response parsing logic
- Multi-telegram assembly
- FCB (Frame Count Bit) toggling
- Error recovery mechanisms

**Dependencies**: Async I/O, Frame parsing

### 1.4 Serial Communication Async
**Status**: 🚧 STUBBED  
**Files**: `src/mbus/serial.rs`
**Current State**:
- Basic sync read/write only
- No timeout handling
- No frame assembly

**Missing**:
- Async serial port operations
- Dynamic timeout calculation
- Byte-to-frame assembly
- Baud rate auto-detection
- Collision recovery

**Dependencies**: Async I/O implementation

## Priority 2: Protocol Completeness (PARTIAL)

These features have basic implementations but are missing critical functionality.

### 2.1 Multi-Telegram Support
**Status**: ⚠️ PARTIAL  
**Files**: `src/mbus/frame.rs`, `src/payload/record.rs`
**Current State**:
- Single frame parsing works
- No continuation handling

**Missing**:
- Multi-block frame assembly
- 16-byte intermediate block validation
- More-records-follow flag handling
- Sequence number tracking

### 2.2 OMS Compact Frames
**Status**: ❌ PLANNED  
**Files**: `src/mbus/frame.rs`
**Current State**: Not implemented

**Missing**:
- CI=0x79 detection
- CRC-16 implementation (polynomial 0x3D65)
- 2-byte signature generation
- LRU cache (256-1024 entries)
- CI=0x76 full frame request
- JSON cache persistence

### 2.3 VIF/DIF Extension Chains
**Status**: ⚠️ PARTIAL  
**Files**: `src/payload/vif.rs`, `src/payload/record.rs`
**Current State**:
- Basic VIF parsing
- Limited extension support

**Missing**:
- Full 10-extension chain support
- Special VIF codes:
  - 0x7C: ASCII unit string
  - 0x7D: Extended VIF follows
  - 0x7E: Wildcard/any VIF
  - 0x7F: Manufacturer-specific
- Complete VIF tables
- Tariff/subunit from DIFE bits

### 2.4 Manufacturer-Specific Handling
**Status**: ⚠️ PARTIAL  
**Files**: `src/payload/vif.rs`
**Current State**:
- Basic 0x7F detection
- No custom parsers

**Missing**:
- Pluggable VIF parser registry
- Manufacturer handler interface
- Dynamic decoder loading
- Vendor-specific data structures

## Priority 3: System Architecture (PLANNED)

These are architectural features that exist only as design goals.

### 3.1 Event-Driven Architecture
**Status**: ❌ PLANNED  
**Files**: None
**Current State**: No implementation

**Missing**:
- Event enum definitions:
  - `DeviceConnected`
  - `FrameReceived`
  - `TransportError`
- Event processing pipeline
- Event handlers
- Async event dispatch

### 3.2 Device Management System
**Status**: ⚠️ PARTIAL  
**Files**: `src/mbus_device_manager.rs`
**Current State**:
- Simple HashMap storage
- Basic scanning

**Missing**:
- Declarative configuration API
- Device state tracking
- Composition-based device model
- State reconciliation
- Event logging
- Persistence layer

### 3.3 Concurrency Model
**Status**: ❌ PLANNED  
**Files**: None
**Current State**: No implementation

**Missing**:
- Transaction queue management
- Connection pooling
- Fair scheduling
- Thread pool/event loop
- Circuit breaker pattern

### 3.4 State Machines
**Status**: ⚠️ PARTIAL  
**Files**: `src/mbus/mbus_protocol.rs`
**Current State**:
- Basic enum for wired states
- No transitions

**Missing**:
- Wired state machine:
  - State transitions
  - Timeout handling
  - Retry logic
- Wireless state machine:
  - Discovery states
  - Authentication flow
  - Unsolicited handling

## Priority 4: Testing & Performance

### 4.1 Benchmarks
**Status**: ❌ PLANNED  
**Files**: `benches/` (empty directory)
**Current State**: No benchmarks

**Missing**:
- Frame parsing benchmarks
- Encryption performance tests
- Memory usage profiling
- <1ms parsing verification

### 4.2 Coverage Metrics
**Status**: ❌ UNVERIFIED  
**Files**: None
**Current State**: Claims 78% coverage, no evidence

**Missing**:
- Tarpaulin configuration
- Coverage reports
- CI integration
- Coverage badges

### 4.3 Hardware Integration Tests
**Status**: ❌ PLANNED  
**Files**: `tests/raspberry_pi_integration.rs` (basic)
**Current State**: Minimal tests

**Missing**:
- Real device tests
- Radio hardware tests
- Timing validation
- Protocol compliance tests

### 4.4 Mock Infrastructure
**Status**: ⚠️ PARTIAL  
**Files**: `src/mbus/serial_mock.rs`
**Current State**: Basic mock

**Missing**:
- Advanced mock features:
  - Timing simulation
  - Error injection
  - Protocol simulation
  - Multi-device mocking

## Priority 5: Wireless M-Bus Completion

### 5.1 Radio Driver Completion
**Status**: ⚠️ PARTIAL  
**Files**: `src/wmbus/radio/driver.rs`
**Current State**:
- Basic SPI commands
- Some register definitions

**Missing**:
- Complete IRQ handling
- Full GFSK configuration
- S/T/C mode support:
  - S-mode: 32.768 kbps
  - T-mode: 100 kbps
  - C-mode: 100 kbps
- Frequency configurations:
  - T-mode: 868.3 MHz (inconsistent)
  - C-mode: 868.95 MHz

### 5.2 Listen Before Talk (LBT)
**Status**: ⚠️ PARTIAL  
**Files**: `src/wmbus/radio/driver.rs`
**Current State**: Basic channel clear

**Missing**:
- ETSI EN 300 220 compliance:
  - -85 dBm threshold
  - 5ms assessment
  - Exponential backoff
- Pre-TX integration
- Duty cycle tracking

### 5.3 Mode Switching
**Status**: ⚠️ PARTIAL  
**Files**: `src/wmbus/mode_switching.rs`
**Current State**:
- Enums defined
- No protocol implementation

**Missing**:
- T1→S1→C1 cycling
- 10ms delays
- Capability frames (CI=0x7A)
- Statistics tracking

## Priority 6: Platform Expansion

### 6.1 Additional Platforms
**Status**: ❌ PLANNED  
**Current State**: Raspberry Pi only

**Missing Platforms**:
- **RP2040/RP2350**: 
  - Embassy-rs integration
  - PIO for protocols
  - Dual-core utilization
- **ESP32**:
  - ESP-HAL implementation
  - WiFi bridge
  - MQTT gateway
- **STM32**:
  - STM32 HAL
  - CAN bus integration
  - RS-485 control
- **nRF52**:
  - SoftDevice integration
  - BLE gateway
  - Low-energy beacons

## Implementation Notes

### Dependencies Between Gaps
1. Async I/O blocks protocol completion
2. Encryption blocks security compliance
3. Protocol completion blocks OMS features
4. Testing blocks performance claims

### Estimated Effort
- **High** (>2 weeks): Async I/O, Encryption, Protocol Layer
- **Medium** (1-2 weeks): OMS features, State machines, Radio completion
- **Low** (<1 week): Testing, Benchmarks, Mock improvements

### Recommended Implementation Order
1. Complete async I/O foundation
2. Implement basic encryption (Mode 5)
3. Finish protocol layer
4. Add OMS features
5. Complete testing infrastructure
6. Optimize performance
7. Expand platform support

## Contributing

To address these gaps:
1. Pick a gap from Priority 1 or 2
2. Check dependencies are resolved
3. Write tests first
4. Implement functionality
5. Update this document when complete

See [CONTRIBUTING.md](CONTRIBUTING.md) for coding standards and PR process.