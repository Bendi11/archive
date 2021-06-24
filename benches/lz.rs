use bar::compress::{
    lz77::{Lz77, LzSS},
    Compressor, Optimize,
};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::io::Cursor;

const LOREMIPSUM: &[u8] = include_bytes!("./loremipsum.txt");

fn lz_compress(c: &mut Criterion) {
    c.bench_function("LzSS_compress", move |b| {
        b.iter_with_setup(
            || (LzSS::new(Cursor::new(LOREMIPSUM)), Cursor::new(Vec::new())),
            |(mut compressor, mut out)| {
                black_box(
                    compressor.compress(
                        &mut out,
                        Optimize::Less,
                        indicatif::ProgressBar::new(0).with_style(
                            indicatif::ProgressStyle::default_bar()
                                .template(
                                    "[{bar}] {bytes}/{total_bytes} {binary_bytes_per_sec}: {msg}",
                                )
                                .progress_chars("=>."),
                        ),
                    ),
                )
            },
        )
    });

    /*c.bench_function("Lz77_compress", move |b| {
        b.iter_with_setup(
            || (Lz77::new(Cursor::new(LOREMIPSUM)), Cursor::new(Vec::<u8>::new())),
            |(mut compressor, _)| {
                black_box(
                    compressor.compress(),
                )
            },
        )
    }); */
}

criterion_group!(lz, lz_compress);
criterion_main!(lz);
