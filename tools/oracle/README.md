# Differential decode oracle (metermon-rs vs. wmbusmeters)

An **optional, external** cross-check: run our decoder and the upstream
[`wmbusmeters`](https://github.com/wmbusmeters/wmbusmeters) decoder over the same
captured telegrams and compare the normalised output. It is a plain script — never run
by `cargo test` or CI, never a build dependency.

Its purpose is to catch decode regressions and to give the Multical 21 status work an
independent reference, per design decision **D3/D4** in
[`docs/design/vendor-layers.md`](../../docs/design/vendor-layers.md).

## The licensing boundary — read this first

`wmbusmeters` is **GPL-3.0-or-later**; this crate is **MIT**. This tool stays on the
right side of that line by treating wmbusmeters strictly as a runtime **oracle**:

- **Only its output is read.** We compare decoded values. No table, fixture, XMQ driver,
  or line of its code is transcribed into this repository.
- **Our tables come from primary sources** — EN 13757, OMS, and Kamstrup's own technical
  descriptions — validated *against* the oracle, not copied *from* it.
- **The oracle is a comparison, not truth.** A mismatch is resolved against the raw
  bytes, the standard, and Kamstrup documentation — never by adopting upstream's answer
  on authority.

## Install the oracle (pinned)

Written against **wmbusmeters 1.17.1**. A different version may rename JSON fields; if
you upgrade, re-check the field mapping in `compare.py`.

```sh
# Debian/Ubuntu/RPi
sudo apt install wmbusmeters            # or build from source, pinned to the tag above

# macOS
brew install wmbusmeters
```

The harness runs fine without it — it prints our normalised output and a notice — so you
can use it as a decode dump even with no oracle present.

## Keys and captures stay local

Real AES keys and real meter captures are **never committed**. Everything under
`keys/` and any `*.local.hex` / `*.keys` file is gitignored.

- Put real captured telegrams (one hex frame per line) in `captures.local.hex`.
- Put keys in a JSON map `{ "<meterid>": "<32-hex key>" }` under `keys/` (or reuse the
  gateway's key file). The committed `captures.sample.hex` is a synthetic frame built
  with a published test key and decodes to nothing real.

## Run

```sh
(cd metermon-rs && cargo build --release)

# synthetic sample (mechanics only)
tools/oracle/compare.py --captures tools/oracle/captures.sample.hex

# real differential run
tools/oracle/compare.py \
    --captures tools/oracle/captures.local.hex \
    --keys keys/meters.json \
    --driver multical21
```

Output is a per-meter, per-field table: `OK` where the two agree, `!!` where they
disagree (investigate against raw bytes), `..` where only one produced the field. The
run never fails the build on a mismatch — resolving one is a human, evidence-based step.

## Reading the results — a worked example

A real run on meter 74644444 (Multical 21 cold water) validated our decoder and also
surfaced a genuine behavioural difference:

- **Values agree where both decode.** wmbusmeters (`kamwater` driver) produced
  `total 25.539 m³, target 25.375, flow 18 °C, ext 24 °C, status OK`; our decoder
  produces the same values on the same bytes. That agreement is the point of the run.
- **Coverage differs on compact frames.** wmbusmeters decodes a compact frame standalone
  from its built-in driver layout; we require a **CRC-valid full frame** first, because
  learning a layout from a corrupt frame would poison every compact frame after it (a
  deliberate safety choice — see `wmbus::compact_frame`). On a marginal-link meter whose
  full frames fail CRC, the harness will therefore show `ours=None` for a compact frame
  wmbusmeters still decodes. This is expected, not a decode bug: it is the safety/coverage
  tradeoff made visible, and it is resolved by capturing one clean full frame, not by
  relaxing the gate.

`status OK` above also confirms this meter's INFO field is 0 (no fault) in every capture
we hold — which is exactly why the Multical 21 status *table* stays evidence-blocked
(design D3): there is no fault to interpret and no primary-source table in the repo, so
the crate emits the raw bitmask and no condition names.

## Scope

- **Water first.** Our decryptable meters are Multical 21 *water* meters; use
  `--driver multical21`. Heat meters (`kamheat`) are deferred until we hold a heat key.
- This tool does not decode LoRa/LoRaWAN and does not exercise the radio — it is a
  decode oracle only.
