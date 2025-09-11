use crate::error::MBusError;

#[derive(Debug, Clone, Copy)]
pub enum WMBusMode {
    S, // Stationary
    T(UplinkDownlink), // Frequent Transmit
    C(UplinkDownlink), // Compact
}

#[derive(Debug, Clone, Copy)]
pub enum UplinkDownlink {
    Uplink,
    Downlink,
}

#[derive(Debug, Clone, Copy)]
pub enum EncodingType {
    Nrz, // Default for SX126x
    Manchester, // Mode S
    ThreeOutOfSix, // Mode T/C downlink
}

pub fn encode_payload(mode: WMBusMode, payload: &[u8]) -> Vec<u8> {
    match mode {
        WMBusMode::S => manchester_encode(payload),
        WMBusMode::T(ud) => if ud == UplinkDownlink::Downlink { three_out_of_six_encode(payload) } else { nrz_encode(payload) },
        WMBusMode::C(ud) => if ud == UplinkDownlink::Downlink { three_out_of_six_encode(payload) } else { nrz_encode(payload) },
    }
}

fn manchester_encode(payload: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(payload.len() * 2);
    for &byte in payload {
        for i in (0..8).rev() {
            let bit = (byte >> i) & 1;
            encoded.push(if bit == 0 { 0b01 } else { 0b10 } as u8);
        }
    }
    encoded
}

const THREE_OUT_OF_SIX_TABLE: [u8; 16] = [
    0b011100, 0b011010, 0b011001, 0b010110, 0b010101, 0b010011, 0b101100, 0b101010,
    0b101001, 0b100110, 0b100101, 0b100011, 0b110100, 0b110010, 0b110001, 0b001110,
];

fn three_out_of_six_encode(payload: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::new();
    for &byte in payload {
        let high_nibble = (byte >> 4) & 0xF;
        let low_nibble = byte & 0xF;
        encoded.extend_from_slice(&pack_nibble(THREE_OUT_OF_SIX_TABLE[high_nibble as usize]));
        encoded.extend_from_slice(&pack_nibble(THREE_OUT_OF_SIX_TABLE[low_nibble as usize]));
    }
    encoded
}

fn pack_nibble(nibble: u8) -> [u8; 1] {
    // Pack 6 bits into bytes; simplified
    [nibble]
}

fn nrz_encode(payload: &[u8]) -> Vec<u8> {
    payload.to_vec()
}
