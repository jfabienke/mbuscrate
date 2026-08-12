# Brunata wM-Bus — research brief (scoping)

Status: **scoping only, capture-gated.** No Brunata telegrams decoded yet. The purpose
here is to decide *how much* work Brunata is before committing decoder effort — the same
first pass that de-risked Techem. The answer is materially different from Techem, in ways
that matter both ways.

## TL;DR

- **Brunata's FLAG is `BHG` (0x0907)** — already in our registry (currently mislabeled
  "Brunata Hürth"; wmbusmeters and the FLAG registry say **"Brunata, Denmark"**). Correct
  the label; no new code registration needed for identity.
- **There is NO `wmbusmeters` clean-room oracle for Brunata.** Unlike Techem, we cannot
  lift golden telegrams and field layouts from a maintained driver. Brunata decode work is
  **capture-gated** — it needs a real Brunata frame from the field.
- **The look-alike trap:** `BMT` ("BMETERS, Italy") is a *different company*. Its products
  (`HydroCal M3/M4`, `HydroDigit`, `RFM-TX1`, `IWM-TX5`, `RFM-AMB`) are well covered by
  `wmbusmeters` and are mostly standard OMS — but **none of that is Brunata.** Do not wire
  BMT drivers expecting Brunata coverage. (`BRU` = "Klaus Bruchmann" is a third trap.)
- **The real leverage is Zenner, not wmbusmeters.** Brunata's wM-Bus network modules are
  branded **ZENNER EDC B.One** and **ZENNER PDC B.One** — the exact Zenner platforms we
  have already reverse-engineered (`edcl` and `pdcl2` firmware atlases; CI 0x51 optical
  command set). Brunata is likely the vendor where our existing Zenner moat transfers most
  directly.

## Identity

| FLAG | id | who | in our registry? |
|---|---|---|---|
| **BHG** | 0x0907 | **Brunata, Denmark** ← our target | ✅ (label wrong: "Brunata Hürth") |
| BMT | 0x09B4 | BMETERS, Italy (look-alike; HydroCal/HydroDigit) | ❌ (unrelated) |
| BMP | — | BMETERS Polska | ❌ (unrelated) |
| BRU | — | Klaus Bruchmann, Germany (trap) | — |
| ZRI / ZRM / ZEN | 0x68AE etc. | ZENNER International | ✅ (Brunata's module vendor) |

First code step is a one-line correction: `BHG` → `"Brunata (Denmark)"`, and a doc-comment
warning it apart from `BMT`/`BRU`. Identity is otherwise done.

## The Zenner-module connection (the whole thesis)

Brunata (part of the Minol-ZENNER group since 2018) ships its wM-Bus telemetry on
**Zenner EDC / PDC "B.One" communication modules** (brunata.com/products/network/…). We
already own deep RE of those platforms:

- **EDC → `edcl`** and **PDC → `pdcl2`** firmware atlases are already built and embedded
  (`core/assets/cached_atlases/`), resolving parameter name → address per firmware version.
- **CI 0x51** Zenner optical/manufacturer command set (GetVersion / ReadMemory / GetMBusKey
  …) is implemented and hardware-verified (`protocols/zenner.rs`).

Implication: for Brunata, the **commissioning / key-custody path may already work** via the
Zenner EDC/PDC optical protocol with little new code — which is the defensible half of the
operator model and the point of the Copenhagen play. The open question is the *radio* format
(below).

## Open questions (all capture-gated)

1. **Radio wire format** — do BHG devices emit **standard OMS** (DIF/VIF, mode-5/7 AES —
   generic path already handles) or a **Zenner-proprietary** wM-Bus payload (needs a
   positional decoder, but one we can likely model on the Zenner atlas)? This single answer
   sets the effort.
2. **Module + firmware** — EDC vs PDC, and which firmware version → which atlas (`edcl` vs
   `pdcl2`). Determines whether address resolution is already covered.
3. **Device families + `(version, device_type)`** — Brunata's line is HCAs (Futura et al.),
   heat/cooling meters, and water. The submetering value is the **HCAs**; confirm their
   device-type bytes and whether they differ from the meter families.
4. **Key custody** — are keys readable via the Zenner `GetMBusKey` (0x18) path on the
   EDC/PDC module, as on Zenner's own meters? If so, the key-seizure moat transfers directly.

## Effort estimate

**Cannot be finalized without a capture** — and that's the honest headline, unlike Techem
(where the wmbusmeters corpus let us scope precisely). Two branches:

- **If radio is standard OMS** → *small*: register nothing new (BHG exists), add an
  `enrich_device_header` naming hook for the BHG `(version, device_type)` variants, validate
  a golden through the generic path. An afternoon.
- **If radio is Zenner-proprietary** → *moderate but leveraged*: a positional decoder built
  on the existing `edcl`/`pdcl2` atlas + Zenner frame builders, not a from-scratch RE. Days,
  not weeks, because the platform is already ours.

Either way the **commissioning/optical path is expected to reuse Zenner directly.**

## Recommendation

1. **Correct the `BHG` label** now (trivial, factual) and note the BMT/BRU traps in-code.
2. **Obtain a Brunata capture** from the Copenhagen fleet (a single BHG telegram, ideally
   with the device's module type + firmware) — this is the gate. Everything past identity
   needs it.
3. **Probe the optical path against a Brunata EDC/PDC module** with our existing Zenner
   CI 0x51 tooling — this may show the key-custody moat already transfers, independent of the
   radio-format answer.

Do **not** start a positional radio decoder blind: without a capture we'd be guessing, and
the `BMT` corpus is the wrong company. Brunata is a *Zenner-leverage* play, not a
*wmbusmeters clean-room* play.

## Sources

- FLAG table (`wmbusmeters` `src/manufacturers.h`): `BHG` = "Brunata, Denmark"; `BMT` =
  "BMETERS, Italy"; `BRU` = "Klaus Bruchmann"; `ZRI`/`ZRM` = "ZENNER International".
- Brunata network modules branded ZENNER EDC/PDC B.One — <https://brunata.com/wireless-m-bus/>
  and the EDC/PDC product pages.
- Our Zenner RE: `core/docs/ZENNER_OPTICAL_PROTOCOL.md`, `protocols/zenner.rs`, atlases
  `edcl`/`pdcl2` in `core/assets/cached_atlases/`.
- Our registry: `src/vendors/manufacturer.rs` (BHG 0x0907; ZRI/ZRM/ZEN).
