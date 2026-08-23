# mbus-rs Standards Support

This document states what mbus-rs implements and how it is tested, against the
standards it targets. It deliberately avoids a single "compliance percentage" —
the previous version of this file carried fabricated figures ("100% compliant",
"98% coverage") that were not derived from any measurement, and mislabelled the
crypto modes. What follows is verified against the code.

Maturity is uneven and stated per area: the **wired** path is mature; the
**wireless** path decodes real meter traffic but is still being consolidated
toward a 1.0 release. Where something is implemented but has not been checked
against live meter traffic, that is said explicitly.

## Summary

| Area | What is implemented | Validated against |
|------|--------------------|-------------------|
| EN 13757-2/3 (wired) | Frame parse/pack, fixed + variable records, DIF/VIF(E) chains | Golden frames from real devices; VIF exponents pinned to spec Table 10 by test |
| EN 13757-4 (wireless) | Mode-C link layer (Type A/B), per-block CRC-16/EN-13757 | CRC check value `0xC2B7`; C-mode receive on hardware. **S/T framing: not validated against live traffic** |
| Extended Link Layer | CI 0x8C–0x8F, AES-128-CTR | Captured Kamstrup Multical 21 traffic |
| Compact frames | Layout learned from a full frame and re-applied | Format signature confirmed against captured traffic |
| OMS v4.0.4 security | Mode 5 (AES-128-CBC), Mode 9 (AES-128-GCM, 12-byte tag) | Known-answer vectors. **Not validated against live Mode 5/9 traffic** |
| ETSI EN 300 220 | ToA calculation, duty-cycle tracking, LBT | Unit tests |
| Hardware | SX126x + RFM69 drivers, Raspberry Pi HAL | C-mode RX verified on a Pi 5 gateway |
| `mbus-core` | `no_std`, no-heap, linker-verified panic-free decode core | Builds for `thumbv6m-none-eabi`; panic ratchet in CI |

**Not implemented:** OMS Mode 7; OMS master-key derivation (AES-CMAC); Mode 13 TLS
(IP transport, out of scope for this crate).

## Wired M-Bus (EN 13757-2/3)

- **Frame formats:** long (0x68) and short (0x10) frames; single-character (0xE5)
  ACK; L-field, C/A/CI fields; checksum (sum C..end excl. check/stop); 0x16 stop.
- **Secondary addressing:** A=0xFD / CI=0x52 selection; wildcard narrowing on
  collision; fabrication-number / enhanced-ID / bus-address VIF searches.
- **VIF handling:** primary and extended (VIFE) chains up to 10 extensions;
  special codes 0x7C/0xFC (ASCII unit), 0x7D/0xFD (extended VIF), 0x7E/0xFE
  (wildcard), 0x7F/0xFF (manufacturer-specific raw). Exponents are pinned to the
  spec table by test.
- **Data types:** 8/16/24/32/48/64-bit integers, BCD, ASCII, real; date/time.
  Large integer and BCD counters keep their exact 64-bit value (no `f64` folding
  at parse time).
- **Physical layer:** auto-baud probe across the standard rates; collision retry.

## Wireless M-Bus (EN 13757-4)

- **Link layer:** mode-C, frame types A and B, multi-block with per-block
  CRC-16/EN-13757 (poly `0x3D65`, init `0x0000`, xorout `0xFFFF`). The standard
  check value `"123456789" → 0xC2B7` is asserted by test. Mode-C receive is
  exercised on hardware.
- **S and T mode framing** is implemented but has **not** been validated against
  live traffic.
- **Compact frames (CI 0x79):** the record layout is learned from a full frame
  and re-applied to later compact frames; the 2-byte format signature is
  confirmed against captured traffic. Full-frame request (CI 0x76) on a cache miss.
- **Communication modes (reference):** S1/S2 (868.3 MHz, Manchester), T1/T2
  (868.95 MHz, 3-out-of-6), C1/C2 (868.95 MHz, NRZ). R2 is a stub.

## Encryption

Built on the audited RustCrypto crates (`aes`, `ctr`, `cbc`, `aes-gcm`, `cmac`,
`hmac`, `sha1`), never hand-rolled. Enabled by the default `crypto` feature;
without that feature the crypto module does not exist (it cannot silently degrade
to a no-op).

- **Extended Link Layer — AES-128-CTR.** Verified against captured Kamstrup
  Multical 21 traffic.
- **OMS Mode 5 — AES-128-CBC** (Security Profile A). Covered by known-answer
  vectors. *(Note: earlier docs called this "CTR"; the cipher is CBC. The CTR
  path was a bug and was retired.)*
- **OMS Mode 9 — AES-128-GCM.** 11-byte AAD (L+C+M+A+V+T+Access), 12-byte IV,
  12-byte truncated tag per OMS (16-byte tag available for compatibility).
  Covered by known-answer vectors and a dedicated tag test.
- **OMS Mode 7 — not implemented.**
- **Key handling.** Keys are used as provisioned (`KeyMode::Direct`, the default).
  OMS master-key derivation (AES-CMAC) is **not** implemented; the only derivation
  path is a legacy XOR that is deprecated and documented in-code as *not a KDF*
  (XOR is reversible and leaks the master key). Supply the per-device key directly.

The implemented OMS modes have **not** been validated against live Mode 5/9 meter
traffic — the meters reachable from the test gateway use ELL encryption.

## Regulatory (ETSI EN 300 220)

- **Duty-cycle limits** tracked with a rolling window: 1% (868.0–868.6 MHz),
  0.1% (868.7–869.2 MHz), 10% (869.4–869.65 MHz).
- **Listen Before Talk:** −85 dBm threshold, pre-TX check with backoff.
- **Time-on-air:** per-mode chip calculation (S 2× Manchester, T 1.5× 3-out-of-6,
  C 1× NRZ at 100 kcps).

These are implemented and unit-tested; they have not been independently certified.

## Hardware

- **Radios:** SX126x and RFM69 drivers; SX126x supports a dual-mode wM-Bus
  GFSK / LoRa profile. SPI up to 16 MHz.
- **Platform:** Raspberry Pi 4/5 HAL over rppal (GPIO/SPI). C-mode receive is
  verified end-to-end on a Pi 5 gateway.
- **Core portability:** the protocol core (`mbus-core`) is `no_std` with no heap
  and no reachable panics on the decode path, and builds for `thumbv6m-none-eabi`.

## Testing

- **Golden frames:** 5 wired + 7 wireless, from real devices (e.g. Elster,
  Kamstrup, Engelmann).
- **Crypto:** ~15 unit tests in `crypto.rs` plus a dedicated Mode 9 GCM tag test;
  OMS Mode 5 CBC vectors in `mbus-core`.
- **Property tests:** proptest for VIF decoding.
- **Fuzzing:** cargo-fuzz targets for frame parsing, data encoding, VIF decode,
  LoRaWAN, and multi-telegram reassembly (`fuzz/fuzz_targets/`).
- **Line coverage:** 66.8% (region 69.3%), measured with `cargo llvm-cov`, default
  features, 2026-08-23. Reproduce with `cargo llvm-cov --summary-only`.

## Not implemented / out of scope

- **OMS Mode 7** (AES-128-CBC dynamic) — no code path exists.
- **OMS master-key derivation (AES-CMAC)** — supply the device key directly.
- **Mode 13 TLS** — IP transport; belongs in a separate crate.
- **R2 receive-only mode** — stub.
