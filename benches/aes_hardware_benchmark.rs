//! Benchmarks for AES hardware acceleration on Raspberry Pi 5
//!
//! This benchmark suite compares software vs hardware AES implementations
//! across different encryption modes (ECB, CBC, CTR, GCM).

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mbus_rs::wmbus::crypto::{WMBusCrypto, AesKey, EncryptionMode, DeviceInfo};
use mbus_rs::wmbus::crypto_hardware::{get_aes_backend, init_crypto_backend};
use std::time::Duration;

/// Test data sizes
const SIZES: &[usize] = &[16, 64, 256, 1024, 4096, 16384];

/// Generate test frame data
fn generate_frame(size: usize) -> Vec<u8> {
    let mut frame = vec![0u8; size];
    // Typical wM-Bus frame structure
    if size >= 12 {
        frame[0] = 0x44; // Length
        frame[1] = 0x44; // C-field
        frame[2] = 0x2D; // Manufacturer
        frame[3] = 0x2C; // Manufacturer
        frame[10] = 0x7A; // CI field (encrypted)
    }
    // Fill rest with pattern
    for i in 12..size {
        frame[i] = (i % 256) as u8;
    }
    frame
}

/// Benchmark ECB mode (single block)
fn bench_aes_ecb(c: &mut Criterion) {
    let mut group = c.benchmark_group("aes_ecb");
    group.measurement_time(Duration::from_secs(10));

    let backend = get_aes_backend();
    let key = [0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
               0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c];
    let input = [0x32, 0x43, 0xf6, 0xa8, 0x88, 0x5a, 0x30, 0x8d,
                 0x31, 0x31, 0x98, 0xa2, 0xe0, 0x37, 0x07, 0x34];

    group.throughput(Throughput::Bytes(16));

    // Benchmark hardware implementation
    group.bench_function(backend.name(), |b| {
        let mut output = [0u8; 16];
        b.iter(|| {
            backend.encrypt_block(black_box(&input), black_box(&key), &mut output);
        })
    });

    // If we have hardware, also benchmark software for comparison
    #[cfg(feature = "crypto")]
    {
        use aes::{
            cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit},
            Aes128,
        };

        group.bench_function("Software (aes crate)", |b| {
            b.iter(|| {
                let cipher = Aes128::new(GenericArray::from_slice(&key));
                let mut block = GenericArray::clone_from_slice(&input);
                cipher.encrypt_block(&mut block);
                black_box(block);
            })
        });
    }

    group.finish();
}

/// Benchmark full frame encryption/decryption
fn bench_frame_crypto(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_crypto");
    group.measurement_time(Duration::from_secs(10));

    init_crypto_backend(); // Initialize and log backend

    let key = AesKey::from_bytes(&[0x00; 16]).unwrap();
    let mut crypto = WMBusCrypto::new(key);

    let device_info = DeviceInfo {
        device_id: 0x12345678,
        manufacturer: 0x2D2C,
        version: 1,
        device_type: 7,
        access_number: Some(0x44),
    };

    for &size in SIZES {
        let frame = generate_frame(size);

        group.throughput(Throughput::Bytes(size as u64));

        // Benchmark encryption
        group.bench_with_input(
            BenchmarkId::new("encrypt", size),
            &frame,
            |b, frame| {
                b.iter(|| {
                    crypto.encrypt_frame(
                        black_box(frame),
                        black_box(&device_info),
                        black_box(EncryptionMode::Mode5Ctr),
                    )
                })
            },
        );

        // Prepare encrypted frame for decryption benchmark
        let encrypted = crypto
            .encrypt_frame(&frame, &device_info, EncryptionMode::Mode5Ctr)
            .unwrap();

        // Benchmark decryption
        group.bench_with_input(
            BenchmarkId::new("decrypt", size),
            &encrypted,
            |b, encrypted| {
                b.iter(|| {
                    crypto.decrypt_frame(black_box(encrypted), black_box(&device_info))
                })
            },
        );
    }

    group.finish();
}

/// Benchmark different encryption modes
fn bench_encryption_modes(c: &mut Criterion) {
    let mut group = c.benchmark_group("encryption_modes");
    group.measurement_time(Duration::from_secs(5));

    let key = AesKey::from_bytes(&[0x00; 16]).unwrap();
    let mut crypto = WMBusCrypto::new(key);

    let device_info = DeviceInfo {
        device_id: 0x12345678,
        manufacturer: 0x2D2C,
        version: 1,
        device_type: 7,
        access_number: Some(0x44),
    };

    let frame = generate_frame(256); // Standard frame size

    let modes = [
        ("Mode5_CTR", EncryptionMode::Mode5Ctr),
        ("Mode7_CBC", EncryptionMode::Mode7Cbc),
        ("Mode9_GCM", EncryptionMode::Mode9Gcm),
        ("ELL_ECB", EncryptionMode::EllEcb),
    ];

    group.throughput(Throughput::Bytes(256));

    for (name, mode) in &modes {
        group.bench_function(name, |b| {
            b.iter(|| {
                crypto.encrypt_frame(black_box(&frame), black_box(&device_info), black_box(*mode))
            })
        });
    }

    group.finish();
}

/// Benchmark parallel operations (simulating gateway workload)
fn bench_parallel_crypto(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_crypto");
    group.measurement_time(Duration::from_secs(10));

    let backend = get_aes_backend();
    let key = [0x00u8; 16];

    // Simulate processing multiple frames in parallel
    let batch_sizes = [10, 100, 1000];

    for &batch_size in &batch_sizes {
        let frames: Vec<[u8; 16]> = (0..batch_size)
            .map(|i| {
                let mut frame = [0u8; 16];
                frame[0] = (i % 256) as u8;
                frame
            })
            .collect();

        group.throughput(Throughput::Elements(batch_size as u64));

        group.bench_with_input(
            BenchmarkId::new("batch_encrypt", batch_size),
            &frames,
            |b, frames| {
                b.iter(|| {
                    let mut outputs = vec![[0u8; 16]; batch_size];
                    for (input, output) in frames.iter().zip(outputs.iter_mut()) {
                        backend.encrypt_block(black_box(input), black_box(&key), output);
                    }
                    black_box(outputs);
                })
            },
        );
    }

    group.finish();
}

/// Benchmark power efficiency (throughput per watt estimation)
fn bench_power_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("power_efficiency");
    group.measurement_time(Duration::from_secs(30)); // Longer for thermal stability

    let backend = get_aes_backend();
    println!("Backend: {}", backend.name());
    println!("Hardware accelerated: {}", backend.is_hardware_accelerated());

    // Large sustained workload
    let data_size = 1024 * 1024; // 1 MB
    let blocks = data_size / 16;
    let key = [0x00u8; 16];

    group.throughput(Throughput::Bytes(data_size as u64));

    group.bench_function("sustained_1mb", |b| {
        b.iter(|| {
            let mut total = 0u8;
            for _ in 0..blocks {
                let input = [total; 16];
                let mut output = [0u8; 16];
                backend.encrypt_block(&input, &key, &mut output);
                total = total.wrapping_add(output[0]);
            }
            black_box(total);
        })
    });

    // Note: Actual power measurement would require external tools
    // On Pi 5: vcgencmd measure_volts core && vcgencmd measure_temp
    println!("\nTo measure power on Raspberry Pi:");
    println!("  sudo vcgencmd measure_volts core");
    println!("  sudo vcgencmd measure_temp");
    println!("  sudo cat /sys/class/thermal/thermal_zone0/temp");

    group.finish();
}

criterion_group!(
    benches,
    bench_aes_ecb,
    bench_frame_crypto,
    bench_encryption_modes,
    bench_parallel_crypto,
    bench_power_efficiency
);
criterion_main!(benches);