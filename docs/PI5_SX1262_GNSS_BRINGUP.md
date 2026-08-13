# Raspberry Pi 5 — SX1262 + GNSS HAT bring-up (wM-Bus radio + GPS)

Field notes for bringing the **Waveshare SX1262 LoRaWAN/GNSS HAT** up on a **Raspberry Pi 5**
as a `metermon-rs` gateway: the wM-Bus radio over SPI, and the on-board GNSS feeding a gateway
position into the `op:observed` up-sync via `gpsd`.

The radio side is straightforward; the GNSS side has **three stacked traps** that make it look
like dead hardware when it is actually fine. All three are documented below with the exact
working configuration, verified on a live Pi 5 (kernel 6.6.20-rpi-2712).

---

## TL;DR working configuration

| Layer | Setting |
|-------|---------|
| wM-Bus radio | SX1262 on `/dev/spidev0.1`, `BUSY=GPIO20`, `DIO1=GPIO16`, `RESET=GPIO18` |
| GNSS chip | **CASIC AT6558R** (GPS+GLONASS, URANUS5 fw) — *not* an L76K |
| GNSS UART | `/dev/ttyAMA0` @ **9600** baud (GPIO14=TXD0 / GPIO15=RXD0) |
| Enable UART | `dtparam=uart0=on` in `/boot/firmware/config.txt` |
| STANDBY switch | **`ON` = GNSS active** (the silkscreen is reverse-labelled — see below) |
| gpsd | `DEVICES="/dev/ttyAMA0"`, `GPSD_OPTIONS="-n"`, `USBAUTO="false"` |
| metermon | `"gps": "127.0.0.1:2947"` in `metermon.conf` |

A position fix additionally requires the GNSS antenna to have **sky view** — indoors the
receiver streams NMEA but reports no fix (mode 1, 0 satellites), and `metermon` only adds
`gw_pos` to the up-sync once the fix is valid.

---

## The three traps

### 1. Pi 5 UART naming: `serial0` is NOT the 40-pin header

On the Pi 5, `/dev/serial0 → ttyAMA10` and the default `console=ttyAMA10,115200` refer to the
**dedicated 3-pin DEBUG UART connector** (PL011 at MMIO `0x107d001000`), **not** the 40-pin
GPIO header. Sniffing `serial0` for the HAT's GNSS is a dead end.

The HAT wires the GNSS to the **40-pin header UART on GPIO14/15**, which is **disabled by
default** (`pinctrl get 14,15` shows `none`). Enabling it with `dtparam=uart0=on` exposes it as
a *separate* device, **`/dev/ttyAMA0`** (PL011 AXI at MMIO `0x1f00030000`), and leaves the debug
console untouched — so **the serial console never needs to be disabled** for GPS.

### 2. The STANDBY switch is reverse-labelled

On this HAT the sliding STANDBY switch is **opposite to its silkscreen and to Waveshare's wiki
text**:

- Silkscreen **`OFF`** → GNSS in **standby**, emits nothing.
- Silkscreen **`ON`** → GNSS **active**, streams NMEA.

Leave it at **`ON`**. The `SET` button is a red herring for this — it does not start output.

### 3. It's a CASIC AT6558R, not an L76K

The module identifies itself in its start-up banner as a **CASIC AT6558R** running URANUS5
firmware, GPS+GLONASS, default **9600** baud. Sentences are `$GNxxx` (GN talker), and
`$GPTXT,...,ANTENNA OK` confirms the antenna is detected.

```
$GPTXT,01,01,02,MA=CASIC*27
$GPTXT,01,01,02,IC=AT6558R-5N-52-1C580901*15
$GPTXT,01,01,02,SW=URANUS5,V5.3.0.0*1D
$GPTXT,01,01,01,ANTENNA OK*35
```

---

## Bring-up procedure

All steps need `sudo`. `metermon.conf` is root-owned (`0644`), so edit it with `sudo`.

### 1. Enable the 40-pin header UART

```bash
sudo cp -n /boot/firmware/config.txt /boot/firmware/config.txt.bak-preuart
grep -q '^dtparam=uart0=on' /boot/firmware/config.txt \
  || echo 'dtparam=uart0=on' | sudo tee -a /boot/firmware/config.txt
sudo reboot
```

After reboot, verify the pins are muxed and the device exists:

```bash
pinctrl get 14,15          # expect: 14: a4 ... TXD0   15: a4 ... RXD0
ls -l /dev/ttyAMA0         # the header UART (distinct from serial0→ttyAMA10)
```

### 2. Set the STANDBY switch to ON

Physically slide STANDBY to **`ON`** (active — see trap #2). Confirm NMEA is flowing:

```bash
sudo stty -F /dev/ttyAMA0 9600 -echo raw
timeout 6 cat /dev/ttyAMA0 | grep -m8 '^\$G'
# expect $GNGGA/$GNRMC/$GNGSA/... and $GPTXT,...,ANTENNA OK
```

Zero bytes here almost always means the switch is at `OFF` (standby) or you're reading
`serial0`/`ttyAMA10` instead of `ttyAMA0`.

### 3. Install and configure gpsd

```bash
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y gpsd gpsd-clients
sudo tee /etc/default/gpsd >/dev/null <<'CONF'
START_DAEMON="true"
USBAUTO="false"
DEVICES="/dev/ttyAMA0"
GPSD_OPTIONS="-n"
CONF
sudo systemctl enable gpsd.socket gpsd.service
sudo systemctl restart gpsd.socket gpsd.service
```

Verify gpsd owns the device and is parsing it:

```bash
gpspipe -w -n 10 | grep -oE '"class":"[A-Z]+"' | sort | uniq -c   # DEVICE, VERSION, TPV, SKY…
```

### 4. Point metermon at gpsd

`metermon`'s `gps.rs` is a **gpsd client** (not a direct NMEA driver); the config key is a
gpsd address, absent = no GPS.

```bash
sudo cp -n /home/<user>/metermon.conf /home/<user>/metermon.conf.bak-pregps
sudo python3 - <<'PY'
import json
p = "/home/<user>/metermon.conf"
d = json.load(open(p)); d["gps"] = "127.0.0.1:2947"
json.dump(d, open(p, "w"), indent=2)
PY
sudo systemctl restart metermon-rs
```

Confirm the wiring in the log:

```
metermon-rs] GPS: streaming fixes from gpsd 127.0.0.1:2947
```

### 5. Get a fix

Move the **GNSS antenna to sky view** (window/outside). Until then:

```bash
gpspipe -w -n 40 | grep -m1 TPV     # mode 1 = no fix, 2 = 2D, 3 = 3D
```

Once `mode ≥ 2`, `metermon` marks the fix valid and starts adding
`gw_pos {lat, lon, fix_ts, eph_m?}` to each `op:observed` up-sync (it is dropped again if the
fix is lost).

---

## Durability

All of the above survives a reboot: `dtparam=uart0=on` is in `config.txt`, `gpsd` is
`enable`d, the STANDBY switch is physical, and the `gps` key is persisted in `metermon.conf`.
Backups are left at `config.txt.bak-preuart` and `metermon.conf.bak-pregps`.

## Troubleshooting quick table

| Symptom | Cause | Fix |
|---------|-------|-----|
| 0 bytes on `ttyAMA0` | STANDBY at `OFF` (standby) | slide to **ON** |
| 0 bytes, switch at ON | reading `serial0`/`ttyAMA10` (debug UART) | read `/dev/ttyAMA0` |
| `pinctrl get 14,15` = `none` | header UART not enabled | add `dtparam=uart0=on`, reboot |
| NMEA flows, no fix | antenna indoors / cold start | give the antenna sky view |
| gpsd up, metermon no GPS | `gps` key missing | add `"gps":"127.0.0.1:2947"`, restart |

The wM-Bus radio (SPI + `BUSY/DIO1/RESET`) is independent of all of this and keeps receiving
throughout the GNSS bring-up.
