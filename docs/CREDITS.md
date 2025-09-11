# Credits and Acknowledgments

`mbus-rs` is an original Rust implementation of the M-Bus protocol, developed from international standards and informed by community knowledge. We acknowledge the following sources that contributed to the robustness and correctness of this implementation.

## Standards and Specifications

All protocol implementations are based directly on official standards:

- **EN 13757-2/3/4**: European standard for M-Bus physical, link, and application layers. All frame structures, CRC algorithms, and data encoding follow these specifications.
- **OMS v4.0.4**: Open Metering System specification for advanced features including compact frames (CI=0x79), encryption modes, and profile negotiation.
- **OMS 7.3.6**: Mode 9 AES-128-GCM specification with 11-byte AAD, 12-byte IV, and truncated authentication tags for RF transport.
- **OMS 7.3.7**: Mode 13 TLS specification for IP-based transport (documented but not implemented as IP transport is out of scope).
- **NIST SP 800-38D**: Recommendation for Block Cipher Modes of Operation: Galois/Counter Mode (GCM) and GMAC.
- **ETSI EN 300 220-1**: European telecommunications standard for duty cycle limits and Listen Before Talk requirements in the 868 MHz band.

## Community Knowledge and Test Data

### Test Vectors
- **libmbus Project**: This implementation uses publicly available meter data captures from the libmbus project (https://github.com/rscada/libmbus) for validation testing. These golden frames from real meters (Elster, Engelmann, Kamstrup) ensure interoperability with actual hardware. The test data is used solely for validation purposes under fair use principles.

### Implementation Insights
The M-Bus community has collectively identified common implementation challenges over the years. This project benefits from that shared knowledge:

- **Bit Ordering**: Proper handling of MSB-first transmission in certain frame types
- **FIFO Management**: Strategies for avoiding buffer overruns in radio modules
- **CRC Calculation**: Correct implementation of the wM-Bus polynomial with complement
- **Frame Assembly**: Robust handling of multi-block frames with proper validation
- **Error Recovery**: Graceful handling of partial frames and communication errors

These insights, documented in various open-source projects and forums, have informed our implementation choices to ensure robustness.

## Hardware References

- **Semtech SX126x**: Reference manual v2.2 for radio driver implementation
- **HopeRF RFM69HCW**: Datasheet for alternative radio module support
- **Raspberry Pi**: BCM2835/2711 documentation for GPIO and SPI interfaces

## Development Tools

- **Rust Ecosystem**: tokio, nom, proptest, criterion, and other excellent crates
- **Testing Frameworks**: Property-based testing for edge case validation
- **CI/CD**: GitHub Actions for cross-platform validation

## Special Thanks

We extend our gratitude to:
- The M-Bus and OMS standardization committees for comprehensive specifications
- The open-source community for sharing knowledge and test data
- Contributors who have helped improve this implementation

## Contributing

We welcome contributions, bug reports, and additional test vectors. Please see our [Contributing Guidelines](CONTRIBUTING.md) for more information.

## License Note

`mbus-rs` is licensed under MIT/Apache-2.0. Test data from external sources is used for validation only and is not redistributed as part of this crate. All code is original and written specifically for this project.