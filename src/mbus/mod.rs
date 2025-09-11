//! The mbus module contains the components responsible for the core M-Bus protocol
//! implementation, including frame parsing and packing, as well as serial communication.

pub mod frame;
pub mod mbus_protocol;
pub mod secondary_addressing;
pub mod serial;

#[cfg(test)]
pub mod serial_mock;
#[cfg(test)]
pub mod serial_testable;

pub use frame::*;
pub use mbus_protocol::*;
pub use secondary_addressing::*;
pub use serial::*;

/// Represents an M-Bus frame.
pub use frame::MBusFrame;

/// Represents the different types of M-Bus frames.
pub use frame::MBusFrameType;
