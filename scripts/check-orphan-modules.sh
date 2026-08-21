#!/usr/bin/env bash
#
# Fail if any src/**/*.rs is not declared somewhere in the module tree.
#
# Why this exists: Rust gives *no* feedback for a file that isn't in a `mod` tree. It is
# not dead code (which at least gets a `dead_code` lint) — it is simply not part of the
# crate. No compile error, no clippy warning, no failing test, no coverage delta. rustc
# never opens the file. Six such files accumulated here over eleven months, one of them
# empty and one beginning with an unexecuted instruction comment, and nothing ever
# objected. Two of the six no longer compiled when wired in.
#
# What this checks: *declaration*, not compilation. A module behind `#[cfg(feature=...)]`
# is declared and legitimate even though a default build never reads it — checking
# "was it compiled" would flag every feature-gated file (raspberry_pi.rs, rfm69.rs, the
# defmt writers) and be useless.
#
# Handles `mod foo;`, `pub mod foo;`, and `#[path = "foo.rs"] mod bar;` — the last of
# which broke two hand-rolled versions of this check: modulation_tests.rs was reported as
# orphaned when in fact it is declared via #[path] and its 8 tests run on every build.
set -uo pipefail
cd "$(dirname "$0")/.."

# Entry points and directory modules are declared by their location, not by a `mod` line.
EXEMPT_BASENAMES='lib.rs|main.rs|mod.rs|build.rs'

fail=0
while IFS= read -r file; do
    base=$(basename "$file")
    [[ "$base" =~ ^($EXEMPT_BASENAMES)$ ]] && continue
    stem="${base%.rs}"

    # `mod stem;` / `pub mod stem;` / `pub(crate) mod stem;`  — or a #[path] naming the file.
    if grep -rqE "^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?mod[[:space:]]+${stem}[[:space:]]*;" src/ \
       || grep -rqF "path = \"${base}\"" src/ \
       || grep -rqF "/${base}\"" src/; then
        continue
    fi
    echo "ORPHAN: $file is not declared in any module tree"
    fail=1
done < <(find src -name '*.rs' | sort)

if [ "$fail" -ne 0 ]; then
    cat <<'EOF'

An undeclared .rs file is invisible to rustc: it is never compiled, linted or tested,
and it rots silently against API drift. Either declare it (`mod <name>;`, behind a
`#[cfg(feature = ...)]` if that is what it needs), or delete it — git keeps the content
either way, so deleting loses nothing that a commit message cannot point back to.
EOF
    exit 1
fi
echo "PASS: every src/**/*.rs is declared in the module tree"
