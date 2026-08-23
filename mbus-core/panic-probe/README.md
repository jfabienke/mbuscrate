# panic-probe

Links `mbus-core` into a bare-metal binary (`thumbv6m-none-eabi`) so that two properties
can be **measured** rather than asserted:

1. whether the core needs a heap, and
2. whether any reachable path can panic.

Run `./check-panic-free.sh`. It is a **ratchet**: it compares reality against the recorded
`EXPECT_PANIC_FREE` and fails if they differ in *either* direction, so the state cannot
regress silently and an improvement cannot land without updating the record.

## Status: panic-free and heap-free — now across the FULL decode pipeline

As of 2026-08-23 the probe exercises the entire wM-Bus + M-Bus path: `packet_size`,
`parse_wmbus_frame`, `decode_mode_c`, `verify_blocks`, `parse_variable_record_consumed`
and `parse_fixed_record`. So `EXPECT_PANIC_FREE=1` now means the whole decoder — the most
complex untrusted-input handling in the crate, where an out-of-bounds write was found and
fixed this session — provably cannot panic on ANY byte sequence. That protects the Linux
gateway as much as a bare-metal target: a hostile meter frame cannot DoS the parser.

Getting there required replacing nom's runtime-length `take` with `split_at_checked`
(nom's `take` keeps a `split_at` panic branch the linker cannot eliminate) and binding
fixed-size header windows so the compiler proves each index in bounds.

### Original status (2026-08-21)

The core links for `thumbv6m-none-eabi` with **no global allocator and no reachable panic
path**. `EXPECT_PANIC_FREE=1`. The bump allocator this probe used to carry is gone — it
existed only to make the heap dependency visible, and there is no longer one to show.

Verified three ways rather than asserted: the trap symbol is present in the handler, the
code under test is genuinely linked (`.text` ≈ 8 kB, not a GC'd stub), and an injected
out-of-bounds index still fails the link.

## How it was reached: panic-freedom and no-heap are the same task

Measured 2026-08-21 by bisecting the exercised entry points:

| exercised path | result |
|---|---|
| empty body (control) | panic-free |
| `parse_link_adr_ans` — no allocation | panic-free |
| `JoinRequest::parse` | **can panic** |
| `DataFrame::parse` | **can panic** |
| a bare `Vec::push`, nothing else | **can panic** |

That last row was the whole answer. On stable `no_std` + `alloc`, allocation failure is a
panic by construction: `Vec` growth calls `handle_alloc_error`, which is divergent and
pulls the panic handler in. **Every `Vec`-returning function in the public API was
therefore a panic path**, however carefully its own logic avoided indexing or unwrapping.

So the two remaining items on the no_std plan were never independent:

* the hand-written panic sites removed earlier (`new_from_slice().expect(..)`, the
  `FOptsLen` overflow) were real and worth removing, but they were never the binding
  constraint;
* **panic-freedom was unreachable until the API stopped allocating.**

Replacing `Vec`/`String` with fixed-capacity `heapless` types removed the allocator, and
the panic paths went with it. One change, both properties.

## Two earlier versions of this check were wrong, and both reported success

Recorded because the failure mode is more instructive than the result:

1. **The probe never called the code under test.** `_start` was a bare `loop {}`, so the
   linker garbage-collected everything: 732 bytes, zero symbols, "no panic symbols found".
   A clean result that measured nothing. Fixed by having `_start` actually call `exercise`
   on a `static` input — the binary went from 732 B to 8.8 kB, which is how the emptiness
   was noticed at all.

2. **Scanning the linked binary for panic message strings.** A `#[panic_handler]` that
   ignores its `PanicInfo` lets LLVM drop the message formatting entirely, so a
   *deliberately injected out-of-bounds index* left no strings behind and the check passed.
   Caught only by running a negative control.

3. A third, in the bisect harness: it detected panics by grepping build output for the
   undefined symbol, so a **compile error** also failed to match and was reported as
   "panic-free". This produced one wrong row ("bare `Vec` push → panic-free") that inverted
   the conclusion, until the harness was changed to require a linked binary on disk.

The common shape: *an absent signal read as a negative result.* Hence the `exit 2`
INCONCLUSIVE path in the script — "did not build" must never be reported as "did not panic".

**If you change the script, re-run the negative controls**: claim `EXPECT_PANIC_FREE=1`
while a panic path exists (must fail), and break the probe's compilation (must report
INCONCLUSIVE, exit 2).

## Why the linker, not the binary

`#[panic_handler]` calls an undefined `extern` symbol. If any reachable path can panic the
handler is live and the link fails on that symbol; if not, it is dead code, `--gc-sections`
drops it, and the link succeeds. This is the `panic-never` technique and it does not depend
on symbol names, strings, or debug info surviving.

A failing link is the **result**, not a build break. Do not fix it by defining the symbol.
