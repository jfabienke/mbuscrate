# Techem wM-Bus support — design

Design for decoding Techem telegrams in `mbus-rs`. Companion to
`TECHEM_RESEARCH.md` (the variant matrix, field maps, and integration survey this
design builds on). Not yet implemented.

## Guiding principle: OMS is generic, manufacturer-specifics are isolated and normalized

The repo already separates two concerns, and this design stays strictly inside
that separation:

- **Generic OMS decode** (the DIF/VIF walker, VIF→quantity mapping, TPL/ELL/AES)
  is vendor-agnostic. Generic code never string-matches a manufacturer.
- **Manufacturer-specifics** live behind `VendorExtension` (additive hooks for the
  standard-reserved manufacturer slots — CI `0xA0–0xB7`, DIF `0x0F/0x1F`, VIF
  `0x7F/0xFF`, status bits, header enrichment, keys) and `VendorQuirks` (scope-
  matched reinterpretation of otherwise-standard records).

Techem maps onto this cleanly because of the research's central finding: **its
newer generation is already OMS** (needs no decode code), and only the **legacy
positional payloads** are manufacturer-specific. The design's load-bearing idea:

> A Techem positional decoder **translates** the proprietary payload into
> **standard `MBusRecord`s** (correct DIF/VIF, computed dates, scaled units), so
> everything downstream — quantity mapping, unit derivation, JSON — stays generic.
> The quirk is contained at the point of entry and normalized away immediately.

This is the same shape as `src/vendors/zenner.rs` (device-type-gated vendor field
interpretation), scaled to whole-payload translation.

## Two decode paths, one extension

```
                         WMBusLinkFrame (mfr=TCH, version, device_type, payload@CI)
                                          │
                        decode_frame_with_cache  (metermon-rs/src/decode.rs)
                                    match ci ┤
   ┌───────────────────────────────────────┴─────────────────────────────────┐
   │ 0x72/0x7A/0x79/0x8C..0x8F  (OMS: TPL/compact/ELL)                         │
   │   → decrypt (mode-5/7) → insert_records → parse_variable_record_in_context│  PATH A (existing)
   │   → generic DIF/VIF records.   Techem newer cells decode here UNCHANGED.  │
   ├───────────────────────────────────────────────────────────────────────── ┤
   │ 0xA0..=0xB7  (manufacturer-specific CI)   ── NEW GENERIC SEAM ──           │
   │   → ctx.extension()?.handle_ci_manufacturer_range(ci, app_data, ctx)      │  PATH B (new)
   │        └─ TechemExtension: variant-select → positional decode             │
   │           → returns Vec<MBusRecord> (standard, normalized)                │
   │   → insert_records (same sink as Path A)                                  │
   └───────────────────────────────────────────────────────────────────────── ┘
```

- **Path A is untouched.** `fhkvdataiv`, `mkradio3a/4a`, `vario411`, `vario451mid`
  decode through the existing stack once decrypted. The extension contributes only
  identity + keys for these (below).
- **Path B is one new *generic* branch** in `decode.rs`: any CI in `0xA0–0xB7` is
  delegated to the resolved vendor extension. No Techem string appears in
  `decode.rs`. This resolves research open-question #1 (the interception seam) in
  the way that honors the isolation principle — the generic layer gains a generic
  capability, and all Techem knowledge stays in `techem.rs`.

> Verify during implementation: whether `handle_ci_manufacturer_range` is already
> invoked anywhere, and its exact signature/return type; adjust the seam call to
> match. Today the CI match handles only `0x78/0x79/0x72/0x7A/0x8C–0x8F`, so the
> `0xA0–0xB7` branch is the concrete new wiring.

## Module: `src/vendors/techem.rs`

```rust
pub struct TechemExtension;

impl VendorExtension for TechemExtension {
    // PATH B — translate a legacy positional payload into standard records.
    fn handle_ci_manufacturer_range(
        &self, ci: u8, app_data: &[u8], ctx: &DecodeContext,
    ) -> Option<Vec<MBusRecord>> {
        let v = TechemVariant::select(ctx.version, ctx.device_type, ci)?;
        v.decode_positional(app_data)          // -> normalized MBusRecords
    }

    // BOTH paths — name the device from (version, device_type).
    fn enrich_device_header(&self, id: &mut DeviceHeader, ctx: &DecodeContext) { … }

    // Techem TPL status bits, if vendor-meaningful.
    fn decode_status_bits(&self, status: u8) -> Option<String> { … }

    // Reuse existing key provisioning; see key custody below.
}
```

### Variant selection (the matrix, in code)

Mirror `zenner::classify`, but keyed on the full tuple:

```rust
enum Category { Hca, Water, Heat }

struct TechemVariant { category: Category, layout: Layout }

impl TechemVariant {
    // Returns Some only for LEGACY positional cells; newer OMS cells return None
    // so Path A handles them.
    fn select(version: u8, device_type: u8, ci: u8) -> Option<TechemVariant> {
        match (version, device_type, ci) {
            (0x69|0x94, 0x80, 0xA0)               => Hca(FhkvIii),
            (0x70|0x95, 0x62|0x72, 0xA2)          => Water(MkRadioLegacy),
            (0x45|0x22|0x39, 0x04|0x43|0xC3, 0xA1|0xA2) => Heat(Compact5),
            (0x27, 0x04|0xC3, 0xA2)               => Heat(Vario451),
            _ => None,                             // incl. all OMS cells
        }
    }
    fn decode_positional(&self, data: &[u8]) -> Option<Vec<MBusRecord>> { … }
}
```

### Positional decoders → normalized records

Each layout emits `MBusRecord`s tagged with the **documented DIF/VIF keys**, so the
generic VIF→quantity path yields the right fields with no Techem awareness:

| Cell | Emits (DIF/VIF → meaning) |
|---|---|
| `FhkvIii` | `026E`→current HCA, `426E`→previous HCA, current/previous **date** (unpacked → standard type-G date `0x6C`), `0265`→room °C, `025D`→radiator °C |
| `MkRadioLegacy` | `0215`→current volume, `4215`→previous volume; total = curr+prev |
| `Compact5` | `037F`→current kWh, `037E`→previous kWh; total = curr+prev |
| `Vario451` | `027F`/`027E` raw **mGJ**; convert → kWh (÷3.6); total = curr+prev |

Helpers (small, unit-tested, no I/O):
- `date_prev(u16)` / `date_curr(u16, prev_year)` — Techem bit-packing
  (prev: `day=raw&0x1F, month=(raw>>5)&0x0F, year=raw>>9`; curr: `day=(raw>>4)&0x1F,
  month=(raw>>9)&0x0F`, year from prev + rollover; epoch 2000).
- `mgj_to_kwh(raw) = raw / 3.6`.

**Normalization is the point:** the decoder unpacks Techem's date and scales its
units *here*, emitting standard records — so downstream never learns Techem exists.

## Path A (newer OMS cells): identity + keys only

No decode code. The extension:
- `enrich_device_header` — maps `(version, device_type)` to a model/media label for
  both legacy and OMS cells (so `fhkvdataiv`, `vario451mid`, etc. are named).
- Key provisioning — reuse `provision_key`; the AES cells decrypt via the existing
  `oms::decrypt_mode5_cbc` path. **Two-keys-per-device wrinkle:** allow the key
  lookup to try candidate keys for a meter id (infra note; small change to the key
  store's `get`).

A golden test per OMS cell asserts it decodes through the unchanged path.

## Dual telegrams — a consumer concern, not the decoder's

A single Techem HCA emits both an unencrypted positional (Path B) and an encrypted
OMS (Path A) telegram. **The decoder stays pure and stateless** — it decodes each
frame faithfully. Preference/dedup ("prefer OMS when both exist for a meter id")
belongs in the **metermon device store**, not `mbus-rs`. Design hook: tag the
decoded output with a provenance marker (`oms` vs `manufacturer_specific`) so the
store can prefer OMS. Store-side dedup is a follow-up, out of scope for the decoder
v1.

## Registration

- Register `"TCH" → TechemExtension` in `VendorRegistry::with_defaults`
  (`src/vendors/mod.rs:360`, beside the `KAM` placeholder) — in-crate, since
  `techem.rs` lives in `mbus-rs`.
- Flip `manufacturer.rs:98` `has_quirks` to `true`.
- Selection is automatic via `DecodeContext::resolve` (keyed on `"TCH"`, gated on
  `crc_ok` — fail-closed P6 preserved).

## Testing

- **Golden-frame tests per legacy cell**, decoded clean-room and asserted against
  the `wmbusmeters` reference telegrams (see research brief; attribute, and prefer
  our own captures when available). Fixtures live in `techem.rs` `#[cfg(test)]`.
- **Pass-through tests** for the OMS cells (decode via the generic path; assert
  fields).
- **Helper unit tests** for date-unpacking and mGJ→kWh against known values.

## Principle check

| Principle | How this design honors it |
|---|---|
| OMS generic, specifics isolated | Path A untouched; all Techem code in `techem.rs` |
| Generic code never names a vendor | `decode.rs` gains a *generic* `0xA0–0xB7 → extension` branch |
| Normalize at the boundary | Positional decoder emits standard records (dates unpacked, units scaled) |
| Fail-closed | Vendor hooks run only on `crc_ok` frames (unchanged) |
| Extension vs Quirks | `VendorExtension` (CI-range translation) used; `VendorQuirks` reserved, none needed yet |

## Phasing

1. **Seam + skeleton** — generic `0xA0–0xB7 → extension` branch in `decode.rs`;
   `TechemExtension` registered; `has_quirks` flipped. No behavior change yet.
2. **Legacy decoders** — `TechemVariant` + the four positional layouts + date/unit
   helpers + golden tests.
3. **Identity + keys** — `enrich_device_header`; key provisioning; OMS pass-through
   tests.
4. **Later (capture-gated)** — store-side dual-telegram dedup; `mkradio3a` reading
   history; the undecoded `TCH,6a,08` HCA cell.

## Open items for review before coding

- Confirm `handle_ci_manufacturer_range`'s real signature/return and whether any
  caller exists (drives the exact `decode.rs` seam edit).
- Decide the provenance-marker mechanism for dual-telegram preference (a field on
  the decoded frame vs a per-record attribute).
- Confirm `MBusRecord` construction API for synthetic records (build from DIF/VIF +
  value) used by the positional decoders.
