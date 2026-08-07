# E22-900T22S test rig

The gateway's independent LoRa peer: an Ebyte E22 (its own SX1262) driven from
the Mac over USB-serial. Stage 1 it transmits and the gateway must hear it;
stage 2 the roles flip and it verifies the gateway's transmissions — the
prerequisite for the LoRaWAN join responder.

## Hardware setup

1. Plug the HAT's USB into the Mac. `uv run e22ctl.py detect` lists ports
   (the Alveo FTDI and RPP probes are recognised and excluded).
2. Two separate jumper blocks, and both matter:
   - **MODE SELECT (M0/M1)** — caps pull the pins LOW:
     - **Mode 3 config**: **both caps REMOVED** (M0=1 M1=1) — for `info` / `setup`
     - **Mode 0 transparent**: **both caps FITTED** (M0=0 M1=0) — for `tx` / `rx`
     Config mode is *not* "M0 low, M1 high" — that is Mode 2 (WOR receive), which
     ignores register commands and is indistinguishable from a dead module.
   - **UART routing (A/B/C)** — `A = USB-LoRa`, `B = Pi-LoRa`, `C = USB-PI`.
     Both caps on **A** for this tool.
   Replug USB after moving any cap; the module samples the pins only at power-up.

3. **Read the channel before hunting.** Until `setup` succeeds the module sits on
   its factory-default channel, which is not necessarily 868 MHz — E22-900T22S
   units commonly ship on channel 23 (873.125 MHz). Hunting at an assumed
   frequency while the module transmits elsewhere looks exactly like a broken
   radio; `info` reports the real channel.

## Proving gateway LoRa RX

```sh
uv run e22ctl.py info                      # confirm config mode + defaults
uv run e22ctl.py setup --channel 18 --power 10   # 868.125 MHz, minimum power
# move both caps on, replug, then:
uv run e22ctl.py tx --interval 2
```

Gateway side (the E22's air-rate -> SF/BW mapping and sync word are
undocumented, so the receiver sweeps the whole space; the 2s beacon guarantees
every dwell of the matching point catches frames):

```sh
metermon-rs lora-rx --hunt-e22 --freq-hz 868125000 --dwell 5 --seconds 600
```

A decode prints the full winning configuration (SF, BW, sync) — record it
here once found, and pin future tests to it.

## Findings

- (pending first successful decode)
