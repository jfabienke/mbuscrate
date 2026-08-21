//! Protocol core for M-Bus, wireless M-Bus and LoRaWAN.
//!
//! Everything in this crate is **bytes in, bytes out**: parsing, decoding and
//! cryptography, with no hardware, no operating system and — deliberately — no clock.
//! It builds `no_std` against `alloc`, so the same protocol code runs on a Linux gateway
//! and on a microcontroller.
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
//! # Targets
//!
//! Verified to build for `thumbv6m-none-eabi` (Cortex-M0+, no FPU, no atomics beyond
//! load/store) as well as the host. If it compiles for that, it will compile for anything
//! Embassy runs on.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(feature = "crypto")]
pub mod lorawan;
