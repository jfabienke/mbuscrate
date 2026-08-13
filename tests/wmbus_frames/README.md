# wM-Bus test-vector provenance

This directory documents the wireless M-Bus (wM-Bus) telegrams the test suite
asserts against. The goal of Phase 1.2 is that every wireless vector is checked
against a **known-good value from an independent source** — a real meter capture,
a published standard vector, or a ciphertext produced by an independent AES
implementation — and never merely round-tripped through this crate's own encoder.

The vectors themselves live inline in the tests (as hex string constants) rather
than as separate binary files, so they are diff-visible and self-documenting; this
README is the index that records where each one came from and what it proves.

## Frames asserted in `tests/wmbus_golden_frames.rs`

| Vector | Source | Type | Encrypted | What it proves |
|--------|--------|------|-----------|----------------|
| CRC-16/EN-13757 check value `"123456789" → 0xC2B7` | EN 13757-4 / CRC catalogue standard check | — | — | The canonical `wmbus::crc::calculate_wmbus_crc` matches the standard check vector, byte-for-byte, not our own round-trip. |
| KAM Type B, meter `74644444` | Real capture (Kamstrup, 868 MHz, CI 0x8D) | Type B | no | The public `decode_mode_c` recovers the correct BCD serial, manufacturer (`0x2C2D` = "KAM") and passes block-CRC on a genuine over-the-air frame. |
| KAM Type A, meter `85312884` | Real capture (Kamstrup, 0xCD sync) | Type A | no | `decode_mode_c` recovers the correct serial from a real Type A frame (regression guard against the old offset-by-one CI/type parse). |
| ELL-II encrypted, key `000102…0f` | Synthetic, built with **PyCryptodome** (independent AES-128-CTR) | Type B / ELL-II | AES-128-CTR | The public ELL path decrypts to the **independently-computed** plaintext `0000780413e8030000`; a real meter key is never used. |

## Encrypted-vector policy

- **No real meter AES key is ever committed.** The only key material in-tree is the
  published Zenner factory-default (`ZDK`, a documented constant in Zenner's shipped
  software) and synthetic test keys (`000102…0f`). Both are safe fixtures.
- Encrypted known-answer vectors are produced by an implementation **other than this
  crate** (PyCryptodome for the ELL frame) so the test cannot pass by round-tripping a
  bug in our own cipher wiring.
- OMS Mode 5 (AES-128-CBC, Security Profile A) IV construction and the idle-fill key
  oracle are asserted against fixed known values in
  `src/wmbus/oms.rs` and `tests/wmbus_golden_frames.rs`; the IV is cross-checked
  against the epulse C++ reference (`wmbus-device.cc:179`).

## What is deliberately absent

There is no captured **real** OMS Mode 5/7 telegram with a known plaintext here,
because that requires a real meter key, which must never enter the repository. Mode 5
CBC is therefore proven by (a) the independent NIST SP 800-38A CBC known-answer test
on the primitive, (b) the fixed-value IV construction, and (c) the wrong-key oracle —
not by a captured encrypted payload.
