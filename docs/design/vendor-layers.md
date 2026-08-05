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
difference. (Today `02 FF 20` decodes to `Manufacturer specific: 0.0` — exactly this
failure.)

**P4 — Quirks are evidence-gated, narrowly scoped, and default-off.**
Every quirk carries: the deviation (what the standard says versus what the device
does), the evidence, and the narrowest scope that fits — manufacturer *and* version or
device type where known, not manufacturer-wide by reflex. Standard behaviour is always
the default; the quirk is the exception that justifies itself.

**P5 — A quirk that fires must say so.**
If a quirk changed the outcome — CRC tolerated, field reinterpreted — the decoded
result records it. Otherwise two gateways silently disagree and neither can be audited.

**P6 — Never apply a quirk on the strength of unverified data.**
The manufacturer comes from the frame; a corrupt frame yields a corrupt manufacturer.
Vendor dispatch requires that the frame passed its integrity checks. This is not
hypothetical: applying one meter's cached record layout to another's values is exactly
how the gateway came to report `1591213 W` from a water meter.

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
    /// Whether the frame passed its integrity checks. Quirks do not dispatch
    /// when false (P6).
    pub integrity: Integrity,
}

pub struct DeviceIdentity {
    pub manufacturer: ManufacturerCode, // e.g. "KAM"
    pub version: u8,
    pub device_type: u8,
    pub address: u32,
}
```

An empty `VendorBinding` is the normal case and costs one branch.

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

## 5. Current classification

### KAM — Kamstrup Multical 21

Evidence: live captures from the production gateway, decrypted and cross-checked
against an independent implementation.

| Behaviour | Layer | Notes |
|---|---|---|
| ELL CI `0x8D`, AES-CTR | 0 | Frame-declared (P1). Already generic. |
| Compact frames, format signature | 0 | OMS mechanism; signature confirmed on KAM traffic. A vendor differing would miss the lookup — safe failure. Assumption labelled in `wmbus::compact_frame`. |
| Type B framing, BCD address | 0 | Standard. |
| `02 FF 20` info codes | **1** | VIF `0xFF` is the standard's manufacturer slot. Currently decodes to `Manufacturer specific: 0.0` — a P3 violation. |
| Leading 2-byte ELL field is not a payload CRC | **0 for now** | See decision D1. |

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

### Others observed (KAW, RVI, YKM, KEM, ZZI, UNK …)

Header-only sightings, largely from CRC-failed frames. **No classification**: P4
requires evidence and P6 forbids attributing anything from a frame that failed
integrity. Their presence in the manufacturer database is Layer 0 metadata, not an
extension.

## 6. Decisions

**D1 — The KAM ELL leading field stays Layer 0 for now.**
Our captures prove Kamstrup does not populate it as a payload CRC (identical values
across differing payloads; no CRC-16 variant reproduces it). But this may be an
ambiguity in the standard rather than a deviation, and P4 requires a conforming
interpretation for the device to contradict. Resting state: generic, tolerant, not used
for authentication, documented in `wmbus::ell`. **Promotion to a quirk is blocked on
one ELL-II sample from a non-Kamstrup device.** If that sample carries a valid CRC over
its payload, the generic path starts validating and KAM gains
`kam-ell-leading-field-not-crc`.

**D2 — KAM info codes ship as a provisional Layer 1 extension.**
Implement now, decoding the INFO bitmask, with
`Evidence::Documented { source: "MULTICAL 602 register map" }` and
`Status::Provisional`, surfaced as provisional in the output. Rationale: it is strictly
better than the current `0.0` (which violates P3), it cites a source rather than
inventing meaning, and the manifest makes the uncertainty legible. Our meter is a
Multical 21 while the documented bitmask is from a 602, so the semantics are upgraded
to `Verified` only when decoded from one of our own captures.

## 7. Migration

Each step is independently shippable and behaviour-preserving unless stated.

1. **Introduce `DecodeContext` and collapse to one path** (P7, P9). Registry empty;
   no behaviour change.
2. **Thread the context from metermon-rs.** QUNDIS becomes reachable for the first
   time. Remove the hardcoded `mfr_id == "QDS"` branch from `payload::record`.
3. **Split the traits**; port the QUNDIS date handling to `VendorQuirks` with a
   manifest and its existing capture-backed test. *Behaviour change:* QDS dates decode
   correctly in the live path, and the quirk is reported.
4. **Add the KAM extension** for info codes per D2.
5. **Surface `applied_quirks`** in metermon-rs JSON and the device store (P5).
6. **Add the conformance harness** (§4.6) to CI.
7. **Retire the `*_with_vendor` duplicates.** Keep `has_quirks` as the database's
   declaration of intent, but derive registration from the registry's own entries so
   the flag and the registered set cannot drift apart.

## 8. Out of scope

The generic core is not up for renegotiation here: ELL, compact-frame expansion, the
VIF exponent table and record value decoding stay Layer 0 and are unaffected. LoRaWAN
device credentials (e.g. the Zenner LoRa HCA) belong to the radio stack, not to vendor
layering. This document does not add a second registry for wired M-Bus; the same
binding applies to both transports, since the manufacturer is a property of the device,
not the link.
