#![no_main]

use libfuzzer_sys::fuzz_target;
use mbus_rs::mbus::frame::parse_frame;

fuzz_target!(|data: &[u8]| {
    // Fuzz the frame parser with arbitrary input
    // The parser should handle any malformed input gracefully
    let _ = parse_frame(data);
    
    // Additional targeted fuzzing for specific frame types
    if !data.is_empty() {
        match data[0] {
            0xE5 => {
                // ACK frame - should be exactly 1 byte
                let _ = parse_frame(&[0xE5]);
            }
            0x10 => {
                // Short frame - test with various lengths
                if data.len() >= 5 {
                    let _ = parse_frame(&data[..5]);
                }
            }
            0x68 => {
                // Long/Control frame - test length field consistency
                if data.len() >= 6 {
                    // Mutate length fields to test validation
                    let mut mutated = data.to_vec();
                    if mutated.len() > 2 {
                        mutated[1] = (data.len() as u8).wrapping_sub(6);
                        mutated[2] = mutated[1]; // Should match
                        let _ = parse_frame(&mutated);
                        
                        // Test mismatched lengths
                        mutated[2] = mutated[1].wrapping_add(1);
                        let _ = parse_frame(&mutated);
                    }
                }
            }
            _ => {
                // Invalid start byte - should fail gracefully
                let _ = parse_frame(data);
            }
        }
    }
    
    // Test checksum validation by corrupting last byte
    if data.len() > 10 {
        let mut corrupted = data.to_vec();
        if let Some(last) = corrupted.last_mut() {
            *last = last.wrapping_add(1);
            let _ = parse_frame(&corrupted);
        }
    }
});