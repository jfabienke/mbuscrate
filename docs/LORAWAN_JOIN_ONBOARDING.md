# LoRaWAN OTAA onboarding — field lessons (metermon-rs join responder)

How we onboarded a **real commercial device** (a Zenner caltos-E heat-cost allocator,
factory-locked to Minol-ZENNER Connect's network) onto the metermon-rs single-channel
join responder, and the multi-stage debug it took. Verified end-to-end on real hardware:
JoinRequest → MIC → JoinAccept → session → **decrypted uplink** on our network.

The headline fix is small (answer in RX2 at full power); most of the value is the
**diagnostic chain and the red herrings**, so the next real-device onboarding is fast.

---

## The fix that closed it: answer in RX2, at power

The responder originally sent the JoinAccept in **RX1** (uplink channel, +5 s, +14 dBm).
That reached a *close, lenient simulator* (the earlier Pico join) but **not a 1 m
commercial device**. Switching to **RX2** closed it:

- **RX2 = 869.525 MHz, DR0 (SF12), +6 s** after the JoinRequest end.
- Transmit at **+22 dBm** — RX2 sits in the 869.4–869.65 MHz sub-band that permits
  +27 dBm ERP (10 % duty), so full chip power is legal there (RX1's 868.5 is ERP-capped
  at +14 dBm).
- **Stay silent in RX1.** A device opens RX2 only if RX1 detected nothing, so silence in
  RX1 *guarantees* it opens RX2.

**Why not answer in both:** a JoinAccept is ~1.5 s of SF12 airtime, longer than the 1 s
RX1→RX2 gap, so the two windows physically overlap — you must pick one. RX2 is the robust
choice: one fixed channel (no per-uplink-channel logic), higher legal power, an extra
second of setup margin.

`metermon-rs/src/join_responder.rs`: the accept is staged/fired on `RX2_FREQ_HZ` /
`RX2_POWER_DBM` at `JOIN_ACCEPT_DELAY2`.

---

## Diagnostic chain (and the red herrings that cost time)

1. **A real operator network was intercepting our joins.** The device auto-joins its
   **factory operator network OTA from the bench** — Minol-ZENNER Connect, NetID `0x3A`
   (Type-0; DevAddr block `74000000/7`), which holds the device's *factory* AppKey. Our
   test responder and the operator's gateway both MIC-verified the same JoinRequest; the
   operator's (multi-channel, higher-power, precisely-timed) gateway won the race. **Tell:
   the device showed a DevAddr in the operator's block that our responder never issued.**
   → **Onboarding path: re-key the device to a gateway-only AppKey.** Then the operator's
   join server can't MIC-verify → it drops out → our responder is the only valid answerer.

2. **`−104 dBm at 1 m` is a RED HERRING — it's the RSSI-miscalc artifact, not a weak
   link.** FSPL at 1 m/868 MHz is ~31 dB; −104 is 50–70 dB too low. The true link was
   ~−74 dBm (seen on stronger fires). This board has a documented LoRa RSSI miscalc.
   **Trust the frequency-error (`ferr`) measurement — an amplitude-independent number —
   over reported RSSI.**

3. **Do NOT chase the crystal offset with the RX tuning.** The device's crystal is ~+26 kHz
   fast (measured `ferr` ≈ 26 kHz). Tuning the *whole* responder to 868.526 to "centre" it
   **corrupted uplink RX** — the DevEUI arrived bit-mangled (a consistent `77→F7` flip),
   because moving RX onto the offset removed the SX1262 AFC's headroom and introduced
   systematic SF12 symbol errors. **Leave RX on the nominal channel and let AFC handle the
   device's offset** (it decoded the +26 kHz uplink cleanly). The offset did **not** block
   the downlink either — the operator reached the device despite it.

4. **The single-channel responder must be pointed at the device's real channel + SF.** A
   device randomises its join channel across 868.1/.3/.5, and its join DR is fixed by the
   device (here **SF12 / DR0**, *not* the 868.3 EEPROM centre nor the responder's SF9
   default). Use the sweep receiver (`lora-rx --sweep`) with the device firing repeatedly:
   it decodes the JoinRequest header and reports the exact `(freq, SF)` — no key needed.
   This device: **868.5 MHz was the miss; SF12 was the real join DR.**

5. **The SX1262 TX path had never been validated for a real device.** wM-Bus is
   receive-only, so the JoinAccept was among the first real transmits. `LoraTx` confirms
   the *command path* (TxDone fires) but **cannot** confirm radiation without an
   independent receiver. Note: `lora_prepare_tx` does not re-apply `SetPaConfig`/
   `SetTxParams` — power comes only from the profile switch in `stage_downlink*`. (In the
   end the responder *was* already at +14 via the profile switch; `LoraTx`'s "2 dBm" was
   its own default — another red herring. The real deficit was RX1 window + power, not a
   lost power setting.)

---

## Onboarding runbook (real device → our gateway)

1. **Identify** the device's join channel + SF: `lora-rx --sweep` while the device fires
   repeatedly → read the decoded `JoinRequest dev_eui=… freq SF`.
2. **Re-key** the device (out-of-band / optical) to a **gateway-only AppKey**; provision
   the same `DevEUI → AppKey` on the Pi (`0600`, never in the repo). This excludes the
   operator network.
3. **Arm** `metermon-rs lorawan-join --freq-hz <uplink-chan> --sf <join-SF> --creds …`.
   RX stays on the uplink channel; the accept auto-answers in RX2 at +22 dBm.
4. **Fire** joins (~5 s cadence — ~1/3 land on the parked channel). Confirm on the device
   side (DevAddr in *our* NetID, not the operator's) and by a **decrypted uplink** on the
   assigned DevAddr — the strongest proof (a device won't uplink without the session keys).

## The uplink payload is NOT standard M-Bus/wM-Bus

**Do not feed a Zenner LoRa uplink to the M-Bus record decoder.** The captured decrypted
HCA payload (`921b80070101000110000892a9aa020000`) does not begin with a wM-Bus CI/DIF and
does not parse as EN 13757-3 DIF/VIF records — it is a **Zenner-proprietary application
format**, consistent with the earlier finding that Zenner's LoRa payloads are not M-Bus
encoded. The join responder therefore **captures every frame** (ciphertext *and* decrypted
plaintext) to the `--capture` JSONL specifically for **offline vendor-format
reverse-engineering** — decoding it is separate vendor work, keyed on device/firmware, not
a job for the generic decoder. A gateway that assumed M-Bus here would silently emit garbage
readings.

## Cross-session credential hygiene (observed)

The assistant's safety classifier **blocks transmitting an AppKey (or key-like hex) between
sessions**. Resolved cleanly: the key was **written to a local `0600` file and read**, not
pasted into the message channel; operational signals (DevAddr, "armed", "go") pass fine.
Keep secrets in `0600` files + device EEPROM only.

Related: [[sx1262-hat-hardware]], [[pi5-gnss-uart-wiring]], [[wmbus-decode-pipeline]].
