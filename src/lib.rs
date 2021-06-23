pub mod compress;
use compress::{lz77::LzSS, Compressor, Optimize};
use indicatif::{ProgressBar, ProgressStyle};

use std::{
    fs::File,
    io::{BufReader, BufWriter, Cursor},
};

fn main() {
    let file = BufReader::new(File::open("./Top Gear S11E01.mkv").unwrap());
    let mut out = BufWriter::new(File::create("./compressed.archive").unwrap());
    LzSS::compress_progress(
        file,
        &mut out,
        Optimize::Average,
        ProgressBar::new(0).with_style(
            ProgressStyle::default_bar()
                .template("[{bar}] {bytes}/{total_bytes} {binary_bytes_per_sec}: {msg}")
                .progress_chars("=>."),
        ),
    )
    .unwrap();
    //println!("{}", data);

    //std::fs::write("./Top Gear S11E01-Restored.mkv", LzSS::deccompress_vec(Cursor::new(data), Optimize::Average).unwrap()).unwrap();
    //println!("{:#?}", decar.decompress())
}
