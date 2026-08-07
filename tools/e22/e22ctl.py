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

Modes (E22 convention, pins have pull-ups; a jumper cap grounds the pin):
  transparent  M0=0 M1=0   both caps fitted     - tx/rx of raw payloads
  config       M0=0 M1=1   M0 cap on, M1 cap off - register access at 9600 8N1

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
    return serial.Serial(port, 9600, timeout=1.0)


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
                f"no config-mode response on {port}. Check the jumpers: config "
                "mode is M0 cap ON, M1 cap OFF, then power-cycle the module "
                "(replug USB)."
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
                f"no config-mode response on {port}. Jumpers: M0 cap ON, "
                "M1 cap OFF, then replug USB."
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
    print("now move BOTH jumper caps on (M0=0 M1=0), replug USB, and run `tx`.")


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
