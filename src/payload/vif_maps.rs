//! VIF/VIFE/VIB Mapping Tables and Lookup Functions
//!
//! This module provides lookups for M-Bus Value Information Fields (VIF)
//! and Value Information Field Extensions (VIFE) as defined in EN 13757-3.

use crate::payload::vif::VifInfo;

/// Primary VIF codes (0x00–0xFF) as defined in EN 13757-3 standard.
/// Validated against reference implementations for correctness.
pub const VIF_CODES: &[(u8, &str, f64, &str)] = &[
    (0x00, "Wh", 0.001, "Energy"),
    (0x01, "Wh", 0.01, "Energy"),
    (0x02, "Wh", 0.1, "Energy"),
    (0x03, "Wh", 1.0, "Energy"),
    (0x04, "Wh", 10.0, "Energy"),
    (0x05, "Wh", 100.0, "Energy"),
    (0x06, "Wh", 1000.0, "Energy"),
    (0x07, "Wh", 1e4, "Energy"),
    (0x08, "J", 1.0, "Energy"),
    (0x09, "J", 10.0, "Energy"),
    (0x0A, "J", 100.0, "Energy"),
    (0x0B, "J", 1000.0, "Energy"),
    (0x0C, "J", 1e4, "Energy"),
    (0x0D, "J", 1e5, "Energy"),
    (0x0E, "J", 1e6, "Energy"),
    (0x0F, "J", 1e7, "Energy"),
    (0x10, "m^3", 1e-6, "Volume"),
    (0x11, "m^3", 1e-5, "Volume"),
    (0x12, "m^3", 1e-4, "Volume"),
    (0x13, "m^3", 0.001, "Volume"),
    (0x14, "m^3", 0.01, "Volume"),
    (0x15, "m^3", 0.1, "Volume"),
    (0x16, "m^3", 1.0, "Volume"),
    (0x17, "m^3", 10.0, "Volume"),
    (0x18, "kg", 0.001, "Mass"),
    (0x19, "kg", 0.01, "Mass"),
    (0x1A, "kg", 0.1, "Mass"),
    (0x1B, "kg", 1.0, "Mass"),
    (0x1C, "kg", 10.0, "Mass"),
    (0x1D, "kg", 100.0, "Mass"),
    (0x1E, "kg", 1000.0, "Mass"),
    (0x1F, "kg", 1e4, "Mass"),
    (0x20, "s", 1.0, "On time"),
    (0x21, "10^-1 s", 1e-1, "On time"),
    (0x22, "10^-2 s", 1e-2, "On time"),
    (0x23, "10^-3 s", 1e-3, "On time"),
    (0x24, "s", 1.0, "Operating time"),
    (0x25, "10^-1 s", 1e-1, "Operating time"),
    (0x26, "10^-2 s", 1e-2, "Operating time"),
    (0x27, "10^-3 s", 1e-3, "Operating time"),
    (0x28, "W", 0.001, "Power"),
    (0x29, "W", 0.01, "Power"),
    (0x2A, "W", 0.1, "Power"),
    (0x2B, "W", 1.0, "Power"),
    (0x2C, "W", 10.0, "Power"),
    (0x2D, "W", 100.0, "Power"),
    (0x2E, "W", 1000.0, "Power"),
    (0x2F, "W", 1e4, "Power"),
    (0x30, "J/h", 1.0, "Power"),
    (0x31, "J/h", 10.0, "Power"),
    (0x32, "J/h", 100.0, "Power"),
    (0x33, "J/h", 1000.0, "Power"),
    (0x34, "J/h", 1e4, "Power"),
    (0x35, "J/h", 1e5, "Power"),
    (0x36, "J/h", 1e6, "Power"),
    (0x37, "J/h", 1e7, "Power"),
    (0x38, "m^3/h", 1e-6, "Volume flow"),
    (0x39, "m^3/h", 1e-5, "Volume flow"),
    (0x3A, "m^3/h", 1e-4, "Volume flow"),
    (0x3B, "m^3/h", 0.001, "Volume flow"),
    (0x3C, "m^3/h", 0.01, "Volume flow"),
    (0x3D, "m^3/h", 0.1, "Volume flow"),
    (0x3E, "m^3/h", 1.0, "Volume flow"),
    (0x3F, "m^3/h", 10.0, "Volume flow"),
    (0x40, "m^3/min", 1e-7, "Volume flow"),
    (0x41, "m^3/min", 1e-6, "Volume flow"),
    (0x42, "m^3/min", 1e-5, "Volume flow"),
    (0x43, "m^3/min", 1e-4, "Volume flow"),
    (0x44, "m^3/min", 0.001, "Volume flow"),
    (0x45, "m^3/min", 0.01, "Volume flow"),
    (0x46, "m^3/min", 0.1, "Volume flow"),
    (0x47, "m^3/min", 1.0, "Volume flow"),
    (0x48, "m^3/s", 1e-9, "Volume flow"),
    (0x49, "m^3/s", 1e-8, "Volume flow"),
    (0x4A, "m^3/s", 1e-7, "Volume flow"),
    (0x4B, "m^3/s", 1e-6, "Volume flow"),
    (0x4C, "m^3/s", 1e-5, "Volume flow"),
    (0x4D, "m^3/s", 1e-4, "Volume flow"),
    (0x4E, "m^3/s", 0.001, "Volume flow"),
    (0x4F, "m^3/s", 0.01, "Volume flow"),
    (0x50, "kg/h", 0.001, "Mass flow"),
    (0x51, "kg/h", 0.01, "Mass flow"),
    (0x52, "kg/h", 0.1, "Mass flow"),
    (0x53, "kg/h", 1.0, "Mass flow"),
    (0x54, "kg/h", 10.0, "Mass flow"),
    (0x55, "kg/h", 100.0, "Mass flow"),
    (0x56, "kg/h", 1000.0, "Mass flow"),
    (0x57, "kg/h", 1e4, "Mass flow"),
    (0x58, "C", 0.001, "Flow temperature"),
    (0x59, "C", 0.01, "Flow temperature"),
    (0x5A, "C", 0.1, "Flow temperature"),
    (0x5B, "C", 1.0, "Flow temperature"),
    (0x5C, "C", 0.001, "Return temperature"),
    (0x5D, "C", 0.01, "Return temperature"),
    (0x5E, "C", 0.1, "Return temperature"),
    (0x5F, "C", 1.0, "Return temperature"),
    (0x60, "K", 0.001, "Temperature difference"),
    (0x61, "K", 0.01, "Temperature difference"),
    (0x62, "K", 0.1, "Temperature difference"),
    (0x63, "K", 1.0, "Temperature difference"),
    (0x64, "C", 0.001, "External temperature"),
    (0x65, "C", 0.01, "External temperature"),
    (0x66, "C", 0.1, "External temperature"),
    (0x67, "C", 1.0, "External temperature"),
    (0x68, "bar", 0.001, "Pressure"),
    (0x69, "bar", 0.01, "Pressure"),
    (0x6A, "bar", 0.1, "Pressure"),
    (0x6B, "bar", 1.0, "Pressure"),
    (0x6C, "-", 1.0, "Time point (date)"),
    (0x6D, "-", 1.0, "Time point (date & time)"),
    (0x6E, "Units for H.C.A.", 1.0, "H.C.A."),
    (0x6F, "Reserved", 0.0, "Reserved"),
    (0x70, "s", 1.0, "Averaging Duration"),
    (0x71, "10^-1 s", 1e-1, "Averaging Duration"),
    (0x72, "10^-2 s", 1e-2, "Averaging Duration"),
    (0x73, "10^-3 s", 1e-3, "Averaging Duration"),
    (0x74, "s", 1.0, "Actuality Duration"),
    (0x75, "10^-1 s", 1e-1, "Actuality Duration"),
    (0x76, "10^-2 s", 1e-2, "Actuality Duration"),
    (0x77, "10^-3 s", 1e-3, "Actuality Duration"),
    (0x78, "", 1.0, "Fabrication No"),
    (0x79, "", 1.0, "(Enhanced) Identification"),
    (0x7A, "", 1.0, "Bus Address"),
    (0x7B, "", 1.0, "Any VIF"),
    (0x7C, "", 1.0, "Any VIF"),
    (0x7D, "", 1.0, "Any VIF"),
    (0x7E, "", 1.0, "Any VIF"),
    (0x7F, "", 1.0, "Manufacturer specific"),
    (0xFE, "", 1.0, "Any VIF"),
    (0xFF, "", 1.0, "Manufacturer specific"),
];

/// VIFE codes for FD extension as specified in EN 13757-3.
/// These extensions provide additional information for primary VIF codes.
pub const VIFE_FD_CODES: &[(u8, &str, f64, &str)] = &[
    (
        0x00,
        "Credit of 10nn-3 of the nominal local legal currency units",
        0.0,
        "Credit",
    ), // nn from status
    (
        0x04,
        "Debit of 10nn-3 of the nominal local legal currency units",
        0.0,
        "Debit",
    ),
    (
        0x08,
        "Access Number (transmission count)",
        1.0,
        "Transmission Count",
    ),
    (0x09, "Medium (as in fixed header)", 1.0, "Medium"),
    (
        0x0A,
        "Manufacturer (as in fixed header)",
        1.0,
        "Manufacturer",
    ),
    (0x0B, "Parameter set identification", 1.0, "Parameter Set"),
    (0x0C, "Model / Version", 1.0, "Model/Version"),
    (0x0D, "Hardware version #", 1.0, "Hardware Version"),
    (0x0E, "Firmware version #", 1.0, "Firmware Version"),
    (0x0F, "Software version #", 1.0, "Software Version"),
    (0x10, "Customer location", 1.0, "Customer Location"),
    (0x11, "Customer", 1.0, "Customer"),
    (0x12, "Access Code User", 1.0, "Access Code User"),
    (0x13, "Access Code Operator", 1.0, "Access Code Operator"),
    (
        0x14,
        "Access Code System Operator",
        1.0,
        "Access Code System Operator",
    ),
    (0x15, "Access Code Developer", 1.0, "Access Code Developer"),
    (0x16, "Password", 1.0, "Password"),
    (0x17, "Error flags", 1.0, "Error Flags"),
    (0x18, "Error mask", 1.0, "Error Mask"),
    (0x19, "Reserved", 1.0, "Reserved"),
    (0x1A, "Digital output (binary)", 1.0, "Digital Output"),
    (0x1B, "Digital input (binary)", 1.0, "Digital Input"),
    (0x1C, "Baudrate", 1.0, "Baudrate"),
    (0x1D, "response delay time", 1.0, "Response Delay"),
    (0x1E, "Retry", 1.0, "Retry"),
    (0x1F, "Reserved", 1.0, "Reserved"),
    (
        0x20,
        "First storage # for cyclic storage",
        1.0,
        "First Storage",
    ),
    (
        0x21,
        "Last storage # for cyclic storage",
        1.0,
        "Last Storage",
    ),
    (0x22, "Size of storage block", 1.0, "Storage Block Size"),
    (0x23, "Reserved", 1.0, "Reserved"),
    // For 0x24-0x27: Storage interval, handled separately
    (0x28, "Storage interval month(s)", 1.0, "Storage Interval"),
    (0x29, "Storage interval year(s)", 1.0, "Storage Interval"),
    (0x2A, "Reserved", 1.0, "Reserved"),
    (0x2B, "Reserved", 1.0, "Reserved"),
    // For 0x2C-0x2F: Duration since last readout, handled separately
    (0x30, "Start (date/time) of tariff", 1.0, "Tariff Start"),
    // For 0x30-0x3B: Duration/Period of tariff, handled separately
    (0x3A, "dimensionless / no VIF", 1.0, "Dimensionless"),
    (0x3B, "Reserved", 1.0, "Reserved"),
    // For 0x3C: Reserved, handled separately
    // For 0x40-0x4F: Voltage, handled separately
    // For 0x50-0x5F: Current, handled separately
    (0x60, "Reset counter", 1.0, "Reset Counter"),
    (0x61, "Cumulation counter", 1.0, "Cumulation Counter"),
    (0x62, "Control signal", 1.0, "Control Signal"),
    (0x63, "Day of week", 1.0, "Day of Week"),
    (0x64, "Week number", 1.0, "Week Number"),
    (0x65, "Time point of day change", 1.0, "Day Change Time"),
    (
        0x66,
        "State of parameter activation",
        1.0,
        "Parameter Activation",
    ),
    (0x67, "Special supplier information", 1.0, "Supplier Info"),
    // For 0x68-0x6F: Duration since last cumulation, handled separately
    (
        0x70,
        "Date and time of battery change",
        1.0,
        "Battery Change Date",
    ),
    // For 0x70-0x7F: Reserved, handled separately
];

/// VIFE codes for FB extension as specified in EN 13757-3.
/// These extensions handle special manufacturer and error conditions.
pub const VIFE_FB_CODES: &[(u8, &str, f64, &str)] = &[
    // For 0x40-0x4F: Voltage 10^(nnnn-9) V
    // For 0x50-0x5F: Current 10^(nnnn-12) A
    // For 0x70-0x77: External Temperature
    // For 0x74-0x77: Cold/Warm Temperature Limit
    // For 0x78-0x7F: Cumulative count max power
    // These are handled by calculating exponent from code
];

/// Looks up primary VIF code.
pub fn lookup_primary_vif(code: u8) -> Option<VifInfo> {
    VIF_CODES
        .iter()
        .find(|(c, _, _, _)| *c == code)
        .map(|(_, unit, exponent, quantity)| VifInfo {
            vif: code as u16,
            unit,
            exponent: *exponent,
            quantity,
        })
}

/// Looks up VIFE FD extension code.
pub fn lookup_vife_fd(code: u8) -> Option<VifInfo> {
    // Algorithmic ranges first: the table above only lists the discrete codes and
    // marks these "handled separately", which previously meant not handled at all —
    // a voltage record decoded to the right number with no quantity and no unit.
    // EN 13757-3 defines them as exponent ranges, not table entries.
    if let Some(info) = vife_fd_algorithmic(code) {
        return Some(info);
    }
    VIFE_FD_CODES
        .iter()
        .find(|(c, _, _, _)| *c == code)
        .map(|(_, unit, exponent, quantity)| VifInfo {
            vif: 0x100u16 + code as u16,
            unit,
            exponent: *exponent,
            quantity,
        })
}

/// The exponent-bearing FD ranges: voltage `0x40-0x4F` as 10^(nnnn-9) V and
/// current `0x50-0x5F` as 10^(nnnn-12) A.
fn vife_fd_algorithmic(code: u8) -> Option<VifInfo> {
    let nnnn = (code & 0x0F) as i32;
    let (unit, exponent, quantity) = match code {
        0x40..=0x4F => ("V", 10f64.powi(nnnn - 9), "Voltage"),
        0x50..=0x5F => ("A", 10f64.powi(nnnn - 12), "Current"),
        _ => return None,
    };
    Some(VifInfo {
        vif: 0x100u16 + code as u16,
        unit,
        exponent,
        quantity,
    })
}

/// Looks up VIFE FB extension code.
pub fn lookup_vife_fb(code: u8) -> Option<VifInfo> {
    VIFE_FB_CODES
        .iter()
        .find(|(c, _, _, _)| *c == code)
        .map(|(_, unit, exponent, quantity)| VifInfo {
            vif: 0x200u16 + code as u16,
            unit,
            exponent: *exponent,
            quantity,
        })
}

#[cfg(test)]
mod vife_fd_range_tests {
    use super::*;

    #[test]
    fn fd_voltage_and_current_ranges_carry_unit_and_exponent() {
        // 0xFD 0x46: nnnn = 6, so 10^(6-9) V = millivolts. A meter's battery record.
        let v = lookup_vife_fd(0x46).expect("voltage range must resolve");
        assert_eq!(v.unit, "V");
        assert_eq!(v.quantity, "Voltage");
        assert!((v.exponent - 1e-3).abs() < 1e-15);

        // 0xFD 0x59: nnnn = 9, so 10^(9-12) A = milliamps.
        let i = lookup_vife_fd(0x59).expect("current range must resolve");
        assert_eq!(i.unit, "A");
        assert_eq!(i.quantity, "Current");
        assert!((i.exponent - 1e-3).abs() < 1e-15);

        // The discrete entries outside those ranges must still resolve.
        assert_eq!(
            lookup_vife_fd(0x61).map(|x| x.quantity),
            Some("Cumulation Counter")
        );
    }
}
