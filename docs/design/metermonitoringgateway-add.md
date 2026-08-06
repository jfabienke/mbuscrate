# Historical provenance

This document preserves `docs/arch/ADD.md` from the retired `metermonitoringgateway`
repository. It was extracted on 2026-08-05 immediately before that repository was
deleted.

The source repository was an uncommitted Rust scaffold: it had no commits, no wired
modules, no dependencies, and no tests beyond a `Hello, world!` binary. The material
below records historical design intent and product aspirations; it is not evidence of
implemented behavior or a current API contract.

The active implementation is maintained in this repository. Current transport, vendor,
and radio decisions supersede this document where they differ:
[`wired-transport-refactor.md`](wired-transport-refactor.md),
[`vendor-layers.md`](vendor-layers.md), and
[`sx1262-reconciliation.md`](sx1262-reconciliation.md).

---

Meter Monitoring Gateway Architecture Description Document
Introduction
This document provides a high-level architectural overview of the Rust-based portable meter monitoring gateway, outlining its design goals, supported platforms, and key features. The gateway is designed to collect data from various utility meters (heating, electricity, water, gas, etc.) using multiple communication protocols, including M-Bus, wM-Bus, and LoRa, and relay the data to a central backend system via MQTT.

Architectural Goals and Constraints
Portability: Support both Linux and Zephyr operating systems.
Memory Efficiency: Minimize memory usage due to limited resources on embedded platforms.
Reliability: Ensure robust error handling and data integrity.
Maintainability: Design for modularity and code reusability.
Security: Incorporate security best practices for communication and firmware updates.
Architectural Overview
The gateway architecture is based on a layered approach with clear separation of concerns.

Hardware Abstraction Layer (HAL): Abstracts hardware-specific details, allowing the application to interact with peripherals through common traits.
Operating System Abstraction Layer (OSAL): Abstracts operating system functionalities like threading, synchronization, and time.
Protocol Handlers: Implement the logic for encoding and decoding messages for each supported protocol (M-Bus, wM-Bus, LoRa).
Application Logic: Manages the overall gateway functionality, including device discovery, data collection, and MQTT communication.
Functionality
The Rust-based implementation of the portable meter monitoring gateway builds upon and enhances the functionality provided by the existing C++ implementation. Below is a consolidated overview of the key features:

Meter Discovery: Scans connected buses for wired M-Bus meters and actively listens for wireless wM-Bus and LoRaWAN meters.
Data Collection: Periodically polls wired M-Bus meters and listens for radio packets from wM-Bus and LoRaWAN meters for readings.
Data Decoding: Handles M-Bus and wM-Bus payload decoding, including various data types, Value Information Fields (VIFs), and AES decryption for secure wM-Bus communication.
LoRa Packet Relaying: Receives and forwards raw LoRa payloads.
MQTT Communication: Publishes meter readings to an MQTT broker and subscribes to a control topic for basic commands such as rescan, key installation, reset, upgrade, shell command execution, LoRa packet transmission, and time synchronization. All MQTT communication gets encapsulated in JSON.
Logging: Provides basic console logging and an in-memory circular buffer for logs.
Cross-Platform Support: Operates seamlessly on Linux and Zephyr RTOS through platform-agnostic abstractions.
Enhanced LoRaWAN Handling: Implements a minimal LoRaWAN layer, supporting Join Requests/Accepts and basic data packet formatting for backend parsing.
Flexible Configuration: Introduces a robust configuration system with validation using libraries like serde, supporting multiple configuration formats.
Comprehensive Error Handling: Utilizes the Result type and custom error types for consistent, detailed error management and debugging.
Structured Logging: Incorporates structured logging with JSON output for improved log analysis and integration with monitoring tools, alongside defmt integration for development. Errors and logs are published on dedicated MQTT topics.
Secure Firmware Updates: Adds over-the-air (OTA) firmware update capabilities for gateways as well as devices (where supported) with signature verification for security.
Remote Meter Parameter Setting: Enables remote configuration of meter parameters (where supported), with robust error handling and reporting.
Gateway Status Reporting: Provides commands to retrieve detailed gateway status information, including uptime, resource usage, and connected devices.
Module Structure
The gateway is organized into the following modules:

src/core: Core functionality, including error handling, logging, and configuration.
src/hal: Hardware Abstraction Layer (HAL) implementations for different platforms (Zephyr and Linux).
src/os: Operating System Abstraction Layer (OSAL) implementations for different platforms (Zephyr and Linux).
src/network: Network-related functionality, including MQTT communication and socket handling.
src/protocols: Protocol-specific implementations for M-Bus, wM-Bus, and LoRa.
src/devices: Device-specific implementations for different meter types.
src/error: Error handling and reporting.
src/config: Configuration management.
src/logging: Logging functionality.
src/utils: Utility functions for various tasks.
Class Hierarchy
The gateway's class hierarchy is as follows:

traits::Core: Core traits, including error handling and logging.
Device: Device-specific traits, including initialization, reading, and writing.
ProtocolHandler: Protocol-specific traits, including encoding, decoding, and validation.
Radio: Radio-specific traits, including initialization, sending, and receiving.
Socket: Socket-specific traits, including connecting, sending, and receiving.
MqttClient: MQTT client-specific traits, including connecting, publishing, and subscribing.
Trait Implementation
The gateway's traits are implemented as follows:

Error: Error handling and reporting.
Spi: SPI (Serial Peripheral Interface) implementation.
Uart: UART (Universal Asynchronous Receiver-Transmitter) implementation.
Gpio: GPIO (General-Purpose Input/Output) implementation.
Thread: Thread management.
Mutex: Mutex (Mutual Exclusion) implementation.
Timer: Timer management.
MemoryPool: Memory pool management.
Log: Logging functionality.
Config: Configuration management.
Data Flow
The gateway's data flow is as follows:

Data Acquisition: Device implementations interact with physical meters using ProtocolHandler implementations via Radio or Uart (from the HAL).
Data Processing: Raw data is decoded, validated, and transformed by the ProtocolHandler.
MQTT Publishing: Processed data is encoded as JSON and published to the MQTT broker using the mqtt module.
Control Commands: Control commands are received from the MQTT broker, parsed, and dispatched to the appropriate modules (e.g., Device implementations for device-specific commands).
Error Handling: Errors are logged using the logging module and published to a dedicated MQTT error topic.
Technology Choices
Programming Language: Rust
RTOS (Embedded): Zephyr (with minimal custom OSAL)
MQTT Client: minimq (or similar lightweight crate)
Serialization: miniserde (or custom implementation for extreme memory efficiency)
Logging: Custom implementation with structured JSON output. defmt for development builds.
Security Considerations
Secure Boot (Zephyr): Utilize Zephyr's secure boot features where applicable.
Firmware Updates: Implement secure firmware updates with signature verification.
Encrypted Communication: Support AES encryption for WM-BUS communication.
Deployment View
The gateway will be deployed on either a Raspberry Pi 4/5 running Linux or a RP2350-based Pico 2 running Zephyr. Communication with meters will occur over the respective communication buses (M-Bus, wM-Bus, LoRa). The gateway will connect to an MQTT broker for data backhauling and control.

Migration Plan (from C++ implementation)
The migration to Rust will involve a complete rewrite of the existing C++ codebase, following the new architecture and module structure outlined in this document. The migration will be conducted incrementally, starting with the HAL and OSAL, followed by the protocol handlers and device implementations, and finally the main application logic.
