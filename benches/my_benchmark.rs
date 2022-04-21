use ae_rs::{Decoder, Encoder};
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use std::io::Cursor;

fn criterion_benchmark(c: &mut Criterion) {
    let mut g = c.benchmark_group("bazoopy");
    g.throughput(Throughput::Bytes(64 * 1024 as u64));
    g.bench_function("Encode 64KB", |b| {
        let mut output = Vec::new();
        output.reserve(1 * 1024 * 1024);
        let mut encoder = Encoder::new(&mut output);
        let x = black_box(37);
        b.iter(move || {
            for _ in 0..64 * 1024 {
                encoder.encode_next(x).unwrap();
            }
        });
    });
    g.bench_function("Decode 64KB", |b| {
        let mut output = Vec::new();
        output.reserve(1 * 1024 * 1024);
        {
            let mut encoder = Encoder::new(&mut output);
            let x = black_box(37);
            for _ in 0..64 * 1024 {
                encoder.encode_next(x).unwrap();
            }
        }
        let mut cursor = Cursor::new(&output);
        let mut decoder = Decoder::new(&mut cursor).unwrap();
        b.iter(move || {
            for _ in 0..64 * 1024 {
                decoder.decode_next().unwrap();
            }
        });
    });
    g.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
