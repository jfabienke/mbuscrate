//! Radio Manager: the layer that owns the *system*, where the driver owns the *chip*.
//!
//! The seam, agreed with the driver authors: **if a decision needs information the driver
//! structurally cannot have, it belongs here.** An SX1262 driver cannot know an SX1302 is
//! transmitting; it cannot know the regulatory budget spent by another radio; it cannot know
//! that two meters at −34 dBm read 65 % CRC because of a Qundis framing quirk rather than a
//! bad link. All of that is system knowledge.
//!
//! Today that role was performed by a human over SSH — stopping metering, checking `lsof`,
//! watching for D-state, sequencing three sessions' radio requests. The wedge that once
//! needed a power cycle is what its absence costs.
//!
//! * [`blanking`] — which received frames our own transmitter corrupted, and how much of a
//!   silence the gateway caused itself.
//! * [`duty`] — EU868 duty-cycle budget per sub-band, across every radio.
//! * [`attribution`] — whether a reception symptom is actually an RF problem, before
//!   anything adapts to it.

pub mod attribution;
pub mod blanking;
pub mod duty;
