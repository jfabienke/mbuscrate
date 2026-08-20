# Raspberry Pi 5 gateway thermals — measured

Measurements from the production gateway (Pi 5, Waveshare SX1262 HAT, official Active
Cooler), taken with `metermon-rs thermal --watch`. They exist to answer one recurring
question: **can a HAT that covers the SoC — a LoRa concentrator, for instance — be fitted
without cooking the board or losing clock?**

Short answer: **yes at room temperature, with ~6 °C of margin, and an underclock is not
the lever.**

## The workload is negligible — measure it before designing around it

```
load average : 0.08 / 0.09 / 0.09   (4 cores)
metermon-rs  : 1.3 % of one core
```

The gateway is SPI polling, decode, MQTT and redb writes: **~0.3 % of the machine**. Any
thermal test using synthetic multi-core load is measuring a machine we never run — it
overstates the real workload by roughly 300× and produces an alarming number that does not
describe this system. Test with the real workload.

## Active cooling (fan working)

| condition | temp | fan | clock |
|---|---|---|---|
| idle | 55–59 °C | 1825 rpm (29 %), step 1/4 | 1500–1600 MHz |
| `cargo build` | 63 °C | 3434 rpm (49 %), step 2/4 | 2400 MHz |
| 4-core spin, 100 s | peak **78.2 °C** | **8232 rpm (98 %)**, step 4/4 | 2400 MHz, no throttle |

Note the last row: under synthetic full load the fan is already **flat out** with only
~2 °C of headroom. That is the bound for a machine doing heavy compute — not for this
gateway.

## Passive cooling (fan off, heatsink only, real workload)

60 minutes, 180 samples, fan stopped and the thermal governor disabled:

```
start           58.4 °C   (fan just stopped)
+180 s          64.4 °C
+600 s          68.8 °C
+3582 s         73.2 °C
final 15 min    73.5 °C mean (44 samples)   <- steady state
peak            74.9 °C
throttled       NEVER (0x0 across all samples)
clock           1500-1600 MHz throughout
load            0.06-0.13
```

**It plateaus at ~73.5 °C and never throttles.** The board is not in thermal runaway; it
reaches a stable equilibrium roughly 6.5 °C below the 80 °C soft limit.

## What follows from this

1. **Passive is viable at room temperature.** A HAT that blocks the fan but leaves the
   heatsink is survivable for this workload.

2. **The margin is ambient-dependent, and that is the actual risk.** 73.5 °C steady leaves
   ~6.5 °C. This gateway throttles at roughly **+7 °C ambient** — a warm plant room, a
   sealed cabinet or a hot summer eats the margin directly. Design against the ambient,
   not against the 73.5 °C.

3. **An underclock is not the mitigation.** The CPU already runs at 1500–1600 MHz under
   this workload, because `ondemand` never has reason to boost. We are heated by the SoC's
   *idle* power, not by compute, so `arm_freq=1500` gives back a clock we were not using.
   The lever is **airflow or conduction**: taller standoffs so the cooler still works, a
   GPIO ribbon extender to move the HAT off the board, or an enclosure fan.

4. **Watch for the fan that is supposed to be running.** The dangerous case is not a
   deliberate passive build; it is a HAT fouling a fan the thermal governor still believes
   in. `ThermalStatus::FanStalled` (PWM commanded, tacho zero) reports that as a hardware
   fault in its own right, outranking temperature — see `metermon-rs/src/thermal.rs`.

Incidentally the passive peak of 74.9 °C sits just under the classifier's 75 °C `hot`
band, so a healthy passive gateway reports `warm` rather than crying wolf.

## Reproducing

```bash
metermon-rs thermal                  # one snapshot
metermon-rs thermal --watch --interval 20 --seconds 3600 [--json]
```

To test passively, stop the governor driving the fan and then stop the fan — and **restore
both on every exit path**:

```bash
echo disabled | sudo tee /sys/class/thermal/thermal_zone0/mode
echo 1 | sudo tee /sys/class/hwmon/hwmon1/pwm1_enable   # locate by name, not index
echo 0 | sudo tee /sys/class/hwmon/hwmon1/pwm1
```

Two traps worth knowing:

- **hwmon numbering is not stable across boots.** Find the fan by reading the `name` file
  (`pwmfan`) and the cooling device by its `type` (`pwm-fan`). `thermal.rs` does this.
- **A signal handler that restores the fan must also exit.** A bare
  `trap restore TERM` restores the fan and then *resumes the sampling loop*, silently
  invalidating the run — the samples keep coming with the fan back on. Use
  `trap "restore; exit" INT TERM`. This cost one run here.

Always guard a passive soak with an automatic abort (e.g. 82 °C) and restore the fan on
`EXIT`. Firmware throttling protects the SoC independently of the Linux governor, and the
critical trip is 110 °C, but do not rely on either as the first line of defence.
