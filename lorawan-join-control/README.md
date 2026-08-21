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

**This crate lives in the `mbuscrate` repo** (vendored 2026-08-21 as a git subtree, with
its history), and is a member of that workspace. `metermon-rs` depends on it by path:

```toml
lorawan-join-control = { path = "../lorawan-join-control", optional = true }
```

It was previously a standalone sibling directory that both apps path-depended on as
`../../lorawan-join-control`. That arrangement worked only on a machine where both repos
sat side by side: the crate had no git remote, so **CI could never resolve it** — and
because cargo resolves path dependencies even when the owning feature is disabled, a
default build failed outright rather than merely skipping the feature. That is what kept
the join-control PR unmergeable.

### device-config

`device-config` still points at the old sibling path and is therefore unaffected today,
but it now has a *second copy* of this contract — exactly the drift this crate exists to
prevent. Repoint it at the vendored copy (or vendor it there too and treat one as
canonical) before the schema next changes; a contract with two independent copies is not
a contract.

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
