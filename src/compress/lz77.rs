//! Contains structs like the all-important [Archive] struct

use indicatif::ProgressBar;
use std::{
    io::{Read, Seek, SeekFrom, Write},
    u8,
};
use thiserror::Error;

use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter};

use super::{Compressor, Optimize};

trait ReadByteExt {
    fn byte(&mut self) -> std::io::Result<u8>;

    fn bytes_at(&mut self, pos: u64, len: u64) -> std::io::Result<Vec<u8>>;

    fn byte_at(&mut self, pos: u64) -> std::io::Result<u8>;
}

impl<R: Read + Seek> ReadByteExt for R {
    /// Read a single byte from the reader and return it
    fn byte(&mut self) -> std::io::Result<u8> {
        let mut buf = [0u8; 1];
        self.read_exact(&mut buf)?; //Read the byte
        Ok(buf[0])
    }

    /// Read a certain number of bytes from a position
    fn bytes_at(&mut self, pos: u64, len: u64) -> std::io::Result<Vec<u8>> {
        let mut out = vec![0u8; len as usize];
        self.seek(SeekFrom::Start(pos))?;
        self.read_exact(&mut out)?; //Read the bytes into the output
        Ok(out)
    }

    /// Read a single byte at a certain position, and leave the current reader position there
    #[inline]
    fn byte_at(&mut self, pos: u64) -> std::io::Result<u8> {
        self.seek(SeekFrom::Start(pos))?; //Seek to the position
        self.byte()
    }
}

/// The `Lz77` struct compresses any type that implements the `Read` and `Seek` traits using the lz77 compression
/// algorithm
pub struct Lz77<R: Read + Seek> {
    /// The input buffer that we are compressing
    data: R,
}

impl<R: Read + Seek> Lz77<R> {
    /// Create a new Lz77 compressor from an input reader
    pub fn new(data: R) -> Self {
        Self { data }
    }

    /// Return the length of the input bytes using seek operations
    fn len(&mut self) -> u64 {
        self.data.seek(SeekFrom::End(0)).unwrap(); //Seek to the end of the stream
        let len = self.data.stream_position().unwrap(); //Get the current stream position
        len
    }

    /// Compress the input reader into a vector of bytes
    pub fn compress(&mut self) -> Vec<u8> {
        let mut out = Vec::new(); //Create an output buffer

        let mut pos = 0u64; //Start at byte 0
        let len = self.len();
        while pos < len {
            let (off, matchlen) = self.longest_match(pos); //Get the best match in the previous data
            out.push(off); //Write the 0 offset
            if off == 0 {
                out.push(self.data.byte_at(pos).unwrap()); //Write the byte literal
                pos += 1;
            } else {
                out.push(matchlen);
                pos += matchlen as u64;
            }
        }

        out
    }

    /// Compress data but show it as a human readable format
    pub fn debug_compress(&mut self) -> String {
        let mut out = String::new(); //Create an output buffer

        let mut pos = 0u64; //Start at byte 0
        let len = self.len();
        while pos < len {
            let (off, matchlen) = self.longest_match(pos); //Get the best match in the previous data
            if off == 0 {
                out.push_str("(0)");
                out.push(self.data.byte_at(pos).unwrap() as char);
                out.push(' ');
                pos += 1;
            } else {
                out.push_str(&*format!(
                    "({}, {}):({}) ",
                    off,
                    matchlen,
                    String::from_utf8(
                        self.data
                            .bytes_at(pos - off as u64, matchlen as u64)
                            .unwrap()
                    )
                    .unwrap()
                ));
                pos += matchlen as u64;
            }
        }

        out
    }

    /// Decompress the input data, assumes that it is valid Lz77 format
    pub fn decompress(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut pos = 0u64;
        let len = self.len();
        while pos + 1 < len {
            let (first, second) = (self.data.byte().unwrap(), self.data.byte().unwrap());
            pos += 2;
            match (first, second) {
                //Null pointer, byte literal
                (0u8, item) => {
                    out.push(item);
                }
                //Offset and len to read
                (offset, mut match_len) => {
                    let offpos = out.len() - offset as usize;
                    let mut matching = Vec::new();
                    while match_len > 0 {
                        if match_len >= offset {
                            match_len -= offset;
                            for i in offpos..(offpos + offset as usize) {
                                matching.push(out[i])
                            }
                        } else {
                            for i in offpos..(offpos + match_len as usize) {
                                matching.push(out[i])
                            }
                            break;
                        }
                    }
                    out.extend(matching);
                }
            }
        }

        out
    }

    /// Search our window for the longest match and return the pair of (offset, len) or (0, 0) if there is no match
    fn longest_match(&mut self, pos: u64) -> (u8, u8) {
        let mut bestoff = 0u8; //The best offset that we have found
        let mut bestlen = 0u8;
        //Get the start position to seek to
        let start = if pos > 255 { pos - 255 } else { 0 };

        for off in start..pos {
            let len = self.match_len(off, pos);
            if len > bestlen {
                bestoff = (pos - off) as u8;
                bestlen = len;
            }
        }

        (bestoff, bestlen)
    }

    /// Return the number of bytes that match between the current offset and the position
    fn match_len(&mut self, mut off: u64, mut pos: u64) -> u8 {
        let old_pos = self.data.stream_position().unwrap(); //Get the current stream position to restore
        let mut len = 0u8; //The length of the matched string

        while off < pos
            && pos < self.len()
            && self.data.byte_at(off).unwrap() == self.data.byte_at(pos).unwrap()
            && len < u8::MAX
        {
            pos += 1;
            off += 1;
            len += 1;
        }

        self.data.seek(SeekFrom::Start(old_pos)).unwrap();
        len
    }
}

/// The `LzSS` struct compresses any type that implements the `Read` and `Seek` traits using the lzss compression
/// algorithm. It has a selectable window size and requires bitwise I/O for compression and decompression,
/// but can often compress more than lz77
pub struct LzSS<R: Read + Seek> {
    /// The input buffer that we are compressing
    data: R,

    /// The length of the input buffer, pre-calculated
    len: u64,
}

/// Any error that can occur when compressing or decompressing with the [LzSS] algorithm
#[derive(Error, Debug)]
pub enum LzSSErr {
    #[error("An internal Input/Output error occurred")]
    IO(#[from] std::io::Error),

    #[error("An invalid pointer value was detected")]
    InvalidPointer,
}

type LzSSResult<T> = Result<T, LzSSErr>;

impl<R: Read + Seek> LzSS<R> {
    /// Create a new Lz77 compressor from an input reader
    pub fn new(mut data: R) -> Self {
        Self {
            len: Self::len(&mut data).unwrap(),
            data,
        }
    }

    /// Return the length of the input bytes using seek operations
    fn len(data: &mut R) -> LzSSResult<u64> {
        data.seek(SeekFrom::End(0))?; //Seek to the end of the stream
        let len = data.stream_position()?; //Get the current stream position
        Ok(len)
    }

    /// Compress the input reader into a vector of bytes
    pub fn compress<W: Write>(
        &mut self,
        writer: &mut W,
        opt: Optimize,
        progress: ProgressBar,
    ) -> LzSSResult<()> {
        let mut out = BitWriter::endian(writer, bitstream_io::LittleEndian); //Create an output buffer
        
        let mut pos = 0u64; //Start at byte 0
        
        let bitsize = Self::opt_bitsize(opt);
        let max = Self::opt_max(opt);
        let len = self.len;
        //progress.set_length(len);
        while pos < len {
            let (off, matchlen) = self.longest_match(pos, bitsize, max as u64)?; //Get the best match in the previous data
            if off == 0 {
                out.write_bit(true)?; //Write that this is a literal
                out.write::<u8>(8, self.data.byte_at(pos)?)?; //Write the byte literal
                pos += 1;
                //progress.inc(1);
            } else {
                out.write_bit(false)?; //Indicate that this is a pointer
                out.write::<u16>(bitsize, off)?;
                out.write::<u16>(bitsize, matchlen)?;
                pos += matchlen as u64;
                //progress.inc(matchlen as u64);
            }
        }

        out.byte_align()?;
        Ok(())
    }

    /// Compress data but show it as a human readable format
    pub fn debug_compress(&mut self) -> LzSSResult<String> {
        let mut out = String::new(); //Create an output buffer

        let mut pos = 0u64; //Start at byte 0
        let bitsize = Self::opt_bitsize(Optimize::Average);
        let max = Self::opt_max(Optimize::Average);
        let len = self.len;
        while pos < len {
            let (off, matchlen) = self.longest_match(pos, bitsize, max as u64)?; //Get the best match in the previous data
            if off == 0 {
                out.push_str("(1)");
                out.push(self.data.byte_at(pos)? as char);
                out.push(' ');
                pos += 1;
            } else {
                out.push_str(&*format!(
                    "(0)({}, {}):({}) ",
                    off,
                    matchlen,
                    String::from_utf8(self.data.bytes_at(pos - off as u64, matchlen as u64)?)
                        .unwrap()
                ));
                pos += matchlen as u64;
            }
        }

        Ok(out)
    }

    /// Return the bitsizes for a given optimization level
    #[inline(always)]
    fn opt_bitsize(opt: Optimize) -> u32 {
        match opt {
            Optimize::Ultra => 15,   //32768B window size for large files
            Optimize::High => 14,     
            Optimize::Average => 12, //4096B for average files
            Optimize::Less => 10,   //10248B for small files
        }
    }

    /// Get the maximum value for a given optimization level, this is used to determine window size
    #[inline(always)]
    fn opt_max(opt: Optimize) -> usize {
        2usize.pow(Self::opt_bitsize(opt)) - 1
    }

    /// Decompress the input data, assumes that it is valid Lz77 format
    pub fn decompress<W: Write>(
        &mut self,
        out: &mut W,
        opt: Optimize,
        progress: ProgressBar,
    ) -> LzSSResult<()> {
        const MAX_SIZE: usize = u16::MAX as usize;
        let mut window = vec![0u8; MAX_SIZE]; //Get a window buffer for replacements
                                              //let out_len = 0usize; //The length of the output in bytes
        let mut pos = 0u64;

        let len = self.len * 8;
        progress.set_length(len / 8); //Set the length of the progress bar

        self.data.seek(SeekFrom::Start(0))?;

        let mut bits = BitReader::endian(&mut self.data, bitstream_io::LittleEndian);
        while pos + 8 < len {
            let sign = bits.read_bit()?; //Read the bit that determines if this is a literal or a pointer
            pos += 1;
            match sign {
                true => {
                    let literal = bits.read::<u8>(8).unwrap(); //Read the raw byte
                    out.write(&[literal])?;
                    window.push(literal);

                    progress.inc(1); //Increment one byte
                    pos += 8;
                }
                false => {
                    let (offset, mut match_len) = (
                        bits.read::<u16>(Self::opt_bitsize(opt))?,
                        bits.read::<u16>(Self::opt_bitsize(opt))?,
                    );
                    pos += (Self::opt_bitsize(opt) * 2) as u64;

                    let offpos = window.len() - offset as usize;
                    let mut matching = Vec::new();
                    while match_len > 0 {
                        if match_len >= offset {
                            match_len -= offset;
                            for i in offpos..(offpos + offset as usize) {
                                matching.push(window[i])
                            }
                        } else {
                            for i in offpos..(offpos + match_len as usize) {
                                matching.push(window[i])
                            }
                            break;
                        }
                    }
                    progress.inc(matching.len() as u64);
                    out.write_all(matching.as_ref())?;
                    window.extend(matching.iter());
                    //out_len += matching.len();
                }
            }
            //Truncate the window
            let drain = window.drain(..(window.len() - MAX_SIZE));
            drop(drain);
        }
        Ok(())
    }

    /// Search our window for the longest match and return the pair of (offset, len) or (0, 0) if there is no match
    #[inline]
    fn longest_match(&mut self, pos: u64, bitsize: u32, max: u64) -> LzSSResult<(u16, u16)> {
        let mut bestoff = 0u16; //The best offset that we have found
        let mut bestlen = 0u16;
        //Get the start position to seek to
        let start = if pos > max {
            pos - max
        } else {
            0
        };

        for off in start..pos {
            let len = self.match_len(off, pos, max)?;
            if len > bestlen {
                bestoff = (pos - off) as u16;
                bestlen = len;
            }
        }

        //If we don't break even, then return 0
        Ok(if bestlen < (bitsize / 4) as u16 {
            (0, 0)
        } else {
            (bestoff, bestlen)
        })
    }

    /// Return the number of bytes that match between the current offset and the position
    fn match_len(&mut self, mut off: u64, mut pos: u64, max: u64) -> LzSSResult<u16> {
        //let old_pos = self.data.stream_position().unwrap(); //Get the current stream position to restore
        let mut len = 0u16; //The length of the matched string

        while off < pos
            && pos < self.len
            && self.data.byte_at(off)? == self.data.byte_at(pos)?
            && len < max as u16
        {
            pos += 1;
            off += 1;
            len += 1;
        }

        //self.data.seek(SeekFrom::Start(old_pos)).unwrap();
        Ok(len)
    }
}

impl<R: Read + Seek> Compressor<R> for LzSS<R> {
    type Error = LzSSErr;

    fn name() -> &'static str {
        "lzss"
    }

    #[inline]
    fn compress_progress<W: Write>(
        reader: R,
        writer: &mut W,
        opts: Optimize,
        p: ProgressBar,
    ) -> Result<(), Self::Error> {
        let mut me = Self::new(reader);
        me.compress(writer, opts, p)?;
        Ok(())
    }

    #[inline]
    fn decompress_progress<W: Write>(
        reader: R,
        writer: &mut W,
        opts: Optimize,
        p: ProgressBar,
    ) -> Result<(), Self::Error> {
        let mut me = Self::new(reader);
        me.decompress(writer, opts, p)?;
        Ok(())
    }
}
