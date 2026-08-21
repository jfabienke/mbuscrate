//! Protocol core for M-Bus, wireless M-Bus and LoRaWAN.
//!
//! Everything in this crate is **bytes in, bytes out**: parsing, decoding and
//! cryptography, with no hardware, no operating system and — deliberately — no clock.
//! It builds `no_std` with **no heap and no panics**, so the same protocol code runs on a
//! Linux gateway and on a microcontroller.
//!
//! # What belongs here, and what does not
//!
//! The rule that keeps the split honest: **a dependency that needs `std` belongs in
//! `mbus-rs`, not here.** Transports, HALs, async runtimes, filesystem and sockets all
//! live upstairs. If a function needs to know the time, it takes it as an argument.
//!
//! That last point is a real constraint rather than a stylistic one. A decoder that reads
//! the wall clock is not a pure function: it cannot be golden-tested reproducibly, and on
//! a microcontroller there may be no wall clock at all. Callers stamp; the core decodes.
//!
//! Durable state is subject to the same rule, which is why the LoRaWAN `JoinStore` lives
//! in `mbus-rs`: a store is storage. The *rules* it enforces are pure and stay here.
//!
//! # No heap, no panics
//!
//! Public types are fixed-capacity (`heapless`), and every capacity is a protocol bound
//! rather than a tuning choice — see the constants in [`lorawan`]. This is not only about
//! memory: on stable `no_std`, allocation failure *is* a panic, so as long as the API
//! allocated, no amount of care in the surrounding logic could make it panic-free. Dropping
//! the allocator removed both properties' obstacles at once.
//!
//! Neither property is asserted. `panic-probe/` links this crate for a bare-metal target
//! with a `#[panic_handler]` that references an undefined symbol, so a reachable panic
//! fails the link; CI runs it as a ratchet that fails if the state changes in either
//! direction. See `panic-probe/README.md`, which also records the two earlier versions of
//! that check that reported success while measuring nothing.
//!
//! # Targets
//!
//! Verified to build for `thumbv6m-none-eabi` (Cortex-M0+, no FPU, no atomics beyond
//! load/store) as well as the host. If it compiles for that, it will compile for anything
//! Embassy runs on.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

#[cfg(feature = "crypto")]
pub mod lorawan;
