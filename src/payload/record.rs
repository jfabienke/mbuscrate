use crate::constants::*;
use crate::error::MBusError;
use crate::payload::data_encoding::mbus_data_str_decode;
use crate::vendors;
use nom::{bytes::complete::take, number::complete::be_u8, IResult};
use std::time::SystemTime;

/// Represents an M-Bus data record.
#[derive(Debug)]
pub struct MBusRecord {
    pub timestamp: SystemTime,
    pub storage_number: u32,
    pub tariff: i32,
    pub device: i32,
    pub is_numeric: bool,
    pub value: MBusRecordValue,
    pub unit: String,
    pub function_medium: String,
    pub quantity: String,
    pub drh: MBusDataRecordHeader,
    pub data_len: usize,
    pub data: [u8; 256],
    pub more_records_follow: u8,
}

/// Represents the M-Bus data record header.
#[derive(Debug)]
pub struct MBusDataRecordHeader {
    pub dib: MBusDataInformationBlock,
    pub vib: MBusValueInformationBlock,
}

/// Represents the M-Bus data information block.
#[derive(Debug)]
pub struct MBusDataInformationBlock {
    pub dif: u8,
    pub ndife: usize,
    pub dife: [u8; 10],
}

/// Represents the M-Bus value information block.
#[derive(Debug)]
pub struct MBusValueInformationBlock {
    pub vif: u8,
    pub nvife: usize,
    pub vife: [u8; 10],
    pub custom_vif: String,
}

/// Represents the value of an M-Bus data record.
#[derive(Debug, Clone)]
pub enum MBusRecordValue {
    Numeric(f64),
    String(String),
}

// Constants for fixed-length medium units (based on M-Bus spec)
#[allow(dead_code)]
const FIXED_MEDIUM_UNITS: &[(u8, &str, f64, &str)] = &[
    (0x00, "Wh", 1e-3, "Energy"),
    (0x01, "10^-1 Wh", 1e-4, "Energy"),
    (0x02, "10^-2 Wh", 1e-5, "Energy"),
    (0x03, "10^-3 Wh", 1e-6, "Energy"),
    (0x04, "10^-4 Wh", 1e-7, "Energy"),
    (0x05, "10^-5 Wh", 1e-8, "Energy"),
    (0x06, "10^-6 Wh", 1e-9, "Energy"),
    (0x07, "10^-7 Wh", 1e-10, "Energy"),
    (0x08, "J", 1e0, "Energy"),
    (0x09, "10^-1 J", 1e-1, "Energy"),
    (0x0A, "10^-2 J", 1e-2, "Energy"),
    (0x0B, "10^-3 J", 1e-3, "Energy"),
    (0x0C, "10^-4 J", 1e-4, "Energy"),
    (0x0D, "10^-5 J", 1e-5, "Energy"),
    (0x0E, "10^-6 J", 1e-6, "Energy"),
    (0x0F, "10^-7 J", 1e-7, "Energy"),
    (0x10, "m^3", 1e-6, "Volume"),
    (0x11, "10^-1 m^3", 1e-7, "Volume"),
    (0x12, "10^-2 m^3", 1e-8, "Volume"),
    (0x13, "10^-3 m^3", 1e-9, "Volume"),
    (0x14, "10^-4 m^3", 1e-10, "Volume"),
    (0x15, "10^-5 m^3", 1e-11, "Volume"),
    (0x16, "10^-6 m^3", 1e-12, "Volume"),
    (0x17, "10^-7 m^3", 1e-13, "Volume"),
    (0x18, "kg", 1e-3, "Mass"),
    (0x19, "10^-1 kg", 1e-4, "Mass"),
    (0x1A, "10^-2 kg", 1e-5, "Mass"),
    (0x1B, "10^-3 kg", 1e-6, "Mass"),
    (0x1C, "10^-4 kg", 1e-7, "Mass"),
    (0x1D, "10^-5 kg", 1e-8, "Mass"),
    (0x1E, "10^-6 kg", 1e-9, "Mass"),
    (0x1F, "10^-7 kg", 1e-10, "Mass"),
    (0x20, "s", 1.0, "On time"),
    (0x21, "10^-1 s", 1e-1, "On time"),
    (0x22, "10^-2 s", 1e-2, "On time"),
    (0x23, "10^-3 s", 1e-3, "On time"),
    (0x24, "s", 1.0, "Operating time"),
    (0x25, "10^-1 s", 1e-1, "Operating time"),
    (0x26, "10^-2 s", 1e-2, "Operating time"),
    (0x27, "10^-3 s", 1e-3, "Operating time"),
    (0x28, "W", 1e-3, "Power"),
    (0x29, "10^-1 W", 1e-4, "Power"),
    (0x2A, "10^-2 W", 1e-5, "Power"),
    (0x2B, "10^-3 W", 1e-6, "Power"),
    (0x2C, "10^-4 W", 1e-7, "Power"),
    (0x2D, "10^-5 W", 1e-8, "Power"),
    (0x2E, "10^-6 W", 1e-9, "Power"),
    (0x2F, "10^-7 W", 1e-10, "Power"),
    (0x30, "J/h", 1e0, "Power"),
    (0x31, "10^-1 J/h", 1e-1, "Power"),
    (0x32, "10^-2 J/h", 1e-2, "Power"),
    (0x33, "10^-3 J/h", 1e-3, "Power"),
    (0x34, "10^-4 J/h", 1e-4, "Power"),
    (0x35, "10^-5 J/h", 1e-5, "Power"),
    (0x36, "10^-6 J/h", 1e-6, "Power"),
    (0x37, "10^-7 J/h", 1e-7, "Power"),
    (0x38, "m^3/h", 1e-6, "Volume flow"),
    (0x39, "10^-1 m^3/h", 1e-7, "Volume flow"),
    (0x3A, "10^-2 m^3/h", 1e-8, "Volume flow"),
    (0x3B, "10^-3 m^3/h", 1e-9, "Volume flow"),
    (0x3C, "10^-4 m^3/h", 1e-10, "Volume flow"),
    (0x3D, "10^-5 m^3/h", 1e-11, "Volume flow"),
    (0x3E, "10^-6 m^3/h", 1e-12, "Volume flow"),
    (0x3F, "10^-7 m^3/h", 1e-13, "Volume flow"),
    (0x40, "m^3/min", 1e-7, "Volume flow"),
    (0x41, "10^-1 m^3/min", 1e-8, "Volume flow"),
    (0x42, "10^-2 m^3/min", 1e-9, "Volume flow"),
    (0x43, "10^-3 m^3/min", 1e-10, "Volume flow"),
    (0x44, "10^-4 m^3/min", 1e-11, "Volume flow"),
    (0x45, "10^-5 m^3/min", 1e-12, "Volume flow"),
    (0x46, "10^-6 m^3/min", 1e-13, "Volume flow"),
    (0x47, "10^-7 m^3/min", 1e-14, "Volume flow"),
    (0x48, "m^3/s", 1e-9, "Volume flow"),
    (0x49, "10^-1 m^3/s", 1e-10, "Volume flow"),
    (0x4A, "10^-2 m^3/s", 1e-11, "Volume flow"),
    (0x4B, "10^-3 m^3/s", 1e-12, "Volume flow"),
    (0x4C, "10^-4 m^3/s", 1e-13, "Volume flow"),
    (0x4D, "10^-5 m^3/s", 1e-14, "Volume flow"),
    (0x4E, "10^-6 m^3/s", 1e-15, "Volume flow"),
    (0x4F, "10^-7 m^3/s", 1e-16, "Volume flow"),
    (0x50, "kg/h", 1e-3, "Mass flow"),
    (0x51, "10^-1 kg/h", 1e-4, "Mass flow"),
    (0x52, "10^-2 kg/h", 1e-5, "Mass flow"),
    (0x53, "10^-3 kg/h", 1e-6, "Mass flow"),
    (0x54, "10^-4 kg/h", 1e-7, "Mass flow"),
    (0x55, "10^-5 kg/h", 1e-8, "Mass flow"),
    (0x56, "10^-6 kg/h", 1e-9, "Mass flow"),
    (0x57, "10^-7 kg/h", 1e-10, "Mass flow"),
    (0x58, "°C", 1e-3, "Flow temperature"),
    (0x59, "10^-1 °C", 1e-4, "Flow temperature"),
    (0x5A, "10^-2 °C", 1e-5, "Flow temperature"),
    (0x5B, "10^-3 °C", 1e-6, "Flow temperature"),
    (0x5C, "°C", 1e-3, "Return temperature"),
    (0x5D, "10^-1 °C", 1e-4, "Return temperature"),
    (0x5E, "10^-2 °C", 1e-5, "Return temperature"),
    (0x5F, "10^-3 °C", 1e-6, "Return temperature"),
    (0x60, "K", 1e-3, "Temperature difference"),
    (0x61, "10^-1 K", 1e-4, "Temperature difference"),
    (0x62, "10^-2 K", 1e-5, "Temperature difference"),
    (0x63, "10^-3 K", 1e-6, "Temperature difference"),
    (0x64, "°C", 1e-3, "External temperature"),
    (0x65, "10^-1 °C", 1e-4, "External temperature"),
    (0x66, "10^-2 °C", 1e-5, "External temperature"),
    (0x67, "10^-3 °C", 1e-6, "External temperature"),
    (0x68, "bar", 1e-3, "Pressure"),
    (0x69, "10^-1 bar", 1e-4, "Pressure"),
    (0x6A, "10^-2 bar", 1e-5, "Pressure"),
    (0x6B, "10^-3 bar", 1e-6, "Pressure"),
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
];

/// Parses a fixed-length M-Bus data record.
pub fn parse_fixed_record(input: &[u8]) -> Result<MBusRecord, MBusError> {
    if input.len() < crate::constants::MBUS_DATA_FIXED_LENGTH {
        return Err(MBusError::FrameParseError(
            "Fixed data too short".to_string(),
        ));
    }

    let device_id_bcd = match crate::payload::data_encoding::decode_bcd(&input[0..4]) {
        Ok((_, val)) => val,
        Err(_) => {
            return Err(MBusError::FrameParseError(
                "Invalid BCD device ID".to_string(),
            ))
        }
    };
    let manufacturer_val = u16::from_be_bytes([input[4], input[5]]);
    if !(0x0421..=0x6B5A).contains(&manufacturer_val) {
        return Err(MBusError::FrameParseError(
            "Invalid manufacturer".to_string(),
        ));
    }
    let _manufacturer = manufacturer_val as i32;
    let _version = input[6];
    let medium = input[7];
    let _access_number = input[8];
    let status = input[9];
    let _signature = match crate::payload::data_encoding::decode_int(&input[10..12], 2) {
        Ok((_, val)) => val,
        Err(_) => return Err(MBusError::FrameParseError("Invalid signature".to_string())),
    };
    let counter1 = if (status & crate::constants::MBUS_DATA_FIXED_STATUS_FORMAT_MASK)
        == crate::constants::MBUS_DATA_FIXED_STATUS_FORMAT_BCD
    {
        match crate::payload::data_encoding::decode_bcd(&input[12..16]) {
            Ok((_, val)) => val as i32,
            Err(_) => {
                return Err(MBusError::FrameParseError(
                    "Invalid BCD counter".to_string(),
                ))
            }
        }
    } else {
        match crate::payload::data_encoding::decode_int(&input[12..16], 4) {
            Ok((_, val)) => val,
            Err(_) => {
                return Err(MBusError::FrameParseError(
                    "Invalid int counter".to_string(),
                ))
            }
        }
    };
    let counter2 = 0; // Assuming no second counter for simplicity

    let (unit1, value1, quantity1) = normalize_fixed_unit(medium, counter1 as f64)?;
    let (unit2, value2, quantity2) = normalize_fixed_unit(medium, counter2 as f64)?;

    let record = MBusRecord {
        timestamp: SystemTime::now(),
        storage_number: device_id_bcd,
        tariff: -1,
        device: -1,
        is_numeric: true,
        value: MBusRecordValue::Numeric(value1 + value2),
        unit: format!("{unit1}, {unit2}"),
        function_medium: "Fixed".to_string(),
        quantity: format!("{quantity1}, {quantity2}"),
        drh: MBusDataRecordHeader {
            dib: MBusDataInformationBlock {
                dif: 0,
                ndife: 0,
                dife: [0; 10],
            },
            vib: MBusValueInformationBlock {
                vif: medium,
                nvife: 0,
                vife: [0; 10],
                custom_vif: String::new(),
            },
        },
        data_len: input.len(),
        data: {
            let mut data = [0; 256];
            data[..input.len()].copy_from_slice(input);
            data
        },
        more_records_follow: 0,
    };

    Ok(record)
}

/// Parses a variable-length M-Bus data record.
pub fn parse_variable_record(input: &[u8]) -> Result<MBusRecord, MBusError> {
    let (mut remaining, mut record) = parse_variable_record_inner(input)
        .map_err(|e| MBusError::FrameParseError(format!("Nom error: {e:?}")))?;

    // For manufacturer-specific or more-records-follow, data is already populated
    if record.drh.dib.dif != MBUS_DIB_DIF_MANUFACTURER_SPECIFIC
        && record.drh.dib.dif != MBUS_DIB_DIF_MORE_RECORDS_FOLLOW
    {
        // re-calculate data length, if of variable length type
        if (record.drh.dib.dif & MBUS_DATA_RECORD_DIF_MASK_DATA) == 0x0D {
            record.data_len = parse_variable_data_length(*remaining.first().unwrap_or(&0))?;
            remaining = &remaining[1..];
        }

        if record.data_len > remaining.len() {
            return Err(MBusError::PrematureEndAtData);
        }

        for j in 0..record.data_len {
            record.data[j] = *remaining.get(j).unwrap_or(&0);
        }
    }

    Ok(record)
}

fn parse_variable_record_inner(input: &[u8]) -> IResult<&[u8], MBusRecord> {
    let mut record = MBusRecord {
        timestamp: SystemTime::now(),
        storage_number: 0,
        tariff: -1,
        device: -1,
        is_numeric: true,
        value: MBusRecordValue::Numeric(0.0),
        unit: String::new(),
        function_medium: String::new(),
        quantity: String::new(),
        drh: MBusDataRecordHeader {
            dib: MBusDataInformationBlock {
                dif: 0,
                ndife: 0,
                dife: [0; 10],
            },
            vib: MBusValueInformationBlock {
                vif: 0,
                nvife: 0,
                vife: [0; 10],
                custom_vif: String::new(),
            },
        },
        data_len: 0,
        data: [0; 256],
        more_records_follow: 0,
    };

    // Skip idle filler bytes if present (they are optional)
    let i = input;
    let (i, _) = nom::bytes::complete::take_while(|b| b == MBUS_DIB_DIF_IDLE_FILLER)(i)?;

    let (i, dif) = be_u8(i)?;
    record.drh.dib.dif = dif;

    if record.drh.dib.dif == MBUS_DIB_DIF_MANUFACTURER_SPECIFIC
        || record.drh.dib.dif == MBUS_DIB_DIF_MORE_RECORDS_FOLLOW
    {
        if record.drh.dib.dif == MBUS_DIB_DIF_MORE_RECORDS_FOLLOW {
            record.more_records_follow = 1;
        }

        // For manufacturer-specific or more-records-follow,
        // all remaining data belongs to this record
        record.data_len = i.len();
        record.data[..i.len()].copy_from_slice(i);

        mbus_data_record_append(&mut record);
        return Ok((&[], record));
    }

    record.data_len = mbus_dif_datalength_lookup(record.drh.dib.dif);

    // Parse DIF extensions if DIF has extension bit set
    let mut i_temp = i;
    if (record.drh.dib.dif & MBUS_DIB_DIF_EXTENSION_BIT) != 0 {
        let mut dife_count = 0;
        loop {
            if i_temp.is_empty() || dife_count >= 10 {
                break;
            }
            let dife = i_temp[0];
            record.drh.dib.dife[dife_count] = dife;
            dife_count += 1;
            i_temp = &i_temp[1..];
            // Continue if this DIFE also has extension bit
            if (dife & MBUS_DIB_DIF_EXTENSION_BIT) == 0 {
                break;
            }
        }
        record.drh.dib.ndife = dife_count;
    }
    let i = i_temp;

    let (i, vif) = be_u8(i)?;
    record.drh.vib.vif = vif;

    if (record.drh.vib.vif & MBUS_DIB_VIF_WITHOUT_EXTENSION) == 0x7C {
        let (i, var_vif_len) = be_u8(i)?;
        if var_vif_len > MBUS_VALUE_INFO_BLOCK_CUSTOM_VIF_SIZE {
            return Err(nom::Err::Error(nom::error::Error::new(
                i,
                nom::error::ErrorKind::Tag,
            )));
        }

        let (_i, custom_vif) = take(var_vif_len)(i)?;
        mbus_data_str_decode(&mut record.drh.vib.custom_vif, custom_vif, custom_vif.len());
    }

    // Parse VIF extensions if VIF has extension bit set
    let mut i_temp = i;
    if (record.drh.vib.vif & MBUS_DIB_VIF_EXTENSION_BIT) != 0 {
        let mut vife_count = 0;
        loop {
            if i_temp.is_empty() || vife_count >= 10 {
                break;
            }
            let vife = i_temp[0];
            record.drh.vib.vife[vife_count] = vife;
            vife_count += 1;
            i_temp = &i_temp[1..];
            // Continue if this VIFE also has extension bit
            if (vife & MBUS_DIB_VIF_EXTENSION_BIT) == 0 {
                break;
            }
        }
        record.drh.vib.nvife = vife_count;
    }
    let i = i_temp;

    Ok((i, record))
}

/// Normalizes a fixed-length M-Bus data record.
#[allow(dead_code)]
fn normalize_fixed(
    medium_unit1: u8,
    medium_unit2: u8,
    counter1: i32,
    counter2: i32,
) -> Result<(String, f64, String), MBusError> {
    let (unit1, value1, quantity1) = normalize_fixed_unit(medium_unit1, counter1 as f64)?;
    let (unit2, value2, quantity2) = normalize_fixed_unit(medium_unit2, counter2 as f64)?;

    Ok((
        format!("{unit1}, {unit2}"),
        value1 + value2,
        format!("{quantity1}, {quantity2}"),
    ))
}

/// Normalizes a single fixed-length M-Bus data record unit.
#[allow(dead_code)]
fn normalize_fixed_unit(medium_unit: u8, value: f64) -> Result<(String, f64, String), MBusError> {
    if let Some((_, unit, exponent, quantity)) = FIXED_MEDIUM_UNITS
        .iter()
        .find(|(code, _, _, _)| *code == medium_unit)
    {
        Ok((unit.to_string(), value * exponent, quantity.to_string()))
    } else {
        Err(MBusError::UnknownVif(medium_unit))
    }
}

/// Looks up the data length from a DIF field in the data record.
pub fn mbus_dif_datalength_lookup(dif: u8) -> usize {
    match dif & 0x0F {
        0x0 => 0,
        0x1 => 1,
        0x2 => 2,
        0x3 => 3,
        0x4 => 4,
        0x5 => 6,
        0x6 => 8,
        0x7 => 0, // Special case
        0x8 => 0, // Special case
        0x9 => 1,
        0xA => 2,
        0xB => 3,
        0xC => 4,
        0xD => 0, // Variable length
        0xE => 6,
        0xF => 8,
        _ => 0,
    }
}

pub fn mbus_data_record_append(record: &mut MBusRecord) {
    // For manufacturer-specific or more records follow, set appropriate fields
    if record.drh.dib.dif == MBUS_DIB_DIF_MANUFACTURER_SPECIFIC {
        record.quantity = "Manufacturer specific".to_string();
    }
    if record.drh.dib.dif == MBUS_DIB_DIF_MORE_RECORDS_FOLLOW {
        record.more_records_follow = 1;
    }
    // Additional logic can be added here as needed
}

/// Parse variable record with vendor extension support
pub fn parse_variable_record_with_vendor(
    input: &[u8],
    manufacturer_id: Option<&str>,
    registry: Option<&vendors::VendorRegistry>,
) -> Result<MBusRecord, MBusError> {
    let mut record = parse_variable_record(input)?;

    // Check for vendor-specific DIF handling (0x0F or 0x1F)
    if let (Some(mfr_id), Some(reg)) = (manufacturer_id, registry) {
        if record.drh.dib.dif == 0x0F || record.drh.dib.dif == 0x1F {
            if let Some(vendor_records) = vendors::dispatch_dif_hook(reg, mfr_id, record.drh.dib.dif, &record.data[..record.data_len])? {
                // Convert first vendor record to MBusRecord (simplified)
                if let Some(first) = vendor_records.first() {
                    record.unit = first.unit.clone();
                    record.quantity = first.quantity.clone();
                    record.value = match &first.value {
                        vendors::VendorVariable::Numeric(n) => MBusRecordValue::Numeric(*n),
                        vendors::VendorVariable::String(s) => MBusRecordValue::String(s.clone()),
                        _ => MBusRecordValue::String("Vendor specific".to_string()),
                    };
                }
            }
        }

        // Check for vendor-specific VIF handling (0x7F or 0xFF)
        if record.drh.vib.vif == 0x7F || record.drh.vib.vif == 0xFF {
            if let Some((unit, exp, qty, var)) = vendors::dispatch_vif_hook(reg, mfr_id, record.drh.vib.vif, &record.data[..record.data_len])? {
                record.unit = unit;
                record.quantity = qty;
                record.value = match var {
                    vendors::VendorVariable::Numeric(n) => {
                        // Apply exponent
                        let scaled = n * 10_f64.powi(exp as i32);
                        MBusRecordValue::Numeric(scaled)
                    },
                    vendors::VendorVariable::String(s) => MBusRecordValue::String(s),
                    _ => MBusRecordValue::String("Vendor specific".to_string()),
                };
            }
        }

        // Check for status bits in data (if present)
        if record.data_len > 0 {
            // Status byte is often at the end of fixed data structures
            let status_byte = record.data[record.data_len - 1];
            if (status_byte & 0xE0) != 0 {  // Check bits [7:5]
                if let Some(status_vars) = vendors::dispatch_status_hook(reg, mfr_id, status_byte)? {
                    // Add status to quantity/unit for visibility
                    let status_str = status_vars.iter()
                        .filter_map(|v| match v {
                            vendors::VendorVariable::Boolean(true) => Some("ALARM"),
                            vendors::VendorVariable::ErrorFlags { flags } if *flags != 0 => Some("ERROR"),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    if !status_str.is_empty() {
                        record.quantity = format!("{} [{}]", record.quantity, status_str);
                    }
                }
            }
        }
    }

    Ok(record)
}

fn parse_variable_data_length(input: u8) -> Result<usize, MBusError> {
    if input <= 0xBF {
        Ok(input as usize)
    } else if (0xC0..=0xCF).contains(&input) {
        Ok((input - 0xC0) as usize * 2)
    } else if (0xD0..=0xDF).contains(&input) {
        Ok(((input - 0xD0) as usize * 2) + 1)
    } else if (0xE0..=0xEF).contains(&input) {
        Ok(((input - 0xE0) as usize) + 64)
    } else if (0xF0..=0xFA).contains(&input) {
        Ok(((input - 0xF0) as usize) + 1120)
    } else {
        Err(MBusError::UnknownDif(input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::MBusError;
    use std::time::SystemTime;

    #[test]
    fn test_mbus_dif_datalength_lookup_all_cases() {
        // Table-driven test for all DIF values
        let test_cases = vec![
            (0x00, 0),
            (0x01, 1),
            (0x02, 2),
            (0x03, 3),
            (0x04, 4),
            (0x05, 6),
            (0x06, 8),
            (0x07, 0), // Special case
            (0x08, 0), // Special case
            (0x09, 1),
            (0x0A, 2),
            (0x0B, 3),
            (0x0C, 4),
            (0x0D, 0), // Variable length
            (0x0E, 6),
            (0x0F, 8),
            (0x10, 0), // Out of range, defaults to 0
        ];
        for (dif, expected) in test_cases {
            assert_eq!(mbus_dif_datalength_lookup(dif), expected);
        }
    }

    #[test]
    fn test_parse_variable_data_length_edge_cases() -> Result<(), MBusError> {
        // Direct length
        assert_eq!(parse_variable_data_length(0xBF)?, 191);

        // Even lengths (C0-CF)
        assert_eq!(parse_variable_data_length(0xC0)?, 0);
        assert_eq!(parse_variable_data_length(0xCF)?, 30);

        // Odd lengths (D0-DF)
        assert_eq!(parse_variable_data_length(0xD0)?, 1);
        assert_eq!(parse_variable_data_length(0xDF)?, 31);

        // Large even (E0-EF)
        assert_eq!(parse_variable_data_length(0xE0)?, 64);
        assert_eq!(parse_variable_data_length(0xEF)?, 79);

        // Large odd (F0-FA)
        assert_eq!(parse_variable_data_length(0xF0)?, 1120);
        assert_eq!(parse_variable_data_length(0xFA)?, 1130);

        // Invalid
        assert!(matches!(
            parse_variable_data_length(0xFB),
            Err(MBusError::UnknownDif(0xFB))
        ));
        assert!(matches!(
            parse_variable_data_length(0xFF),
            Err(MBusError::UnknownDif(0xFF))
        ));

        Ok(())
    }

    #[test]
    fn test_normalize_fixed_unit_all_cases() -> Result<(), MBusError> {
        // Test all defined units
        for (code, unit, exponent, quantity) in FIXED_MEDIUM_UNITS.iter() {
            let result = normalize_fixed_unit(*code, 100.0)?;
            assert_eq!(result.0, unit.to_string());
            assert_eq!(result.1, 100.0 * *exponent);
            assert_eq!(result.2, quantity.to_string());
        }

        // Test unknown unit
        assert!(matches!(
            normalize_fixed_unit(0xFF, 100.0),
            Err(MBusError::UnknownVif(0xFF))
        ));

        Ok(())
    }

    #[test]
    fn test_parse_fixed_record_invalid_cases() {
        // Too short input
        let short_input = [0u8; 11];
        assert!(matches!(
            parse_fixed_record(&short_input),
            Err(MBusError::FrameParseError(_))
        ));

        // Invalid BCD device ID
        let invalid_bcd = [
            0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ];
        assert!(matches!(
            parse_fixed_record(&invalid_bcd),
            Err(MBusError::FrameParseError(_))
        ));

        // Invalid manufacturer
        let invalid_man = [
            0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ];
        assert!(matches!(
            parse_fixed_record(&invalid_man),
            Err(MBusError::FrameParseError(_))
        ));

        // Invalid signature
        let invalid_sig = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00,
            0x00, 0x00,
        ];
        assert!(matches!(
            parse_fixed_record(&invalid_sig),
            Err(MBusError::FrameParseError(_))
        ));

        // Invalid BCD counter
        let invalid_bcd_counter = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0xFF, 0xFF,
            0xFF, 0xFF,
        ]; // Status for BCD
        assert!(matches!(
            parse_fixed_record(&invalid_bcd_counter),
            Err(MBusError::FrameParseError(_))
        ));

        // Invalid int counter
        let invalid_int_counter = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF,
            0xFF, 0xFF,
        ]; // Status for int
        assert!(matches!(
            parse_fixed_record(&invalid_int_counter),
            Err(MBusError::FrameParseError(_))
        ));
    }

    #[test]
    fn test_mbus_data_record_append() {
        let mut record = MBusRecord {
            // Minimal record
            timestamp: SystemTime::now(),
            storage_number: 0,
            tariff: -1,
            device: -1,
            is_numeric: true,
            value: MBusRecordValue::Numeric(0.0),
            unit: String::new(),
            function_medium: String::new(),
            quantity: String::new(),
            drh: MBusDataRecordHeader {
                dib: MBusDataInformationBlock {
                    dif: MBUS_DIB_DIF_MANUFACTURER_SPECIFIC,
                    ndife: 0,
                    dife: [0; 10],
                },
                vib: MBusValueInformationBlock {
                    vif: 0,
                    nvife: 0,
                    vife: [0; 10],
                    custom_vif: String::new(),
                },
            },
            data_len: 0,
            data: [0; 256],
            more_records_follow: 0,
        };
        mbus_data_record_append(&mut record);
        assert_eq!(record.quantity, "Manufacturer specific");

        // Test more records follow
        record.drh.dib.dif = MBUS_DIB_DIF_MORE_RECORDS_FOLLOW;
        mbus_data_record_append(&mut record);
        assert_eq!(record.more_records_follow, 1);
    }
}
