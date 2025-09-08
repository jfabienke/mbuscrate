//! # Wireless M-Bus (wM-Bus) Module
//!
//! This module provides the necessary functionality for handling the wireless
//! M-Bus (wM-Bus) protocol, which is an extension of the wired M-Bus protocol
//! for wireless communication with utility meters.
//!
pub mod encryption;
pub mod frame;
pub mod handle;
pub mod network;

// Re-export the necessary types and functions from the submodules
pub use encryption::WMBusEncryption;
pub use frame::WMBusFrame;
pub use handle::WMBusHandle;
pub use network::WMBusNetwork;
