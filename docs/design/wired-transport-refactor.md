# Design: Unified wired M-Bus transport / codec / session

Status: **proposed — design only, not implemented.** Scope: wired M-Bus (`src/mbus/`) only.
Version: **v2.1** (second review pass).

## Revisions in v2.1 (second review → resolution)

| # | Finding | Resolution |
|---|---|---|
| 1 | Short-frame prefix `[0x10,C,A,CS]` (4 bytes) returned `Complete(5)` → slice panic | `inspect_prefix` returns `NeedMore(1)` at 4 bytes; `Complete(5)` only at 5 bytes with stop `0x16`; wrong stop → `Err` (§1). |
| 2 | Request retry wrongly toggled FCB | **Retransmission keeps the same FCB**; FCB toggles only for the *next* telegram after a successful more-frames response. Two-level retry matrix added (§1b). |
| 3 | Step 7 pulled in secondary addressing, which can't run through the handle (`send_request_to_secondary` unimplemented, `mbus_device_manager.rs:344`) | Secondary-addressing scenario **deferred** alongside `FrameHandler`/manager wiring (§5, §8). |
| 4 | Enhanced-CI e2e command doesn't reference `$CI_FEATURES` | Exact command specified: `cargo test --test e2e_scenarios --features test-util -- --test-threads=1 --nocapture` (§5, §7). |
| 5 | `bus` moved into `Box::new` before `bus.baud_changes()` | Builder returns `(VirtualBus, VirtualBusProbe)`; probe exposes `baud_changes()` + `assert_finished()` (§4). |
| 6 | `checksum_errors`/`frame_errors`/`io_errors` absent from public `CollisionStatistics` (`serial.rs:139`) | Made **private `Session` counters** (or routed to `instrumentation::stats`); public struct unchanged → non-breaking claim holds (§1b, §7). |
| 7 | e2e file is **block-commented**, not `#[ignore]`d; count said seven, listed eight | Scenarios are rewritten (not un-ignored); 8 scenarios, secondary deferred → 7 migrated + the happy path; `create_response_frame` (`e2e_scenarios.rs:26-46`) is another duplicate to delete (§5, §6). |

## Revisions in v2 (review findings → resolution)

| # | Finding | Resolution |
|---|---|---|
| 1 | `ConfigurableTransport` extension trait can't be recovered from `Box<dyn ByteTransport>` | Baud is a `set_baud_rate` **method on `ByteTransport`** with a default `Err(Unsupported)` — one seam, object-safe (§1). |
| 2 | `frame_len(start)` left Session doing header staging / byte preservation — the original bug class | Replaced with an **incremental codec API** `inspect_prefix(&buf) -> DecodeProgress`; Session only appends, codec owns every boundary and validates duplicated long-frame length/start fields (§1, §1b). |
| 3 | Collision-retry-on-checksum contradicts production (retries only timeout) | Explicit **error taxonomy**: `Timeout` is the only retryable class; checksum/parse/transport errors are terminal; each names the stat it bumps (§1b). |
| 4 | Timeout/cancellation semantics undefined | **One absolute deadline per receive attempt** (`timeout_at`), `read()` is a documented **cancellation-safe** contract, and `Silence` is consumed at read-start so retries advance (§1b, §4). |
| 5 | `cfg(test)` isn't set for the lib when building `tests/*.rs`; `test-util` didn't exist | Add a **`test-util` feature** gating the fakes + `with_transport`; e2e target uses `required-features`; add to enhanced-CI `CI_FEATURES` (§5, §7). |
| 6 | Moving `frame.rs` into `codec` breaks public `mbus::frame` | **`mbus::frame` stays** — it *is* the codec module; the incremental API is added to it. "codec" is a role, not a new path (§2, §3, §7). |
| 7 | Multi-telegram modeled as two unsolicited replies | Corrected: **another REQ_UD2 with toggled FCB**; scripted as `ExpectWrite → Reply(more) → ExpectWrite(FCB^) → Reply(final)` (§4). |
| 8 | `disconnect` still a no-op under Drop-only | Session holds `Option<Box<dyn ByteTransport>>`; `disconnect()` **takes/drops** it; later ops return `Closed` (§1, §8-ops). |
| — | Simplify `VirtualBus` | Dropped the semantic `Collision`/`Garbage` variants for **composable wire events**; malformed/collision are just `Reply(bytes)` (§4). |
| — | Defer optional `FrameHandler` wiring | Removed from the migration sequence; listed as an explicit follow-up (§5). |

---

## Problem

The bug that triggered the correctness sweep (`recv_frame_single_attempt` dropping length bytes)
survived because the production receive path and every test path are *different code*:

- `MBusDeviceHandle` (`serial.rs:168`) is a concrete struct hard-bound to
  `tokio_serial::SerialStream` (`serial.rs:169`). **No transport seam.**
- Production receive = `recv_frame` (`serial.rs:411`) → `recv_frame_with_collision_handling`
  (retry loop, `serial.rs:416`; retries only `NomError` containing `"timeout"`, `:429`) →
  `recv_frame_single_attempt` (byte accumulation + length math, `serial.rs:461`, start-byte
  dispatch `:478-503`) → `frame.rs::parse_frame`.
- The wired "mock" is a **parallel reimplementation**: `serial_testable.rs::recv_frame`
  (`:65-141`) re-derives the timeout table and start-byte/length math; `serial_mock.rs::queue_frame_response`
  (`:65-106`) hand-assembles frames with its **own checksum loop** (`:87-91`).
- `add_wmbus_handle_mock` (`mbus_device_manager.rs:60`) is a **wireless** mock and never touches
  the wired path. `tests/e2e_scenarios.rs` references a third set of stubs, but its imports and every
  scenario body are **block-commented** (`:9-22`, `:48`–EOF) — zero compiled e2e coverage — and it
  carries yet another hand-rolled checksum builder (`create_response_frame`, `:26-46`).

Net: the wired receive path has **no black-box coverage through a handle**. Any bug in
buffering, length math, timeout, or retry is invisible to the suite.

## Invariants

1. **Transport moves bytes only** — no framing, parsing, retry, scan, or protocol state (the one
   acknowledged exception, baud reconfiguration, is a single method on the same trait, §1).
2. **One shared codec + session layer** owns buffering, frame boundaries, checksums, timeouts,
   retries, collision handling.
3. **Real serial, in-memory, and scripted virtual bus all exercise that exact layer.**
4. **Tests cannot substitute an alternate device-handle implementation** — one `MBusDeviceHandle`;
   tests vary only the injected transport.
5. The **virtual bus** models fragmented reads, delayed responses, malformed frames, collisions,
   timeouts, and consecutive frames — from composable wire primitives.
6. **Wired first.** Do not force wM-Bus radio semantics into this abstraction yet.
7. **Bisectable migration**; compatibility adapters removed at the end.

---

## 1. Trait signatures + ownership / concurrency model

```rust
// src/mbus/transport.rs — the ONLY seam. Bytes in, bytes out, plus the one acknowledged
// serial-specific poke (set_baud_rate). No framing knowledge.
#[async_trait]                                   // async-trait already a dep (Cargo.toml:22)
pub trait ByteTransport: Send {
    /// Read up to buf.len() bytes. Ok(0) means end-of-stream (closed).
    /// MUST NOT impose its own timeout — the session owns timing (invariant 1 & 2).
    /// MUST be cancellation-safe: if the returned future is dropped before completion
    /// (a timeout elapses), no bytes may be consumed-and-lost. (Contract detail in §1b.)
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError>;
    async fn write_all(&mut self, buf: &[u8]) -> Result<(), TransportError>;
    async fn flush(&mut self) -> Result<(), TransportError>;

    /// The one acknowledged exception to "bytes only". Object-safe: a concrete method with a
    /// default, no generics. Non-serial transports inherit `Unsupported`; SerialTransport
    /// reconfigures the port; VirtualBus records the request for assertion (§4).
    fn set_baud_rate(&mut self, _baud: MBusBaudRate) -> Result<(), TransportError> {
        Err(TransportError::Unsupported)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("transport I/O: {0}")] Io(#[from] std::io::Error),
    #[error("transport closed")]   Closed,
    #[error("operation not supported by this transport")] Unsupported,
}
```

```rust
// src/mbus/frame.rs — KEPT at this path (source-compat, finding 6). Pure, synchronous, zero I/O.
// The SOLE framing authority. parse_frame/pack_frame/verify_frame unchanged; the incremental
// boundary API is ADDED here so the Session never does length math (finding 2).

pub enum DecodeProgress {
    /// Need at least this many MORE bytes appended before asking again.
    NeedMore(usize),
    /// A complete frame occupies exactly this many leading bytes.
    Complete(usize),
}

/// Decide, from whatever prefix has been accumulated so far, how to make progress. Owns ALL
/// boundary logic currently inlined at serial.rs:478-503 and duplicated at
/// serial_testable.rs:92-136 — and validates the duplicated long-frame fields (L1==L2, the
/// second 0x68) here, not in the session. Returns Err for a structurally invalid prefix.
pub fn inspect_prefix(buf: &[u8]) -> Result<DecodeProgress, FrameError>;

pub fn parse_frame(input: &[u8]) -> IResult<&[u8], MBusFrame>;    // unchanged (frame.rs:93)
pub fn pack_frame(frame: &MBusFrame) -> Vec<u8>;                  // unchanged (frame.rs:192)
pub fn verify_frame(frame: &MBusFrame) -> Result<(), MBusError>; // unchanged (frame.rs:238)
```

`inspect_prefix` contract (the whole point of finding 2 — the session only appends):

| prefix `buf` | returns |
|---|---|
| `[]` | `NeedMore(1)` |
| `[0xE5]` | `Complete(1)` (ACK) |
| `[0x10]` | `NeedMore(4)` (short frame is 5 bytes: `10 C A CS 16`) |
| `[0x10, C, A, CS]` (4 bytes — no stop yet) | `NeedMore(1)` (v2.1 #1) |
| `[0x10, C, A, CS, 0x16]` | `Complete(5)` |
| `[0x10, C, A, CS, s]` where `s != 0x16` | `Err(FrameError::Stop)` |
| `[0x68]` | `NeedMore(3)` (need L,L,0x68) |
| `[0x68, L1, L2, s]` where `L1!=L2` or `s!=0x68` | `Err(FrameError::LongHeader)` |
| `[0x68, L, L, 0x68]` | `NeedMore(2 + L)` (total = 6 + L) |
| `[0x68, L, L, 0x68, …(6+L bytes total), last==0x16]` | `Complete(6 + L)` |
| `[0x68, L, L, 0x68, …(6+L bytes total), last!=0x16]` | `Err(FrameError::Stop)` |
| leading byte ∉ {0xE5,0x10,0x68} | `Err(FrameError::StartByte)` |

`inspect_prefix` validates the trailing stop byte `0x16` before returning `Complete` for both frame
types — a full-length prefix with a wrong stop is `Err(FrameError::Stop)`, never `Complete` (v2.1 #1),
so the session never slices a structurally invalid frame.

```rust
// src/mbus/session.rs — owns buffering, timeouts, retries, collision handling, req/resp, scan.
// Generic over the transport TRAIT; NEVER re-implements framing (calls frame:: only).
pub struct Session {
    transport: Option<Box<dyn ByteTransport>>,   // Option so disconnect() can take/drop it (finding 8)
    baud: MBusBaudRate,
    config: SerialConfig,
    stats: CollisionStatistics,
}

impl Session {
    pub fn new(transport: Box<dyn ByteTransport>, config: SerialConfig) -> Self;

    pub async fn recv_frame(&mut self) -> Result<MBusFrame, MBusError>;   // collision loop → attempt
    pub async fn send_frame(&mut self, f: &MBusFrame) -> Result<(), MBusError>;
    pub async fn send_request(&mut self, addr: u8) -> Result<Vec<MBusRecord>, MBusError>;
    pub async fn scan_devices(&mut self) -> Result<Vec<String>, MBusError>;
    pub async fn disconnect(&mut self) -> Result<(), MBusError>;          // takes/drops transport

    // Single receive ATTEMPT. Append-only buffer; codec owns boundaries; ONE deadline (finding 4).
    async fn recv_attempt(&mut self, deadline: Instant) -> Result<MBusFrame, MBusError> {
        let t = self.transport.as_mut().ok_or(MBusError::Transport(TransportError::Closed))?;
        let mut buf = Vec::new();
        loop {
            match frame::inspect_prefix(&buf)? {                 // FrameError -> terminal
                DecodeProgress::NeedMore(n) => fill(t, &mut buf, n, deadline).await?, // Timeout if deadline passes
                DecodeProgress::Complete(len) => {
                    let (_, f) = frame::parse_frame(&buf[..len]).map_err(to_parse_err)?;
                    frame::verify_frame(&f)?;                     // checksum -> terminal
                    return Ok(f);
                }
            }
        }
    }
}
```

`fill` reads exactly `n` more bytes into `buf` using `tokio::time::timeout_at(deadline, t.read(..))`;
on elapse → `MBusError::Timeout`; on `Ok(0)` → `MBusError::Transport(Closed)`. Because `fill`
reads exactly what `inspect_prefix` asked for, `buf` never overshoots a frame boundary.

```rust
// src/mbus/serial.rs — MBusDeviceHandle: thin facade over Session, SAME public name and method
// signatures (finding 6 / §7). lib.rs and the manager are source-compatible.
pub struct MBusDeviceHandle { session: Session, port_name: String }

impl MBusDeviceHandle {
    pub async fn connect(port: &str) -> Result<Self, MBusError>;                 // builds SerialTransport
    pub async fn connect_with_config(port: &str, cfg: SerialConfig) -> Result<Self, MBusError>;

    #[cfg(feature = "test-util")]                                                // finding 5
    pub fn with_transport(t: Box<dyn ByteTransport>, cfg: SerialConfig) -> Self;

    // send_frame/recv_frame/send_request/scan_devices/disconnect delegate to session — unchanged sigs.
}
```

**Ownership / concurrency.** Wired M-Bus is half-duplex request/response → single owner: `&mut
self` over an owned `Option<Box<dyn ByteTransport>>` (no `Arc<Mutex>`). `ByteTransport: Send` +
async-trait Send futures keep `HashMap<String, MBusDeviceHandle>` (`mbus_device_manager.rs:20`)
working unchanged.

**Boxed, not `MBusDeviceHandle<T>`** — a generic param would leak into the manager's `HashMap`
and every public signature naming the handle (breaking). Boxing costs one vtable dispatch per
read, far below serial latency.

## 1b. Error taxonomy, retry, and timeout contract (findings 3 & 4)

**Error classes** — exhaustive and explicit:

| Error | Class | Retry at recv? | Stat bumped | Notes |
|---|---|---|---|---|
| `MBusError::Timeout` | **transient** | yes — backoff, ≤ `max_collision_retries` | `timeout_errors`, then `total_collisions` on exhaustion | replaces `NomError("timeout")` at `serial.rs:466/485/500` and the string match `:429` |
| `InvalidChecksum` | **terminal** | no | `checksum_errors` | surfaced to caller |
| `FrameParseError` / `FrameError` (bad start, L1≠L2, bad stop) | **terminal** | no | `frame_errors` | surfaced |
| `Transport(Closed)` | **terminal** | no | — | surfaced |
| `Transport(Io)` | **terminal** | no | `io_errors` | surfaced |

This **preserves production semantics** (only timeout is retried; `serial.rs:446-448` returns all
else immediately) while making the classification typed instead of a substring match. A
checksum/parse failure is *not* a recv-level collision retry. Collision-**during-scan** (a garbled
response meaning "≥2 devices matched a wildcard") is a distinct concern handled in `scan_devices`,
not the generic recv loop.

`checksum_errors`/`frame_errors`/`io_errors` are **private `Session` counters** (or routed to the
existing `instrumentation::stats`), **not** additions to the public `CollisionStatistics`
(`serial.rs:139`, which has only `total_collisions`/`successful_communications`/`timeout_errors`/
`baud_rate_switches`/`collision_rate`). Only those existing public fields are touched, so the
non-breaking claim (§7) holds (v2.1 #6).

**Retry / FCB matrix (v2.1 #2).** The FCB (Frame Count Bit) is exactly what lets a slave tell a
*retransmission* from a *new* transaction, so a retransmit MUST reuse the same FCB; FCB toggles only
to advance to the next telegram after a successful response.

| Situation | Action | FCB |
|---|---|---|
| recv timeout | re-read (≤ `max_collision_retries`, backoff, `serial.rs:416`); no re-send | unchanged |
| recv retries exhausted | *request-level* may re-send the **same** REQ_UD2 (`send_request`, max 3, `serial.rs:523`) | **same** (retransmission) |
| success, more-records-follow set | send next REQ_UD2 for the next telegram | **toggled** |
| parse / checksum / transport error | terminal — surface to caller | n/a |

Baud adaptation (`send_request_with_adaptation`, `:364`) sits above this and re-sends at a new baud
via `set_baud_rate`; that is a transport reconfigure, independent of FCB.

**Timeout / cancellation contract (finding 4):**
- **One absolute deadline per receive *attempt*.** `recv_attempt` computes `deadline =
  Instant::now() + baud.timeout()` once; every fragment read uses `timeout_at(deadline, …)`. A
  slow byte trickle does **not** reset the clock per fragment. Each attempt in the collision loop
  gets a fresh deadline.
- **`read()` is cancellation-safe.** When `timeout_at` elapses it drops the read future; the impl
  must not have consumed-and-lost bytes. `tokio`'s `AsyncReadExt::read` satisfies this;
  `SerialTransport` is a thin map over it; `VirtualBus` satisfies it by advancing its script
  cursor at read-*start* (below).
- **`Silence` is consumed at read-start**, so a timed-out read that is dropped still advances the
  script — otherwise every retry would re-hit the same `Silence` forever. On a mid-frame timeout
  the attempt discards its `buf` and the *request-level* retry re-sends; the script models the
  meter re-responding.
- Determinism: timeout tests run under `#[tokio::test(start_paused = true)]`; the harness advances
  the paused clock, so no wall-clock flake.

---

## 2. Current → new component mapping

| Current (file:line) | New home | Fate |
|---|---|---|
| `serial.rs:461` start-byte dispatch + length math `:478-503` | `frame::inspect_prefix` (pure) + `Session::recv_attempt` (append-only) | **split** |
| `serial.rs:416` collision/backoff loop | `Session::recv_frame` | move; retry class = `Timeout` only |
| `serial.rs:523/364/586` `send_request`/`…_with_adaptation`/`attempt_communication` | `Session` | move |
| `serial.rs:464/483/498` `SerialStream` reads; `:399-406` writes | `SerialTransport: ByteTransport` | move behind seam |
| `serial.rs:466/485/500` `NomError("timeout")` + match `:429` | `MBusError::Timeout` (typed) in `Session` | **replace** (§1b) |
| baud reconfigure (`auto_adapt_baud_rate`) | `ByteTransport::set_baud_rate` (default `Unsupported`) | move to seam (finding 1) |
| `serial.rs:389` `disconnect` (no-op) | `Session::disconnect` takes/drops `Option` transport | **fix** (finding 8) |
| `serial_testable.rs:65-141` duplicate recv | — | **delete** (subsumed by Session over VirtualBus) |
| `serial_testable.rs:13,35` `SerialPort` trait + `TestableDeviceHandle` | `ByteTransport` + `with_transport` | **replace** |
| `serial_mock.rs:15/65-106` `MockSerialPort` + own checksum builder | `VirtualBus`/`InMemoryTransport`; tests build frames via `frame::pack_frame` | **replace/delete** |
| `tests/mock_support.rs:6-7` empty stubs | `VirtualBus` | **delete** |
| `frame.rs:93/192/238` parse/pack/verify | **stays in `mbus::frame`**; gains `inspect_prefix` | keep (finding 6) |
| `frame.rs` checksum | called unchanged | SIMD removal has since landed — `calculate_mbus_checksum` is now a plain fold in `mbus-core` |
| `mbus_protocol.rs:664/673` `FrameHandler` "not implemented" | borrow `&mut Session` | **deferred follow-up** |

---

## 3. Layer boundaries and dependency direction

```
                depends on ──►
  transport  ◄─────  session  ◄─────  device_handle/facade  ◄─── manager, lib.rs
  (bytes+baud)         │
     ▲                 └──►  mbus::frame (codec)  ◄── also used directly by tests, examples
     │                        (pure framing; zero I/O, zero transport deps)
  SerialTransport / VirtualBus / InMemoryTransport  implement `ByteTransport`
```

- **`mbus::frame` (codec)** depends on nothing but data types — no async, no I/O, no transport.
- **`mbus::transport`** depends on nothing framing-related.
- **`mbus::session`** depends on the `ByteTransport` *trait* and on `mbus::frame`. Never on
  `SerialStream` or a concrete transport.
- **facade/manager** depend on `session`; `mbus_protocol::StateMachine` sits beside it.

Rule in one line: *no module below `session` knows how frames are shaped; no module at or above
`transport` knows how bytes arrive.* No cycles.

---

## 4. Virtual-bus scripting model (composable wire events)

`VirtualBus` is a `ByteTransport` that plays a script of **minimal, composable wire primitives**
— no semantic `Collision`/`Garbage` variants. It emits only **raw bytes**; a *valid* frame in a
script is built with `frame::pack_frame`, and validity is judged only by
`frame::parse_frame`/`verify_frame`. That "no second framing implementation" property is what
makes the key-review-question answer "yes".

```rust
pub enum Wire {
    ExpectWrite(Vec<u8>),               // assert the session wrote exactly these bytes (the request)
    Reply(Vec<u8>),                     // deliver raw bytes to subsequent reads
    ReplyChunked(Vec<u8>, Vec<usize>),  // deliver across N reads → fragmentation
    Delay(Duration),                    // virtual-time gap before the next delivery → delayed response
    Silence,                            // never deliver; consumed at read-start → timeout (finding 4)
}
```

Semantics composed from primitives (no dedicated variants):
- **malformed frame** = `Reply(bytes_that_fail_verify)` → terminal `InvalidChecksum`/`FrameParseError`.
- **collision garble** = `Reply(garbled_bytes)` → terminal parse error on a normal request; a
  scan probe's own logic interprets it as "≥2 devices".
- **consecutive frames / multi-telegram** = `ExpectWrite`/`Reply` pairs (see below).
- **baud change** = asserted via `probe.baud_changes() -> Vec<MBusBaudRate>`, recorded by the
  `set_baud_rate` override — not a `Wire` step.

`build()` returns **both** the transport and a `VirtualBusProbe`, because the transport is moved
into `Box::new(..)` and can no longer be observed directly (v2.1 #5). The probe holds shared state
(`Arc`) into the bus:

```rust
pub struct VirtualBusProbe { /* Arc into the bus's script cursor + baud log */ }
impl VirtualBusProbe {
    pub fn baud_changes(&self) -> Vec<MBusBaudRate>;   // recorded by the set_baud_rate override
    pub fn assert_finished(&self);                     // panics if any scripted step was NOT consumed
}
```

Example exchanges:

```rust
// Happy path
let (bus, probe) = VirtualBus::script()
    .expect_write(pack_frame(&req_ud2(addr, /*fcb=*/false)))
    .reply(pack_frame(&long_response))
    .build();
let mut h = MBusDeviceHandle::with_transport(Box::new(bus), cfg);
let recs = h.send_request(addr).await?;                 // runs the REAL production path
probe.assert_finished();                                // unconsumed expectations cannot pass silently
```

- **Fragmented long frame:** `ExpectWrite(req)`, `ReplyChunked(pack_frame(&long), vec![1, 2, 40])`
  — start byte, then the two length bytes + second 0x68, then the body. Exercises the append-only
  read loop and proves `inspect_prefix` handles split reads.
- **Timeout → retry → success (recv-level, no re-send):** `ExpectWrite(req)`, `Silence`,
  `Reply(pack_frame(&resp))` under a paused clock. Attempt 1 read times out (`Silence` consumed at
  read-start), backoff, attempt 2 succeeds.
- **Multi-telegram (corrected, finding 7):** `ExpectWrite(pack_frame(&req_ud2(addr, false)))`,
  `Reply(pack_frame(&frame_more))` *(more-records-follow set)*, `ExpectWrite(pack_frame(&req_ud2(addr,
  true)))` *(FCB toggled)*, `Reply(pack_frame(&frame_final))`. Asserts the session re-requests with
  the toggled FCB, not that the meter volunteers a second frame.
- **Baud adaptation:** script replies only after the expected baud; assert `probe.baud_changes() ==
  [MBusBaudRate::Baud2400]`. Adaptation *policy* is fully covered; only the physical reconfigure
  lives in `SerialTransport`, hardware-validated.

`InMemoryTransport` is the degenerate `VirtualBus` (preloaded reply, no timing/faults) for the
simplest unit tests; droppable if `VirtualBus`'s trivial script is ergonomic enough.

---

## 5. Migration sequence (small compiling steps, bisectable)

Every step compiles and keeps the suite green.

1. **Introduce the seam, no behavior change.** Add `ByteTransport`/`TransportError` +
   `SerialTransport` (wraps the existing `SerialStream`, `set_baud_rate` reconfigures the port).
   `Session` holds `Option<Box<dyn ByteTransport>>`; `MBusDeviceHandle` wraps a `Session` whose
   transport is a real `SerialTransport`. Byte-identical production path. *Compatibility-adapter
   step.* All existing tests pass unchanged.
2. **Add `frame::inspect_prefix`/`DecodeProgress`/`FrameError`** and rewrite the receive loop as
   append-only over it (`Session::recv_attempt`). Delete the length math from the old single-attempt
   path. Behavior identical; `frame_tests`/`golden_frames` unaffected.
3. **Typed timeout + taxonomy.** Add `MBusError::Timeout`; `fill` returns it; the collision loop
   matches the variant; delete the `msg.contains("timeout")` guard (`serial.rs:429`) and the three
   `NomError("timeout")` sites. Classify all recv errors per §1b. Behavior-preserving.
4. **Add `test-util` feature, `VirtualBus`(+`InMemoryTransport`), and `with_transport`.** No
   production change; the real handle can now run over a fake transport.
5. **Prove the path.** The e2e scenarios are **block-commented** (`e2e_scenarios.rs:9-22` and
   `:48`–EOF), so they are rewritten, not un-ignored (v2.1 #7). Rebuild
   `e2e_connect_and_read_single_device` as a real happy-path read over `VirtualBus` through the real
   handle, and delete the file's hand-rolled `create_response_frame` (`:26-46`, another duplicate
   checksum path) — build frames with `frame::pack_frame`. Run it in CI with
   `cargo test --test e2e_scenarios --features test-util -- --test-threads=1 --nocapture` (the
   enhanced-CI e2e command must pass `--features test-util`, v2.1 #4).
6. **`disconnect` becomes real.** `Session::disconnect` takes/drops the transport; post-disconnect
   ops return `Transport(Closed)`. Update `MBusDeviceHandle::disconnect` (was `serial.rs:389`
   no-op) and add a VirtualBus test asserting closed-after-disconnect.
7. **Rewrite the remaining scenarios**, one per commit, onto `VirtualBus`: scan (`:60`),
   multi-telegram/FCB (`:67`), error-recovery/retries (`:108`), baud adaptation (`:160`), collision
   handling (`:186`), complete-workflow (`:217`, minus any secondary-address leg), performance-
   under-load (`:297`). **Secondary addressing (`:133`) is deferred** — it needs
   `send_request_to_secondary` (`mbus_device_manager.rs:344`, not implemented) wired first (v2.1 #3).
8. **Delete the parallel paths.** Remove `serial_testable.rs`, `serial_mock.rs`'s bespoke builder,
   and `tests/mock_support.rs`. Their assertions are now covered by §6.
9. **Remove adapters.** Delete any transitional shim from step 1; confirm `MBusDeviceHandle`'s
   public surface is unchanged.

**Deferred (not in this refactor):** (a) wiring `FrameHandler`/`DataRetrievalManager` to `&mut
Session` (`mbus_protocol.rs:664/673`); (b) **secondary-address selection/scan** through the manager
(`send_request_to_secondary`, `mbus_device_manager.rs:344`; `select_device_by_secondary_address`,
`mbus_protocol.rs:111`) and the `e2e_secondary_addressing` scenario that depends on it (v2.1 #3).

---

## 6. Tests moved or replaced at each step

- **Steps 1–3:** no test changes. `frame_tests` (12), `frame_advanced_tests` (9), `golden_frames`
  (5), `standards_compliance` wired cases (4), `mbus_protocol_tests` (all) hit codec/StateMachine,
  unaffected. `inspect_prefix` gets its own unit tests (the table in §1 becomes assertions,
  including the `L1≠L2` / bad-second-`0x68` rejections — the original bug, now a codec test).
- **Steps 4–5:** `e2e_connect_and_read_single_device` is rebuilt from a block-commented stub into a
  real assertion, and the file's `create_response_frame` duplicate (`:26-46`) is deleted; new
  Session-level unit tests over `VirtualBus` replace the deleted `serial_testable.rs` tests
  one-for-one **through the production path**: `recv ack/short/long` → `Reply(pack_frame(..))`;
  `recv timeout` → `Silence` + paused clock; `invalid start`/`bad checksum` → `Reply(bad bytes)`;
  `request_response` → `ExpectWrite`+`Reply`; baud mapping → `probe.baud_changes()`.
- **Step 6:** closed-after-disconnect test.
- **Step 7:** rewrite 7 of the 8 block-commented `e2e_scenarios` (secondary addressing deferred,
  v2.1 #3), each backed by `VirtualBus`.
- **Step 8 deletes:** `serial_testable.rs` unit tests (10), `serial_mock.rs` unit tests (6),
  `mock_support.rs` stubs. Coverage **moves from the parallel mock path onto the production path**
  — the entire point.

---

## 7. Public API compatibility impact

- **Non-breaking for the wired public API.** `MBusDeviceHandle` keeps its name and every method
  signature, so `lib.rs` facade functions (`lib.rs:90-152`) and `MBusDeviceManager`
  (`mbus_device_manager.rs:20`) are source-compatible.
- **`mbus::frame` stays** (finding 6): `parse_frame`/`pack_frame`/`verify_frame`/`MBusFrame`/
  `MBusFrameType` keep their path; `inspect_prefix`/`DecodeProgress`/`FrameError` are additive.
- **`test-util` feature** gates `with_transport`, `VirtualBus`, `InMemoryTransport`. Cargo.toml:
  `[[test]] name = "e2e_scenarios" required-features = ["test-util"]`. The enhanced-CI e2e job's
  command does **not** interpolate `$CI_FEATURES` (`ci-enhanced.yml:135`), so it must be changed
  explicitly to `cargo test --test e2e_scenarios --features test-util -- --test-threads=1
  --nocapture` (v2.1 #4); adding `test-util` to `CI_FEATURES` only reaches the `test`/integration
  jobs, not this command. Fallback if a feature is unwanted: keep `with_transport` public and
  `#[doc(hidden)]`.
- **Additive public items:** `ByteTransport`, `TransportError` (and the `test-util` fakes).
- **Boxed, not generic** — no `<T>` leaks into the handle type or the manager `HashMap`.
- **Removals are test-only** (`serial_testable`, `serial_mock`, `mock_support`) → no public impact.
- **Not touched here:** the `pub use serial::*` / `frame::*` globs (`mbus/mod.rs:15-18`) — narrowing
  them is workstream #2.

## §8-ops. Disconnect / lifecycle contract (finding 8)

`Session.transport: Option<Box<dyn ByteTransport>>`. `disconnect()` does `self.transport.take()`
then drops it (a final `flush().await.ok()` first for serial). Every I/O method starts with
`self.transport.as_mut().ok_or(MBusError::Transport(TransportError::Closed))?`, so any operation
after `disconnect()` returns `Closed` rather than panicking or silently succeeding. This replaces
the current no-op (`serial.rs:389`) and composes with the manager's `disconnect_all` (which then
drops the handles).

---

## 8. Non-goals (explicit)

- **Crypto rebuild** (RustCrypto / CMAC KDF / mode-from-Config-Field / Mode 5 CBC) — untouched.
- **Decoder consolidation** (`lora/decoder.rs` + `decoder_nom.rs` + `decoders/`;
  `smart_decoder_v2.rs`) — untouched.
- **Radio-driver unification** (`RadioDriver`/`WMBusHandle` generic) — untouched. The wM-Bus radio
  is deliberately **not** folded into `ByteTransport` (invariant 6); wireless keeps
  `WMBusHandleWrapper`. An analogous seam can come later once wired is proven.
- **`FrameHandler`/`DataRetrievalManager` transport wiring** — deferred follow-up (§5).
- **SIMD checksum removal** — done (was orthogonal to this refactor). `src/mbus/simd.rs`
  is gone; `calculate_mbus_checksum` is a plain wrapping fold in `mbus-core`.
- **Record/VIF parsing** — untouched.
- **Public-surface narrowing / workspace split** (`mbus-rs` vs `metermon-rs`) — separate (#2, #5).

---

## Key review question

> Can every production serial behavior be reproduced through the virtual bus **without
> duplicating any framing logic**?

**Yes**, with one named exception (baud reconfiguration), and "no duplication" is now load-bearing:

| Production behavior | Reproduced by | Boundary/validity owned by |
|---|---|---|
| start-byte dispatch + length math + L1==L2 / second-0x68 check | `Reply`/`ReplyChunked` | `frame::inspect_prefix` (once) |
| fragmented reads | `ReplyChunked` | Session append loop + `inspect_prefix` |
| delayed response / timeout | `Delay` / `Silence` + paused clock, single deadline | Session (`MBusError::Timeout`) |
| collision + backoff retry | `Silence` (→ Timeout) | Session retry loop (Timeout-only, §1b) |
| malformed frame | `Reply(bad bytes)` | `frame::parse_frame`/`verify_frame` |
| consecutive / multi-telegram (FCB) | `ExpectWrite`(FCB^)/`Reply` pairs | Session drain + codec |
| **baud-rate adaptation** | `set_baud_rate` override + `probe.baud_changes()` | Session *policy* covered; physical reconfigure is `SerialTransport`-only, hardware-validated |

The virtual bus emits only raw bytes; frames are built by `frame::pack_frame` and judged only by
`frame::parse_frame`/`verify_frame`. There is no second framing implementation anywhere — the
precise property whose absence let the original bug survive. The single exception (baud) is a
default-`Unsupported` method on the one seam, its policy fully virtual-bus-testable.
