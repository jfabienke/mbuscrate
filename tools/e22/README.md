# E22-900T22S test rig

The gateway's independent LoRa peer: an Ebyte E22 (its own SX1262) driven from
the Mac over USB-serial. Stage 1 it transmits and the gateway must hear it;
stage 2 the roles flip and it verifies the gateway's transmissions — the
prerequisite for the LoRaWAN join responder.

## Hardware setup

1. Plug the HAT's USB into the Mac. `uv run e22ctl.py detect` lists ports
   (the Alveo FTDI and RPP probes are recognised and excluded).
2. Mode is set by the M0/M1 jumper caps (cap fitted = pin grounded = 0):
   - **config**: M0 cap ON, M1 cap OFF — for `info` / `setup`
   - **transparent**: both caps ON — for `tx` / `rx`
   Replug USB after moving caps; the module samples them at power-up.
   `info` failing with "no config-mode response" means the caps are wrong.

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
