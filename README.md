# lorawan-join-control

Shared, transport-agnostic message schema for coordinating a **LoRaWAN OTAA join**
between two apps that each own one half of the join:

- **`device-config`** — drives the meter optically over MinoConnect (SET OTAA `0x24`,
  Join Request `0x06`, read `CheckJoinAccept 0x07`).
- **`metermon`** (gateway) — a single-channel join responder that hears the
  JoinRequest, MIC-checks it, transmits the JoinAccept, and assigns a DevAddr.

Neither side alone can confirm a full join: the gateway knows it *sent* an accept, only
the meter knows it *received* one. This crate is the single source of truth for the
messages they exchange to agree on one `JoinOutcome`.

## Use

Both apps add it as a path dependency:

```toml
lorawan-join-control = { path = "../../lorawan-join-control" }
```

(`device-config` from `core/Cargo.toml`; `metermon-rs` from its `Cargo.toml` — both
resolve to the same sibling directory.)

## Protocol

```
device-config → gateway : ArmRequest      park responder on channel + SF
gateway → device-config : ArmReply        armed + what it's listening on
device-config → gateway : FiredNotice     one per optical Join Request (correlation)
gateway → device-config : JoinStatus      heard? mic_ok? assigned DevAddr?
device-config → gateway : VerifyRequest   the meter's own 0x07 DevAddr
gateway → device-config : VerifyReply     final JoinOutcome
```

`JoinOutcome::reconcile(&status, device_dev_addr)` is the verification handshake:

| outcome | meaning |
|---|---|
| `NotHeard` | no JoinRequest on the armed channel/SF |
| `HeardMicFail` | heard, but MIC failed (wrong AppKey / corruption) |
| `JoinedGatewayOnly` | accept sent, but the meter's `0x07` doesn't show it (downlink didn't land) |
| `VerifiedBothSides` | meter's `0x07` DevAddr == gateway's assignment — confirmed end-to-end |

Rides the gateway's existing MQTT broker (`topics` module: `join/<gateway_id>/…`); the
payload of each topic is the JSON of the same-named type. Nothing in the crate depends
on MQTT, so the schema works over any transport.

## Key custody

Credentials (the AppKey) are **never** carried in this protocol — they are provisioned
out-of-band and held by the gateway. Every message references a device only by its
DevEUI.
