#!/usr/bin/env bash
#
# The single mandatory check before any push. Run this LAST — not an earlier partial
# run, not "I tested the crate I edited." Every master breakage this project has had came
# from verification whose scope was narrower than the change's: a blanket edit that
# reached a crate the tests didn't, or a final check that ran one workspace's suite while
# the regression sat in another.
#
# It covers every crate and every property a push can break EXCEPT the Linux-only radio
# path in metermon-rs, which cannot build on macOS — that half runs on the Pi (see the
# RADIO note printed at the end). A green preflight plus a green Pi radio gate is the
# complete gate; neither alone is.
set -uo pipefail
cd "$(dirname "$0")/.."
fail=0
step() { printf '\n=== %s ===\n' "$1"; }
check() { if [ "$1" -ne 0 ]; then echo "FAIL: $2"; fail=1; fi; }

step "mbus-rs tests (all crates in the workspace)"
cargo test --quiet 2>&1 | tail -3; check "${PIPESTATUS[0]}" "mbus-rs tests"

step "mbus-core tests (its OWN suite — where the type is defined)"
cargo test --quiet -p mbus-core 2>&1 | tail -3; check "${PIPESTATUS[0]}" "mbus-core tests"

step "mbus-core bare-metal (no_std, no alloc) — catches heap/panic the host hides"
cargo build --quiet -p mbus-core --target thumbv6m-none-eabi --no-default-features
check $? "thumbv6m no-default"
cargo build --quiet -p mbus-core --target thumbv6m-none-eabi --no-default-features --features crypto
check $? "thumbv6m crypto"

step "panic-freedom ratchet"
./mbus-core/panic-probe/check-panic-free.sh; check $? "panic ratchet"

step "no orphaned modules"
./scripts/check-orphan-modules.sh; check $? "orphan check"

step "clippy, all targets, both workspaces"
cargo clippy --quiet --all-targets 2>&1 | grep -E "^(warning|error)" | head; \
  [ "$(cargo clippy --all-targets 2>&1 | grep -cE '^warning:|^error')" -eq 0 ]; check $? "root clippy"
( cd metermon-rs && [ "$(cargo clippy --all-targets 2>&1 | grep -cE '^warning:|^error')" -eq 0 ] ); check $? "metermon clippy (host)"

step "metermon-rs host build (it is OUTSIDE the workspace — root tests never touch it)"
( cd metermon-rs && cargo build --quiet ); check $? "metermon host build"

step "size guard + fmt"
cargo test --quiet --test size_check 2>&1 | tail -1; check "${PIPESTATUS[0]}" "size guard"
cargo fmt --all -- --check; check $? "root fmt"
( cd metermon-rs && cargo fmt --all -- --check ); check $? "metermon fmt"

echo
if [ "$fail" -eq 0 ]; then
  echo "PREFLIGHT PASS."
  echo "RADIO: if this change touches the mbus-rs public API, ALSO run on the Pi before pushing:"
  echo "  cargo clippy --features radio --all-targets -- -D warnings  &&  cargo test --features radio"
  echo "  (metermon's radio-gated code is invisible to every check above.)"
else
  echo "PREFLIGHT FAIL — do not push."
  exit 1
fi
