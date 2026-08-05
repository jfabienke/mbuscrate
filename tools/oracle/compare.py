#!/usr/bin/env python3
"""Differential decode oracle: metermon-rs vs. wmbusmeters.

Runs our decoder (`metermon-rs replay`) and, when it is installed, the upstream
`wmbusmeters` decoder over the SAME captured telegrams, normalises both outputs to a
common shape, and reports where they agree and differ.

This is a *comparison* tool, not a source of truth (design doc D4): wmbusmeters is an
oracle to cross-check against, and a mismatch is resolved against the raw bytes, the
standard, and Kamstrup documentation — not by copying upstream. Nothing from
wmbusmeters (tables, fixtures, code) is transcribed; only its runtime output is read.
See README.md for the GPL/MIT boundary.

It is intentionally external: a plain script, never run by `cargo test` or CI. Real
meter keys and captures are supplied through gitignored/local inputs and are never
committed.

Usage:
    tools/oracle/compare.py --captures <file.hex> [--keys <keys.json>] \\
        [--driver multical21] [--metermon <path-to-binary>]

Exit code is 0 when the run completes (including when the oracle is absent); it does
not fail the build on a field mismatch, because resolving a mismatch is a human,
evidence-based step.
"""
import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

# Pin the wmbusmeters version this harness was validated against; a different version may
# change the JSON field names and should be re-validated. README documents the install.
PINNED_WMBUSMETERS = "3.0.0"

# Canonical field <- (our quantity substrings, wmbusmeters JSON keys). Best-effort: the
# two projects name and unit fields differently, so we map a handful of common physical
# quantities and report the rest as unmatched rather than forcing a false correspondence.
FIELD_MAP = {
    "volume_m3": (["volume"], ["total_m3"]),
    "target_volume_m3": (["target", "due"], ["target_m3"]),
    "flow_temperature_c": (["flow temp"], ["flow_temperature_c"]),
    "external_temperature_c": (["external temp", "ambient"], ["external_temperature_c"]),
}


def run_metermon(binary: str, captures: Path, keys: Path | None) -> list[dict]:
    cmd = [binary, "replay", str(captures)]
    if keys:
        cmd += ["--keys", str(keys)]
    out = subprocess.run(cmd, capture_output=True, text=True)
    if out.returncode != 0:
        sys.exit(f"metermon-rs replay failed:\n{out.stderr}")
    rows = []
    for line in out.stdout.splitlines():
        line = line.strip()
        if line:
            rows.append(json.loads(line))
    return rows


def strip_mode_marker(telegram: str) -> str:
    """wmbusmeters wants the frame starting at the L byte; our RFM69 captures carry the
    mode-C sync marker (0x3D type B / 0xCD type A) in front of it. Drop it."""
    if len(telegram) >= 2 and telegram[:2].lower() in ("3d", "cd"):
        return telegram[2:]
    return telegram


def run_wmbusmeters(telegram: str, driver: str, meter_id: str, key: str | None) -> dict | None:
    """One telegram through wmbusmeters; returns its JSON or None.

    Invocation validated against 3.0.0: `wmbusmeters --format=json <hex> <name> <driver>
    <id> <key>`, with `auto` letting it pick the driver (e.g. kamwater). The key, when
    present, comes from the caller's gitignored key store and is passed on argv to the
    tool only — it is never logged or written by this harness.
    """
    tg = strip_mode_marker(telegram)
    cmd = ["wmbusmeters", "--format=json", tg, "oracle", driver, meter_id, key or ""]
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return None
    for line in out.stdout.splitlines():
        line = line.strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    return None


def normalise_ours(row: dict) -> dict:
    fields = {}
    for rec in row.get("records") or []:
        q = str(rec.get("quantity", "")).lower()
        val = rec.get("value")
        if not isinstance(val, (int, float)):
            continue
        for canonical, (ours, _theirs) in FIELD_MAP.items():
            if any(sub in q for sub in ours) and canonical not in fields:
                fields[canonical] = float(val)
    return {"id": str(row.get("meterid", "")), "fields": fields}


def normalise_theirs(row: dict) -> dict:
    fields = {}
    for canonical, (_ours, theirs) in FIELD_MAP.items():
        for key in theirs:
            if key in row and isinstance(row[key], (int, float)):
                fields[canonical] = float(row[key])
                break
    return {"id": str(row.get("id", "")), "fields": fields}


def compare(ours: dict, theirs: dict | None) -> None:
    print(f"\nmeter {ours['id']}")
    if theirs is None:
        for f, v in sorted(ours["fields"].items()):
            print(f"  {f:24} ours={v:<12} oracle=(absent)")
        return
    keys = sorted(set(ours["fields"]) | set(theirs["fields"]))
    if not keys:
        print("  (no comparable fields)")
    for f in keys:
        a = ours["fields"].get(f)
        b = theirs["fields"].get(f)
        agree = a is not None and b is not None and abs(a - b) <= 1e-6
        mark = "OK " if agree else ("!! " if a is not None and b is not None else ".. ")
        print(f"  {mark}{f:24} ours={a!s:<12} oracle={b!s}")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--captures", required=True, type=Path)
    ap.add_argument("--keys", type=Path, help="gitignored key file (JSON map or op:key lines)")
    ap.add_argument("--driver", default="auto", help="wmbusmeters driver, or 'auto'")
    ap.add_argument(
        "--metermon",
        default="metermon-rs/target/release/metermon-rs",
        help="path to the metermon-rs binary",
    )
    args = ap.parse_args()

    if not args.captures.exists():
        sys.exit(f"captures file not found: {args.captures}")
    metermon = shutil.which(args.metermon) or (
        args.metermon if Path(args.metermon).exists() else None
    )
    if not metermon:
        sys.exit(
            f"metermon-rs binary not found at {args.metermon}\n"
            "build it first: (cd metermon-rs && cargo build --release)"
        )

    have_oracle = shutil.which("wmbusmeters") is not None
    if not have_oracle:
        print(
            "NOTE: wmbusmeters not installed — showing our normalised output only.\n"
            f"      install v{PINNED_WMBUSMETERS} (see README.md) for a differential run.",
            file=sys.stderr,
        )

    keys = {}
    if args.keys and args.keys.exists():
        try:
            keys = {str(k): str(v) for k, v in json.loads(args.keys.read_text()).items()}
        except (json.JSONDecodeError, AttributeError):
            print(f"WARNING: could not parse key map {args.keys}", file=sys.stderr)

    ours_rows = run_metermon(metermon, args.captures, args.keys)
    telegrams = [t.strip() for t in args.captures.read_text().splitlines()
                 if t.strip() and not t.startswith("#")]

    ok = 0
    for i, ours in enumerate(ours_rows):
        theirs = None
        meter_id = str(ours.get("meterid", ""))
        if have_oracle and i < len(telegrams):
            theirs = run_wmbusmeters(telegrams[i], args.driver, meter_id, keys.get(meter_id))
        compare(normalise_ours(ours), normalise_theirs(theirs) if theirs else None)
        ok += 1
    print(f"\n{ok} telegram(s) compared; oracle {'present' if have_oracle else 'absent'}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
