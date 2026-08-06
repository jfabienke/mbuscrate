# Vendor layering: generic core, standard extensions, device quirks

## Purpose

`mbus-rs` decodes meters from many manufacturers. Some of those manufacturers use
extension points the standard reserves for them; a few deviate from the standard
outright. Without a rule for where such code belongs, manufacturer-specific behaviour
leaks into the generic decoder (it already has — see [§5](#5-current-classification)),
and once it does, no one can tell which parts of a decode are standard and which are
one vendor's habit.

This document defines three layers, the principles that assign code to them, and the
end state the crate should converge on. It is a design document: it records decisions
and the evidence behind them, not an implementation.

## 1. The three layers

**Layer 0 — generic core.** EN 13757 and OMS as written. Everything that a conforming
device of any manufacturer would do. This is where the overwhelming majority of the
crate lives and should stay.

**Layer 1 — standard-sanctioned extensions.** Places where the standard *delegates* to
the manufacturer and generic code therefore cannot be correct, because it has no
information: manufacturer-specific VIF (`0x7F`, `0xFF`), manufacturer-specific DIF
blocks (`0x0F`, `0x1F`), the manufacturer CI range, vendor-defined status bits,
vendor key provisioning.

**Layer 2 — device quirks.** Places where the device diverges from the standard.
Generic code *would be* correct per specification but yields the wrong answer for this
device.

### Why the split matters

The two upper layers look similar and behave oppositely:

|  | Layer 1 (extension) | Layer 2 (quirk) |
|---|---|---|
| Relationship to spec | fills a gap the spec leaves | contradicts what the spec says |
| Effect of registering | **additive** — unknown becomes known | **overriding** — one answer replaces another |
| Effect of *not* registering | output is correct, just less informative | output is wrong for that device |
| Failure mode | safe (opaque bytes) | dangerous (masks corruption, or silently changes values) |
| Blast radius if mis-scoped | none — nothing else reads those bytes | every other vendor loses correct behaviour |

An extension can only ever turn "I don't know" into "I know". A quirk takes something
the crate believed and replaces it. They therefore deserve different defaults,
different evidence bars, and different visibility in the output — which is what the
principles below encode.

### 1.1 Where the crate stops: the decode/configuration boundary

All three layers live in `mbus-rs`, but the crate is not the whole system. It sits
between two neighbours, and the boundary between them is **decoding versus
configuration**, not generic versus proprietary:

- **The crate decodes what a meter broadcasts.** All metering and instrumentation
  decoding, including the *meaning* of status/alarm/INFO bits (see D2), value-fixing
  quirks, and the wire mechanics of bidirectional/session frames. Given the bytes and
  the inputs it needs, it produces typed readings and decoded status.
- **The backend Device Manager owns device parameters and reference data**: meter
  constants and scaling factors, and the authoritative model/firmware/configuration
  catalog. It applies constants to the crate's readings, and it **sends the resolved
  device profile downstream to the gateways on request**, so the crate can select a
  model-specific decode where the frame alone cannot (§4.3).
- **The configuration app commissions meters.** Commissioning is initiated there;
  the crate supplies the bidirectional wire exchange so a round-trip through
  gateway → backend confirms the provisioning chain end to end.

This fixes four boundary cases that the layer model alone did not settle:

1. **Status/alarm/INFO interpretation → crate.** Interpreting a *received* status field
   is decoding the telegram's meaning, so the crate owns the bit tables and emits named
   conditions. The model needed to pick the right table comes from case 3.
2. **Meter constants / scaling factors → Device Manager.** These are configured device
   parameters, absent from the telegram. The crate emits the raw typed value and the
   Device Manager scales it; the crate ships no constants and never guesses one.
3. **Model / firmware resolution → Device Manager, pushed to gateways on request.** The
   crate decodes identity *bytes*; the Device Manager resolves them to a product and
   sends that profile down, which the crate consumes as a decode input. Absent a
   profile it cannot derive, the crate fails safe to raw (§4.3).
4. **Bidirectional / session / commissioning → crate mechanics, config-app policy.** The
   crate parses/packs session frames and runs the link state machine; *when* to
   commission is the config app's decision; the confirming round-trip runs
   config app → backend → gateway → crate → device and back.

The dependency arrow: the Device Manager and gateways depend on the crate for decode
primitives; the crate depends on nothing above it — reference data it needs (a device
profile, a key) arrives as an **input**, never as a compiled-in table it sources itself,
with the single exception of status-bit semantics, which are decode knowledge (case 1).

## 2. Principles

**P1 — Frame-declared beats manufacturer-keyed.**
If the frame carries the information, the generic path reads it. Never branch on
manufacturer for something a byte already declares.
*Test:* could a conforming device from another manufacturer legitimately send this same
byte and mean the same thing? Then it is Layer 0.
*Worked example:* "Kamstrup uses ELL" is not a Kamstrup fact — CI `0x8D` says so, and
the session number's top three bits select the security profile. ELL is Layer 0.

**P2 — Extensions may only add meaning to bytes the standard leaves undefined.**
An extension must never change how a standard-defined field is parsed.
*Test:* unregister it. The output must remain **correct but less informative** — opaque
bytes instead of a decoded value. If unregistering changes a *value* rather than
removing detail, it is not an extension; it is a quirk.

**P3 — Never invent a value.**
Where neither layer knows what bytes mean, emit the raw bytes and a reason. A number
that means nothing is worse than "unknown", because a consumer cannot tell the
difference. (Today `02 FF 20` decodes to a bare `Manufacturer specific: 0.0`: the value
is real, but presented as an unidentified float, so a *nonzero* INFO would read as a
meaningless number rather than named conditions or a labelled raw bitmask.)

**P4 — Quirks are evidence-gated, narrowly scoped, and standard-by-default.**
Every quirk carries: the deviation (what the standard says versus what the device
does), the evidence, and the narrowest scope that fits — manufacturer *and* version or
device type where known, not manufacturer-wide by reflex. "Default" means the standard
behaviour: a device the scope does not match is decoded per specification, and a quirk
whose evidence is merely `Provisional` does not apply unless the caller opts in. A
`Verified` quirk, once registered, applies automatically within its scope — a quirk no
decode path applies protects nobody.

**P5 — A quirk that fires must say so.**
If a quirk changed the outcome — CRC tolerated, field reinterpreted — the decoded
result records it. Otherwise two gateways silently disagree and neither can be audited.

**P6 — Never apply a quirk on the strength of unverified data.**
The manufacturer comes from the frame; a corrupt frame yields a corrupt manufacturer.
Vendor dispatch therefore requires that the *identity-bearing* region — the link
header carrying the M and A fields — passed its own integrity check. Checks **beyond**
that region are exactly what a `tolerate_crc` quirk exists to relax, so integrity is
tracked per region, never as one frame-level boolean: a quirk may tolerate a failed
payload-block CRC, but no quirk may be selected by a manufacturer code that itself
arrived corrupted. (Mode C type A provides this granularity directly — the header
block carries its own CRC.) This is not hypothetical: applying one meter's cached
record layout to another's values is exactly how the gateway came to report
`1591213 W` from a water meter.

**P7 — One path, not two.**
No parallel `*_with_vendor` forks of the main decoder. A single path that takes a
possibly-empty registry cannot silently diverge from itself. The current duplicated API
is precisely why the vendor system is inert: every vendor-aware entry point exists, is
tested, and is called by nothing.

**P8 — Extensions are code, not credentials.**
Keys live in the keystore. A vendor entry may describe *how* a key is located or
derived; it never carries key material.

**P9 — Resolve vendor behaviour once per frame, not per field.**
Look up the applicable extension and quirk set once, after the link header yields the
device identity, and pass it down in the decode context.

## 3. Deciding the layer

For any behaviour, in order:

1. Does a byte in the frame declare it? → **Layer 0** (P1).
2. Does the standard mark this region manufacturer-defined? → **Layer 1**.
3. Does the device contradict a *conforming* reading of the standard, with evidence?
   → **Layer 2**.
4. Otherwise — the standard is ambiguous, or we simply lack data → **stay Layer 0**,
   choose the tolerant behaviour, document the assumption, and let the first
   counter-example promote it.

Step 4 is deliberate. An unexplained observation from one manufacturer is not yet a
quirk; treating it as one bakes a guess into the shape of the code.

## 4. End state

### 4.1 One decode path with a context

Replace the threaded `registry: Option<&VendorRegistry>` parameters (and the duplicate
`*_with_vendor` entry points) with a single context, per P7 and P9:

```rust
pub struct DecodeContext<'a> {
    /// Applicable vendor behaviour, resolved once per frame. May be empty.
    pub vendor: VendorBinding<'a>,
    /// Identity from the link header — the dispatch key.
    pub device: DeviceIdentity,
    /// Per-region integrity. The identity-bearing header block must be valid for
    /// ANY vendor dispatch (P6); failures in later blocks are what a
    /// `tolerate_crc` quirk may relax.
    pub integrity: Integrity,
}

pub struct Integrity {
    /// The link-header block (M/A fields) validated. Gates all vendor dispatch.
    pub header_valid: bool,
    /// Validity of the payload blocks that followed; a scoped quirk may tolerate
    /// failures here, recording that it did so (P5).
    pub blocks: BlockValidity,
}

pub struct DeviceIdentity {
    // Decoded from the frame's link header.
    pub manufacturer: ManufacturerCode, // e.g. "KAM"
    pub version: u8,
    pub device_type: u8,
    pub address: u32,
    /// Resolved product/firmware profile supplied by the Device Manager (§1.1 case 3),
    /// when available. Selects model-specific decode — e.g. which INFO bit table
    /// applies. `None` when the gateway holds no profile for this device; the crate
    /// then decodes from the frame alone and falls back to raw for anything the frame
    /// cannot disambiguate.
    pub profile: Option<DeviceProfile>,
}
```

An empty `VendorBinding` is the normal case and costs one branch. `profile` is likewise
usually `None` for a gateway that has not yet been sent a device catalog — decoding
still works, it is simply less specific.

### 4.2 Two traits, one registration point

```rust
/// Layer 1. Additive: may only decode bytes the standard leaves to the manufacturer.
pub trait VendorExtension: Send + Sync {
    fn provenance(&self) -> &Provenance;
    fn decode_manufacturer_vif(&self, ctx: &DecodeContext, vif: u8, vife: &[u8], data: &[u8])
        -> Option<VendorValue>;
    fn decode_manufacturer_dif_block(&self, ctx: &DecodeContext, dif: u8, data: &[u8])
        -> Option<Vec<VendorDataRecord>>;
    fn decode_manufacturer_ci(&self, ctx: &DecodeContext, ci: u8, payload: &[u8])
        -> Option<VendorPayload>;
    fn decode_status_bits(&self, ctx: &DecodeContext, status: u8) -> Option<Vec<StatusFlag>>;
}

/// Layer 2. Overriding: changes what the generic path would have concluded.
/// Every method returns evidence when it fires, so the outcome is auditable (P5).
pub trait VendorQuirks: Send + Sync {
    fn manifest(&self) -> &QuirkManifest;
    fn reinterpret_record(&self, ctx: &DecodeContext, rec: &mut MBusRecord) -> Option<QuirkApplied>;
    fn tolerate_crc(&self, ctx: &DecodeContext, err: &CrcErrorContext) -> Option<QuirkApplied>;
    fn adjust_payload_offset(&self, ctx: &DecodeContext, ci: u8, payload: &[u8]) -> Option<(usize, QuirkApplied)>;
}
```

Splitting the traits is what makes P2 and P4 enforceable rather than aspirational: the
type a vendor implements declares which contract it is signing up to, and reviewers
apply the corresponding bar.

### 4.3 Scope: dispatch on more than the manufacturer

Quirks are usually per model or firmware generation, not per manufacturer. Dispatch
must therefore be able to express that (P4):

```rust
pub struct VendorScope {
    pub manufacturer: ManufacturerCode,
    pub versions: Option<RangeInclusive<u8>>,   // None = any
    pub device_types: Option<&'static [u8]>,    // None = any
}
```

Most specific match wins; manufacturer-wide is the fallback, not the default.

**Discriminators the frame does not carry.** Some behaviour varies by firmware or
configuration code — Kamstrup's INFO bit tables differ across MULTICAL 602/66C/III/403
(`docs/MULTICAL_VENDOR_EVENTS.md`), and neither firmware nor config code appears in the
M/A link header. The resolved discriminator arrives out of band as
`DeviceIdentity.profile`, sent to the gateway by the Device Manager (§1.1 case 3). When
no profile is available and the frame cannot determine the model, the entry must **fail
safe to raw** (P3) rather than guess: the value is emitted undecoded (or, for a status
field, as the raw bitmask). A vendor entry may never assume a model it cannot prove from
the frame or the supplied profile.

### 4.4 Provenance is part of the entry

Both layers carry provenance, so a reader can distinguish a behaviour verified against
a capture from one inherited from a datasheet:

```rust
pub enum Evidence {
    /// Decoded from a frame this project captured. The gold standard.
    Captured { capture: &'static str, note: &'static str },
    /// Taken from vendor or standards documentation, not yet seen in our traffic.
    Documented { source: &'static str },
    /// Inferred from behaviour; no authority. Requires review before use.
    Inferred { rationale: &'static str },
}

pub enum Status { Verified, Provisional }

pub struct QuirkManifest {
    pub id: &'static str,          // stable, e.g. "qds-vif04-date"
    pub scope: VendorScope,
    pub deviation: &'static str,   // what the standard says vs. what the device does
    pub evidence: Evidence,
    pub status: Status,
}
```

`Provisional` values are surfaced as such in the decoded output, so a consumer can
choose whether to trust them. This makes "documented but not verified here" a
first-class state instead of a comment.

### 4.5 Quirks are visible in the output

A decoded frame carries `applied_quirks: Vec<QuirkApplied>`, and metermon-rs echoes it
into its JSON and its device store. A CRC tolerated by a quirk is not the same fact as
a CRC that passed, and the difference must survive to the consumer (P5).

### 4.6 Conformance harness

Two properties are machine-checkable and should be CI-enforced:

- **Extensions are additive (P2).** Decode a frame corpus with and without each
  extension registered; every standard-defined field must be byte-identical, and the
  only permitted difference is added detail.
- **Quirks are evidenced and visible (P4, P5).** Every registered quirk has a
  capture-backed or explicitly `Documented`/`Provisional` manifest, and a test proving
  that applying it changes the output *and* records a `QuirkApplied`.
- **Identity gating holds (P6).** Decode a corpus of frames whose header block fails
  its CRC; no vendor hook may have been consulted for any of them.

## 5. Current classification

### KAM — Kamstrup Multical 21 (cold water)

Evidence: live captures from the production gateway, decrypted and cross-checked against
an independent implementation (wmbusmeters, oracle-only). The KAM *heat* meters we also
hear (the ~22-meter cluster in §5 Others) are unclassified — no keys, so header-only.

| Behaviour | Layer | Notes |
|---|---|---|
| ELL CI `0x8D`, AES-CTR | 0 | Frame-declared (P1). Already generic. |
| Compact frames, format signature | 0 | OMS mechanism; signature confirmed on KAM traffic. A vendor differing would miss the lookup — safe failure. Assumption labelled in `wmbus::compact_frame`. |
| Type B framing, BCD address | 0 | Standard. |
| `02 FF 20` info codes | **0 transport / 1 meaning** | Arrives as a standard OMS record, so reading the raw bitmask is Layer 0; only the bit *meanings* are Layer 1 (D2). Today it decodes to a bare `Manufacturer specific: 0.0` — the value is real (INFO 0) but unidentified as a status field, so a nonzero value would surface as a meaningless number. Interpretation table is evidence-blocked (D3). |
| Leading 2-byte ELL field | **0** | Almost certainly a *standard* ELL payload CRC we had not decoded — not a vendor deviation (D1, corrected). Algorithm to be reproduced from EN 13757-4 §12; would then authenticate ELL decrypts. |

### QDS — QUNDIS HCA (implemented today, misfiled)

QUNDIS repurposes **VIF `0x04`** — Energy, 10¹ Wh in the standard — as a date field
with non-contiguous year bits. Apply P2's test: unregistering does not remove detail,
it changes the value into a different quantity. **This is a Layer 2 quirk implemented
through a Layer 1 hook.**

Consequent defects, all traceable to the misclassification:

- A hardcoded `mfr_id == "QDS"` branch sits in generic code
  (`src/payload/record.rs:670`) — violates P1 and P7.
- It is unreachable at runtime. QDS *is* correctly flagged `has_quirks: true` in the
  manufacturer database (the only entry that is), and both `with_defaults()` and
  `with_manufacturer_detection()` would register it — but **neither constructor is ever
  called from a decode path**, and `metermon-rs` never builds a registry at all. The
  wiring is complete on the vendor side and absent on the caller side.
- The module doc's example uses `[0x04, 0x6D, …]` (DIF `0x04`, VIF `0x6D` — a *standard*
  date record that never reaches the QDS branch), while the unit test correctly uses
  VIF `0x04`. Documentation and dispatch disagree.

QUNDIS is therefore the pilot for the migration: repairing it exercises both dimensions
on a vendor that already has a behavioural test.

### ZRI — Zenner (meter 55298170, live)

CI `0x72` long TPL header, Mode 5, medium `0x16`, five encrypted blocks, access number
incrementing per frame. All standard. **Layer 0 only — no vendor code needed.** This
null result matters: the abstraction must not pressure us into writing an extension for
every manufacturer we can name.

### KAW — six cold-water meters (evidenced 2026-08-06)

Once reception improved (~40 dB), six KAW cold-water meters appeared with **CRC-valid**
frames (`53231343`, `53231360`, `53231368`, `53231369`, `53231731`, `53520685`). So KAW
is now evidenced, not a ghost artefact — but everything observed is standard OMS, and we
hold no keys, so like ZRI it is **Layer 0 only, no vendor code**. Open question worth
resolving before any KAW-specific work: KAW is a *distinct* manufacturer flag from KAM
(Kamstrup, `0x2C2D`) — confirm whether it is a Kamstrup sub-brand or a different maker,
since that decides whether it could ever share a profile.

### Others observed (RVI, YKM, KEM, ZZI, UNK …)

Header-only sightings. **No classification**: P4 requires evidence and P6 forbids
attributing anything from a frame that failed integrity. Their presence in the
manufacturer database is Layer 0 metadata, not an extension. The store now holds **31
real devices** across these codes (24 KAM incl. a ~22-meter district-heating cluster,
6 KAW, 1 ZRI) — a useful reminder that the layers must stay thin: nearly all of that
traffic is Layer 0, and only two meters (the keyed KAM water pair) are even readable.

## 6. Decisions

**D1 — The KAM ELL leading field is Layer 0, and its CRC status is REOPENED (corrected
2026-08-06).**
This decision previously claimed the field is *not* a payload CRC. That was wrong on two
counts, found via the oracle's `--analyze` trace:

- **The evidence was mis-attributed.** The "identical value across differing payloads"
  observation came from the *Zenner* meter (55298170, TPL Mode 5) — not the KAM ELL
  field. For KAM the field *does* vary with the payload (`1cc5` for two identical compact
  readings, `3d19` for the full frame), which is how a CRC behaves.
- **The oracle validates it as a CRC.** `wmbusmeters --analyze` reports
  `017 : 1cc5 payload crc (calculated 1cc5 OK)` — it treats the field as a payload CRC
  and confirms it.

So this is almost certainly a **standard ELL payload CRC we simply had not decoded**, not
a vendor deviation — which removes the earlier "promote to a quirk" framing entirely.
The open part: no standard CRC-16 variant we tried reproduces `1cc5` over the obvious
plaintext range, so the exact computation is not yet understood. It must be derived from
EN 13757-4 §12 (the ELL definition), **not** from wmbusmeters' GPL code (D3).

Resting state unchanged until the algorithm is pinned: the field is exposed as
`leading_field`, not used for authentication, and decrypt acceptance rests on the TPL-CI
plausibility heuristic. **Next step:** reproduce the ELL payload CRC from the spec; if
confirmed, use it to authenticate ELL decrypts (replacing the ~97% heuristic with a real
integrity check) — a **Layer 0** improvement, since it is standard behaviour, not
Kamstrup-specific.

**D2 — KAM info codes ship as a provisional status-bit interpretation, not a
manufacturer-VIF extension.**
`docs/MULTICAL_VENDOR_EVENTS.md` reshapes this. Kamstrup's own 403/603/803 profile
documentation identifies register `369` as `Info bits` carried in ordinary C1
datagrams — i.e. the INFO field very likely arrives as a **standard OMS/M-Bus data
record**, not a proprietary CI payload. If so, the *transport* is Layer 0 (the generic
record parser reads the bytes), and only the *bit meanings* are vendor knowledge. That
makes this a `decode_status_bits`-style interpretation keyed to the model, not a
`decode_manufacturer_vif` hook — a materially different implementation from a
manufacturer-VIF extension, and one to confirm against a decoded capture before building.

The interpretation lives in the crate (§1.1 case 1): status/alarm meaning is decode
knowledge, so the crate owns the bit tables and emits named conditions, not just a
number. It ships **provisional**: `Evidence::Documented { source: "MULTICAL 602 register
map" }` and `Status::Provisional`, surfaced as provisional in the output, with the raw
bitmask value always emitted alongside the interpretation so P3 holds even if the
semantics differ. The evidence document is explicit that the bit tables differ across
MULTICAL 602, 66C, III and 403/603/803, so the applicable table is selected by the
model in `DeviceIdentity.profile` — supplied by the Device Manager (§1.1 case 3) — and
**must not** be applied Kamstrup-wide (P4). With no profile, the crate emits the raw
bitmask only. Upgrade to `Verified` only from one of our own captures with a correlated
optical read of register `0x0063`.

**D3 — wmbusmeters is a differential oracle, not a source to transcribe.**
The upstream `wmbusmeters` project (GPL-3.0-or-later) has mature Kamstrup decoders and
is a valuable reference. Because this crate is MIT, the boundary is strict and the use
is narrow:

- **Oracle only.** We run a pinned `wmbusmeters` build on *our own* captured telegrams
  and keys and compare its normalized output against ours. Comparing outputs copies
  nothing — it carries no license obligation. This is the sanctioned use.
- **Tables come from primary sources, never from the driver.** VIF/unit meanings are
  EN 13757, not wmbusmeters' work, so reproducing them is reproducing the standard. But
  the vendor status-bit → condition tables are the driver's own compiled work and the
  legally riskiest thing to lift; those are derived from Kamstrup's own documentation
  (the MULTICAL technical descriptions) plus our decrypted captures, and validated
  against the oracle — not read out of the XMQ.
- **Vectors are ours.** A real captured telegram is a fact, but to sidestep any question
  of upstream fixture provenance, committed regression vectors are our own captures
  (keys never committed). This is also how the still-open Phase 1.2 test-vector work
  gets done.

Two consequences that scope the immediate work:

- **Water first, heat later.** `kamheat.xmq` covers *heat* meters (302/403/602/603/803).
  Our meters that decrypt today — 74644444 and 63398862 — are Multical 21 *water* meters,
  a different driver family, and their status record is `02 FF 20` where heat is
  `02 FF 22` (same manufacturer, different VIFE — exactly the model-specificity D2
  predicts, so the tables are not interchangeable). The heat meter we own (80504381) is
  the kamheat target but we hold no key, so heat-meter work is deferred until that key
  is available.
- **The Multical 21 status *table* is currently evidence-blocked.** Every valid capture
  shows INFO = 0 (no fault), and the repo holds no primary-source Multical 21 water bit
  table (only the 602 heat table, in `MULTICAL_VENDOR_EVENTS.md`). So the crate exposes
  the raw INFO bitmask now and ships **no** named-condition mapping until a
  primary-source table or a fault-correlated capture exists. Inventing the mapping from
  the heat table would violate P4. The oracle harness is precisely what will let us
  build and confirm the table safely when the evidence arrives.

## 7. Migration

**Status (2026-08-06, PR #8):** steps 1–3 and 6–7 are implemented. Remaining: step 4
(Device Manager → gateway profile channel, needs the backend side), step 5 (KAM status
interpretation, evidence-blocked per D2/D3), step 8 (delete the deprecated shims).

Each step is independently shippable and behaviour-preserving unless stated.

1. **Introduce `DecodeContext` and collapse to one path** (P7, P9). Registry empty;
   no behaviour change.
2. **Thread the context from metermon-rs.** QUNDIS becomes reachable for the first
   time. Remove the hardcoded `mfr_id == "QDS"` branch from `payload::record`.
3. **Split the traits**; port the QUNDIS date handling to `VendorQuirks` with a
   manifest and its existing capture-backed test. *Behaviour change:* QDS dates decode
   correctly in the live path, and the quirk is reported.
4. **Add `DeviceIdentity.profile`** as a decode input and the Device Manager → gateway
   channel that supplies it (§1.1 case 3). Behaviour-preserving while the channel is
   empty.
5. **Add the KAM status-bit interpretation** for info codes per D2, table selected by
   the supplied profile, raw bitmask always emitted.
6. **Surface `applied_quirks`** (and applied status interpretation) in metermon-rs JSON
   and the device store (P5).
7. **Add the conformance harness** (§4.6) to CI.
8. **Retire the `*_with_vendor` duplicates.** Keep `has_quirks` as the database's
   declaration of intent, but derive registration from the registry's own entries so
   the flag and the registered set cannot drift apart.

The Device Manager's constant application (§1.1 case 2) and the session/commissioning
round-trip (§1.1 case 4) are separate deliverables in their own repos, tracked there;
this migration covers only the crate-side capability each depends on.

### 7.2 Profile channel wire contract (step 4)

The Device Manager → gateway profile channel reuses the proven key-channel shape:
request-driven (§1.1 case 3), MQTT-carried, and **durable-before-live** — a profile is
persisted to redb before it is installed in memory, and loaded from redb at startup
before any broker contact, so the gateway keeps its device knowledge across restarts
and broker outages exactly as it keeps its keys.

**Request** (gateway → backend, on the data topic, alongside `op:startup`):

```json
{ "op": "profile_request", "gw": "6543", "meters": [74644444, 63398862] }
```

Sent at monitor startup with the ids the device store currently tracks. The backend
also treats a plain `op:startup` as an implicit request for everything it knows about
that gateway.

**Response** (backend → gateway, on the control topic, one message per device):

```json
{ "op": "profile", "meterid": 74644444, "model": "MULTICAL 21", "firmware": null }
```

`model` is required and bounded; `firmware` optional. The gateway validates, persists
(`profiles` table in redb, keyed by meter id), installs, and from the next frame
onward builds `DeviceIdentity.profile = Some(DeviceProfile { model, firmware })` for
that meter. The decode JSON carries `"profile": "<model>"` so the effect is
observable end to end even before anything *interprets* the profile (that is step 5).

The crate itself needs no change for this step: `DeviceIdentity.profile` has been the
waiting seam since migration step 1. Until the real Device Manager exists, a **mock
backend** (`metermon-rs mock-backend`, catalog-file driven) implements the contract so
the gateway side is testable end to end; the mock is the contract's reference
implementation and is superseded by the Device Manager repo, not extended.

### 7.1 Cross-repo dependencies and the enabling seam

Two clarifications about how this lands across repos:

- **`DeviceIdentity.profile` is net-new infrastructure**, not a refactor. Today's
  gateway receives no device profile from the backend — the Device Manager → gateway
  push in step 4 does not exist yet. It is behaviour-preserving while the channel is
  empty (decode simply stays less specific and falls back to raw), so the crate side can
  land ahead of the backend side. But it is the **seam that makes decisions 1 and 3
  real rather than notional**: without a profile arriving, the KAM status
  interpretation (D2) cannot be model-scoped and must emit the raw bitmask only. Model-
  specific status decoding is therefore gated on this channel existing end to end, not
  on the crate code alone.
- **The boundary is a contract, not just a split.** The crate must emit enough for the
  Device Manager to do its half: raw typed values (for constant application, case 2),
  decoded identity bytes (for the catalog lookup that produces the profile, case 3), and
  which quirks and status interpretations fired (case 1, P5). That output shape is the
  interface between the repos; changing it is a cross-repo change, and it should be
  versioned as one.

## 8. Out of scope

The generic core is not up for renegotiation here: ELL, compact-frame expansion, the
VIF exponent table and record value decoding stay Layer 0 and are unaffected. LoRaWAN
device credentials (e.g. the Zenner LoRa HCA) belong to the radio stack, not to vendor
layering. This document does not add a second registry for wired M-Bus; the same
binding applies to both transports, since the manufacturer is a property of the device,
not the link.

**Owned by the Device Manager, not the crate (§1.1).** Meter constants and scaling
factors (pulse weight, meter factor, CT/VT ratios) are configured device parameters:
the crate decodes and emits the raw typed value, and the Device Manager applies the
constant. The authoritative model/firmware/configuration catalog is likewise the Device
Manager's; the crate consumes a resolved profile as an input and ships no catalog of its
own.

**Session mechanics in the crate, commissioning policy in the config app (§1.1 case
4).** The crate parses and packs bidirectional/session frames and runs the link state
machine; deciding *when* to commission a meter is the configuration app's, and the
confirming round-trip runs config app → backend → gateway → crate → device. The crate
provides the wire capability; it does not own the commissioning workflow.

**A different protocol, not a quirk.** `docs/MULTICAL_VENDOR_EVENTS.md` also documents
Kamstrup's KMP logger commands (`A0`–`A3`, `9B`, `9C`), the 50-entry INFO history, and
the event/error-hour counters. These are the **optical/wired KMP protocol**, and the
evidence document states there is no proof the periodic wM-Bus telegram carries any of
them. They are not vendor extensions to wM-Bus; they are a separate protocol that would
warrant its own adapter, and are out of scope for the vendor layers entirely. Only the
current INFO *bitmask*, if it appears in the wireless record stream, is in scope here
(D2).
