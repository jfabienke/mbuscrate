//! Performance benchmarks for hardware-accelerated crypto operations
//!
//! This benchmark suite measures the performance of all crypto operations
//! including checksums, CRCs, block validation, and LoRaWAN MIC calculations.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mbus_rs::mbus::frame::calculate_mbus_checksum;
use mbus_rs::wmbus::frame::{calculate_wmbus_crc, verify_wmbus_crc};
use mbus_rs::wmbus::block::{calculate_block_crc, verify_blocks};
use mbus_rs::wmbus::crypto::{WMBusCrypto, AesKey};
use std::time::Duration;

/// Test data sizes for throughput testing
const SMALL_FRAME: usize = 16;
const MEDIUM_FRAME: usize = 64;
const LARGE_FRAME: usize = 256;
const XLARGE_FRAME: usize = 1024;

/// Generate test data of specified size
fn generate_test_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 256) as u8).collect()
}

/// Benchmark M-Bus checksum calculation
fn bench_mbus_checksum(c: &mut Criterion) {
    let mut group = c.benchmark_group("mbus_checksum");
    group.measurement_time(Duration::from_secs(10));

    for size in &[SMALL_FRAME, MEDIUM_FRAME, LARGE_FRAME, XLARGE_FRAME] {
        let data = generate_test_data(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &data,
            |b, data| {
                b.iter(|| {
                    calculate_mbus_checksum(black_box(data))
                })
            },
        );
    }

    group.finish();
}

/// Benchmark wM-Bus CRC calculation
fn bench_wmbus_crc(c: &mut Criterion) {
    let mut group = c.benchmark_group("wmbus_crc");
    group.measurement_time(Duration::from_secs(10));

    for size in &[SMALL_FRAME, MEDIUM_FRAME, LARGE_FRAME, XLARGE_FRAME] {
        let data = generate_test_data(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &data,
            |b, data| {
                b.iter(|| {
                    calculate_wmbus_crc(black_box(data))
                })
            },
        );
    }

    group.finish();
}

/// Benchmark block CRC operations
fn bench_block_crc(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_crc");
    group.measurement_time(Duration::from_secs(10));

    // Test single block (16 bytes)
    let block_data = generate_test_data(14); // 14 data bytes for block
    group.bench_function("single_block", |b| {
        b.iter(|| {
            calculate_block_crc(black_box(&block_data))
        })
    });

    // Test multi-block validation
    let multi_block_sizes = vec![32, 64, 128, 256]; // 2, 4, 8, 16 blocks
    for size in multi_block_sizes {
        let data = generate_test_data(size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("multi_block", size),
            &data,
            |b, data| {
                b.iter(|| {
                    let _ = verify_blocks(black_box(data), false);
                })
            },
        );
    }

    group.finish();
}

/// Benchmark LoRaWAN MIC calculation
#[cfg(feature = "crypto")]
fn bench_lorawan_mic(c: &mut Criterion) {
    let mut group = c.benchmark_group("lorawan_mic");
    group.measurement_time(Duration::from_secs(10));

    // Setup crypto instance
    let key_bytes = [0x2Bu8; 16];
    let aes_key = AesKey::from_bytes(&key_bytes).expect("Valid key");
    let crypto = WMBusCrypto::new(aes_key);

    // Test different payload sizes
    for size in &[16, 32, 64, 128, 256] {
        let payload = generate_test_data(*size);
        let dev_addr = 0x12345678u32;
        let fcnt = 42u32;

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &payload,
            |b, payload| {
                b.iter(|| {
                    let _ = crypto.calculate_lorawan_mic(
                        black_box(&key_bytes),
                        black_box(payload),
                        black_box(0), // uplink
                        black_box(dev_addr),
                        black_box(fcnt),
                    );
                })
            },
        );
    }

    group.finish();
}

/// Benchmark HMAC-SHA1 for Qundis authentication
#[cfg(feature = "crypto")]
fn bench_hmac_sha1(c: &mut Criterion) {
    let mut group = c.benchmark_group("hmac_sha1");
    group.measurement_time(Duration::from_secs(10));

    // Setup crypto instance
    let key_bytes = [0x42u8; 16];
    let aes_key = AesKey::from_bytes(&key_bytes).expect("Valid key");
    let crypto = WMBusCrypto::new(aes_key);

    // Test different message sizes
    for size in &[8, 16, 32, 64, 128] {
        let message = generate_test_data(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &message,
            |b, message| {
                b.iter(|| {
                    crypto.calculate_hmac_sha1(
                        black_box(&key_bytes),
                        black_box(message),
                    )
                })
            },
        );
    }

    group.finish();
}

/// Benchmark complete frame validation pipeline
fn bench_frame_validation_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_validation_pipeline");
    group.measurement_time(Duration::from_secs(10));

    // Simulate gateway processing different frame types

    // Standard wM-Bus frame
    let wmbus_frame = {
        let mut frame = vec![0x44u8]; // Length
        frame.extend_from_slice(&[0x93, 0x15]); // Control + Manufacturer
        frame.extend_from_slice(&[0x68, 0x61, 0x05, 0x28]); // Device address
        frame.extend_from_slice(&[0x74, 0x37]); // Version + Type
        frame.extend_from_slice(&generate_test_data(32)); // Payload
        let crc = calculate_wmbus_crc(&frame);
        frame.extend_from_slice(&crc.to_le_bytes());
        frame
    };

    group.bench_function("wmbus_frame_validation", |b| {
        b.iter(|| {
            verify_wmbus_crc(black_box(&wmbus_frame))
        })
    });

    // Multi-block frame (Type A)
    let multiblock_frame = {
        let mut frame = vec![0x80u8]; // Length for multi-block
        frame.extend_from_slice(&[0x93, 0x15]); // Control + Manufacturer
        frame.extend_from_slice(&[0x68, 0x61, 0x05, 0x28]); // Device address
        frame.extend_from_slice(&[0x74, 0x37]); // Version + Type

        // Add 4 blocks (64 bytes)
        for _ in 0..4 {
            let block_data = generate_test_data(14);
            let block_crc = calculate_block_crc(&block_data);
            frame.extend_from_slice(&block_data);
            frame.extend_from_slice(&block_crc.to_le_bytes());
        }

        let crc = calculate_wmbus_crc(&frame);
        frame.extend_from_slice(&crc.to_le_bytes());
        frame
    };

    group.bench_function("multiblock_validation", |b| {
        b.iter(|| {
            // Extract payload for block validation
            let payload = &multiblock_frame[10..multiblock_frame.len() - 2];
            let _ = verify_blocks(black_box(payload), false);
        })
    });

    group.finish();
}

/// Benchmark batch processing (gateway scenario)
fn bench_batch_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_processing");
    group.measurement_time(Duration::from_secs(10));

    // Generate 1000 frames of varying sizes
    let frames: Vec<Vec<u8>> = (0..1000)
        .map(|i| {
            let size = 16 + (i % 64) * 4; // Vary from 16 to 268 bytes
            generate_test_data(size)
        })
        .collect();

    group.throughput(Throughput::Elements(frames.len() as u64));

    // Benchmark checksum calculation for all frames
    group.bench_function("batch_checksum", |b| {
        b.iter(|| {
            for frame in &frames {
                let _ = calculate_mbus_checksum(black_box(frame));
            }
        })
    });

    // Benchmark CRC calculation for all frames
    group.bench_function("batch_crc", |b| {
        b.iter(|| {
            for frame in &frames {
                let _ = calculate_wmbus_crc(black_box(frame));
            }
        })
    });

    // Benchmark mixed operations (checksum + CRC)
    group.bench_function("batch_mixed", |b| {
        b.iter(|| {
            for (i, frame) in frames.iter().enumerate() {
                if i % 2 == 0 {
                    let _ = calculate_mbus_checksum(black_box(frame));
                } else {
                    let _ = calculate_wmbus_crc(black_box(frame));
                }
            }
        })
    });

    group.finish();
}

/// Performance comparison: scalar vs potential SIMD
fn bench_optimization_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("optimization_comparison");
    group.measurement_time(Duration::from_secs(10));

    // Test data for comparison
    let data_256 = generate_test_data(256);
    let data_1k = generate_test_data(1024);
    let data_4k = generate_test_data(4096);

    // Current implementation (will be optimized with SIMD later)
    for (size, data) in &[(256, &data_256), (1024, &data_1k), (4096, &data_4k)] {
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(
            BenchmarkId::new("current", size),
            data,
            |b, data| {
                b.iter(|| {
                    calculate_mbus_checksum(black_box(data))
                })
            },
        );

        // Placeholder for SIMD implementation
        // Will be replaced with actual SIMD when implemented
        group.bench_with_input(
            BenchmarkId::new("future_simd", size),
            data,
            |b, data| {
                b.iter(|| {
                    // For now, same as current
                    calculate_mbus_checksum(black_box(data))
                })
            },
        );
    }

    group.finish();
}

#[cfg(feature = "crypto")]
criterion_group!(
    benches,
    bench_mbus_checksum,
    bench_wmbus_crc,
    bench_block_crc,
    bench_lorawan_mic,
    bench_hmac_sha1,
    bench_frame_validation_pipeline,
    bench_batch_processing,
    bench_optimization_comparison
);

#[cfg(not(feature = "crypto"))]
criterion_group!(
    benches,
    bench_mbus_checksum,
    bench_wmbus_crc,
    bench_block_crc,
    bench_frame_validation_pipeline,
    bench_batch_processing,
    bench_optimization_comparison
);

criterion_main!(benches);