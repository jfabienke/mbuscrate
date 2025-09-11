#![no_main]

use libfuzzer_sys::fuzz_target;
use mbus_rs::mbus::frame::{parse_frame, MBusFrame, MBusFrameType};

fuzz_target!(|data: &[u8]| {
    // Simulate multi-telegram scenarios
    if data.len() < 20 {
        return; // Need enough data for meaningful multi-frame testing
    }
    
    // Split data into multiple "frames"
    let chunk_size = (data.len() / 3).max(10);
    let chunks: Vec<&[u8]> = data.chunks(chunk_size).collect();
    
    // Test frame reassembly logic
    let mut accumulated_data = Vec::new();
    let mut fcb_state = false;
    
    for (i, chunk) in chunks.iter().enumerate() {
        // Create a mock frame with CI field indicating more frames
        let mut frame_data = Vec::new();
        
        // Start with valid long frame header
        frame_data.push(0x68); // Start
        let len = chunk.len().min(252) as u8 + 3; // Length (max 255, -3 for C/A/CI)
        frame_data.push(len);
        frame_data.push(len); // Length repeat
        frame_data.push(0x68); // Start repeat
        
        // Control field with FCB bit
        let control = if fcb_state { 0x5B } else { 0x7B };
        frame_data.push(control);
        
        // Address
        frame_data.push(0x01);
        
        // Control Information with "more records follow" bit
        let ci = if i < chunks.len() - 1 {
            0x72 | 0x10 // RSP_UD with more frames bit
        } else {
            0x72 // Last frame
        };
        frame_data.push(ci);
        
        // Add actual data
        frame_data.extend_from_slice(&chunk[..chunk.len().min(252)]);
        
        // Calculate and add checksum
        let checksum: u8 = frame_data[4..frame_data.len()]
            .iter()
            .fold(0u8, |acc, b| acc.wrapping_add(*b));
        frame_data.push(checksum);
        
        // Stop byte
        frame_data.push(0x16);
        
        // Try to parse this frame
        let _ = parse_frame(&frame_data);
        
        // Simulate frame accumulation
        accumulated_data.extend_from_slice(chunk);
        
        // Toggle FCB for next frame
        fcb_state = !fcb_state;
    }
    
    // Test various multi-frame attack scenarios
    
    // 1. Frames with inconsistent FCB
    if chunks.len() >= 2 {
        let mut frame1 = create_test_frame(&chunks[0], 0x5B, true); // FCB=1, more=true
        let mut frame2 = create_test_frame(&chunks[1], 0x5B, false); // FCB=1 again (should be 0)
        let _ = parse_frame(&frame1);
        let _ = parse_frame(&frame2);
    }
    
    // 2. Frame with "more" bit but no follow-up
    if !chunks.is_empty() {
        let incomplete = create_test_frame(&chunks[0], 0x7B, true); // more=true
        let _ = parse_frame(&incomplete);
        // Simulate timeout waiting for next frame
    }
    
    // 3. Oversized multi-frame (>16 blocks)
    for i in 0..20 {
        let mock_data = vec![0xAA; 50];
        let frame = create_test_frame(&mock_data, if i % 2 == 0 { 0x5B } else { 0x7B }, i < 19);
        let _ = parse_frame(&frame);
    }
    
    // 4. Mixed frame types in sequence
    if data.len() >= 30 {
        // Short frame followed by long frame
        let short_frame = vec![0x10, 0x5B, 0x01, 0x5C, 0x16];
        let _ = parse_frame(&short_frame);
        
        let long_frame = create_test_frame(&data[..20], 0x7B, false);
        let _ = parse_frame(&long_frame);
    }
});

// Helper to create a valid frame for testing
fn create_test_frame(data: &[u8], control: u8, more_follows: bool) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.push(0x68);
    let len = (data.len().min(252) + 3) as u8;
    frame.push(len);
    frame.push(len);
    frame.push(0x68);
    frame.push(control);
    frame.push(0x01); // Address
    frame.push(if more_follows { 0x72 | 0x10 } else { 0x72 }); // CI
    frame.extend_from_slice(&data[..data.len().min(252)]);
    
    let checksum: u8 = frame[4..frame.len()]
        .iter()
        .fold(0u8, |acc, b| acc.wrapping_add(*b));
    frame.push(checksum);
    frame.push(0x16);
    
    frame
}