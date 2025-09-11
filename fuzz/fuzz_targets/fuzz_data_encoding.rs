#![no_main]

use libfuzzer_sys::fuzz_target;
use mbus_rs::payload::data_encoding::{decode_bcd, decode_integer, decode_real};

fuzz_target!(|data: &[u8]| {
    // Test BCD decoding with various lengths
    for len in 1..=8 {
        if data.len() >= len {
            let _ = decode_bcd(&data[..len]);
            
            // Test invalid BCD patterns (>9 in nibbles)
            let mut invalid_bcd = data[..len].to_vec();
            for byte in &mut invalid_bcd {
                *byte = (*byte & 0xF0) | 0x0F; // Set lower nibble to F
                let _ = decode_bcd(&invalid_bcd);
                *byte = 0xFF; // Both nibbles invalid
                let _ = decode_bcd(&invalid_bcd);
            }
        }
    }
    
    // Test integer decoding (2, 4, 6, 8 bytes)
    for len in [2, 4, 6, 8] {
        if data.len() >= len {
            let _ = decode_integer(&data[..len]);
            
            // Test boundary values
            let mut boundary = vec![0xFF; len];
            let _ = decode_integer(&boundary);
            boundary.fill(0x00);
            let _ = decode_integer(&boundary);
            
            // Test endianness edge cases
            if len >= 4 {
                let mut alternating = vec![0xAA, 0x55, 0xAA, 0x55];
                alternating.resize(len, 0xFF);
                let _ = decode_integer(&alternating);
            }
        }
    }
    
    // Test real/float decoding
    if data.len() >= 4 {
        let _ = decode_real(&data[..4]);
        
        // Test special float values
        let special_floats = [
            [0x00, 0x00, 0x00, 0x00], // Zero
            [0xFF, 0xFF, 0xFF, 0x7F], // NaN
            [0x00, 0x00, 0x80, 0x7F], // +Infinity
            [0x00, 0x00, 0x80, 0xFF], // -Infinity
            [0x01, 0x00, 0x00, 0x00], // Smallest positive
            [0xFF, 0xFF, 0x7F, 0x7F], // Largest positive
        ];
        
        for float_bytes in &special_floats {
            let _ = decode_real(float_bytes);
        }
    }
    
    // Test variable length data
    if !data.is_empty() {
        // LVAR field (first byte is length)
        let lvar_len = data[0] as usize;
        if data.len() > lvar_len {
            let lvar_data = &data[1..=lvar_len.min(data.len() - 1)];
            // Simulate LVAR processing
            let _ = std::str::from_utf8(lvar_data);
        }
    }
    
    // Test date/time encoding patterns
    if data.len() >= 6 {
        // Type F: Date and Time (6 bytes)
        // Format: minutes, hours, day, month, year, day of week
        let datetime_data = &data[..6];
        // Validate ranges
        let _minutes = datetime_data[0] & 0x3F; // 0-59
        let _hours = datetime_data[1] & 0x1F;   // 0-23
        let _day = datetime_data[2] & 0x1F;     // 1-31
        let _month = datetime_data[3] & 0x0F;   // 1-12
        let _year = datetime_data[4];           // 0-99
    }
    
    // Edge case: empty data
    let _ = decode_bcd(&[]);
    let _ = decode_integer(&[]);
    let _ = decode_real(&[]);
});