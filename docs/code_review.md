## Overview

The project is a Rust crate designed for interacting with both wired and wireless Meter-Bus (M-Bus) devices, commonly used for reading utility meters like those for water, gas, electricity, and heat. This crate, named mbus-rs, aims to provide a comprehensive solution for applications that require communication with and management of M-Bus devices. Here's a summary of its key components and functionalities:

## Core Components

M-Bus and Wireless M-Bus Protocols: The crate includes implementations for both the wired M-Bus protocol and its wireless counterpart (wM-Bus), addressing the needs of various utility metering applications.

MBusDeviceManager: A central manager for handling connections to both M-Bus and wM-Bus devices. It allows clients to manage multiple device connections through a unified interface, supporting operations like connecting, disconnecting, sending requests, and receiving data from M-Bus devices.

Connection Handles: Separate handle structures (MBusDeviceHandle for wired and WMBusHandle for wireless) manage the state and operations of individual device connections. These handles abstract the complexities of M-Bus communication, providing simpler interfaces for sending and receiving data.

Encryption and Security: The WMBusEncryption component handles encryption and decryption for wireless M-Bus communications, ensuring secure data transmission. It includes functionalities for managing encryption keys and algorithms.

Frame Parsing and Building: Components for both wired and wireless protocols include functionalities to parse incoming frames and construct outgoing frames, adhering to the M-Bus specifications.

Error Handling: The MBusError enum centralizes error management within the crate, defining various error types for different error conditions encountered across wired and wireless M-Bus communications.

Logging: A logging module facilitates debugging and monitoring the library's operations, offering different levels of log messages (error, warn, info, debug) to help diagnose issues or understand the crate's behavior.

## Key Functionalities

Device Discovery and Management: The crate supports scanning for M-Bus devices, establishing connections, and managing these connections through the device manager and individual device handles.

Data Communication: It allows for sending requests to M-Bus devices to read or configure meter readings and receiving responses, including handling the specifics of M-Bus data encoding and decoding.

Security: For wireless M-Bus devices, it provides robust security features, including encryption of communications to protect against eavesdropping and tampering.

Asynchronous Design: The crate leverages Rust's asynchronous programming features to handle I/O operations efficiently, making it well-suited for applications that require non-blocking communication with M-Bus devices.

## Development Considerations

The project emphasizes modular design, error handling, and security, critical for the reliable operation of utility meter reading applications.

It includes comprehensive testing and documentation to ensure the crate's functionalities are well-understood and can be reliably used in production environments.

This Rust crate represents a significant tool for developers working with M-Bus devices, offering a rich set of features designed to simplify the complexities of M-Bus communication while ensuring robustness and security

## Key Focus Areas

### Core Protocol Implementation

Wired and Wireless Protocols: Solidify the base by ensuring that both wired M-Bus and wireless wM-Bus protocols are correctly implemented. This includes accurate frame parsing, building, and handling protocol-specific nuances. Focus on the critical path of data communication—sending requests and parsing responses.

### Encryption and Security

Security for Wireless M-Bus: Given the sensitivity of utility metering data, implementing robust encryption and security measures for wireless M-Bus communications is crucial. This involves setting up secure key management, implementing supported encryption algorithms, and ensuring data integrity checks are in place.

### Error Handling and Logging

Comprehensive Error Management: Develop a detailed and user-friendly error handling system that can guide users in diagnosing and resolving issues. This encompasses not just internal errors but also protocol-specific error conditions.

### Logging Infrastructure

Establish a flexible logging system that can aid in debugging and operational monitoring. Ensure that it provides enough detail for diagnosing issues without overwhelming users with too much information.

### Device Connection Management

MBusDeviceManager: Since this component acts as the gateway for users to interact with both wired and wireless M-Bus devices, getting its design, implementation, and API right is critical. Focus on connection management capabilities, including robust support for connecting to, managing, and disconnecting from devices.

### Testing and Documentation

Comprehensive Testing: Before extending the library with more features, ensure that the core functionalities are thoroughly tested. This includes unit tests for protocol parsing and building, integration tests covering typical communication scenarios, and tests for error conditions.

### Clear Documentation

Start documenting the existing functionalities, focusing on API documentation, usage examples, and setup instructions. Documentation is not just about usage; it's also crucial for inviting contributions and feedback from the community.
