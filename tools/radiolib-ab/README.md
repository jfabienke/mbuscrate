# RadioLib A/B reference receiver

Independent oracle for the crate's SX126x GFSK receive path: the same radio,
frequency, framing and output contract, driven by [RadioLib](https://github.com/jgromes/RadioLib)
(MIT) instead of our driver. If the two disagree, one of them is wrong — and
RadioLib is the most complete open GFSK implementation for this part.

## Build (on the Pi)

```sh
sudo apt install liblgpio-dev          # lgpio, packaged on Raspberry Pi OS
git clone --depth 1 https://github.com/jgromes/RadioLib
g++ -O2 -std=c++17 -I RadioLib/src rlrx.cpp \
    $(find RadioLib/src -name '*.cpp') -llgpio -o rlrx
```

Pin note: the constructor is `PiHal(spiChannel, speed, spiDevice, gpioDevice)`.
`gpioDevice` must name the chip that carries the 40-pin header — **4** on
Pi 5 kernels before ~6.6.45 (this gateway), **0** after the renumbering.
A wrong chip presents as "GPIO not allocated" on the BUSY pin.

## A/B protocol

Run alternating windows so time-varying meter traffic cancels out, then feed
both captures through the same decoder:

```sh
metermon-rs sx1262-rx --seconds 120 --capture rust_1.hex   # ours
./rlrx 120 > rl_1.hex 2> rl_1.log                          # reference
metermon-rs replay rust_1.hex > rust_1.jsonl
metermon-rs replay rl_1.hex   > rl_1.jsonl
```

Compare frames, CRC-ok ratio, and the *set* of CRC-valid meters — the ratio
catches systematic decode errors, the set catches sensitivity gaps.

2026-08-07 baseline (3×120 s interleaved): ours 228 frames / 70% CRC-ok /
33 CRC-valid meters; RadioLib 251 / 70% / 36, our meter set a strict subset
of theirs. Differences within Poisson noise.

Deliberately not aligned: RadioLib derives a 24-bit preamble detector from
this configuration where we use 8 bits. That divergence is part of the test.
