//! The payload module contains the components responsible for decoding and processing
//! the data payload of the M-Bus protocol.

pub mod data;
pub mod data_encoding;
pub mod record;
pub mod vif;
pub mod vif_maps;

pub use data::mbus_data_record_decode;
pub use data_encoding::*;
pub use record::*;
pub use vif::*;
pub use vif_maps::*;

/// Represents a data record in the M-Bus protocol.
pub use record::MBusRecord;

/// Represents the value of an M-Bus data record.
pub use record::MBusRecordValue;
