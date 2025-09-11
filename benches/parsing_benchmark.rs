use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use mbus_rs::mbus::frame::{parse_frame, pack_frame, verify_frame, MBusFrame, MBusFrameType};
use mbus_rs::payload::vif::{parse_vif, parse_vib};
use mbus_rs::payload::data_encoding::{decode_bcd, decode_int};
use mbus_rs::payload::data::mbus_data_record_decode;
use std::time::Duration;

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    hex.chars()
        .collect::<Vec<char>>()
        .chunks(2)
        .map(|chunk| {
            u8::from_str_radix(&chunk.iter().collect::<String>(), 16).unwrap_or(0)
        })
        .collect()
}

// Test data samples
const SHORT_FRAME_HEX: &str = "10400150E516";
const LONG_FRAME_HEX: &str = "6831316808017245585703B40534049E0027B60306F934150315C6004D052E00000000053D00000000055B22F32642055FC7DA0D42FA16";
const ACK_FRAME_HEX: &str = "E5";

// Performance targets (must complete within these times)
const TARGET_FRAME_PARSE_MS: f64 = 1.0;
const TARGET_VIF_DECODE_MS: f64 = 0.1;
const TARGET_RECORD_PARSE_MS: f64 = 0.5;
const TARGET_CHECKSUM_MS: f64 = 0.05;

fn benchmark_frame_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_parsing");
    
    // Set measurement time to ensure stable results
    group.measurement_time(Duration::from_secs(10));
    group.warm_up_time(Duration::from_secs(3));
    
    // Benchmark ACK frame (smallest)
    let ack_data = hex_to_bytes(ACK_FRAME_HEX);
    group.bench_function("ack_frame", |b| {
        b.iter(|| {
            let _ = parse_frame(black_box(&ack_data));
        })
    });
    
    // Benchmark short frame
    let short_data = hex_to_bytes(SHORT_FRAME_HEX);
    group.bench_function("short_frame", |b| {
        b.iter(|| {
            let _ = parse_frame(black_box(&short_data));
        })
    });
    
    // Benchmark long frame (most common)
    let long_data = hex_to_bytes(LONG_FRAME_HEX);
    group.bench_function("long_frame", |b| {
        b.iter(|| {
            let _ = parse_frame(black_box(&long_data));
        })
    });
    
    // Benchmark maximum size frame (255 bytes)
    let mut max_frame = vec![0x68, 0xFF, 0xFF, 0x68, 0x08, 0x01, 0x72];
    max_frame.extend(vec![0xAA; 252]); // Fill with data
    let checksum: u8 = max_frame[4..max_frame.len()]
        .iter()
        .fold(0u8, |acc, b| acc.wrapping_add(*b));
    max_frame.push(checksum);
    max_frame.push(0x16);
    
    group.bench_function("max_size_frame", |b| {
        b.iter(|| {
            let _ = parse_frame(black_box(&max_frame));
        })
    });
    
    group.finish();
}

fn benchmark_vif_decoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("vif_decoding");
    
    // Single VIF byte
    group.bench_function("single_vif", |b| {
        b.iter(|| {
            let _ = parse_vif(black_box(&[0x13])); // Volume in liters
        })
    });
    
    // VIF with extensions (common case)
    let vif_chain = vec![0x93, 0x3C]; // Volume with extension
    group.bench_function("vif_with_extension", |b| {
        b.iter(|| {
            let _ = parse_vib(black_box(&vif_chain));
        })
    });
    
    // VIF with multiple extensions
    let extended_vif = vec![0x93, 0xFD, 0x3C]; // Volume with VIFE
    group.bench_function("vif_multiple_extensions", |b| {
        b.iter(|| {
            let _ = parse_vib(black_box(&extended_vif));
        })
    });
    
    // Full VIB parsing
    let vif_data = vec![0x13]; // Volume
    group.bench_function("parse_vib_simple", |b| {
        b.iter(|| {
            let _ = parse_vib(black_box(&vif_data));
        })
    });
    
    group.finish();
}

fn benchmark_data_encoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("data_encoding");
    
    // BCD decoding (common for meter readings)
    let bcd_data = vec![0x12, 0x34, 0x56, 0x78];
    group.bench_function("decode_bcd_4bytes", |b| {
        b.iter(|| {
            let _ = decode_bcd(black_box(&bcd_data));
        })
    });
    
    // Integer decoding
    let int_data = vec![0x01, 0x02, 0x03, 0x04];
    group.bench_function("decode_int_4bytes", |b| {
        b.iter(|| {
            let _ = decode_int(black_box(&int_data), 4);
        })
    });
    
    // Variable record parsing
    let record_data = vec![0x04, 0x13, 0x34, 0x12, 0x00, 0x00]; // DIF + VIF + 4-byte value
    group.bench_function("parse_variable_data_record", |b| {
        b.iter(|| {
            let _ = mbus_data_record_decode(black_box(&record_data));
        })
    });
    
    group.finish();
}

fn benchmark_checksum_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("checksum");
    
    // Create test frame
    let frame = MBusFrame {
        frame_type: MBusFrameType::Long,
        control: 0x08,
        address: 0x01,
        control_information: 0x72,
        data: vec![0xAA; 100],
        checksum: 0, // Will be calculated
        more_records_follow: false,
    };
    
    // Benchmark checksum verification
    group.bench_function("verify_checksum", |b| {
        b.iter(|| {
            let _ = verify_frame(black_box(&frame));
        })
    });
    
    // Benchmark frame packing (includes checksum calculation)
    group.bench_function("pack_frame_with_checksum", |b| {
        b.iter(|| {
            let _ = pack_frame(black_box(&frame));
        })
    });
    
    group.finish();
}

fn benchmark_multi_telegram(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_telegram");
    
    // Simulate parsing multiple frames in sequence
    let frames = vec![
        hex_to_bytes(LONG_FRAME_HEX),
        hex_to_bytes(LONG_FRAME_HEX),
        hex_to_bytes(LONG_FRAME_HEX),
    ];
    
    group.bench_function("parse_3_frames_sequence", |b| {
        b.iter(|| {
            for frame_data in &frames {
                let _ = parse_frame(black_box(frame_data));
            }
        })
    });
    
    group.finish();
}

// Performance regression check
fn check_performance_targets(c: &mut Criterion) {
    let mut group = c.benchmark_group("performance_targets");
    group.significance_level(0.05); // 95% confidence
    
    // Test that frame parsing meets target
    let long_data = hex_to_bytes(LONG_FRAME_HEX);
    group.bench_with_input(
        BenchmarkId::new("frame_parse_under_1ms", "long_frame"),
        &long_data,
        |b, data| {
            b.iter(|| {
                let _ = parse_frame(black_box(data));
            });
        },
    );
    
    group.finish();
}

criterion_group!{
    name = benches;
    config = Criterion::default()
        .sample_size(500)
        .noise_threshold(0.05);
    targets = benchmark_frame_parsing,
              benchmark_vif_decoding,
              benchmark_data_encoding,
              benchmark_checksum_operations,
              benchmark_multi_telegram,
              check_performance_targets
}
criterion_main!(benches);
