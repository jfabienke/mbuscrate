# Pico LoRa beacon — reference transmitter

A RadioLib (MIT) LoRa beacon on a Waveshare Pico-LoRa-SX126X, used to prove the
gateway's LoRa receive path. Every air parameter is stated explicitly in
`main.cpp` and matched explicitly on the gateway, so a failure to decode is
interpretable: the fault is in the receiver, not in an unknown transmitter.

That interpretability is the whole point, and it is what the Ebyte E22 could not
give us. The E22's air-rate presets map to an undocumented (SF, BW) pair and its
sync word is unpublished, so when neither our driver nor RadioLib could decode
it, the result meant nothing. This board carries a Core1262-HF — the same module
as the gateway HAT — driven over SPI.

## Build

Needs an ARM toolchain **with newlib**. Homebrew's `arm-none-eabi-gcc` ships
without it (`fatal error: stdio.h: No such file or directory`). The official ARM
toolchain has it, but its cask installer wants an interactive sudo password; the
package can simply be extracted instead, no system install:

```sh
brew fetch --cask gcc-arm-embedded          # downloads, does not install
PKG=$(ls ~/Library/Caches/Homebrew/downloads/*arm-gnu-toolchain*.pkg | head -1)
pkgutil --expand "$PKG" expand
mkdir payload && cd payload && cat ../expand/Payload | gunzip -dc | cpio -idm
```

Then:

```sh
git clone --depth 1 https://github.com/raspberrypi/pico-sdk
git -C pico-sdk submodule update --init lib/tinyusb   # REQUIRED for the console
git clone --depth 1 https://github.com/jgromes/RadioLib

mkdir build && cd build
cmake -DPICO_SDK_PATH=<pico-sdk> -DRADIOLIB_DIR=<RadioLib> -DPICO_BOARD=pico_w \
      -DPICO_TOOLCHAIN_PATH=<toolchain payload dir> ..
make -j8
```

**Do not skip the tinyusb submodule.** Without it the SDK emits a CMake *warning*
("USB support will be unavailable") and builds a perfectly valid UF2 with no USB
console — so the firmware appears to flash and run while giving no diagnostics at
all. A build that produced a UF2 has not necessarily produced the firmware asked
for.

## Flash

First time, or after a build with no USB stdio: hold BOOTSEL, plug in, and copy
`pico-lora-beacon.uf2` to `/Volumes/RPI-RP2`. Afterwards the firmware's own USB
reset interface makes it hands-free:

```sh
picotool load -f -x pico-lora-beacon.uf2     # reboots, loads, runs
picotool reboot -f                            # reset (to re-read the boot banner)
```

## Verified hardware facts

- **Pinout confirmed** (the chip answers): SPI1, SCK GP10, MOSI GP11, MISO GP12,
  NSS GP3, BUSY GP2, DIO1 GP20, RESET GP15.
- **This board's Core1262 uses a TCXO on DIO3 at 1.7 V.** With `tcxoVoltage = 0`
  `begin()` returns -707 (`SPI_CMD_FAILED`) and `GetDeviceErrors` reads `0x0020`
  = `XOSC_START_ERR`. Note -707 means the chip *is* answering over SPI; only -2
  (`CHIP_NOT_FOUND`) indicts the pinout. The firmware sweeps candidate voltages
  and reports which one worked rather than requiring a flash cycle per guess.
- The **gateway HAT genuinely differs**: it runs a plain crystal and receives
  wM-Bus at 100% CRC with no TCXO configuration. Same module part number,
  different board wiring — which is why Waveshare's module and HAT schematics
  disagree. Do not carry either board's answer across to the other.

## Result

2026-08-08: gateway decodes the beacon end to end, consecutive sequence numbers,
no gaps.

```
RX 17B rssi -110 dBm snr -3.5 dB ferr 14950 Hz  868.100 MHz SF7 BW125
   sync 0x1424  text "PICO-BEACON-00110"
```

Frequency error is a consistent ~+15 kHz — the offset between the Pico's TCXO and
the gateway's crystal, comfortably inside BW125's tolerance.

Next: reverse the roles (gateway transmits, this board receives) to prove the
gateway's transmit path, which the LoRaWAN join responder depends on.
