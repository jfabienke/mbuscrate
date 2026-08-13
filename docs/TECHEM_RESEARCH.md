# Techem wireless M-Bus — research brief (pre-design)

Research foundation for adding Techem wM-Bus telegram support to `mbus-rs`.
Compiled 2026-08-10 from the public `wmbusmeters` driver definitions (GPL-3.0 —
see **Licensing**) plus a codebase integration survey. No Techem device is in our
own fleet, so nothing here is yet validated against our captures.

## Thesis (the design premise)

**OMS-first, proprietary-fallback.** Techem's telegrams split into two eras:

- **Legacy generation** — a *positional, non-self-describing* payload under a
  manufacturer-specific CI (`0xA0`/`0xA1`/`0xA2`). Requires a Techem decoder.
- **Newer generation** — *standard DIF/VIF (OMS)* records, often ELL-wrapped
  (`CI 0x8C`) and AES-encrypted. **Decodes through `mbus-rs`'s existing stack**
  once decrypted; needs no Techem-specific code.

The drift to OMS is comprehensive — every device category (HCA, water, heat) has
both a legacy positional cell and a newer OMS cell. Combined with the fleet-churn
cutoff (battery ~10 y; MID recalibration ~5–6 y for water/heat), the
**currently-installed population is overwhelmingly the newer OMS generation**, so
the OMS-first path carries most of the in-scope fleet and the positional decoder
is a *shrinking legacy tail*.

## Manufacturer identity

- FLAG ID **`TCH` = `0x5068`** (wire M-field little-endian `68 50`).
- Already present: `mbus-rs/src/vendors/manufacturer.rs:98`
  (`0x5068 → "TCH", "Techem GmbH", has_quirks=false`). Flip `has_quirks` when
  registering a `VendorExtension`.
- Variant selector on the wire is the tuple **`(manufacturer, version, device_type)`**
  = `(WMBusLinkFrame.manufacturer_id → "TCH", .version, .device_type)`.

## Variant matrix

`mvt` = `(manufacturer, version-byte, device-type-byte)`, hex. "OMS" = standard
DIF/VIF (existing path); "positional" = needs Techem decoder.

| Category | Driver | mvt selector(s) | CI | Enc | Format |
|---|---|---|---|---|---|
| **HCA** | `fhkvdataiii` | `TCH,69,80` · `TCH,94,80` | `0xA0` | no | **positional** |
| **HCA** | `fhkvdataiv` | `TCH,69,08` · `TCH,94,08` | TPL | AES | **OMS** (+ compact-profile history) |
| **Water** | `mkradio4` (warm/cold) | `TCH,{70,95},{62,72}` | `0xA2` | mixed | **positional** (legacy) |
| **Water** | `mkradio3a` | `TCH,50,72` | TPL | — | **OMS** (total + reading history) |
| **Water** | `mkradio4a` | `TCH,95,37` · **`HYD,fe,06`** | TPL | AES | **OMS** (cross-vendor, see below) |
| **Heat** | `compact5` | `TCH,{45,22,39},{04,43,C3}` | `0xA1`/`0xA2` | mixed | **positional** |
| **Heat** | `vario451` | `TCH,27,{04,C3}` | `0xA2` | mixed | **positional** |
| **Heat** | `vario411` | `TCH,28,04` | ELL `0x8C` | AES | **OMS** |
| **Heat** | `vario451mid` | `TCH,17,04` | ELL `0x8C` | AES | **OMS** (MID) |

Notes:
- **Version byte discriminates legacy vs OMS** for water/heat (e.g. heat OMS ver
  `17`/`28` vs positional `22`/`27`/`39`/`45`).
- **HCA is the dual-telegram case:** the *same* versions (`69`/`94`) emit **both**
  an unencrypted positional telegram (type `0x80`, CI `0xA0`) *and* an encrypted
  OMS telegram (type `0x08`). This is the source of "two telegrams from one meter".
- **Un-decoded newest cell:** a Techem "radio 4" HCA at **`TCH,6a,08`** is not yet
  handled even by `wmbusmeters` — the live edge of the matrix.
- **Cross-vendor:** `mkradio4a` also detects **`HYD` (Diehl/Hydrometer)** and is
  labelled a Diehl meter. Newer Techem water is (partly) Diehl-made and OMS-shaped
  — so the OMS-first path yields **Diehl water for free**. OMS support is a general
  win, not Techem-only.

## Positional layouts (the legacy decoder's job)

All small, regular: header, previous-period value, current-period value, plus (HCA)
two temperatures. Values are tagged with synthetic DIF/VIF keys so downstream code
sees normal records.

**`fhkvdataiii` (HCA, CI `0xA0`)** — payload begins with a 1-byte version tag, then:

```
tag '01'/'11':  PrevDate(u16) PrevHca(u16) CurrDate(u16) CurrHca(u16) TempRoom(u16) TempRad(u16)
tag '0F':       PrevDate(u16) PrevHca(u16) CurrDate(u16) CurrHca(u16) Extra(u8) TempRoom(u16) TempRad(u16)
```
Synthetic keys: PrevDate `02FD3A`, PrevHca `426E`, CurrDate `42FD3A`, CurrHca
`026E`, TempRoom `0265`, TempRad `025D`.
Techem **bit-packed date** (u16 `raw`):
- previous: `day = raw & 0x1F`, `month = (raw>>5) & 0x0F`, `year = raw>>9`
- current: `day = (raw>>4) & 0x1F`, `month = (raw>>9) & 0x0F`, year = previous_year
  + rollover (if current month/day < previous, year+1). Base epoch 2000-01-01.

**`mkradio4` (water, CI `0xA2`)**:
```
byte byte byte  Prev(u16)@4215  byte byte  Curr(u16)@0215
```
`total_m3 = prev + curr` (VIF `15` = volume, 0.01 m³... scaling per VIF). Example
below decodes to total 0.4 m³.

**`compact5` (heat, CI `0xA1`/`0xA2`)**:
```
hdr(3B)  Prev(3B)@037E  one_byte  Curr(3B)@037F
```
`total_kwh = prev + curr` (unit kWh). VIF `037E`/`037F` = previous/current billing.

**`vario451` (heat, CI `0xA2`)**:
```
hdr(3B)  Prev(2B)@027E  two_bytes  Curr(2B)@027F
```
Raw units **mGJ (1/1000 GJ)**; `total_kwh = (prev + curr) / 3.6`.

## Newer OMS cells (existing path)

These need **no Techem decoder** — they are standard DIF/VIF and route through the
current `mbus-rs` decode after TPL/ELL + AES:

- `fhkvdataiv` (HCA): `current_consumption` (HCA, Instantaneous, signed),
  `set_date` (Date, storage 1), `consumption_at_set_date` (HCA, storage 1), plus a
  **compact-profile history** (storage 8, then 9…22 as monthly slots).
- `mkradio3a` (water): `total` `03FD3A`, `curr_date` `02FD3A`, and a run of history
  readings `82xxFD3A` (storage-indexed).
- `mkradio4a` (water): `target` Volume + `target` Date (storage 1). AES.
- `vario411` / `vario451mid` (heat): total/target Energy (`AnyEnergyVIF`) + Date
  (storage 1/8), ELL-wrapped (`CI 0x8C`). AES.

### Almanac (storage-indexed billing history) — decoded (2026-08)

The storage-indexed history above collapsed onto storage 0 until the generic record
walker learned to **accumulate the DIF/DIFE storage number** (EN 13757-3 §6.3.2). The
`record.rs` variable-data parser had captured DIFE bytes into `dife[]` but never folded
them into `storage_number` (it stayed `0`), so `82 xx` / `83 xx` history cells were
indistinguishable from the current reading. Fixed in
`payload::record::accumulate_dib_fields` — DIF bit 6 is the storage LSB, each DIFE adds
4 more storage bits (`0x0F`), 2 tariff bits (`0x30`), 1 subunit bit (`0x40`). This is a
**general** fix (every OMS meter with a set-date/history run benefits, not just Techem).

On top of it, `vendors::techem::extract_oms_history` pairs each storage≥1 *value* record
with the Type-G *date* record at the same storage and returns dated `AlmanacEntry`
periods. The history dates are standard EN 13757-3 **Type G (CP16)** (VIF `0x6C`), *not*
Techem's bit-packed positional date — the generic path leaves `0x6C` as a raw u16, so the
Type-G rendering is done in the vendor module.

Verified end-to-end against the `fhkvdataiv` golden (id 14542076, key in the vector table
below): decrypt → walk → `current 2 @storage 0`, `set-date 25 @storage 1 (2020-12-31)`,
`prior 0 @storage 8 (2019-10-31)`. Tests: `payload::record` DIFE unit tests,
`vendors::techem` almanac unit tests, `tests/techem_almanac_golden.rs`.

**Remaining (capture-gated):** the `fhkvdataiv` **compact-profile LVAR block**
(`8D 04 EE1F …`, ~30 bytes of packed monthly deltas) and the KuguHome bi-weekly
`almanac` run are *not* unpacked — the `EE1F` VIFE semantics and the delta packing need a
second oracle or an own capture, and `wmbusmeters`' own compact-profile machinery is GPL
(do not port). `extract_oms_history` skips the LVAR block (non-numeric) rather than
guessing. `mkradio3a`'s `82xxFD3A` run decodes by the same storage-number mechanism but is
not yet golden-validated (no `mkradio3a` reference telegram in hand).

## Golden telegrams (reference — from `wmbusmeters` GPL test suite)

Documented here as wire-format reference; **validate against our own captures
before trusting**, and do not treat as our own test corpus (see Licensing).

- `fhkvdataiv` HCA, key `FCF41938F63432975B52505F547FCEDF`:
  `4E4468507620541494087AAD004005089D86B62A329B3439873999738F82461ABDE3C7AC78692B363F3B41EB68607F9C9160F550769B065B6EA00A2E44346E29FF5DC5CB86283C69324AD33D137F6F`
  → id `14542076`, current 2 HCA, set_date 2020-12-31, at-set-date 25.
- `mkradio4` water (NOKEY):
  `2F446850200141029562A2_06702901006017030004000300…`
  → total 0.4 m³, target 0.1.
- `compact5` heat (NOKEY):
  `36446850626262624543A1_009F2777010060780000000A00…`
  → total 495 kWh (current 120, previous 375).
- `vario451` heat (NOKEY):
  `374468506549235827C3A2_129F25383300A862260000820080…`
  → total 6371.67 kWh.
- `vario451mid` heat (NOKEY), ELL:
  `734468501204439417048c0084900f00…8404062846000082046c9f2c…`
  → total 18377 kWh, set 17960, set_date 2020-12-31.
- `mkradio4a` water (NOKEY): warm `4B44685036494600953772…` → target 16.1 m³
  2021-12-31; cold `4B4468508644710095377202…` → target 75.2 m³.

## Encryption & key custody

- AES on the newer cells (`fhkvdataiv`, `mkradio4a`, `vario411`, `vario451mid`) —
  OMS mode-5 (CBC) / mode-7; reuse the existing `oms::decrypt_mode5_cbc` +
  `VendorExtension::provision_key` path.
- Legacy cells are frequently **unencrypted** (`NOKEY`).
- **Wrinkles:** some water meters emit an encrypted *and* an unencrypted telegram
  with the same id; and there is a report of **two AES keys for one Techem Radio 4**
  device. The key store / dedup must tolerate both.

## Integration map (`mbus-rs`)

Home: **`mbus-rs`** — the decode stack (link layer, TPL/CI dispatch, DIF/VIF
walker, `VendorRegistry`) lives here. `meter-config-vendors` is a downstream
registration crate only. Template: **`src/vendors/zenner.rs`** (device-type-gated
variant selection) in-crate; `meter-config-vendors/src/qundis.rs` for the external
`VendorExtension` registration shape.

Key seams:
1. **Link frame → selector.** `decode_mode_c` (`src/wmbus/mode_c.rs:134`) →
   `WMBusLinkFrame { manufacturer_id, version, device_type, address, link_header,
   payload }`. `payload` **starts at the CI byte**. Becomes `DeviceIdentity`
   (`src/vendors/context.rs:20`).
2. **CI/transport dispatch (OMS path).** `metermon-rs/src/decode.rs`
   `decode_frame_with_cache` matches CI (line 131): `0x7A`/`0x72` → `decode_tpl`
   (mode-5 CBC), `0x8C..0x8F` → `ell::parse_ell`, then `insert_records` (line 391)
   → **`parse_variable_record_in_context`** (`src/payload/record.rs:644`) — the
   standard DIF/VIF walker. **The newer Techem cells already decode here.**
3. **Vendor dispatch.** `VendorRegistry` = `HashMap<3-letter-code, VendorExtension>`;
   `DecodeContext::resolve` (`context.rs:104`) selects by manufacturer, gated on
   `crc_ok`. Hooks incl. **`handle_ci_manufacturer_range` (CI `0xA0–0xB7`)** and
   `parse_vif_manufacturer_specific` / `handle_dif_manufacturer_block`; fired in
   `apply_vendor_hooks` (`record.rs:678`).
4. **Registration.** `VendorRegistry::with_defaults` (`src/vendors/mod.rs:360`,
   near the `KAM` placeholder) or downstream `register_all_vendors`
   (`meter-config-vendors/src/lib.rs:14`).

**The one real seam question:** the vendor hooks fire *inside* the DIF/VIF walk,
but a legacy positional Techem payload (CI `0xA0–0xA2`) is *not* self-describing and
must be intercepted at the **transport dispatch** (`decode.rs match ci`) *before*
the walk. Design decision: route CI `0xA0–0xA2` to `handle_ci_manufacturer_range`
from `decode.rs`, or add an explicit Techem branch there.

## Open design questions

1. **Interception seam** for the positional CIs (above).
2. **Dual-telegram policy** — prefer OMS when the key is present, else positional;
   dedup on meter id.
3. **Key custody** — reuse `provision_key`; handle two-keys-per-device.
4. **Date/scale helpers** — Techem bit-packed date; mGJ→kWh (÷3.6); VIF `15`/`037E`
   scaling.
5. **Scope, capture-driven** — implement cells for which we have golden telegrams;
   prioritise the newer OMS cells (largest in-scope share, least new code); add
   positional cells only as captures/fleet demand shows them.

## Prior art & cross-reference (KuguHome openHAB binding)

[KuguHome/openhab-binding-wmbus](https://github.com/KuguHome/openhab-binding-wmbus)
(EPL-2.0, Java, ~2021) is the most Techem-specific open-source decoder besides
`wmbusmeters`/FHEM. Its `TechemBindingConstants` holds an independent variant table
keyed on **(version, reportedType, coding, deviceType)** — where `coding` is the
manufacturer **CI byte** (0xA0 / 0xA1 / 0xA2), a generational discriminator we do not
key on. Mining it (2026-08) against our decoder and against `wmbusmeters` (the tested
oracle) produced these outcomes:

| KuguHome variant | corroborated by `wmbusmeters`? | our action |
|---|---|---|
| water **0x74** /62,/72 (CI A2) | ✅ `mkradio3` | **added** `Variant::MkRadio3` (+ dates), golden-tested |
| **smoke detector 0x76 / 0xF0** (CI A0/A1) | ✅ `tsd2` | **added** `Variant::SmokeDetector` (status + date), golden-tested |
| HCA **0x45 / 0x43** labelled `HKV45` | ❌ — `wmbusmeters` `compact5` detects `TCH,45,43` as a **HeatMeter** | **no change** — our Compact5 (heat) is correct; KuguHome's own code flags the doubt (`// TODO Isn't this a heat meter?`) |
| HCA **0x61** (reserved type), **0x64** (0x80) | ❌ not in `wmbusmeters` | **not added** — KuguHome-only, uncorroborated |
| heat **0x71 / 0x43**, **0x57 / 0x44** | ❌ not in `wmbusmeters` | **not added** — KuguHome-only, uncorroborated |

Key lessons:

- **`wmbusmeters` is the authority; KuguHome is corroboration only.** KuguHome mis-labels
  0x45/0x43 (an HCA vs a heat meter) — so its labels are not trusted without a second
  source. Anything KuguHome-only is recorded here as *unverified* and left out of the
  code until a capture or a second oracle confirms it.
- **The CI (`coding`) dimension is real.** The same (version, type) appears under
  different CIs (e.g. `tsd2` under A0 *and* A1; KuguHome's WMZ 113/43 under A0 *and*
  A2), and the framings differ. Our `handle_ci_manufacturer_range` receives the CI but
  dispatches on (version, type) + a payload tag byte; the positional decoders must stay
  robust when a known (version,type) arrives under an unexpected CI.
- **`almanac` = bi-weekly (14-day) consumption history.** KuguHome extracts a periodic
  history run. We now decode the **storage-indexed** history (see "Almanac" above:
  `record.rs` DIFE storage-number accumulation + `techem::extract_oms_history`), which
  covers the discrete per-period value+date cells. The tighter **bi-weekly compact-profile
  packing** inside the `fhkvdataiv` `8D 04 EE1F` LVAR block is still not unpacked — that
  remains the capture-gated piece.
- KuguHome is **positional-only** and predates the modern OMS cells, so it has none of
  our FHKV data IV / radio-4 / mkradio-3a/4a / vario-411/451-MID coverage. The two
  projects are complementary.

## Licensing

`wmbusmeters` is **GPL-3.0**; KuguHome's binding is **EPL-2.0**. The *format facts*
used here — FLAG ID, CI codes,
`(version,type)` selectors, field offsets, DIF/VIF keys, date-packing math, unit
scaling — are not copyrightable and are free to use. **Do not** port the `.xmq`
drivers or their C++, nor KuguHome's Java, into this (differently-licensed) repo;
implement clean-room from the facts. Treat the test-telegram corpus as external reference, cite it, and
build our own regression fixtures from real captures where we can.

## Sources

- `wmbusmeters` drivers `drivers/src/{fhkvdataiii,fhkvdataiv,mkradio3,mkradio3a,mkradio4,mkradio4a,compact5,vario411,vario451,vario451mid}.xmq`
  — <https://github.com/wmbusmeters/wmbusmeters>
- Issues: [#1621 (radio-4 HCA ver 0x6a undecoded)](https://github.com/wmbusmeters/wmbusmeters/issues/1621),
  [#1405 (mkradio4 dual telegrams)](https://github.com/wmbusmeters/wmbusmeters/issues/1405),
  [#244 (Radio4 water)](https://github.com/wmbusmeters/wmbusmeters/issues/244),
  [discussion #1312 (two AES keys)](https://github.com/orgs/wmbusmeters/discussions/1312).
- Internal integration survey: `mbus-rs` (`src/wmbus/mode_c.rs`, `metermon-rs/src/decode.rs`,
  `src/payload/record.rs`, `src/vendors/*`), `meter-config-vendors/src/*`.
