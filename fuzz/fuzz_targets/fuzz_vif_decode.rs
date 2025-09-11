#![no_main]

use libfuzzer_sys::fuzz_target;
use mbus_rs::payload::vif::{parse_vif, parse_vif_chain, normalize_vib};

fuzz_target!(|data: &[u8]| {
    // Test single VIF parsing
    if !data.is_empty() {
        let _ = parse_vif(data[0]);
        
        // Test special VIF codes
        match data[0] {
            0x7C => {
                // ASCII VIF - should handle following bytes
                if data.len() > 1 {
                    let _ = parse_vif(0x7C);
                }
            }
            0x7E => {
                // Any VIF - wildcard
                let _ = parse_vif(0x7E);
            }
            0x7F => {
                // Manufacturer specific
                let _ = parse_vif(0x7F);
            }
            0xFB | 0xFD => {
                // First/Second extension VIF
                let _ = parse_vif(data[0]);
            }
            _ => {}
        }
    }
    
    // Test VIF chain parsing (up to 10 extensions per EN 13757-3)
    if data.len() >= 2 {
        let _ = parse_vif_chain(data);
        
        // Test chains with extension bit patterns
        let mut chain = Vec::new();
        for (i, &byte) in data.iter().enumerate() {
            if i >= 10 {
                break; // Max 10 extensions
            }
            // Set extension bit on all but last
            let vif_byte = if i < data.len() - 1 && i < 9 {
                byte | 0x80
            } else {
                byte & 0x7F
            };
            chain.push(vif_byte);
        }
        if !chain.is_empty() {
            let _ = parse_vif_chain(&chain);
        }
    }
    
    // Test full VIB normalization with DIF
    if data.len() >= 3 {
        // Create a mock DIF byte and VIF data
        let dif = data[0];
        let vif_data = &data[1..];
        let _ = normalize_vib(dif, vif_data);
        
        // Test with various DIF patterns
        for dif_variant in [0x01, 0x04, 0x0C, 0x14, 0x84] {
            let _ = normalize_vib(dif_variant, vif_data);
        }
    }
    
    // Edge cases for VIF extension chains
    if data.len() > 5 {
        // All bytes with extension bit set (invalid - no terminator)
        let all_extended: Vec<u8> = data.iter().take(11).map(|b| b | 0x80).collect();
        let _ = parse_vif_chain(&all_extended);
        
        // Alternating extension bits
        let alternating: Vec<u8> = data.iter()
            .take(10)
            .enumerate()
            .map(|(i, b)| if i % 2 == 0 { b | 0x80 } else { b & 0x7F })
            .collect();
        let _ = parse_vif_chain(&alternating);
    }
});