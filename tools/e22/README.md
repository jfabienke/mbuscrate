# E22-900T22S test rig

The gateway's independent LoRa peer: an Ebyte E22 (its own SX1262) driven from
the Mac over USB-serial. Stage 1 it transmits and the gateway must hear it;
stage 2 the roles flip and it verifies the gateway's transmissions — the
prerequisite for the LoRaWAN join responder.

## Hardware setup

1. Plug the HAT's USB into the Mac. `uv run e22ctl.py detect` lists ports
   (the Alveo FTDI and RPP probes are recognised and excluded).
2. Two separate jumper blocks, and both matter:
   - **MODE SELECT (M0/M1)** — a cap grounds the pin, an empty header floats high:

     | M0 | M1 | Mode | Caps |
     |---|---|---|---|
     | low | low | 0 normal (transparent) | both fitted — `tx` / `rx` |
     | high | low | 1 WOR | M0 off, M1 on |
     | low | high | **2 configuration** | **M0 ON, M1 OFF** — `info` / `setup` |
     | high | high | 3 deep sleep | both removed |

     Only Mode 2 answers register commands. Mode 0 and Mode 3 ignore them and are
     indistinguishable from dead hardware, so a wrong cap position reads as a
     hardware fault. Confirmed against Waveshare's wiki and xreef/EByte_LoRa_E22.
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

**2026-08-07 — the module transmits; nothing decodes it as LoRa yet.**

Configuration read successfully in Mode 2: channel 18 (868.125 MHz, the Ebyte
default), air rate 2.4k, 240-byte packets, transparent, no encryption, product
`002210160b0000`. Power lowered 22 -> 10 dBm, since 22 dBm at bench range puts
roughly -15 dBm into the gateway's LNA.

Transmission proven by controlled energy measurement at 868.125 MHz — modulation
agnostic, so it does not depend on demodulating anything:

| | beacon ON | beacon OFF (control) |
|---|---|---|
| peak RSSI | **-24 dBm** | -68 dBm |
| samples >floor+12 | **26.5%** | 0.7% |

Not decoded, by either receiver, across SF5-12 x BW125/250/500 x sync
0x1424/0x3444 x explicit/implicit header: ours finds only noise-triggered
implicit-mode frames (SNR ~-12 dB, RSSI at the floor, random 255-byte payloads),
and **RadioLib finds nothing at all** on the same hardware. Both failing
identically rules out a defect in this crate's driver and points at the
assumed on-air format.

Open question: the air-rate -> (SF, BW) mapping and LoRa sync word this module
actually uses. The widely-repeated mapping (2.4k -> SF10/BW125) is covered by the
sweep, so either it is wrong for this variant or the sync word is neither of the
two standard values. Needs the genuine Ebyte E22-900T22S datasheet — note the
LCSC "C411289" link and one cdebyte download resolve to an antenna-cable
datasheet, not the module.
