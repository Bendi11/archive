use std::io::{Read, Seek, Write};

use indicatif::ProgressBar;
pub mod lz77;

/// The `Optimize` enum represents how a [Compressor] should compress or decompress its input data
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum Optimize {
    /// Optimize for large files
    Ultra,
    /// Big files, not huge
    High,
    /// Optimize for best performance across all files
    Average,
    /// Optimize for many small files
    Less,
}

/// The `Compressor` trait allows an archive to use many different compression methods with one
/// simple API. It contains methods to compress and decompress data from types implementing
/// `Read` and `Seek`.
pub trait Compressor<R: Read + Seek> {
    type Error;

    /// Get a name for the compression format
    fn name() -> &'static str;

    /// Compress some input data and write the compressed output to a type implementing `Write`
    fn compress<W: Write>(reader: R, writer: &mut W, opts: Optimize) -> Result<(), Self::Error> {
        Self::compress_progress(reader, writer, opts, ProgressBar::hidden())
    }

    /// Decompress some input data and write the decompressed bytes to a type implementing `Write`
    fn decompress<W: Write>(reader: R, writer: &mut W, opts: Optimize) -> Result<(), Self::Error> {
        Self::decompress_progress(reader, writer, opts, ProgressBar::hidden())
    }

    /// Compress a reader into a `Vec<u8>` convience wrapper for the `compress` method
    fn compress_vec(reader: R, opts: Optimize) -> Result<Vec<u8>, Self::Error> {
        let mut vec = vec![];
        Self::compress(reader, &mut vec, opts)?;
        Ok(vec)
    }

    /// Decompress a reader into a `Vec<u8>` convience wrapper for the `deccompress` method
    fn deccompress_vec(reader: R, opts: Optimize) -> Result<Vec<u8>, Self::Error> {
        let mut vec = std::io::Cursor::new(vec![]);
        Self::decompress(reader, &mut vec, opts)?;
        Ok(vec.into_inner())
    }

    fn compress_progress<W: Write>(
        reader: R,
        writer: &mut W,
        opts: Optimize,
        prog: ProgressBar,
    ) -> Result<(), Self::Error>;

    fn decompress_progress<W: Write>(
        reader: R,
        writer: &mut W,
        opts: Optimize,
        prog: ProgressBar,
    ) -> Result<(), Self::Error>;
}
