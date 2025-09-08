use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mbus_rs::mbus::frame::parse_frame;
use nom::IResult;

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

fn benchmark_parse_frame(c: &mut Criterion) {
    let hex = "6831316808017245585703B40534049E0027B60306F934150315C6004D052E00000000053D00000000055B22F32642055FC7DA0D42FA16";
    let data = hex_to_bytes(hex);

    c.bench_function("parse_frame", |b| {
        b.iter(|| {
            let result: IResult<&[u8], mbus_rs::MBusFrame> = parse_frame(black_box(&data));
            let _ = black_box(result);
        })
    });
}

criterion_group!(benches, benchmark_parse_frame);
criterion_main!(benches);