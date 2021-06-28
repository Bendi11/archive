use bar::ar::Bar;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use indicatif::ProgressBar;
use std::io::Cursor;

fn pack_nocompress(c: &mut Criterion) {
    c.bench_function("Barchive pack (no compression)", move |b| {
        b.iter_with_setup(
            || std::io::BufWriter::new(std::fs::File::create("./benches/test-out.bar").unwrap()),
            |mut file| {
                let mut bar = black_box(Bar::pack(
                    "./benches/test-in",
                    Cursor::new(vec![0u8; 2048]),
                    "none".parse().unwrap(),
                    ProgressBar::hidden(),
                ))
                .unwrap();
                black_box(bar.save(&mut file)).unwrap();
            },
        )
    });

    c.bench_function("Barchive unpack (no compression)", move |b| {
        b.iter(|| black_box(Bar::unpack("./benches/test-out.bar").unwrap()))
    });
}

criterion_group!(pack, pack_nocompress);
criterion_main!(pack);
