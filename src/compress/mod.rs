use std::io::{Read, Seek, Write};
pub mod lz77;

/// The `Optimize` enum represents how a [Compressor] should compress or decompress its input data
pub enum Optimize {
    /// Optimize for large files
    Large,
    /// Optimize for best performance across all files
    Average,
    /// Optimize for many small files
    Small,
}

/// The `Compressor` trait allows an archive to use many different compression methods with one 
/// simple API. It contains methods to compress and decompress data from types implementing
/// `Read` and `Seek`.
pub trait Compressor<R: Read + Seek> {
    type Error;

    /// Compress some input data and write the compressed output to a type implementing `Write`
    fn compress<W: Write>(reader: R, writer: &mut W, opts: Optimize) -> Result<(), Self::Error>;

    /// Decompress some input data and write the decompressed bytes to a type implementing `Write`
    fn decompress<W: Write>(reader: R, writer: &mut W, opts: Optimize) -> Result<(), Self::Error>;

    /// Compress a reader into a `Vec<u8>` convience wrapper for the `compress` method
    fn compress_vec(reader: R, opts: Optimize) -> Result<Vec<u8>, Self::Error> {
        let mut vec = vec![];
        Self::compress(reader, &mut vec, opts)?;
        Ok(vec)
    }

}