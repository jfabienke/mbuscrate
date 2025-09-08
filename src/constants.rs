//! M-Bus Protocol Constants
//!
//! This module defines constants used in the M-Bus protocol implementation,
//! based on the EN 13757 standard.

/// DIF (Data Information Field) mask for data length
pub const MBUS_DATA_RECORD_DIF_MASK_DATA: u8 = 0x0F;

/// DIF mask for function
pub const MBUS_DATA_RECORD_DIF_MASK_FUNCTION: u8 = 0x30;

/// DIF mask for storage number
pub const MBUS_DATA_RECORD_DIF_MASK_STORAGE_NO: u8 = 0x40;

/// DIFE (Data Information Field Extension) mask for storage number
pub const MBUS_DATA_RECORD_DIFE_MASK_STORAGE_NO: u8 = 0x0F;

/// DIFE mask for tariff
pub const MBUS_DATA_RECORD_DIFE_MASK_TARIFF: u8 = 0x30;

/// DIFE mask for device
pub const MBUS_DATA_RECORD_DIFE_MASK_DEVICE: u8 = 0x40;

/// DIF idle filler
pub const MBUS_DIB_DIF_IDLE_FILLER: u8 = 0x2F;

/// DIF manufacturer specific
pub const MBUS_DIB_DIF_MANUFACTURER_SPECIFIC: u8 = 0x0F;

/// DIF more records follow
pub const MBUS_DIB_DIF_MORE_RECORDS_FOLLOW: u8 = 0x1F;

/// DIF extension bit
pub const MBUS_DIB_DIF_EXTENSION_BIT: u8 = 0x80;

/// VIF without extension
pub const MBUS_DIB_VIF_WITHOUT_EXTENSION: u8 = 0x7F;

/// VIF extension bit
pub const MBUS_DIB_VIF_EXTENSION_BIT: u8 = 0x80;

/// Custom VIF size
pub const MBUS_VALUE_INFO_BLOCK_CUSTOM_VIF_SIZE: u8 = 16;

// ----------------------------------------------------------------------------
// Frame/control/CI constants (aligned with libmbus)
// ----------------------------------------------------------------------------

/// Network layer (secondary addressing) broadcast address
pub const MBUS_ADDRESS_NETWORK_LAYER: u8 = 0xFD;

// Control masks (full control bytes for common commands)
pub const MBUS_CONTROL_MASK_SND_NKE: u8 = 0x40;
pub const MBUS_CONTROL_MASK_SND_UD: u8 = 0x53; // includes DIR M2S
pub const MBUS_CONTROL_MASK_REQ_UD2: u8 = 0x5B; // includes DIR M2S
pub const MBUS_CONTROL_MASK_REQ_UD1: u8 = 0x5A; // includes DIR M2S
pub const MBUS_CONTROL_MASK_RSP_UD: u8 = 0x08;  // S2M response

// Control flag bits
pub const MBUS_CONTROL_MASK_FCB: u8 = 0x20;
pub const MBUS_CONTROL_MASK_FCV: u8 = 0x10;
pub const MBUS_CONTROL_MASK_DIR_M2S: u8 = 0x40;
pub const MBUS_CONTROL_MASK_DIR_S2M: u8 = 0x00;

// Control information (CI) codes
pub const MBUS_CONTROL_INFO_DATA_SEND: u8 = 0x51;
pub const MBUS_CONTROL_INFO_SELECT_SLAVE: u8 = 0x52;
pub const MBUS_CONTROL_INFO_RESP_VARIABLE: u8 = 0x72;
pub const MBUS_CONTROL_INFO_RESP_FIXED: u8 = 0x73;

// Fixed data constants
pub const MBUS_DATA_FIXED_LENGTH: usize = 16;
pub const MBUS_DATA_FIXED_STATUS_FORMAT_MASK: u8 = 0x80;
pub const MBUS_DATA_FIXED_STATUS_FORMAT_BCD: u8 = 0x00;
pub const MBUS_DATA_FIXED_STATUS_FORMAT_INT: u8 = 0x80;
pub const MBUS_DATA_FIXED_STATUS_DATE_MASK: u8 = 0x40;
pub const MBUS_DATA_FIXED_STATUS_DATE_STORED: u8 = 0x40;
pub const MBUS_DATA_FIXED_STATUS_DATE_CURRENT: u8 = 0x00;
