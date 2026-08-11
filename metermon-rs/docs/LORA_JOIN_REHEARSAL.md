# LoRaWAN join rehearsal — bench dress-run before provisioning a real meter

A bench rehearsal of the gateway's OTAA join responder against an **independent**
RadioLib end device (the Waveshare Pico-LoRa-SX126X running `pico-lora-jointest`),
using the **real credentials of meter 179669**, before we provision and join any
physical meter.

## Why this exists

The join responder is now proven interoperable (RadioLib is a foreign stack, so a
join it accepts validates our code rather than agreeing with it). But two gaps
remained until now:

1. **The DevNonce anti-replay was a 1.0.4 high-water counter** (`admit_dev_nonce`,
   strict-greater). The fleet runs **LoRaWAN 1.0.2** on an LMIC stack, which draws
   DevNonce **randomly**. Against random nonces a strict counter admits the first
   join, then rejects every later one whose random draw lands below the running
   watermark — expected acceptance ~`1/(n+1)` after `n` accepted joins, so a meter a
   few joins into its life looks bricked. **Fixed**: the store is now version-aware
   (`DevNoncePolicy::RandomWindow` — a remembered set of used nonces — is the
   default; `Counter` is opt-in 1.0.4 hardening). See `src/lorawan.rs`,
   `metermon-rs/src/join_store.rs`.
2. **RadioLib cannot emit random DevNonces** (it increments unconditionally, even in
   1.0.x mode). So the campaign's earlier persistence tests, which celebrated
   "rolled-back nonces rejected", exercised *counter* behaviour — the exact
   observable that is a **false-reject bug** for a 1.0.2 device. Same telemetry,
   opposite verdict; only the device model decides which. The `r` command
   (below) injects a chosen/random DevNonce into RadioLib's signed nonces buffer so
   the Pico can faithfully stand in for a 1.0.2 meter.

## What this rehearsal proves — and what it does not

Proves: the RF link end-to-end, the OTAA join (MIC verify over 179669's **real
AppKey**, JoinAccept encrypt, session-key derivation, RX1 timing), the credential
file plumbing and DevEUI byte-order **for 179669 specifically**, and that the fixed
responder admits repeated **random** DevNonces while still rejecting genuine
replays.

Does **not** prove: decoding a real meter's application payload. Real Zenner LoRa
devices send their compact SP-packet family, not the OMS/wM-Bus records the Pico
emulator sends. First real uplink decode stays a separate capture-and-map step per
device class.

## Secret handling (do this, don't skip it)

179669's **AppKey is a device secret** and must never enter committed source or the
chat. The plumbing is built so you inject it locally:

- Gateway: a plaintext `--creds` JSON file you create on the bench/Pi, **git-ignored**
  (`metermon-rs/lorawan-creds*.json` is in `.gitignore`).
- Pico: passed as `-DJT_APP_KEY=...` on the CMake line — baked into your local UF2,
  never into the tree (the in-source default stays the published test vector).

The DevEUI and JoinEUI are identifiers, not secrets, but keep them in the same
local files for tidiness.

## Prerequisites

- Meter 179669's **DevEUI (8 bytes)**, **JoinEUI/AppEUI (8 bytes)**, **AppKey (16
  bytes)** — held by you, off-repo.
- Pico wired per `tools/pico-lora-beacon/README.md` (Core1262-HF, TCXO on DIO3 @
  1.7 V, antenna switch on DIO2), USB console attached.
- Gateway Pi (`192.168.25.22`) with the radio HAT; `metermon-rs` built
  `--features radio`.
- **The monitor must be stopped** for the whole rehearsal — the HAT radio and the
  SPI bus are shared. Restart it when done.

## Step 1 — gateway credentials file (on the Pi, git-ignored)

`--creds` maps **DevEUI (big-endian hex)** → **AppKey (32 hex)**. `load_join_creds`
reverses the DevEUI to wire order internally; the responder matches an incoming
join **by DevEUI**, and the MIC over the AppKey is what actually binds it.

```jsonc
// ~/lorawan-creds.json   (NOT in the repo)
{
  "<179669 DevEUI, big-endian hex, 16 chars>": "<179669 AppKey, 32 hex chars>"
}
```

## Step 2 — build & flash the Pico as 179669

Build `pico-lora-jointest` with 179669's identity. The DevEUI/JoinEUI are 64-bit
integer literals (MSB-first, matching the creds-file hex); the AppKey is a
comma-separated 16-byte list.

```bash
cmake -DPICO_SDK_PATH=... -DRADIOLIB_DIR=... -DPICO_BOARD=pico_w \
      -DPICO_TOOLCHAIN_PATH=... \
      -DJT_DEV_EUI=0x<179669 DevEUI>  \
      -DJT_JOIN_EUI=0x<179669 JoinEUI> \
      -DJT_APP_KEY="0x..,0x..,0x..,0x..,0x..,0x..,0x..,0x..,0x..,0x..,0x..,0x..,0x..,0x..,0x..,0x.." \
      ..
make -j8 pico-lora-jointest
picotool load -f -x pico-lora-jointest.uf2 && picotool reboot -f
```

Confirm on the console: `ready. DevEUI=<179669 DevEUI> — send 'j' to join`.

## Step 3 — run the responder (monitor stopped, radio freed, time-boxed)

```bash
sudo systemctl stop metermon-rs        # free the radio + SPI bus
cd ~/metermon-rs
timeout 420 ./target/debug/metermon-rs lorawan-join \
    --creds ~/lorawan-creds.json \
    --join-db ~/lorawan-join.redb \
    --capture ~/join-frames.jsonl \
    --seconds 360
```

The `timeout` wrapper is the belt to the responder's own `--seconds` braces — a
wedged radio never strands the box. The responder returns the radio to standby on
exit.

> Start from a clean `--join-db` (delete `~/lorawan-join.redb`) if you want the
> first `j` to be device-first-seen. Keep it to test restart persistence.

## Step 4 — drive the Pico and read the verdicts

| Cmd | Action | Expect at the responder |
|---|---|---|
| `j` | join, monotonic DevNonce (RadioLib default) | `provisioning chain: <179669 DevEUI> joined as <DevAddr>` |
| `r` | join, **random** DevNonce (1.0.2 emulation, injected into RadioLib's signed nonces buffer) | admitted every time (see PASS-2); the firmware self-checks that the sent nonce equals the injected one and aborts `FATAL` on a layout mismatch |
| `p` | **replay** the last `r` nonce verbatim | `DevNonce replay (last …, seen …), rejected` on every attempt; Pico prints `RESULT no-join` |
| `s` | print session state | — |

### Pass/fail criteria

- **PASS-1 (positive path).** First `j` → responder prints the provisioning-chain
  line naming 179669's DevEUI and a `DevAddr`; Pico prints `JOINED joinNonce=…`.
  This validates the real AppKey + creds plumbing + byte order + RF.
- **PASS-2 (the fix — random re-join).** Issue `r` **repeatedly (≥ 8×)**. Every
  attempt joins. Before the fix this failed roughly `1 − 1/(n+1)` of the time on
  each successive join; after it, only a genuine self-collision (odds `k/65536`,
  negligible at this k) can miss. **If any `r` is rejected as a replay with two
  *different* nonces, the fix is not in the running binary — stop.**
- **PASS-3 (true replay still refused).** After a successful `r`, issue `p` — it
  re-sends that exact nonce. Expect `DevNonce replay (last …, seen …), rejected` at
  the responder on every attempt and `RESULT no-join` on the Pico. **No** JoinAccept
  may be transmitted.
- **PASS-4 (restart persistence).** Note a JoinNonce; `Ctrl-C` the responder and
  re-run Step 3 (same `--join-db`); join again. JoinNonce must **not** regress, and
  a nonce used before the restart must still be rejected (window survived).

Cross-check `~/join-frames.jsonl`: each admitted join has a JoinRequest (ciphertext
+ parsed DevEUI/DevNonce) and the JoinAccept we sent; each uplink has ciphertext +
decrypted FRMPayload.

## Step 5 — restore production

```bash
# responder already returned the radio to standby on exit
sudo systemctl start metermon-rs
sudo systemctl status metermon-rs      # confirm the wM-Bus monitor is back
```

Confirm the fleet (105 meters) is being heard again before walking away.

## After a green rehearsal

The gateway is proven end-to-end for OTAA against 179669's real credentials. The
remaining variables are meter-side only:

1. Provisioning path on the meter (FC 0x35 credential Set + activation=OTAA + mode
   switch), inspected first in the offline write dry-run harness.
2. The first real uplink's **SP-packet** layout — capture raw uplinks
   (`--capture`) and work out the payload per device class; the join and session
   crypto are already proven here.
