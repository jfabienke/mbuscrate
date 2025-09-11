M-Bus Protocol Library (mbus-rs)

├── Wired M-Bus (EN 13757-3)
│   ├── Transport Layer
│   │   ├── Serial Communication (`mbus/serial.rs`)
│   │   │   ├── tokio-serial integration (RS-232/RS-485)
│   │   │   ├── Async I/O with configurable timeouts
│   │   │   ├── Hardware flow control support
│   │   │   └── Mock implementation for testing
│   │   └── Device Management (`mbus_device_manager.rs`)
│   │       ├── Connection pooling for multiple devices
│   │       ├── Device registry and state management
│   │       └── Unified M-Bus/wM-Bus device interface
│   ├── Protocol Layer
│   │   ├── Frame Processing (`mbus/frame.rs`)
│   │   │   ├── nom-based parser combinators
│   │   │   ├── Frame types: ACK, Short, Control, Long
│   │   │   ├── Checksum validation and generation
│   │   │   └── Multi-telegram support
│   │   └── Protocol Management (`mbus/mbus_protocol.rs`)
│   │       ├── DataRetrievalManager state machine
│   │       ├── Request/response coordination
│   │       ├── Network scanning functionality
│   │       └── Device discovery and addressing
│   └── Data Processing (`payload/`)
│       ├── Data Record Parsing (`payload/record.rs`)
│       │   ├── Fixed-Length DR
│       │   │   ├── Device ID extraction (4 bytes)
│       │   │   ├── Transmission counter (1 byte)
│       │   │   ├── Status and medium type (2 bytes)
│       │   │   └── Counter values (8 bytes)
│       │   └── Variable-Length DR
│       │       ├── Data Information Block (DIB)
│       │       │   ├── DIF (Data Information Field)
│       │       │   │   ├── Data type and length determination
│       │       │   │   ├── Storage number and function flags
│       │       │   │   └── Extension handling (DIFE)
│       │       │   └── Parsing and validation
│       │       ├── Value Information Block (VIB)
│       │       │   ├── VIF (Value Information Field)
│       │       │   │   ├── Primary VIF lookup (`payload/vif_maps.rs`)
│       │       │   │   ├── Unit determination and scaling
│       │       │   │   └── Manufacturer-specific codes
│       │       │   ├── VIFE (VIF Extension)
│       │       │   │   ├── Additional scaling factors
│       │       │   │   ├── Tariff and storage information
│       │       │   │   └── Custom unit definitions
│       │       │   └── VIF parsing and normalization (`payload/vif.rs`)
│       │       └── Data Decoding (`payload/data.rs`)
│       │           ├── Type-specific decoders (BCD, Integer, Float)
│       │           ├── Endianness handling
│       │           └── Value extraction and validation
│       ├── Data Encoding (`payload/data_encoding.rs`)
│       │   ├── BCD encoding/decoding with error handling
│       │   ├── Integer format conversion
│       │   ├── Date/time encoding
│       │   └── String encoding (ASCII/UTF-8)
│       └── Data Normalization
│           ├── Unit standardization
│           ├── Value scaling and offset application
│           └── MBusRecord generation with metadata
│
├── Wireless M-Bus (EN 13757-4)
│   ├── Radio Layer (`wmbus/radio/`)
│   │   ├── SX126x Driver (`radio/driver.rs`)
│   │   │   ├── Complete transceiver control
│   │   │   ├── GFSK modulation configuration
│   │   │   ├── wM-Bus S-mode (868.95 MHz, 100 kbps)
│   │   │   ├── Power management and calibration
│   │   │   └── Interrupt-driven operation
│   │   ├── Hardware Abstraction Layer (`radio/hal.rs`)
│   │   │   ├── Platform-agnostic SPI/GPIO interface
│   │   │   ├── Raspberry Pi implementation (`hal/raspberry_pi.rs`)
│   │   │   │   ├── rppal-based SPI communication
│   │   │   │   ├── GPIO control (BUSY, DIO1, DIO2, RESET)
│   │   │   │   ├── Hardware reset functionality
│   │   │   │   └── Builder pattern configuration
│   │   │   └── Cross-compilation support (aarch64, armv7)
│   │   ├── Interrupt Management (`radio/irq.rs`)
│   │   │   ├── IRQ status processing
│   │   │   ├── Event-driven packet handling
│   │   │   └── Error condition management
│   │   ├── Modulation Parameters (`radio/modulation.rs`)
│   │   │   ├── GFSK configuration for wM-Bus
│   │   │   ├── Packet format setup
│   │   │   └── RF parameter optimization
│   │   └── Calibration (`radio/calib.rs`)
│   │       ├── Image and ADC calibration
│   │       ├── Frequency-dependent adjustments
│   │       └── Temperature compensation
│   ├── Protocol Layer (`wmbus/`)
│   │   ├── Wireless Frame Handling (`wmbus/frame.rs`)
│   │   │   ├── wM-Bus frame formats (A, B, C, D)
│   │   │   ├── CRC validation (wireless-specific)
│   │   │   └── Frame type detection
│   │   ├── Network Management (`wmbus/network.rs`)
│   │   │   ├── Device discovery and pairing
│   │   │   ├── Network topology management
│   │   │   └── Routing and forwarding
│   │   ├── Protocol Logic (`wmbus/protocol.rs`)
│   │   │   ├── Wireless-specific communication patterns
│   │   │   ├── Collision avoidance
│   │   │   └── Power optimization
│   │   └── Device Interface (`wmbus/handle.rs`)
│   │       ├── High-level wM-Bus operations
│   │       ├── Connection management
│   │       └── Data collection coordination
│   ├── Data Encoding (`wmbus/encoding.rs`)
│   │   ├── 3-of-6 encoding for robust transmission
│   │   ├── Manchester encoding support
│   │   ├── NRZ (Non-Return-to-Zero) encoding
│   │   └── Forward Error Correction (FEC)
│   └── Security Layer (`wmbus/encryption.rs`)
│       ├── AES-128 encryption implementation
│       ├── Key management and derivation
│       ├── Authentication mechanisms
│       └── Secure key exchange protocols
│
├── Platform Support
│   ├── Cross-compilation (`scripts/build_pi.sh`)
│   │   ├── ARM targets (aarch64, armv7)
│   │   ├── Raspberry Pi 4/5 optimization
│   │   └── Build configuration management
│   ├── Integration Tests (`tests/raspberry_pi_integration.rs`)
│   │   ├── Hardware validation tests
│   │   ├── Mock tests for CI/CD
│   │   └── Performance benchmarking
│   └── Examples
│       ├── Raspberry Pi wM-Bus (`examples/raspberry_pi_wmbus.rs`)
│       ├── Quick Start Guide (`examples/pi_quick_start.rs`)
│       └── Frame Parsing (`examples/parse_frame.rs`)
│
├── Testing Infrastructure
│   ├── Mock Framework (`mbus/serial_mock.rs`)
│   │   ├── Hardware-independent testing
│   │   ├── Configurable response patterns
│   │   └── Timing simulation
│   ├── Golden Frame Tests (`tests/golden_frames.rs`)
│   │   ├── Real device frame validation
│   │   ├── Manufacturer-specific test cases
│   │   └── Protocol compliance verification
│   └── Coverage Analysis (78%+ code coverage)
│       ├── Property-based testing with proptest
│       ├── Integration test suites
│       └── Performance benchmarking
│
└── Development Tools
    ├── Documentation Suite
    │   ├── Architecture documentation
    │   ├── API reference with examples
    │   ├── Protocol specifications
    │   └── Platform setup guides
    ├── CLI Application (`main.rs`)
    │   ├── Device scanning and communication
    │   ├── Frame parsing utilities
    │   └── Debug and diagnostic tools
    └── Error Handling (`error.rs`)
        ├── Comprehensive error types
        ├── Context-aware error messages
        └── Recovery strategies
