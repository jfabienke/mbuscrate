use crate::error::MBusError;
use nom::{number::complete::be_u8, IResult};

/// Visitor for complex VIF decoding.
/// 
/// For now, simple lookup is used, but visitor can be extended for custom decoding.
#[derive(Debug)]
#[derive(Debug)]
#[derive(Debug)]
#[derive(Debug)]
#[derive(Debug)]
#[derive(Debug)]
pub struct VifInfo {
    pub vif: u16,
    pub unit: &'static str,
    pub exponent: f64,
    pub quantity: &'static str,
}

fn parse_vif(input: &[u8]) -> IResult<&[u8], VifInfo> {
    let (remaining, vif) = be_u8(input)?;

    let vif_info = crate::payload::vif_maps::lookup_primary_vif(vif)
        .ok_or(nom::Err::Error(nom::error::Error::new(remaining, nom::error::ErrorKind::Tag)))?;

    Ok((remaining, vif_info))
}

fn parse_vife(input: &[u8]) -> IResult<&[u8], VifInfo> {
    let (remaining, vife) = be_u8(input)?;

    // For now, assume FD extension; in full implementation, check extension type
    let vif_info = crate::payload::vif_maps::lookup_vife_fd(vife)
        .or_else(|| crate::payload::vif_maps::lookup_vife_fb(vife))
        .ok_or(nom::Err::Error(nom::error::Error::new(
            remaining,
            nom::error::ErrorKind::Tag,
        )))?;

    Ok((remaining, vif_info))
}

pub fn parse_vib(input: &[u8]) -> IResult<&[u8], Vec<VifInfo>> {
    let (remaining, vif) = parse_vif(input)?;
    let (remaining, vifes) = nom::multi::many0(parse_vife)(remaining)?;

    Ok((remaining, std::iter::once(vif).chain(vifes).collect()))
}

pub fn normalize_vib(vib: &[VifInfo]) -> Result<(String, f64, String), MBusError> {
    let mut unit = String::new();
    let mut value = 1.0;  // Initialize to 1.0 as multiplier starts neutral
    let mut quantity = String::new();

    for info in vib {
        unit = info.unit.to_string();
        value *= info.exponent;
        quantity = info.quantity.to_string();
    }

    Ok((unit, value, quantity))
}
