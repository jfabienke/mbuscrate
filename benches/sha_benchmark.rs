//! SHA-1 and HMAC-SHA1 performance benchmarks
//!
//! Compares hardware-accelerated vs software implementations on Raspberry Pi 5

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mbus_rs::wmbus::sha_hardware::{calculate_sha1, calculate_hmac_sha1, get_sha_backend};

/// Generate test data of specified size
fn generate_test_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 256) as u8).collect()
}

/// Benchmark SHA-1 with different data sizes
fn bench_sha1(c: &mut Criterion) {
    let mut group = c.benchmark_group("sha1");
    
    // Test with various data sizes
    let sizes = vec![64, 512, 1024, 4096, 16384, 65536]; // 64B to 64KB
    
    for size in sizes {
        let data = generate_test_data(size);
        group.throughput(Throughput::Bytes(size as u64));
        
        group.bench_with_input(BenchmarkId::new("hardware", size), &data, |b, data| {
            b.iter(|| {
                let result = calculate_sha1(black_box(data));
                black_box(result)
            })
        });
    }
    
    group.finish();
}

/// Benchmark HMAC-SHA1 with different data sizes
fn bench_hmac_sha1(c: &mut Criterion) {
    let mut group = c.benchmark_group("hmac_sha1");
    
    let key = b"test_key_for_hmac_benchmark_1234567890";
    let sizes = vec![64, 512, 1024, 4096, 16384, 65536];
    
    for size in sizes {
        let data = generate_test_data(size);
        group.throughput(Throughput::Bytes(size as u64));
        
        group.bench_with_input(BenchmarkId::new("hardware", size), &data, |b, data| {
            b.iter(|| {
                let result = calculate_hmac_sha1(black_box(key), black_box(data));
                black_box(result)
            })
        });
    }
    
    group.finish();
}

/// Benchmark backend initialization overhead
fn bench_backend_selection(c: &mut Criterion) {
    c.bench_function("backend_selection", |b| {
        b.iter(|| {
            let backend = get_sha_backend();
            black_box(backend)
        })
    });
}

/// Benchmark SHA-1 performance across different message patterns
fn bench_sha1_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("sha1_patterns");
    
    // Test different data patterns
    let patterns = vec![
        ("zeros", vec![0u8; 1024]),
        ("ones", vec![0xFFu8; 1024]),
        ("alternating", (0..1024).map(|i| if i % 2 == 0 { 0x00 } else { 0xFF }).collect()),
        ("random", generate_test_data(1024)),
    ];
    
    for (name, data) in patterns {
        group.throughput(Throughput::Bytes(data.len() as u64));
        
        group.bench_with_input(BenchmarkId::new("sha1", name), &data, |b, data| {
            b.iter(|| {
                let result = calculate_sha1(black_box(data));
                black_box(result)
            })
        });
    }
    
    group.finish();
}

/// Benchmark HMAC-SHA1 with different key sizes
fn bench_hmac_key_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("hmac_key_sizes");
    
    let data = generate_test_data(1024);
    let key_sizes = vec![16, 32, 64, 128]; // Different key lengths
    
    for key_size in key_sizes {
        let key = generate_test_data(key_size);
        
        group.bench_with_input(BenchmarkId::new("hmac", key_size), &(key, data.clone()), |b, (key, data)| {
            b.iter(|| {
                let result = calculate_hmac_sha1(black_box(key), black_box(data));
                black_box(result)
            })
        });
    }
    
    group.finish();
}

/// Benchmark SHA-1 with wM-Bus typical frame sizes
fn bench_wmbus_frame_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("wmbus_frames");
    
    // Typical wM-Bus frame sizes
    let frame_sizes = vec![
        ("compact", 12),    // Minimal frame
        ("short", 32),      // Short frame
        ("medium", 64),     // Medium frame
        ("long", 128),      // Long frame
        ("extended", 255),  // Maximum frame
    ];
    
    for (name, size) in frame_sizes {
        let data = generate_test_data(size);
        group.throughput(Throughput::Bytes(size as u64));
        
        group.bench_with_input(BenchmarkId::new("sha1", name), &data, |b, data| {
            b.iter(|| {
                let result = calculate_sha1(black_box(data));
                black_box(result)
            })
        });
        
        // Also benchmark HMAC for authentication scenarios
        let key = b"wmbus_device_key_123";
        group.bench_with_input(BenchmarkId::new("hmac", name), &data, |b, data| {
            b.iter(|| {
                let result = calculate_hmac_sha1(black_box(key), black_box(data));
                black_box(result)
            })
        });
    }
    
    group.finish();
}

/// Benchmark performance under different CPU loads
fn bench_concurrent_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_sha");
    
    let data = generate_test_data(1024);
    let key = b"concurrent_test_key";
    
    // Simulate concurrent operations
    group.bench_function("single_sha1", |b| {
        b.iter(|| {
            let result = calculate_sha1(black_box(&data));
            black_box(result)
        })
    });
    
    group.bench_function("single_hmac", |b| {
        b.iter(|| {
            let result = calculate_hmac_sha1(black_box(key), black_box(&data));
            black_box(result)
        })
    });
    
    // Batch operations
    group.bench_function("batch_sha1_10", |b| {
        b.iter(|| {
            for _ in 0..10 {
                let result = calculate_sha1(black_box(&data));
                black_box(result);
            }
        })
    });
    
    group.bench_function("batch_hmac_10", |b| {
        b.iter(|| {
            for _ in 0..10 {
                let result = calculate_hmac_sha1(black_box(key), black_box(&data));
                black_box(result);
            }
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_sha1,
    bench_hmac_sha1,
    bench_backend_selection,
    bench_sha1_patterns,
    bench_hmac_key_sizes,
    bench_wmbus_frame_sizes,
    bench_concurrent_operations
);
criterion_main!(benches);