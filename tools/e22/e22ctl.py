# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""E22-900T22S controller — drives the Ebyte UART LoRa module from the Mac.

The E22 is the gateway's independent LoRa peer: a second SX1262 we control over
USB-serial, used to prove the gateway's LoRa receive (this module transmits) and
later its transmit (this module receives). It speaks Ebyte's binary register
protocol in configuration mode and raw payload bytes in transparent mode; the
mode is selected by the M0/M1 jumpers on the HAT, not software, so each
subcommand states the jumper position it needs and verifies it by behaviour.

Modes (a jumper cap grounds the pin; an empty header floats it high):

  M0    M1    Mode                       Caps
  ---   ---   ------------------------   -------------------------
  low   low   0 normal (transparent)     both fitted
  high  low   1 WOR                      M0 off, M1 on
  low   high  2 CONFIGURATION            M0 ON, M1 OFF   <- info/setup
  high  high  3 deep sleep               both removed

Verified against two independent sources, because getting this wrong is silent —
the module simply ignores register commands and reads as dead hardware:
  * Waveshare's HAT wiki mode table
  * xreef/EByte_LoRa_E22 lora_e22.py set_mode(), MODE_2_CONFIGURATION

Note deep sleep is BOTH caps removed. Configuration mode's serial is fixed at
9600 8N1 regardless of the configured data baud, and the module needs ~40 ms
after a mode change before it answers.

The board also carries a UART routing block (silkscreen A/B/C, separate from
MODE SELECT): A = USB-LoRa, B = Pi-LoRa, C = USB-PI. Both caps must be on A for
this tool to reach the module.

Frequency: 850.125 MHz + channel. Channel 18 = 868.125 MHz.

Usage:
  uv run e22ctl.py detect
  uv run e22ctl.py info    [--port P]                # needs config jumpers
  uv run e22ctl.py setup   [--port P] [--channel 18] [--air-rate 2.4k]
                           [--power 10]              # needs config jumpers
  uv run e22ctl.py tx      [--port P] [--interval 2] [--count 0=forever]
                           [--baud 9600]             # needs transparent jumpers
  uv run e22ctl.py rx      [--port P] [--baud 9600]  # needs transparent jumpers
"""

import argparse
import glob
import sys
import time

import serial

AIR_RATES = {
    "0.3k": 0b000,
    "1.2k": 0b001,
    "2.4k": 0b010,
    "4.8k": 0b011,
    "9.6k": 0b100,
    "19.2k": 0b101,
    "38.4k": 0b110,
    "62.5k": 0b111,
}
POWERS = {22: 0b00, 17: 0b01, 13: 0b10, 10: 0b11}


def find_port(explicit: str | None) -> str:
    if explicit:
        return explicit
    # The Alveo's FTDI enumerates as usbserial-50241A29CC7H*; skip it and the
    # RPP debug probes. The E22 HAT's CP2102/CH340 is anything else.
    candidates = [
        p
        for p in glob.glob("/dev/cu.*")
        if any(k in p.lower() for k in ("usbserial", "slab", "wch", "usbmodem"))
        and "50241A29CC7H" not in p
        and "RPP_PRO" not in p
    ]
    if not candidates:
        sys.exit(
            "no candidate serial port. Plug the E22 HAT's USB in, then re-run "
            "detect. (Alveo FTDI and RPP probes are excluded on purpose.)"
        )
    if len(candidates) > 1:
        sys.exit(f"multiple candidates, pass --port: {candidates}")
    return candidates[0]


def cmd_probe(args) -> None:
    """Sweep baud rates and both Ebyte command dialects, reporting any reply.

    Written after the module stayed silent in every jumper position: rather than
    guessing one more variable, this varies all of them and prints whatever comes
    back. Total silence across the whole matrix means the bytes are not reaching a
    powered module at all — a wiring or power fault, not a protocol mismatch.
    """
    port = find_port(args.port)
    # E22/E220 read-register vs the older E32 "read all parameters" form.
    commands = {
        "C1 00 09 (E22 read regs)": bytes([0xC1, 0x00, 0x09]),
        "C1 C1 C1 (E32 read all)": bytes([0xC1, 0xC1, 0xC1]),
        "C1 80 07 (E22 product id)": bytes([0xC1, 0x80, 0x07]),
    }
    bauds = [9600, 115200, 57600, 38400, 19200, 4800, 2400]
    print(f"probing {port}\n")
    any_reply = False
    for baud in bauds:
        for label, cmd in commands.items():
            try:
                with serial.Serial(port, baud, timeout=0.4) as ser:
                    # Some bridges hold the module in reset via DTR/RTS; release both.
                    ser.dtr = False
                    ser.rts = False
                    time.sleep(0.05)
                    ser.reset_input_buffer()
                    ser.write(cmd)
                    ser.flush()
                    resp = ser.read(64)
            except serial.SerialException as e:
                print(f"  {baud:>6} {label:<26} port error: {e}")
                continue
            if resp:
                any_reply = True
                print(f"  {baud:>6} {label:<26} -> {resp.hex(' ')}")
            else:
                print(f"  {baud:>6} {label:<26} -> (silence)")
    print()
    if not any_reply:
        print(
            "No reply on any baud with any dialect. Check the jumpers FIRST: register\n"
            "commands are answered only in Mode 2 (M0 cap ON, M1 cap OFF). Mode 0\n"
            "(both fitted) and Mode 3 deep sleep (both removed) both ignore them and\n"
            "are indistinguishable from dead hardware. If the jumpers are right and\n"
            "the A/B/C block is on A, then suspect power: the CP2102 runs off USB but\n"
            "the module may take its 3V3 from the host header."
        )


def cmd_detect(_args) -> None:
    seen = sorted(glob.glob("/dev/cu.*"))
    for p in seen:
        marker = ""
        if "50241A29CC7H" in p:
            marker = "  (Alveo U50C - not the E22)"
        elif "RPP_PRO" in p:
            marker = "  (debug probe - not the E22)"
        print(f"{p}{marker}")


def open_config(port: str) -> serial.Serial:
    # Configuration mode is always 9600 8N1 regardless of the data baud.
    ser = serial.Serial(port, 9600, timeout=1.0)
    # The reference library waits 40 ms after any mode change before talking.
    time.sleep(0.05)
    return ser


def read_registers(ser: serial.Serial, addr: int, length: int) -> bytes | None:
    ser.reset_input_buffer()
    ser.write(bytes([0xC1, addr, length]))
    resp = ser.read(3 + length)
    if len(resp) == 3 + length and resp[0] == 0xC1 and resp[1] == addr:
        return resp[3:]
    return None


def cmd_info(args) -> None:
    port = find_port(args.port)
    with open_config(port) as ser:
        regs = read_registers(ser, 0x00, 9)
        if regs is None:
            sys.exit(
                f"no config-mode response on {port}. Config mode is Mode 2: "
                "FIT the M0 cap, REMOVE the M1 cap, then replug USB (the module "
                "samples the pins only at power-up). Both caps removed is deep "
                "sleep, which answers nothing. Also check the A/B/C routing block "
                "is on A (USB-LoRa)."
            )
        pid = read_registers(ser, 0x80, 7)
    addh, addl, netid, reg0, reg1, ch, reg3, crypt_h, crypt_l = regs
    air = {v: k for k, v in AIR_RATES.items()}[reg0 & 0x07]
    power = {v: k for k, v in POWERS.items()}[reg1 & 0x03]
    print(f"port      {port}")
    print(f"address   0x{addh:02X}{addl:02X}  netid 0x{netid:02X}")
    print(f"channel   {ch}  -> {850.125 + ch:.3f} MHz")
    print(f"air rate  {air}  (REG0=0x{reg0:02X})")
    print(f"power     {power} dBm  (REG1=0x{reg1:02X})")
    print(f"options   REG3=0x{reg3:02X} (fixed-tx={bool(reg3 & 0x40)}, rssi-byte={bool(reg3 & 0x80)})")
    if pid:
        print(f"product   {pid.hex()}")


def cmd_setup(args) -> None:
    port = find_port(args.port)
    air = AIR_RATES[args.air_rate]
    power = POWERS[args.power]
    with open_config(port) as ser:
        if read_registers(ser, 0x00, 9) is None:
            sys.exit(
                f"no config-mode response on {port}. Config mode is Mode 2: "
                "FIT the M0 cap, REMOVE the M1 cap, then replug USB. Both caps "
                "removed is deep sleep. Also check the A/B/C routing block is on "
                "A (USB-LoRa)."
            )
        # ADDH/ADDL/NETID zero (broadcast/transparent), 9600 8N1 + air rate,
        # 240B packets + power, channel, plain transparent mode, crypt off.
        reg0 = (0b011 << 5) | (0b00 << 3) | air
        reg1 = (0b00 << 6) | power
        params = bytes([0x00, 0x00, 0x00, reg0, reg1, args.channel, 0x00, 0x00, 0x00])
        ser.reset_input_buffer()
        ser.write(bytes([0xC0, 0x00, 0x09]) + params)
        resp = ser.read(3 + 9)
        if len(resp) < 12 or resp[0] != 0xC1:
            sys.exit(f"write not acknowledged: {resp.hex() if resp else 'no reply'}")
    print(
        f"configured: channel {args.channel} ({850.125 + args.channel:.3f} MHz), "
        f"air rate {args.air_rate}, {args.power} dBm, transparent, crypt off"
    )
    print("now FIT both M0/M1 caps (Mode 0 transparent), replug USB, and run `tx`.")


def cmd_tx(args) -> None:
    port = find_port(args.port)
    with serial.Serial(port, args.baud, timeout=0.2) as ser:
        n = 0
        print(f"transmitting on {port} every {args.interval}s (ctrl-c to stop)")
        while args.count == 0 or n < args.count:
            n += 1
            payload = f"MBUSCRATE-E22-{n:05d}".encode()
            ser.write(payload)
            ser.flush()
            print(f"tx {n}: {payload.decode()}")
            time.sleep(args.interval)


def cmd_rx(args) -> None:
    port = find_port(args.port)
    with serial.Serial(port, args.baud, timeout=0.5) as ser:
        print(f"listening on {port} (ctrl-c to stop)")
        while True:
            data = ser.read(256)
            if data:
                ts = time.strftime("%H:%M:%S")
                printable = data.decode(errors="replace")
                print(f"{ts} rx {len(data)}B: {printable}  [{data.hex()}]")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("detect").set_defaults(fn=cmd_detect)
    p = sub.add_parser("probe")
    p.add_argument("--port")
    p.set_defaults(fn=cmd_probe)
    p = sub.add_parser("info")
    p.add_argument("--port")
    p.set_defaults(fn=cmd_info)
    p = sub.add_parser("setup")
    p.add_argument("--port")
    p.add_argument("--channel", type=int, default=18)
    p.add_argument("--air-rate", choices=AIR_RATES, default="2.4k")
    p.add_argument("--power", type=int, choices=POWERS, default=10)
    p.set_defaults(fn=cmd_setup)
    p = sub.add_parser("tx")
    p.add_argument("--port")
    p.add_argument("--interval", type=float, default=2.0)
    p.add_argument("--count", type=int, default=0)
    p.add_argument("--baud", type=int, default=9600)
    p.set_defaults(fn=cmd_tx)
    p = sub.add_parser("rx")
    p.add_argument("--port")
    p.add_argument("--baud", type=int, default=9600)
    p.set_defaults(fn=cmd_rx)
    args = ap.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
