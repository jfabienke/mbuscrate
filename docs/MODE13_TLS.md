# Mode 13 TLS Encryption (IP-Only) - Documentation Only

## Overview

**IMPORTANT**: This document describes Mode 13 for reference purposes only. Mode 13 is NOT implemented in mbus-rs as it requires IP-based transport, while this crate focuses on serial and RF communication.

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

**Status**: ❌ NOT IMPLEMENTED - Documentation Only

**Why not implemented:**
- Mode 13 requires TCP/IP transport (not serial or RF)
- This crate focuses on hardware-based communication (serial ports, SPI radios)
- IP-based M-Bus is a different use case (gateway-to-server vs device-to-gateway)

**What IS implemented:**
- ✅ Wired M-Bus via serial ports (RS-232/RS-485)
- ✅ Wireless M-Bus (wM-Bus) via SX126x RF transceivers
- ✅ Modes 5, 7, and 9 encryption for RF transport

## Rationale for Non-Implementation

1. **Transport Mismatch**: Mode 13 requires TCP/IP transport which is fundamentally incompatible with the RF-based wireless M-Bus implementation
2. **Different Use Case**: Mode 13 targets gateway-to-server communication over internet/LAN, not device-to-gateway over RF
3. **Standard TLS Libraries**: For IP-based M-Bus, standard TLS libraries (rustls, native-tls) should be used directly

## Available Security Options in mbus-rs

For secure M-Bus communication using this crate:
- **RF Transport**: ✅ Mode 9 (AES-128-GCM) - Fully implemented with OMS 7.3.6 compliance
- **Serial Transport**: ✅ Mode 5 (AES-CTR) and Mode 7 (AES-CBC) - Both available
- **IP Transport**: ❌ Not supported - Use standard Rust TLS libraries if needed

## Why IP Support Is Out of Scope

IP-based M-Bus is intentionally not included because:
1. **Different hardware layer**: IP uses network cards, not serial ports or SPI radios
2. **Different deployment model**: Server-to-server vs embedded device communication
3. **Standard solutions exist**: Regular TLS libraries (rustls, native-tls) work perfectly
4. **No added value**: M-Bus over TLS is just standard TLS + M-Bus frames
5. **Focus on hardware**: This crate specializes in hardware interfacing (GPIO, SPI, UART)

## Example Architecture (Conceptual Only)

```rust
// ⚠️ THIS CODE IS NOT PART OF mbus-rs
// Shown only for educational purposes
// If you need IP-based M-Bus, build it separately using:
// - tokio for async networking
// - rustls or native-tls for TLS
// - mbus-rs for frame parsing (if needed)

mod theoretical_mbus_tcp {
    use tokio_rustls::TlsAcceptor;
    
    pub struct MBusTlsServer {
        tls_acceptor: TlsAcceptor,
        port: u16, // Default 10001 per EN 13757-5
    }
    
    // You would implement this yourself using standard networking libraries
    // mbus-rs can still be used for parsing the M-Bus frames themselves
}
```

## References

- OMS Group Specification Vol. 2 Issue 7.3.7
- DLMS/COSEM over TCP/IP (IEC 62056-47)
- M-Bus over IP (EN 13757-5)