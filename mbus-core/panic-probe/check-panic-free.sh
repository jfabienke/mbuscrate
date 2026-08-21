#!/usr/bin/env bash
#
# Measure whether mbus-core can panic, by making the LINKER the detector.
#
# How it works: panic-probe's `#[panic_handler]` calls an undefined extern symbol. If any
# reachable path in mbus-core can panic, the handler is live and the link fails on that
# symbol. If every panic path is eliminated, the handler is dead code, --gc-sections drops
# it, and the link succeeds. A failing link is the RESULT, not a build break -- never
# "fix" it by defining the symbol.
#
# This is a ratchet, not a pass/fail: it compares reality against EXPECT_PANIC_FREE below
# and fails if they differ *in either direction*, so the state cannot regress silently and
# an improvement cannot land without updating the record.
#
# ---------------------------------------------------------------------------------------
# Two earlier versions of this check were WRONG and both reported success:
#
#  1. Scanning the binary for panic message strings. A #[panic_handler] that ignores its
#     PanicInfo lets LLVM drop the message formatting, so a deliberately-injected
#     out-of-bounds index left no strings behind and the check passed. Verified by
#     negative control -- which is the only reason it was caught.
#  2. Before that, the probe never called the code under test, so the linker GC'd
#     everything: 732 bytes, zero symbols, a clean result measuring nothing.
#
# If you change this script, re-run the negative control at the bottom.
# ---------------------------------------------------------------------------------------
set -uo pipefail
cd "$(dirname "$0")"

# Current recorded state. 0 = mbus-core can still panic; 1 = proven panic-free.
# Flip to 1 in the same commit that makes it true (see README: this requires heapless).
EXPECT_PANIC_FREE=0

TARGET=thumbv6m-none-eabi
BIN="target/$TARGET/release/mbus-core-panic-probe"
rm -f "$BIN"
OUT=$(cargo build --target "$TARGET" --release 2>&1)

if echo "$OUT" | grep -q "undefined symbol: MBUS_CORE_CAN_PANIC"; then
    ACTUAL=0; DESC="mbus-core CAN panic (handler is reachable)"
elif [ -f "$BIN" ]; then
    ACTUAL=1; DESC="mbus-core is panic-free ($(wc -c < "$BIN" | tr -d ' ') B linked)"
else
    # Neither outcome: the probe failed to build. This is INCONCLUSIVE, and must never be
    # reported as panic-free -- an earlier harness did exactly that.
    echo "INCONCLUSIVE: probe did not build, so nothing was measured"
    echo "$OUT" | grep -m5 '^error' || echo "$OUT" | tail -5
    exit 2
fi

echo "$DESC"
if [ "$ACTUAL" != "$EXPECT_PANIC_FREE" ]; then
    echo "FAIL: recorded state is EXPECT_PANIC_FREE=$EXPECT_PANIC_FREE, measured $ACTUAL."
    [ "$ACTUAL" = 0 ] && echo "  -> a panic path was reintroduced." \
                      || echo "  -> panic-freedom achieved; update EXPECT_PANIC_FREE to 1."
    exit 1
fi
echo "PASS: matches the recorded state (EXPECT_PANIC_FREE=$EXPECT_PANIC_FREE)"
