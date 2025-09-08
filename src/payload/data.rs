use crate::constants::*;
use crate::payload::data_encoding::mbus_data_str_decode;
use crate::error::MBusError;
use crate::payload::vif::{normalize_vib, parse_vib, VifInfo};
use nom::{bytes::complete::take, combinator::map, number::complete::{be_u8, be_u32}, IResult};
use std::time::SystemTime;

#[derive(Debug)]
pub struct MBusDataRecord {
    pub timestamp: SystemTime,
    pub storage_number: u32,
    pub tariff: i32,
    pub device: i32,
    pub is_numeric: bool,
    pub value: MBusRecordValue,
    pub unit: String,
    pub function_medium: String,
    pub quantity: String,
}

#[derive(Debug)]
pub enum MBusRecordValue {
    Numeric(f64),
    String(String),
}

pub fn mbus_data_record_decode(input: &[u8]) -> IResult<&[u8], MBusDataRecord> {
    let (input, timestamp) = map(be_u32, |t| {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(t.into())
    })(input)?;
    let (input, dif) = be_u8(input)?;
    let (input, vib) = parse_vib(input)?;

    let (unit, value, quantity) = normalize_vib(&vib).unwrap_or((String::new(), 0.0, String::new()));

    let (input, data) = take(mbus_dif_datalength_lookup(dif))(input)?;

    let mut record = MBusDataRecord {
        timestamp,
        storage_number: mbus_data_record_storage_number(&vib),
        tariff: mbus_data_record_tariff(&vib),
        device: mbus_data_record_device(&vib),
        is_numeric: true,
        value: MBusRecordValue::Numeric(value),
        unit,
        function_medium: mbus_data_record_function(dif).to_string(),
        quantity,
    };

    if (dif & MBUS_DATA_RECORD_DIF_MASK_DATA) == 0x0D {
        // Variable-length data
        record.is_numeric = false;
        let mut s = String::new();
        mbus_data_str_decode(&mut s, data, data.len());
        record.value = MBusRecordValue::String(s);
    } else {
        let value_result = mbus_data_record_value_decode(dif, data);
        if let Ok(decoded_value) = value_result {
            record.value = MBusRecordValue::Numeric(decoded_value);
        } else {
            crate::logging::log_error(&format!(
                "Error decoding data record value: {:?}",
                value_result
            ));
            record.value = MBusRecordValue::Numeric(0.0);
        }
    }

    Ok((input, record))
}

fn mbus_dif_datalength_lookup(dif: u8) -> usize {
    match dif & MBUS_DATA_RECORD_DIF_MASK_DATA {
        0x00 => 0,
        0x01 => 1,
        0x02 => 2,
        0x03 => 3,
        0x04 => 4,
        0x05 => 4,
        0x06 => 6,
        0x07 => 8,
        0x0D => 0, // Variable-length data, length stored in data field
        _ => 0,
    }
}

fn mbus_data_record_value_decode(dif: u8, data: &[u8]) -> Result<f64, MBusError> {
    let be_to_u64 = |bytes: &[u8]| -> u64 {
        bytes.iter().fold(0u64, |acc, b| (acc << 8) | (*b as u64))
    };
    match dif & MBUS_DATA_RECORD_DIF_MASK_DATA {
        0x01 => Ok(data.get(0).copied().unwrap_or(0) as i32 as f64),
        0x02 => Ok(be_to_u64(&data[..data.len().min(2)]) as i32 as f64),
        0x03 => Ok(be_to_u64(&data[..data.len().min(3)]) as i32 as f64),
        0x04 => Ok(be_to_u64(&data[..data.len().min(4)]) as i32 as f64),
        0x05 => {
            if data.len() >= 4 {
                let bits = ((data[0] as u32) << 24)
                    | ((data[1] as u32) << 16)
                    | ((data[2] as u32) << 8)
                    | (data[3] as u32);
                Ok(f32::from_bits(bits) as f64)
            } else {
                Ok(0.0)
            }
        }
        0x06 => Ok(be_to_u64(&data[..data.len().min(6)]) as i64 as f64),
        0x07 => Ok(be_to_u64(&data[..data.len().min(8)]) as i64 as f64),
        0x09 | 0x0A | 0x0B | 0x0C | 0x0E => {
            let s: String = data.iter().map(|b| format!("{:02X}", b)).collect();
            Ok(u64::from_str_radix(&s, 16).unwrap_or(0) as f64)
        }
        _ => Err(MBusError::UnknownDif(dif)),
    }
}

fn mbus_data_record_storage_number(vib: &[VifInfo]) -> u32 {
    let mut storage_number = 0;
    for info in vib {
        if ((info.vif as u8) & (MBUS_DATA_RECORD_DIF_MASK_STORAGE_NO as u8)) >> 6 != 0 {
            storage_number |= (((info.vif as u8) & (MBUS_DATA_RECORD_DIF_MASK_STORAGE_NO as u8)) >> 6) as u32;
        }
        if ((info.vif as u8) & (MBUS_DATA_RECORD_DIFE_MASK_STORAGE_NO as u8)) != 0 {
            storage_number |= (((info.vif as u8) & (MBUS_DATA_RECORD_DIFE_MASK_STORAGE_NO as u8)) as u32) << 4;
        }
    }
    storage_number
}

fn mbus_data_record_tariff(vib: &[VifInfo]) -> i32 {
    let mut tariff = 0;
    for info in vib {
        if ((info.vif as u8) & (MBUS_DATA_RECORD_DIFE_MASK_TARIFF as u8)) >> 4 != 0 {
            tariff |= (((info.vif as u8) & (MBUS_DATA_RECORD_DIFE_MASK_TARIFF as u8)) >> 4) as i32;
        }
    }
    tariff
}

fn mbus_data_record_device(vib: &[VifInfo]) -> i32 {
    let mut device = 0;
    for info in vib {
        if ((info.vif as u8) & (MBUS_DATA_RECORD_DIFE_MASK_DEVICE as u8)) >> 6 != 0 {
            device |= (((info.vif as u8) & (MBUS_DATA_RECORD_DIFE_MASK_DEVICE as u8)) >> 6) as i32;
        }
    }
    device
}

fn mbus_data_record_function(dif: u8) -> &'static str {
    match dif & MBUS_DATA_RECORD_DIF_MASK_FUNCTION {
        0x00 => "Instantaneous value",
        0x10 => "Maximum value",
        0x20 => "Minimum value",
        0x30 => "Value during error state",
        _ => "Unknown",
    }
}
