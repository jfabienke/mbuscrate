# metermon-rs

A Rust reimplementation of the epulse C++ `metermon` wM-Bus gateway, built on the
`mbus-rs` crate. Its purpose is **A/B validation**: decode the same meter frames the
deployed `metermon` sees and diff the output, using the running gateway as an oracle.

## Architecture

```
             ┌─────────────────────────────────────────────┐
 RFM69 radio │  FrameSource                                 │
 (Pi, feat.  │   ├─ Rfm69Source   (feature = "radio", Pi)   │──┐
  "radio") ──┤   └─ FileReplaySource (any host)  ◄── capture │  │  raw frame bytes
             └─────────────────────────────────────────────┘  ▼
                          decode::decode_frame  ──►  JSON  ──►  MQTT / stdout
                          (routes through mbus-rs:
                           verify_wmbus_crc, WMBusCrypto)
```

The **decode core is platform-independent** and builds/tests on any host. The radio
path is behind the `radio` feature (needs Raspberry Pi + `rppal`, Linux/aarch64 only).
The capture-replay A/B feeds *identical bytes* through the decode core deterministically,
so it needs no radio and does not disturb the live `metermon`.

## Keys come from upstream MQTT, not the config

metermon does **not** store AES keys in `metermon.conf`. It subscribes to the control
topic (`meter/control/<gwid>`) and installs keys as they arrive:

```json
{ "op": "key", "meterid": 305419896, "key": "0102…0f" }
```

(`metermon.cc:273-278` → `CryptoKey::Add`.) metermon-rs mirrors this: the live `run`
path subscribes to the control topic and populates its keystore the same way. For the
offline replay A/B, supply the keys via `--keys <file>` — either a captured stream of
those control messages (one JSON per line) or a flat `{ "<meterid>": "<hex>" }` map.

## Commands

```bash
# Host-independent: decode a capture (one hex frame per line) to JSON lines.
# --keys is optional; without it, encrypted frames decode only to the header.
cargo run -- replay capture.hex --config metermon.conf --keys keys.jsonl

# Live on the Pi: RFM69 -> decode -> MQTT, keys from the control topic. Needs the
# radio feature + metermon stopped (single radio on /dev/spidev0.1).
cargo build --features radio           # on the Pi
./target/debug/metermon-rs run --config metermon.conf --shadow
```

## Status vs. the deployed metermon

| Stage | metermon-rs | Notes |
|---|---|---|
| RFM69 receive | ⚠️ via `RadioDriver` trait | routes around the `WMBusHandle` coupling gap (mbus-rs Phase 2.2) |
| wM-Bus CRC | ⚠️ routes to `verify_wmbus_crc` | **wrong polynomial until mbus-rs Phase 1.3** — real frames read `crc_ok:false` |
| Mode 5 decrypt | ⚠️ routes to `WMBusCrypto` | **CTR-not-CBC + bogus key derivation until Phase 1.4** — plaintext will NOT match metermon yet |
| CI dispatch, header decode | ✅ | mode read from Configuration Word, matching epulse |
| Record decode | ⚠️ first record only | `parse_variable_record` doesn't report bytes consumed; multi-record loop is a follow-up |
| MQTT publish | ✅ | `rumqttc`, live or shadow topic |
| Config parse | ✅ | reads `metermon.conf` |
| Key provisioning | ✅ | control-topic `op:key` (live) / `--keys` capture (replay), matching metermon |

**This is expected.** The scaffold routes every step through the exact `mbus-rs`
functions under repair. As Phases 1.3 (CRC) and 1.4 (crypto: CBC Mode 5, mode-from-CW,
`KeyMode::Direct`) land, this client's output converges on metermon's — and the A/B is
what measures that convergence.

## The A/B test

1. Capture raw frames once (brief coordinated `metermon` stop to free the radio, or a tap).
2. `metermon-rs replay capture.hex > rust.jsonl`
3. Decode the same capture with metermon's own tooling → `metermon.jsonl`
4. Diff — first on decrypted plaintext (the sharpest crypto signal), then per-record.

Non-destructive, deterministic, single-radio-safe: both decoders see identical input.
