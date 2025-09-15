use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mbus_rs::mbus::frame::calculate_mbus_checksum;
use mbus_rs::wmbus::frame_decode::calculate_wmbus_crc_enhanced;

fn bench_unaligned_buffers(c: &mut Criterion) {
    let mut group = c.benchmark_group("unaligned_buffers");

    // Test unaligned buffer access patterns
    for offset in [0, 1, 3, 7, 15].iter() {
        // Create a larger buffer and slice from an offset
        let buffer = vec![0x42u8; 1024 + 16];
        let unaligned = &buffer[*offset..*offset + 1024];

        group.throughput(Throughput::Bytes(1024));

        group.bench_with_input(
            BenchmarkId::new("checksum_offset", offset),
            unaligned,
            |b, data| b.iter(|| calculate_mbus_checksum(black_box(data))),
        );

        group.bench_with_input(
            BenchmarkId::new("crc_offset", offset),
            unaligned,
            |b, data| b.iter(|| calculate_wmbus_crc_enhanced(black_box(data))),
        );
    }

    group.finish();
}

fn bench_partial_frames(c: &mut Criterion) {
    let mut group = c.benchmark_group("partial_frames");

    // Test partial frame sizes (not aligned to SIMD vector width)
    let sizes = [
        13, 17, 19, 23, 29, 31,  // Prime numbers (worst case)
        33, 65, 127, 129, 255,   // Off-by-one from powers of 2
        15, 63, 511, 1023,       // One less than SIMD boundaries
    ];

    for size in sizes.iter() {
        let data = vec![0x68u8; *size];

        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(
            BenchmarkId::new("checksum", size),
            &data,
            |b, data| b.iter(|| calculate_mbus_checksum(black_box(data))),
        );

        group.bench_with_input(
            BenchmarkId::new("crc", size),
            &data,
            |b, data| b.iter(|| calculate_wmbus_crc_enhanced(black_box(data))),
        );
    }

    group.finish();
}

fn bench_worst_case_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("worst_case_patterns");

    // Create data patterns that might stress SIMD implementations
    let patterns = [
        ("all_zeros", vec![0x00u8; 1024]),
        ("all_ones", vec![0xFFu8; 1024]),
        ("alternating", (0..1024).map(|i| if i % 2 == 0 { 0x00 } else { 0xFF }).collect()),
        ("incrementing", (0..1024).map(|i| (i & 0xFF) as u8).collect()),
        ("random", {
            use std::collections::hash_map::RandomState;
            use std::hash::{BuildHasher, Hasher};
            let mut rng = RandomState::new().build_hasher();
            (0..1024).map(|i| {
                rng.write_usize(i);
                (rng.finish() & 0xFF) as u8
            }).collect()
        }),
    ];

    for (name, data) in patterns.iter() {
        group.throughput(Throughput::Bytes(1024));

        group.bench_with_input(
            BenchmarkId::new("checksum", name),
            data,
            |b, data| b.iter(|| calculate_mbus_checksum(black_box(data))),
        );

        group.bench_with_input(
            BenchmarkId::new("crc", name),
            data,
            |b, data| b.iter(|| calculate_wmbus_crc_enhanced(black_box(data))),
        );
    }

    group.finish();
}

fn bench_cache_effects(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_effects");

    // Test different data sizes to observe cache effects
    let sizes = [
        32,      // L1 cache line
        256,     // Typical L1 size
        4096,    // Page size
        32768,   // L2 cache typical
        262144,  // L3 cache stress
        1048576, // Beyond cache
    ];

    for size in sizes.iter() {
        let data = vec![0x42u8; *size];

        group.throughput(Throughput::Bytes(*size as u64));

        // Cold cache benchmark (allocate new data each iteration)
        group.bench_with_input(
            BenchmarkId::new("checksum_cold", size),
            size,
            |b, &size| b.iter(|| {
                let data = vec![0x42u8; size];
                calculate_mbus_checksum(black_box(&data))
            }),
        );

        // Warm cache benchmark (reuse same data)
        group.bench_with_input(
            BenchmarkId::new("checksum_warm", size),
            &data,
            |b, data| b.iter(|| calculate_mbus_checksum(black_box(data))),
        );
    }

    group.finish();
}

fn bench_real_world_edge_cases(c: &mut Criterion) {
    let mut group = c.benchmark_group("real_world_edge");

    // Real-world edge cases from meter data
    let cases = [
        ("min_frame", vec![0x10, 0x5B, 0x00]),  // Minimum valid frame
        ("corrupted_header", vec![0xFF; 19]),    // Corrupted sync
        ("max_ci_field", vec![0x68; 255]),       // Maximum CI field
        ("fragmented", {
            // Simulate fragmented reception
            let mut v = vec![0x68; 8];
            v.extend(&[0x00; 3]);
            v.extend(&[0x68; 8]);
            v
        }),
    ];

    for (name, data) in cases.iter() {
        group.bench_with_input(
            BenchmarkId::new("validation", name),
            data,
            |b, data| b.iter(|| {
                let checksum = calculate_mbus_checksum(black_box(data));
                let crc = calculate_wmbus_crc_enhanced(black_box(data));
                (checksum, crc)
            }),
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_unaligned_buffers,
    bench_partial_frames,
    bench_worst_case_patterns,
    bench_cache_effects,
    bench_real_world_edge_cases
);
criterion_main!(benches);