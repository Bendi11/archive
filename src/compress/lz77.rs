//! Contains structs like the all-important [Archive] struct

use indicatif::ProgressBar;
use std::{io::{BufRead, Read, Seek, SeekFrom, Write}, u8};
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

/// Return the bitsizes for a given optimization level
#[inline(always)]
const fn opt_bitsize(opt: Optimize) -> u32 {
    match opt {
        Optimize::Ultra => 15,   //32768B window size for large files
        Optimize::High => 14,     
        Optimize::Average => 12, //4096B for average files
        Optimize::Less => 10,   //10248B for small files
    }
}

/// Get the maximum value for a given optimization level, this is used to determine window size
#[inline(always)]
const fn opt_max(opt: Optimize) -> usize {
    2usize.pow(opt_bitsize(opt)) - 1
}


/// The `Lz77` struct compresses any type that implements the `Read` and `Seek` traits using the lz77 compression
/// algorithm
pub struct Lz77<R: BufRead + Seek> {
    /// The input buffer that we are compressing
    data: R,

    /// The length of our input data
    len: u64,
}

impl<R: BufRead + Seek> Lz77<R> {
    /// Create a new Lz77 compressor from an input reader
    pub fn new(mut data: R) -> Self {
        Self { len: Self::len(&mut data), data }
    }

    /// Return the length of the input bytes using seek operations
    fn len(data: &mut R) -> u64 {
        data.seek(SeekFrom::End(0)).unwrap(); //Seek to the end of the stream
        let len = data.stream_position().unwrap(); //Get the current stream position
        len
    }

    /// Compress the input reader into a vector of bytes
    pub fn compress(&mut self, writer: &mut impl Write, _opt: Optimize, progress: ProgressBar) -> LzResult<()> {
        let mut pos = 0u64; //Start at byte 0
        let len = self.len;
        progress.set_length(len);

        while pos < len {
            let (off, matchlen) = self.longest_match(pos)?; //Get the best match in the previous data
            writer.write_all(&[off])?;
            if off == 0 {
                writer.write_all(&[self.data.byte_at(pos).unwrap()])?; //Write the byte literal
                pos += 1;
                progress.inc(1);     
            } else {
                writer.write_all(&[matchlen])?;
                pos += matchlen as u64;
                progress.inc(1);
            }
        }

        Ok(())
    }

    /// Compress data but show it as a human readable format
    pub fn debug_compress(&mut self) -> LzResult<String> {
        let mut out = String::new(); //Create an output buffer

        let mut pos = 0u64; //Start at byte 0
        let len = self.len;
        while pos < len {
            let (off, matchlen) = self.longest_match(pos)?; //Get the best match in the previous data
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

        Ok(out)
    }

    /// Decompress the input data, assumes that it is valid Lz77 format
    pub fn decompress(&mut self, writer: &mut impl Write, _opt: Optimize, progress: ProgressBar) -> LzResult<()> {
        let mut window = Vec::new();
        let mut pos = 0u64;
        let len = self.len;
        progress.set_length(len);

        while pos + 1 < len {
            let (first, second) = (self.data.byte().unwrap(), self.data.byte().unwrap());
            pos += 2;
            match (first, second) {
                //Null pointer, byte literal
                (0u8, item) => {
                    writer.write(&[item])?;
                    window.push(item);
                    progress.inc(1);
                }
                //Offset and len to read
                (offset, mut match_len) => {
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
                    writer.write_all(matching.as_ref())?;
                    window.extend(matching);
                }
            }
            let drain = window.drain(..(window.len() - 255));
            drop(drain);
        }

        Ok(())
    }

    /// Search our window for the longest match and return the pair of (offset, len) or (0, 0) if there is no match
    #[inline]
    fn longest_match(&mut self, pos: u64) -> LzResult<(u8, u8)> {
        //Get the start position to seek to
        let start = if pos > 255 {
            pos - 255
        } else {
            0
        };
        let (bestlen, off) = (start..pos)
            .map(|off| (self.match_len(off, pos).unwrap(), off))
            .max_by(|(prev, _), (this, _)| prev.cmp(this))
            .unwrap_or((0, 0));
        let bestoff = (pos - off) as u8;

        //If we don't break even, then return 0
        Ok(if bestlen < (2) as u8 {
            (0, 0)
        } else {
            (bestoff, bestlen)
        })
    }

    /// Return the number of matching bytes that match between the current offset and the position
    fn match_len(&mut self, off: u64, pos: u64) -> LzResult<u8> {
        let off_to_pos = pos - off;
        let pos_read_len = if self.len < pos + 255 {
            self.len - pos
        } else { 255 };
        let window = self.data.bytes_at(pos, off_to_pos)?;
        let read = self.data.bytes_at(pos, pos_read_len)?;

        //Read bytes and compare them
        Ok(window.iter().zip(read).take_while(|(left, right)| *left == right).count() as u8)
    }

}

impl<R: BufRead + Seek> Compressor<R> for Lz77<R> {
    fn name() -> &'static str {
        "lz77"
    }

    type Error = LzErr;
    fn compress_progress<W: Write>(reader: R, writer: &mut W, opts: Optimize, prog: ProgressBar) -> Result<(), Self::Error> {
        let mut me = Self::new(reader);
        me.compress(writer, opts, prog)?;
        Ok(())
    }

    fn decompress_progress<W: Write>(reader: R, writer: &mut W, opts: Optimize, prog: ProgressBar) -> Result<(), Self::Error> {
        let mut me = Self::new(reader);
        me.decompress(writer, opts, prog)?;
        Ok(())
    }
}

/// The `LzSS` struct compresses any type that implements the `Read` and `Seek` traits using the lzss compression
/// algorithm. It has a selectable window size and requires bitwise I/O for compression and decompression,
/// but can often compress more than lz77
pub struct LzSS<R: BufRead + Seek> {
    /// The input buffer that we are compressing
    data: R,

    /// The length of the input buffer, pre-calculated
    len: u64,
}

/// Any error that can occur when compressing or decompressing with the [LzSS] algorithm
#[derive(Error, Debug)]
pub enum LzErr {
    #[error("An internal Input/Output error occurred")]
    IO(#[from] std::io::Error),

    #[error("An invalid pointer value was detected")]
    InvalidPointer,
}

type LzResult<T> = Result<T, LzErr>;

impl<R: BufRead + Seek> LzSS<R> {
    /// Create a new Lz77 compressor from an input reader
    pub fn new(mut data: R) -> Self {
        Self {
            len: Self::len(&mut data).unwrap(),
            data,
        }
    }

    /// Return the length of the input bytes using seek operations
    fn len(data: &mut R) -> LzResult<u64> {
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
    ) -> LzResult<()> {
        let mut out = BitWriter::endian(writer, bitstream_io::LittleEndian); //Create an output buffer
        
        let mut pos = 0u64; //Start at byte 0
        
        let bitsize = opt_bitsize(opt);
        let max = opt_max(opt);
        let len = self.len;
        progress.set_length(len);
        while pos < len {
            let (off, matchlen) = self.longest_match(pos, bitsize, max as u64)?; //Get the best match in the previous data
            if off == 0 {
                out.write_bit(true)?; //Write that this is a literal
                out.write::<u8>(8, self.data.byte_at(pos)?)?; //Write the byte literal
                pos += 1;
                progress.inc(1);
            } else {
                out.write_bit(false)?; //Indicate that this is a pointer
                out.write::<u16>(bitsize, off)?;
                out.write::<u16>(bitsize, matchlen)?;
                pos += matchlen as u64;
                progress.inc(matchlen as u64);
            }
        }

        out.byte_align()?;
        Ok(())
    }

    /// Compress data but show it as a human readable format
    pub fn debug_compress(&mut self) -> LzResult<String> {
        let mut out = String::new(); //Create an output buffer

        let mut pos = 0u64; //Start at byte 0
        let bitsize = opt_bitsize(Optimize::Average);
        let max = opt_max(Optimize::Average);
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


    /// Decompress the input data, assumes that it is valid Lz77 format
    pub fn decompress<W: Write>(
        &mut self,
        out: &mut W,
        opt: Optimize,
        progress: ProgressBar,
    ) -> LzResult<()> {
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
                        bits.read::<u16>(opt_bitsize(opt))?,
                        bits.read::<u16>(opt_bitsize(opt))?,
                    );
                    pos += (opt_bitsize(opt) * 2) as u64;

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
    fn longest_match(&mut self, pos: u64, bitsize: u32, max: u64) -> LzResult<(u16, u16)> {
        let mut bestoff = 0u16; //The best offset that we have found
        let mut bestlen = 0u16;
        //Get the start position to seek to
        let start = if pos > max {
            pos - max
        } else {
            0
        };

        let pos_read_len = if self.len < pos + max {
            self.len - pos
        } else { max };

        let read = self.data.bytes_at(pos, pos_read_len)?; //Read the bytes after our index

        for off in start..pos {
            let len = self.match_len(off, pos, &read[..])?;
            if len > bestlen {
                bestoff = (pos - off) as u16;
                bestlen = len;
            }
        }
        
        /*for ( ((window_pos, window_byte), read_byte), off) in window.iter().enumerate().zip(read.iter()).zip(start..pos) {
            if window_byte == read_byte {
                let len = self.match_len(&window[0..window_pos], &read[..])?;
                if len > bestlen {
                    bestlen = len;
                    bestoff = (pos - off) as u16;
                }
            }
        }*/

        //If we don't break even, then return 0
        Ok(if bestlen < (bitsize / 4) as u16 {
            (0, 0)
        } else {
            (bestoff, bestlen)
        })
    }

    /// Return the number of bytes that match between the current offset and the position
    fn match_len(&mut self, off: u64, pos: u64, read: &[u8]) -> LzResult<u16> {
        let off_to_pos = pos - off;
        /*let pos_read_len = if self.len < pos + max {
            self.len - pos
        } else { max };*/
        let window = self.data.bytes_at(pos, off_to_pos)?;
        //let read = self.data.bytes_at(pos, pos_read_len)?;

        //Read bytes and compare them
        Ok(window.iter().zip(read).take_while(|(left, right)| left == right).count() as u16)
    }
}

impl<R: BufRead + Seek> Compressor<R> for LzSS<R> {
    type Error = LzErr;

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
