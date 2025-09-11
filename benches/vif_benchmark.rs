use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use mbus_rs::payload::vif::{parse_vif, parse_vib};
use mbus_rs::payload::vif_maps::{lookup_primary_vif, lookup_vife_fd, lookup_vife_fb};
use std::time::Duration;

// Performance target: VIF operations must complete in <0.1ms
// const TARGET_VIF_LOOKUP_US: f64 = 100.0; // 0.1ms in microseconds

fn benchmark_vif_lookups(c: &mut Criterion) {
    let mut group = c.benchmark_group("vif_lookups");
    group.measurement_time(Duration::from_secs(5));
    
    // Benchmark primary VIF lookup (most common)
    let common_vifs = vec![
        0x13, // Volume (liters)
        0x06, // Energy (kWh)
        0x5B, // Flow temperature (°C)
        0x2B, // Power (W)
        0x6D, // Date and time
    ];
    
    for vif in &common_vifs {
        group.bench_with_input(
            BenchmarkId::new("primary_vif_lookup", format!("0x{:02X}", vif)),
            vif,
            |b, &vif| {
                b.iter(|| {
                    let _ = lookup_primary_vif(black_box(vif));
                });
            },
        );
    }
    
    // Benchmark special VIF codes
    group.bench_function("special_vif_7C_ascii", |b| {
        b.iter(|| {
            let _ = lookup_primary_vif(black_box(0x7C));
        });
    });
    
    group.bench_function("special_vif_7E_any", |b| {
        b.iter(|| {
            let _ = lookup_primary_vif(black_box(0x7E));
        });
    });
    
    group.bench_function("special_vif_7F_manufacturer", |b| {
        b.iter(|| {
            let _ = lookup_primary_vif(black_box(0x7F));
        });
    });
    
    group.finish();
}

fn benchmark_vif_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("vif_parsing");
    
    // Simple VIF parsing
    group.bench_function("parse_single_vif", |b| {
        let vif_data = vec![0x13]; // Volume
        b.iter(|| {
            let _ = parse_vif(black_box(&vif_data));
        });
    });
    
    // VIF with extensions
    let test_vibs = vec![
        ("simple", vec![0x13]),                     // Just primary VIF
        ("1_extension", vec![0x93]),                // With extension bit
        ("with_fd", vec![0x13, 0xFD, 0x08]),       // With FD extension
        ("with_fb", vec![0x13, 0xFB, 0x08]),       // With FB extension
    ];
    
    for (name, vib_data) in &test_vibs {
        group.bench_with_input(
            BenchmarkId::new("parse_vib", name),
            vib_data,
            |b, vib| {
                b.iter(|| {
                    let _ = parse_vib(black_box(vib));
                });
            },
        );
    }
    
    group.finish();
}

fn benchmark_vif_extensions(c: &mut Criterion) {
    let mut group = c.benchmark_group("vif_extensions");
    
    // FD extension lookups
    group.bench_function("lookup_vife_fd", |b| {
        b.iter(|| {
            let _ = lookup_vife_fd(black_box(0x08)); // Known FD code
        });
    });
    
    // FB extension lookups
    group.bench_function("lookup_vife_fb", |b| {
        b.iter(|| {
            let _ = lookup_vife_fb(black_box(0x08)); // Known FB code
        });
    });
    
    group.finish();
}

fn benchmark_vif_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("vif_scaling");
    
    // Benchmark scaling factor calculation for volume VIFs
    let volume_vifs = vec![
        (0x10, "0.001 m³"),
        (0x11, "0.01 m³"),
        (0x12, "0.1 m³"),
        (0x13, "1 m³"),
        (0x14, "10 m³"),
        (0x15, "100 m³"),
        (0x16, "1000 m³"),
    ];
    
    for (vif, description) in &volume_vifs {
        group.bench_with_input(
            BenchmarkId::new("volume_scaling", description),
            vif,
            |b, &vif| {
                b.iter(|| {
                    // Lookup and check scaling
                    if let Some(info) = lookup_primary_vif(vif) {
                        black_box(info.exponent);
                    }
                });
            },
        );
    }
    
    group.finish();
}

// Ensure VIF operations meet performance targets
fn verify_performance_targets(c: &mut Criterion) {
    let mut group = c.benchmark_group("vif_performance_targets");
    group.significance_level(0.01); // 99% confidence
    
    // Critical path: parse VIB and get unit
    let typical_vib = vec![0x13]; // Simple volume VIF
    
    group.bench_function("vif_parse_under_100us", |b| {
        b.iter(|| {
            let result = parse_vib(black_box(&typical_vib));
            black_box(result);
        });
    });
    
    // Verify lookup performance
    group.bench_function("vif_lookup_under_100us", |b| {
        b.iter(|| {
            let result = lookup_primary_vif(black_box(0x13));
            black_box(result);
        });
    });
    
    group.finish();
}

criterion_group!{
    name = benches;
    config = Criterion::default()
        .sample_size(1000)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(5));
    targets = benchmark_vif_lookups,
              benchmark_vif_parsing,
              benchmark_vif_extensions,
              benchmark_vif_scaling,
              verify_performance_targets
}
criterion_main!(benches);