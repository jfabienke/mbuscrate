# `op:observed` — geo-tagged device up-sync

## Purpose

The gateway already syncs devices **down** from the Device Manager over the MQTT
control topic — `op:profile_request`/`op:profile` (device profiles) and `op:key`
(AES keys), each persisted to redb *before* use and loaded at startup *before*
broker contact, so it survives a backhaul outage.

`op:observed` adds the missing **up** direction: the gateway reports the fleet it
actually hears — presence, signal, counts, and (with GPS) position — so the
backend gains a live, spatial view it cannot get any other way. It is the
network-effect payoff of GPS-on-every-gateway: the same meter heard by several
positioned gateways can be located passively.

## Ownership model (the crux)

Sync only works if each field has one authority. `op:observed` carries **only
gateway-owned observations** and never identity or secrets.

| Field | Authority | Direction | In `op:observed`? |
|---|---|---|---|
| identity, profile, model, firmware | backend Device Manager | down | no |
| AES keys | backend key broker | down | **never** |
| presence, RSSI, first/last seen, frame & CRC counts, mode | **gateway** (the sensor) | **up** | yes |
| gateway's own position | **gateway** (GPS) | up | yes (`gw_pos`) |
| meter position | **contested** (app install-tag vs gateway estimate) | up, advisory | yes (`pos_estimate`, optional) |

Rules the backend applies:
- Gateway-owned fields: last-writer-wins **per (serial, gw)**. Multiple gateways
  hearing one meter are kept as distinct observations, not overwrites.
- `pos_estimate`: **advisory only.** An app install-tag always wins. The estimate
  fills a gap when no install-tag exists, and a large estimate↔install-tag
  disagreement is surfaced for review (possible mislabel or a moved meter). The
  gateway never asserts a meter's canonical location.

## Wire schema

Published by the gateway on its **data topic** (like `op:startup`), consumed by
the Device Manager. Fire-and-forget at QoS AtLeastOnce; an optional
`op:observed_ack` lets the gateway advance its sync watermark deterministically.

```json
{
  "op": "observed",
  "ts": 1723200000,
  "gw": "6543",
  "seq": 42,
  "gw_pos": { "lat": 56.1629, "lon": 10.2039, "hdop": 0.8, "fix_ts": 1723199990 },
  "meters": [
    {
      "serial": 85312884,
      "mfr": "KAM",
      "device_type": 4,
      "mode": "C",
      "encrypted": true,
      "has_key": true,
      "silent": false,
      "first_seen": 1723100000,
      "last_seen": 1723199950,
      "frames_ok": 1234,
      "frames_total": 1300,
      "last_rssi": -63,
      "pos_estimate": { "lat": 56.163, "lon": 10.204, "method": "rssi-multilateration", "confidence_m": 45 }
    }
  ]
}
```

- `seq` — monotonic per-gateway counter for idempotency: QoS AtLeastOnce may
  redeliver, so the backend dedups on `(gw, seq)`. Observations are last-writer
  snapshots, so re-applying one is harmless anyway.
- `gw_pos` — the gateway's current GPS fix (from the `gps` module). Omitted when
  no fix; `fix_ts` lets the backend judge staleness.
- `pos_estimate` — present only when RSSI multilateration has produced one.
- **No `key`, no `model`/`firmware`** — those are backend-owned; leaking a key
  upstream is a defect, not a feature.

`op:observed_ack` (backend → gateway control topic, optional):
```json
{ "op": "observed_ack", "gw": "6543", "seq": 42 }
```

## Gateway side: outbox & offline-first

Mirror the down-sync ethos — durable, survives outages, drains on reconnect.

1. **Watermark, not per-record flags.** Keep `observed_hwm` (a `last_seen` value)
   in the redb `META` table. A sync selects device records with
   `last_seen > observed_hwm`, plus any whose `silent` flag flipped since last
   sync (a silence transition is itself worth reporting).
2. **Build & publish.** `build_observed(gw_id, gw_pos, records, hwm) -> (msg, new_hwm)`
   is a pure function (testable without a radio). Publish on the data topic.
3. **Advance on confirmation.** On `op:observed_ack` for `seq` (or, without acks,
   on publish success at QoS AtLeastOnce), persist `observed_hwm = new_hwm` and
   bump `seq`. If the broker is down, nothing advances — the next sync re-selects
   the same records and drains the backlog. Same "persist before trust" rule the
   key/profile path already follows.
4. **Cadence.** Incremental sync every ~60 s when there is anything above the
   watermark; a full snapshot (all confirmed records, ignoring the watermark)
   every ~1 h as a reconciliation baseline and to catch long-silent meters.
   Chunk `meters` if it would exceed a size bound.

The sync runs **inside the monitor process** (the single redb writer), so it
reads dirty records and advances the watermark on the same handle — no
cross-process lock contention.

## Guardrails

- **Ghost containment.** Only records that already passed the ingestion
  ghost-guard *and* have ≥ N CRC-valid frames are eligible for up-sync, so a
  one-off CRC fluke never becomes a synced asset. Reuse the existing containment;
  up-sync just filters on it.
- **No identity claims.** The gateway reports what it heard, not what a meter
  *is*. The backend reconciles observations against its own identity records.
- **No secrets.** Schema has no key field; the mock logs op counts and serials,
  never key material (existing discipline).

## Backend / mock contract

`mock_backend` is the offline-testable stand-in:
- `responses_for(op:observed)` → `[op:observed_ack {gw, seq}]` (or `[]` if acks
  are disabled).
- `ObservedInventory::apply(msg)` accumulates `(gw, serial) -> Observation`,
  applying the ownership rules above, so a test can assert that after the gateway
  publishes `op:observed`, the mock holds the expected meter at the expected
  `gw_pos`/RSSI — and that a `pos_estimate` never overrides a pre-set install-tag.

## Multi-gateway localization (backend, enabled by the contract)

Because observations are keyed `(serial, gw)` and each carries `last_rssi` +
`gw_pos`, the backend can run RSSI multilateration across all gateways that heard
a serial — a passive position estimate better than any single gateway's, with no
mobile survey. The gateway's only job is accurate `(rssi, gw_pos)`; the solve is
centralized. Accuracy is coarse (path loss varies; building/block level, tens of
metres), suitable for asset maps and "which area," not survey-grade.

## Status

Implemented end-to-end (offline-tested; live run pending):
- Backend/mock: `responses_for(op:observed)` + `ObservedInventory` (8 tests).
- Gateway: `gps` module (gpsd client, 3 tests), `upsync::build_observed` (5 tests),
  `DeviceStore::{observed_since, observed_watermark, observed_seq, advance_observed}`,
  and the periodic sync wired into the monitor loop (60 s cadence, advance on
  publish success, GPS via the optional `gps` config field).
- Radio path cross-checked for aarch64-unknown-linux-musl.

Not yet done: `op:observed_ack`-driven watermark advance (currently advances on
publish success), per-meter `pos_estimate` (backend multilateration), and a live
run on the Pi.
