# Mode 13 TLS Encryption (IP-Only)

## Overview

Mode 13 implements TLS 1.2/1.3 encryption for M-Bus communication over IP networks as specified in OMS 7.3.7. This mode is **exclusively for IP-based transport** and is not applicable to RF (wireless) M-Bus communication.

## Standards Reference

- **OMS 7.3.7**: Mode 13 TLS specification
- **RFC 5246**: TLS 1.2 specification
- **RFC 8446**: TLS 1.3 specification
- **EN 13757-5**: M-Bus over IP networks

## Key Characteristics

### Transport Requirements
- **Protocol**: TCP/IP only
- **Port**: 10001 (default M-Bus over TCP)
- **TLS Version**: Minimum TLS 1.2, TLS 1.3 preferred
- **Certificate**: X.509v3 with ECDSA P-256

### Cipher Suites (TLS 1.2)
- `TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256` (mandatory)
- `TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384` (recommended)

### Cipher Suites (TLS 1.3)
- `TLS_AES_128_GCM_SHA256` (mandatory)
- `TLS_AES_256_GCM_SHA384` (recommended)

## Implementation Status

**Status**: Not Implemented (IP-only, out of scope for RF transport)

Mode 13 is not implemented in this crate as it is specifically designed for IP-based M-Bus communication. The current implementation focuses on:
- Wired M-Bus via serial ports
- Wireless M-Bus (wM-Bus) via RF transceivers
- Modes 5, 7, and 9 for RF encryption

## Rationale for Non-Implementation

1. **Transport Mismatch**: Mode 13 requires TCP/IP transport which is fundamentally incompatible with the RF-based wireless M-Bus implementation
2. **Different Use Case**: Mode 13 targets gateway-to-server communication over internet/LAN, not device-to-gateway over RF
3. **Standard TLS Libraries**: For IP-based M-Bus, standard TLS libraries (rustls, native-tls) should be used directly

## Alternative Approaches

For secure M-Bus communication in this crate:
- **RF Transport**: Use Mode 9 (AES-128-GCM) for authenticated encryption
- **Serial Transport**: Use Mode 5 (AES-CTR) or Mode 7 (AES-CBC)
- **IP Transport**: If IP support is added, integrate with standard TLS libraries

## Future Considerations

If IP-based M-Bus support is added to this crate:
1. Create separate `mbus_tcp` module
2. Integrate with `tokio-rustls` or `native-tls`
3. Implement M-Bus application protocol over TLS
4. Support certificate management and validation
5. Comply with OMS 7.3.7 requirements

## Example Architecture (Conceptual)

```rust
// This is conceptual - not implemented
mod mbus_tcp {
    use tokio_rustls::TlsAcceptor;
    
    pub struct MBusTlsServer {
        tls_acceptor: TlsAcceptor,
        port: u16, // Default 10001
    }
    
    impl MBusTlsServer {
        pub async fn listen(&self) -> Result<(), Error> {
            // Accept TLS connections
            // Process M-Bus frames over TLS
        }
    }
}
```

## References

- OMS Group Specification Vol. 2 Issue 7.3.7
- DLMS/COSEM over TCP/IP (IEC 62056-47)
- M-Bus over IP (EN 13757-5)