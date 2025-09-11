use mbus_rs::payload::data::parse_enhanced_variable_data_record;

fn main() {
    let mut data = vec![
        0x01, // DIF: 8-bit integer
        0x90, // VIF with extension bit set
    ];
    
    // Add exactly 10 VIFEs (the maximum allowed per standard)
    for i in 0..10 {
        if i < 9 {
            data.push(0x80 | (i as u8)); // VIFE with extension bit
        } else {
            data.push(0x09); // Last VIFE without extension bit (value 0x09)
        }
    }
    
    data.push(0x42); // 8-bit value
    
    let result = parse_enhanced_variable_data_record(&data);
    match result {
        Ok((remaining, record)) => {
            println!("Success!");
            println!("VIF chain length: {}", record.vif_chain.len());
            println!("Remaining bytes: {:?}", remaining);
        }
        Err(e) => {
            println!("Error: {:?}", e);
        }
    }
}
