# LoRaWAN join persistence — closing the two 1.0.4 anti-replay gaps

## Problem

The join responder is 1.0.4 **crypto**-compatible — a 1.0.4 device joins and its
uplinks decrypt, because 1.0.4 keeps the 1.0.x join/key/MIC algorithms. What it is
missing is the two **stateful anti-replay obligations** that 1.0.4 tightened, both
of which require durable state:

1. **DevNonce replay protection.** 1.0.4 makes DevNonce a device-side monotonic
   16-bit counter. The network MUST store the highest DevNonce accepted per DevEUI
   and reject any JoinRequest whose DevNonce is not strictly greater. We keep no
   such store (`join_responder.rs:8`), so a replayed JoinRequest is accepted and
   mints a fresh session.

2. **JoinNonce monotonicity across restarts.** 1.0.4 makes JoinNonce (the 1.0.x
   AppNonce) a network-side counter the device tracks; the device rejects any
   JoinAccept whose JoinNonce is not strictly greater than the last it accepted.
   Our counter is in-memory and resets to 1 on reboot (`join_responder.rs:95`) —
   observed live when a gateway restart wiped it. After any restart, a strict
   1.0.4 device would reject our JoinAccepts because the JoinNonce went backwards.

Both are the same shape of fix: **persist a small amount of per-device join state,
durably, before the JoinAccept goes on air.** The gateway already runs a redb
store for device inventory; this extends it.

## Governing principle: durable before live

This is the rule the rest of the gateway already follows (keys, inventory), applied
to the join. The persistence write MUST complete **before** the JoinAccept is
transmitted. The two crash windows both have to be safe:

- Crash *after* persist, *before* transmit → the device never got a JoinAccept and
  retries with a **higher** DevNonce (it increments per attempt), which we accept.
  A JoinNonce is "spent" with no frame sent — harmless, since JoinNonce need only
  be monotonic, and gaps are allowed.
- Crash *after* transmit, *before* persist → **the replay hole and the JoinNonce
  regression we are trying to prevent.** So this ordering is not optional.

RX1 opens 5 s after the JoinRequest; a redb write is sub-millisecond, so persisting
inside the budget is not a concern (staging already measures ~5 ms).

## Where the pieces live

Same split as the crypto/key work: **policy in the crate, storage in metermon.**

- **Crate (`src/lorawan.rs`)** — a pure `JoinNonceGuard` trait and the freshness
  rule, with no I/O. Unit-testable without a database.

  ```rust
  /// Outcome of admitting a JoinRequest's DevNonce.
  pub enum DevNonceVerdict { Fresh, Replay { last: u16, seen: u16 } }

  /// Pure rule: strictly-greater, or first-ever.
  pub fn admit_dev_nonce(last: Option<u16>, seen: u16) -> DevNonceVerdict {
      match last {
          None => DevNonceVerdict::Fresh,               // first join from this DevEUI
          Some(l) if seen > l => DevNonceVerdict::Fresh,
          Some(l) => DevNonceVerdict::Replay { last: l, seen },
      }
  }

  /// Durable per-device join state, implemented by the gateway.
  pub trait JoinStore {
      fn last_dev_nonce(&self, dev_eui: &[u8; 8]) -> Option<u16>;
      /// Record a newly-accepted DevNonce. Must be durable on return.
      fn record_dev_nonce(&mut self, dev_eui: &[u8; 8], nonce: u16) -> Result<(), JoinStoreError>;
      /// Reserve and return the next JoinNonce for this device. Must be durable
      /// on return, and strictly greater than any previously returned for it.
      fn next_join_nonce(&mut self, dev_eui: &[u8; 8]) -> Result<u32, JoinStoreError>;
      /// Clear a device's DevNonce high-water, for a legitimate re-provision.
      fn reset_dev_nonce(&mut self, dev_eui: &[u8; 8]) -> Result<(), JoinStoreError>;
  }
  ```

- **metermon (`metermon-rs/src/join_store.rs`)** — a redb-backed `JoinStore`, one
  new table alongside the existing five.

## redb schema

One table, keyed by DevEUI (wire order, 8 bytes rendered hex for a `&str` key to
match the store's existing key style), value a compact record:

```
const JOIN_STATE: TableDefinition<&str, &str> = TableDefinition::new("join_state");
// key:   dev_eui hex, e.g. "0004a30b00ff0001"
// value: JSON { "last_dev_nonce": u16, "next_join_nonce": u32 }
```

Per-device JoinNonce (not global) is chosen deliberately: it is what 1.0.4
specifies ("device-specific"), it sidesteps any 24-bit global-counter exhaustion
question, and it keeps both counters for a device in one row updated under one
transaction. `next_join_nonce` starts at 1 and increments; DevAddr assignment can
stay as it is (the address space is not replay-sensitive).

## Control flow (revised `handle_join`)

```
on JoinRequest:
    parse; verify MIC under the device AppKey           # unchanged
    if MIC invalid: drop                                # unchanged

    verdict = admit_dev_nonce(store.last_dev_nonce(dev_eui), jr.dev_nonce)
    if verdict == Replay: log security event, drop      # NEW — gap 1
    # --- everything below is durable-before-live ---
    join_nonce = store.next_join_nonce(dev_eui)?        # NEW — persisted, gap 2
    store.record_dev_nonce(dev_eui, jr.dev_nonce)?      # NEW — persisted, gap 1
    build JoinAccept with app_nonce = join_nonce
    derive session keys
    wait to RX1; transmit                               # only after both writes
    persist session (see "companion") ; report upstream
```

Both writes happen before transmit. `next_join_nonce` and `record_dev_nonce` for
the same DevEUI should be a **single redb write transaction** so a crash cannot
leave one advanced and the other not.

## Edge cases

- **First join from a DevEUI** — no row; `admit_dev_nonce(None, _)` is Fresh;
  `next_join_nonce` initialises to 1. Row created.
- **Legitimate re-provision / factory reset** — a re-keyed device restarts its
  DevNonce at 0, which we would now reject as replay. This is correct 1.0.4
  behaviour, not a bug: clearing it is an explicit operator action via
  `reset_dev_nonce`, exposed through the same Device Manager control path that
  provisions keys (an `op:` message), so re-commissioning stays a deliberate act.
- **DevNonce 16-bit wraparound** — a device exceeding 65 535 joins wraps and must
  be re-provisioned; joins are infrequent, so this is a documented limitation, not
  a handled case.
- **Store write failure** — do **not** transmit the JoinAccept. A join we cannot
  record durably is a join we must not grant, or the next boot reopens the replay
  hole. Log and drop; the device retries.

## Companion (not one of the two gaps, but the same mechanism)

The user framed this as the two 1.0.4 gaps; two general LNS gaps share the store
and are cheap to fold in once it exists, and are noted here so the schema is not
redesigned later:

- **Session persistence** — `sessions` is in-memory today and is wiped on reboot
  (observed: `uplink from unknown DevAddr`). Persisting `DevAddr → {NwkSKey,
  AppSKey, FCntUp}` in a sibling table lets uplinks decode across restarts.
- **FCntUp tracking** — with sessions persisted, store and enforce a
  strictly-increasing FCntUp (replay protection for data frames) and carry the
  full 32-bit counter (today the upper 16 bits are assumed 0, which also breaks the
  MIC past 65 536 uplinks).

These are listed, not designed in detail; the two 1.0.4 gaps above are the scope.

## Testing

- **Crate unit** — `admit_dev_nonce`: first-seen Fresh; equal and lesser Replay;
  greater Fresh. Pure, no I/O.
- **metermon persistence** — write DevNonce/JoinNonce, drop the store, reopen,
  assert the DevNonce high-water survived and `next_join_nonce` continues above its
  last value rather than resetting. This is the reboot the live restart exposed.
- **Hardware promotion** — with the Pico OTAA rig: (a) join, note JoinNonce N;
  restart the responder; join again; assert JoinNonce > N and the Pico *accepts*
  (no JoinNonce-regression rejection). (b) Replay a captured JoinRequest verbatim;
  assert it is rejected as a DevNonce replay, and no new session is minted.

## Observability

A rejected DevNonce is a security-relevant event, not routine noise — emit it
(DevEUI, last vs seen) to the gateway health/event stream, the way silent-device
and recovery events are already surfaced, so a replay attempt is visible rather
than silently dropped.

## Scope and status

This closes the two 1.0.4-specific gaps and nothing more. It does **not** add MAC
commands, `LinkADRReq` channel-pinning, RX2, or duty-cycle accounting — those
remain deliberately out of the "join responder, not a network server" boundary.
The design is untested against a real 1.0.4 meter; validation is against the Pico
RadioLib rig and the persistence tests above until a production LoRa device exists.
```
