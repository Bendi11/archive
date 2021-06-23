use criterion::{black_box, criterion_group, criterion_main, Criterion};
use bar::compress::{Compressor, Optimize, lz77::LzSS};
use std::io::Cursor;

const LOREMIPSUM: &[u8] = include_bytes!("./loremipsum.txt");

fn lz_compress(c: &mut Criterion) {
    c.bench_function(
        "lz_compress", 
        move |b| b.iter_with_setup(
            || (LzSS::new(Cursor::new(LOREMIPSUM)), Cursor::new(Vec::new())), 
            |(mut compressor, mut out)| {
                black_box(compressor.compress(&mut out, Optimize::Average, indicatif::ProgressBar::new(0).with_style(indicatif::ProgressStyle::default_bar().template("[{bar}] {bytes}/{total_bytes} {binary_bytes_per_sec}: {msg}").progress_chars("=>."))))
            }) );
}

criterion_group!(lz, lz_compress);
criterion_main!(lz);