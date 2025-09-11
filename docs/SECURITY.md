# Security Policy

## Supported Versions

The following versions of mbus-rs are currently being supported with security updates:

| Version | Supported          |
| ------- | ------------------ |
| 1.0.x   | :white_check_mark: |
| < 1.0   | :x:                |

## Reporting a Vulnerability

We take the security of mbus-rs seriously. If you believe you have found a security vulnerability, please report it to us as described below.

### Where to Report

**Please do not report security vulnerabilities through public GitHub issues.**

Instead, please report them via one of the following methods:

1. **Email**: Send details to jfabienke@arqitekta.com with subject line "[SECURITY] mbus-rs vulnerability"
2. **GitHub Security Advisory**: Use GitHub's private vulnerability reporting feature (if enabled)

### What to Include

Please include the following information in your report:

- Type of vulnerability (e.g., buffer overflow, privilege escalation, data exposure)
- Full paths of source file(s) related to the vulnerability
- Location of the affected source code (tag/branch/commit or direct URL)
- Step-by-step instructions to reproduce the issue
- Proof-of-concept or exploit code (if possible)
- Impact of the vulnerability, including how an attacker might exploit it
- Any potential mitigations you've identified

### Response Timeline

- **Initial Response**: Within 48 hours, we will acknowledge receipt of your report
- **Status Update**: Within 7 days, we will provide an initial assessment
- **Resolution Timeline**: Depending on complexity:
  - Critical: 7-14 days
  - High: 14-30 days
  - Medium: 30-60 days
  - Low: 60-90 days

### Disclosure Policy

- We will work with you to understand and reproduce the issue
- We will prepare a fix and release it as soon as possible
- We will credit you for the discovery (unless you prefer to remain anonymous)
- We ask that you give us reasonable time to address the issue before public disclosure

## Security Considerations for M-Bus Communication

### Physical Layer Security

M-Bus communication over serial ports has inherent security limitations:

1. **Physical Access**: Serial ports require physical access to the device
2. **No Built-in Encryption**: Standard M-Bus protocol doesn't include encryption
3. **Device Authentication**: Limited authentication mechanisms in the protocol

### Best Practices

When using mbus-rs in production:

#### 1. Physical Security
- Secure physical access to M-Bus converters and serial ports
- Use locked cabinets for hardware installations
- Monitor physical access logs

#### 2. Network Isolation
- Isolate M-Bus networks from public networks
- Use VLANs or separate network segments
- Implement firewall rules for any network bridges

#### 3. Access Control
- Implement application-level authentication
- Use principle of least privilege
- Log all access attempts and data requests

#### 4. Data Protection
```rust
// Example: Encrypting sensitive data after collection
use mbus_rs::{send_request, MBusRecord};

async fn secure_data_collection(address: u8) -> Result<Vec<EncryptedRecord>, Error> {
    let records = send_request(address).await?;
    
    // Encrypt sensitive meter readings before storage/transmission
    let encrypted_records = records.into_iter()
        .map(|record| encrypt_record(record))
        .collect();
    
    Ok(encrypted_records)
}
```

#### 5. Input Validation
- Always validate device addresses (1-250 for primary)
- Sanitize data before storage or display
- Implement rate limiting for requests

#### 6. Monitoring and Auditing
```rust
// Example: Audit logging for security events
use log::{info, warn};

async fn audited_device_scan() -> Result<Vec<u8>, MBusError> {
    info!("Security: Device scan initiated by user: {}", user_id);
    
    let result = scan_devices().await;
    
    match &result {
        Ok(devices) => info!("Security: Scan completed, found {} devices", devices.len()),
        Err(e) => warn!("Security: Scan failed - {}", e),
    }
    
    result
}
```

### Known Security Limitations

1. **No Encryption in Wired M-Bus**: The standard M-Bus protocol doesn't support encryption
2. **Limited Authentication**: Only basic address-based device selection
3. **Replay Attacks**: No built-in protection against replay attacks
4. **Man-in-the-Middle**: Physical access allows MITM attacks on serial communication

### Wireless M-Bus Security

When wireless M-Bus support is implemented:

- AES-128 encryption will be supported
- Key management will follow EN 13757-4 standard
- Security modes 5, 7 will be implemented

## Security Updates

Security updates will be released as:

- **Patch versions** (1.0.x) for non-breaking security fixes
- **Minor versions** (1.x.0) if security fixes require API additions
- **Major versions** (x.0.0) only if breaking changes are absolutely necessary

Subscribe to security announcements:
- Watch the GitHub repository for releases
- Enable GitHub security advisories
- Check CHANGELOG.md for security-related updates

## Compliance

This project aims to comply with:

- EU General Data Protection Regulation (GDPR) for meter data handling
- Industry best practices for IoT device communication
- OWASP IoT Security Guidelines where applicable

## Contact

For any security-related questions that don't involve reporting a vulnerability:
- Open a discussion on GitHub (for general security topics)
- Consult the documentation for security best practices

Remember: Security is everyone's responsibility. If you see something, say something!