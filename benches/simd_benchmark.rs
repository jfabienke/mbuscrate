use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mbus_rs::mbus::frame::calculate_mbus_checksum;
use mbus_rs::wmbus::frame_decode::calculate_wmbus_crc_enhanced;

// Scalar reference implementations for comparison
fn calculate_checksum_scalar(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

fn calculate_crc_scalar(data: &[u8]) -> u16 {
    const POLYNOMIAL: u16 = 0x8408;
    const INITIAL: u16 = 0x3791;

    let mut crc = INITIAL;

    for &byte in data {
        crc ^= byte as u16;

        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ POLYNOMIAL;
            } else {
                crc >>= 1;
            }
        }
    }

    crc
}

fn bench_checksum_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("checksum");

    // Test different data sizes
    for size in [16, 64, 256, 1024, 4096, 16384].iter() {
        let data = vec![0x42u8; *size];

        group.throughput(Throughput::Bytes(*size as u64));

        // Benchmark scalar implementation
        group.bench_with_input(
            BenchmarkId::new("scalar", size),
            &data,
            |b, data| b.iter(|| calculate_checksum_scalar(black_box(data))),
        );

        // Benchmark SIMD implementation
        group.bench_with_input(
            BenchmarkId::new("simd", size),
            &data,
            |b, data| b.iter(|| calculate_mbus_checksum(black_box(data))),
        );
    }

    group.finish();
}

fn bench_crc_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("wmbus_crc");

    // Test different data sizes
    for size in [16, 64, 256, 1024, 4096, 16384].iter() {
        let data = vec![0x42u8; *size];

        group.throughput(Throughput::Bytes(*size as u64));

        // Benchmark scalar implementation
        group.bench_with_input(
            BenchmarkId::new("scalar", size),
            &data,
            |b, data| b.iter(|| calculate_crc_scalar(black_box(data))),
        );

        // Benchmark SIMD implementation
        group.bench_with_input(
            BenchmarkId::new("simd", size),
            &data,
            |b, data| b.iter(|| calculate_wmbus_crc_enhanced(black_box(data))),
        );
    }

    group.finish();
}

fn bench_real_world_frames(c: &mut Criterion) {
    let mut group = c.benchmark_group("real_world");

    // Typical wM-Bus frame sizes
    let frame_sizes = [
        ("short_frame", 19),    // Minimum wM-Bus frame
        ("standard_frame", 74), // Common meter reading
        ("long_frame", 234),    // Extended data frame
        ("max_frame", 255),     // Maximum frame size
    ];

    for (name, size) in frame_sizes.iter() {
        let data = vec![0x68u8; *size]; // 0x68 is common start byte

        group.throughput(Throughput::Bytes(*size as u64));

        // Benchmark complete frame processing with SIMD
        group.bench_with_input(
            BenchmarkId::new("frame_checksum", name),
            &data,
            |b, data| b.iter(|| {
                let checksum = calculate_mbus_checksum(black_box(data));
                let crc = calculate_wmbus_crc_enhanced(black_box(data));
                (checksum, crc)
            }),
        );
    }

    group.finish();
}

fn bench_cpu_feature_impact(c: &mut Criterion) {
    // This benchmark shows the impact of different CPU features
    let mut group = c.benchmark_group("cpu_features");

    let data_1k = vec![0x42u8; 1024];
    let data_16k = vec![0x42u8; 16384];

    group.throughput(Throughput::Bytes(1024));

    // Small data benchmark
    group.bench_function("small_data_simd", |b| {
        b.iter(|| calculate_mbus_checksum(black_box(&data_1k)))
    });

    group.throughput(Throughput::Bytes(16384));

    // Large data benchmark
    group.bench_function("large_data_simd", |b| {
        b.iter(|| calculate_mbus_checksum(black_box(&data_16k)))
    });

    // Show CPU features being used
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            println!("NEON: enabled");
        }
        if std::arch::is_aarch64_feature_detected!("crc") {
            println!("CRC: enabled");
        }
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("sse2") {
            println!("SSE2: enabled");
        }
        if is_x86_feature_detected!("avx2") {
            println!("AVX2: enabled");
        }
        if is_x86_feature_detected!("sse4.2") {
            println!("SSE4.2 (CRC32): enabled");
        }
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_checksum_comparison,
    bench_crc_comparison,
    bench_real_world_frames,
    bench_cpu_feature_impact
);
criterion_main!(benches);