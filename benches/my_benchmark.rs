use ae_rs::{Decoder, Encoder};
use criterion::{black_box, criterion_group, criterion_main, Bencher, Criterion, Throughput};
use rand::Rng;
use std::io::Cursor;

fn bench_encode<const SIZE: usize>(b: &mut Bencher, src: &[usize]) {
    let mut output = Vec::new();
    output.reserve(SIZE * 8);
    let mut encoder = Encoder::new(&mut output);
    let src = black_box(&src[..]);

    b.iter(move || {
        for &x in src {
            encoder.encode_next(x).unwrap();
        }
    });
}

fn bench_decode<const SIZE: usize>(b: &mut Bencher, src: &[usize]) {
    let mut output = Vec::new();
    output.reserve(SIZE * 8);
    let src = &src[..];

    {
        let mut encoder = Encoder::new(&mut output);
        for &x in src {
            encoder.encode_next(x).unwrap();
        }
    }
    let output = black_box(output.clone());
    let mut cursor = Cursor::new(&output);
    let mut decoder = Decoder::new(&mut cursor).unwrap();
    b.iter(move || {
        for _ in 0..SIZE {
            decoder.decode_next().unwrap();
        }
    });
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut g = c.benchmark_group("bazoopy");

    let mut rng = rand::thread_rng();

    {
        const SIZE: usize = 64 * 1024;
        g.throughput(Throughput::Bytes(SIZE as u64));
        let src: &mut [usize] = &mut [0usize; SIZE];
        rng.fill(src);
        for i in 0..src.len() {
            src[i] = src[i] % 256;
        }
        g.bench_function("Encode random 64KB", |b| bench_encode::<SIZE>(b, src));
        g.bench_function("Decode random 64KB", |b| bench_decode::<SIZE>(b, src));
    }

    {
        const SIZE: usize = 8 * 1024;
        g.throughput(Throughput::Bytes(SIZE as u64));
        let src: &mut [usize] = &mut [0usize; SIZE];
        rng.fill(src);
        for i in 0..src.len() {
            src[i] = src[i] % 256;
        }
        g.bench_function("Encode random 8KB", |b| bench_encode::<SIZE>(b, src));
        g.bench_function("Decode random 8KB", |b| bench_decode::<SIZE>(b, src));
    }

    {
        const SIZE: usize = 64 * 1024;
        g.throughput(Throughput::Bytes(SIZE as u64));
        let src: &mut [usize] = &mut [0usize; SIZE];
        g.bench_function("Encode zeroes 64KB", |b| bench_encode::<SIZE>(b, src));
        g.bench_function("Decode zeroes 64KB", |b| bench_decode::<SIZE>(b, src));
    }

    {
        const SIZE: usize = 8 * 1024;
        g.throughput(Throughput::Bytes(SIZE as u64));
        let src: &mut [usize] = &mut [0usize; SIZE];
        g.bench_function("Encode zeroes 8KB", |b| bench_encode::<SIZE>(b, src));
        g.bench_function("Decode zeroes 8KB", |b| bench_decode::<SIZE>(b, src));
    }
    g.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
